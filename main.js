const { app, BrowserWindow, ipcMain, dialog, safeStorage, clipboard } = require('electron');
const path = require('path');
const { pathToFileURL } = require('url');
const crypto = require('crypto');
const os = require('os');
const { Client, utils: sshUtils } = require('ssh2');
const SftpClient = require('ssh2-sftp-client');
const ftp = require('basic-ftp');
const Store = require('electron-store');
const { spawn } = require('child_process');
const fs = require('fs');

// Chromium traite un débordement horizontal comme un geste « page précédente ».
// Dans une application à page unique, cela n'a aucun sens : sélectionner du
// texte de droite à gauche jusqu'au bord d'une zone scrollable déclenchait un
// retour en arrière. Doit être posé avant que l'application soit prête.
app.commandLine.appendSwitch('disable-features', 'OverscrollHistoryNavigation');

// ─── Encrypted storage ────────────────────────────────────────
let metaStore = null;
let store = null;

// electron-builder définit PORTABLE_EXECUTABLE_DIR uniquement dans la version
// portable. Dans ce mode, l'utilisateur peut choisir de ne pas chiffrer.
const IS_PORTABLE = !!process.env.PORTABLE_EXECUTABLE_DIR;

function getMetaStore() {
  if (!metaStore) metaStore = new Store({ name: 'ssh-manager-meta' });
  return metaStore;
}

function isSetupDone() {
  return getMetaStore().get('initialized', false);
}

function deriveKey(mnemonic, salt) {
  return crypto.pbkdf2Sync(
    mnemonic.trim().toLowerCase(),
    Buffer.from(salt, 'hex'),
    210000, 32, 'sha512'
  ).toString('hex');
}

function makeVerifyHash(mnemonic, salt) {
  return crypto.createHmac('sha256', Buffer.from(salt, 'hex'))
    .update(mnemonic.trim().toLowerCase())
    .digest('hex');
}

// ─── SSH host key pinning (TOFU — Trust On First Use) ─────────
// Stores the SHA-256 fingerprint of each server's host key on first connection.
// On subsequent connections, a mismatch indicates a possible man-in-the-middle
// attack and the connection is refused.
function computeHostKeyFingerprint(keyBuffer) {
  const digest = crypto.createHash('sha256').update(keyBuffer).digest('base64');
  return 'SHA256:' + digest.replace(/=+$/, '');
}

/**
 * Builds a `hostVerifier` callback for ssh2 that implements TOFU pinning.
 * Returns `{ verifier, state }` — after a failed connection, `state.rejection`
 * holds an explicit error message to surface to the user.
 */
function createHostVerifier(host, port) {
  const hostId = `${host}:${port}`;
  const state = { rejection: null };

  const verifier = (keyBuffer, callback) => {
    const fingerprint = computeHostKeyFingerprint(keyBuffer);
    const meta = getMetaStore();
    const knownHosts = meta.get('hostKeys', {});
    const pinned = knownHosts[hostId];

    if (!pinned) {
      // First time seeing this host — trust and remember it.
      knownHosts[hostId] = fingerprint;
      meta.set('hostKeys', knownHosts);
      return callback(true);
    }
    if (pinned === fingerprint) {
      return callback(true);
    }
    // Fingerprint changed — refuse and prepare an explicit error.
    state.rejection =
      `⚠ ALERTE SÉCURITÉ : la clé d'hôte de ${hostId} a changé.\n\n` +
      `Empreinte connue : ${pinned}\n` +
      `Empreinte reçue : ${fingerprint}\n\n` +
      `Cela peut indiquer une attaque de type « man-in-the-middle », ou une ` +
      `réinstallation légitime du serveur. La connexion a été refusée par sécurité.\n\n` +
      `Si vous êtes certain que ce changement est légitime, supprimez puis ` +
      `recréez cette connexion pour réinitialiser l'empreinte mémorisée.`;
    return callback(false);
  };

  return { verifier, state };
}

function getStoredKey() {
  const meta = getMetaStore();
  const encB64 = meta.get('encryptedKey');
  if (!encB64) return null;
  if (!safeStorage.isEncryptionAvailable()) return null;
  try {
    return safeStorage.decryptString(Buffer.from(encB64, 'base64'));
  } catch { return null; }
}

function initConnStore(key) {
  if (key) {
    store = new Store({ name: 'ssh-manager-connections', encryptionKey: key });
  } else {
    store = new Store({ name: 'ssh-manager-connections' });
  }
}

async function initializeStorage() {
  if (!isSetupDone()) {
    // First run: plain store, will be migrated after setup
    initConnStore(null);
    return;
  }
  // Portable mode: the user may have explicitly opted out of encryption.
  if (getMetaStore().get('encryptionDisabled', false)) {
    initConnStore(null);
    return;
  }
  const key = getStoredKey();
  initConnStore(key);
}

// ─── Sessions ────────────────────────────────────────────────
let mainWindow;
const sshSessions = new Map();
const sftpSessions = new Map();
const ftpSessions = new Map();

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1400, height: 900,
    minWidth: 900, minHeight: 600,
    frame: false,
    // Doit rester aligné sur --bg-base (renderer/src/index.css) : c'est la
    // couleur peinte avant le premier rendu du renderer.
    backgroundColor: '#151020',
    icon: path.join(__dirname, 'assets', 'icon.ico'),
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
    show: false,
  });
  mainWindow.once('ready-to-show', () => mainWindow.show());

  const isDev = process.env.NODE_ENV === 'development';
  const appUrl = isDev
    ? 'http://localhost:5173'
    : pathToFileURL(path.join(__dirname, 'dist', 'renderer', 'index.html')).toString();

  // ── Navigation hardening ──────────────────────────────────
  // The app is single-page: block window.open and any navigation away from the
  // app itself (defense-in-depth against XSS pivoting to external pages).
  //
  // Le filtre porte sur l'URL exacte de l'application, pas sur le simple
  // préfixe « file:// » : déposer un fichier ou une sélection de texte sur la
  // fenêtre fait naviguer Chromium vers le contenu déposé, ce qui suffisait à
  // faire disparaître l'interface.
  mainWindow.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  mainWindow.webContents.on('will-navigate', (event, url) => {
    if (url.startsWith(appUrl)) return;
    event.preventDefault();
    console.warn(`Navigation blocked — the app is single-page, requested: ${url}`);
  });

  mainWindow.loadURL(appUrl);
}

// ─── Pre-uninstall export mode ───────────────────────────────
const IS_PRE_UNINSTALL = process.argv.includes('--pre-uninstall-export');

async function runPreUninstallExport() {
  await initializeStorage();
  if (!store) { app.quit(); return; }

  const connections = store.get('connections', []);
  if (connections.length === 0) {
    dialog.showMessageBoxSync({
      type: 'info', title: 'SSH Manager — Export',
      message: 'Aucune connexion enregistrée à exporter.',
      buttons: ['OK'],
    });
    app.quit();
    return;
  }

  const result = dialog.showSaveDialogSync({
    title: 'Exporter les connexions avant désinstallation',
    defaultPath: `ssh-manager-backup-${new Date().toISOString().slice(0, 10)}.enc`,
    filters: [
      { name: 'Sauvegarde chiffrée SSH Manager', extensions: ['enc'] },
      { name: 'JSON non chiffré', extensions: ['json'] },
    ],
  });

  if (!result) { app.quit(); return; }

  try {
    const isEncrypted = result.endsWith('.enc');
    if (isEncrypted) {
      const meta = getMetaStore();
      const salt = meta.get('salt');
      const key = getStoredKey();
      if (key && salt) {
        const iv = crypto.randomBytes(16);
        const cipher = crypto.createCipheriv('aes-256-gcm', Buffer.from(key, 'hex').slice(0, 32), iv);
        const json = JSON.stringify({ connections, groups: [] });
        const encrypted = Buffer.concat([cipher.update(json, 'utf8'), cipher.final()]);
        const authTag = cipher.getAuthTag();
        fs.writeFileSync(result, JSON.stringify({
          version: 2, encrypted: true, salt,
          iv: iv.toString('hex'), authTag: authTag.toString('hex'),
          data: encrypted.toString('hex'),
        }), 'utf8');
      } else {
        fs.writeFileSync(result, JSON.stringify({ version: 1, connections, groups: [] }, null, 2), 'utf8');
      }
    } else {
      fs.writeFileSync(result, JSON.stringify({ version: 1, connections, groups: [] }, null, 2), 'utf8');
    }
    dialog.showMessageBoxSync({
      type: 'info', title: 'SSH Manager — Export réussi',
      message: `Vos ${connections.length} connexion(s) ont été exportées.\n\n${result}`,
      buttons: ['OK'],
    });
  } catch (err) {
    dialog.showMessageBoxSync({
      type: 'error', title: 'SSH Manager — Erreur export',
      message: 'Impossible d\'exporter : ' + err.message,
      buttons: ['OK'],
    });
  }
  app.quit();
}

app.whenReady().then(async () => {
  if (IS_PRE_UNINSTALL) {
    runPreUninstallExport();
    return;
  }
  await initializeStorage();
  createWindow();
  app.on('activate', () => { if (BrowserWindow.getAllWindows().length === 0) createWindow(); });
});

app.on('window-all-closed', () => {
  for (const [, s] of sshSessions) { try { s.client.end(); } catch {} }
  for (const [, c] of sftpSessions) { try { c.end(); } catch {} }
  for (const [, c] of ftpSessions) { try { c.close(); } catch {} }
  app.quit();
});

// ─── Window controls ─────────────────────────────────────────
ipcMain.handle('app:getVersion', () => app.getVersion());

// ─── Presse-papiers avec effacement différé ──────────────────
// Le minuteur vit ici et non dans le renderer : `navigator.clipboard` exige une
// fenêtre au premier plan, or l'effacement doit justement survenir pendant que
// l'utilisateur est parti coller ailleurs. Le module clipboard d'Electron, lui,
// n'a pas cette contrainte.
let clipboardClearTimeout = null;
/** Valeur en attente d'effacement, pour ne jamais écraser une copie ultérieure. */
let clipboardPendingText = null;

function cancelClipboardClear() {
  if (clipboardClearTimeout !== null) {
    clearTimeout(clipboardClearTimeout);
    clipboardClearTimeout = null;
  }
  clipboardPendingText = null;
}

/** Efface le presse-papiers s'il contient encore la valeur copiée. */
function clearClipboardIfUnchanged(text) {
  if (clipboard.readText() !== text) return false;
  clipboard.clear();
  return true;
}

const PRIVATE_CLIPBOARD_SCRIPT = 'write-private-clipboard.ps1';
const PRIVATE_CLIPBOARD_TIMEOUT_MS = 5000;

function privateClipboardScriptPath() {
  // Empaquetée, l'application vit dans app.asar : PowerShell ne peut pas y lire
  // un fichier. Le script est donc livré en ressource externe.
  return app.isPackaged
    ? path.join(process.resourcesPath, 'scripts', PRIVATE_CLIPBOARD_SCRIPT)
    : path.join(__dirname, 'scripts', PRIVATE_CLIPBOARD_SCRIPT);
}

/**
 * Writes `text` to the clipboard flagged as private, so Windows clipboard
 * history (Win+V) and cloud clipboard skip it. Resolves false when the native
 * path is unavailable — the caller then falls back to a plain copy.
 */
function writePrivateClipboard(text) {
  return new Promise(resolve => {
    const child = spawn('powershell.exe', [
      '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
      '-File', privateClipboardScriptPath(),
    ], { windowsHide: true });

    let stderr = '';
    const timer = setTimeout(() => { child.kill(); resolve(false); }, PRIVATE_CLIPBOARD_TIMEOUT_MS);

    child.stderr.on('data', chunk => { stderr += chunk.toString(); });
    child.on('error', err => {
      clearTimeout(timer);
      console.error('Private clipboard helper could not be started', err);
      resolve(false);
    });
    child.on('close', code => {
      clearTimeout(timer);
      if (code !== 0) {
        console.error(`Private clipboard helper exited with code ${code} — ${stderr.trim()}`);
      }
      resolve(code === 0);
    });

    // Le secret passe par l'entrée standard, jamais en argument : la ligne de
    // commande d'un processus est lisible par les autres processus.
    child.stdin.end(text);
  });
}

ipcMain.handle('clipboard:copy', async (_, { text, autoClear, delaySeconds }) => {
  cancelClipboardClear();

  // Le marquage « privé » est propre à Windows ; ailleurs, copie ordinaire.
  const privateMode = process.platform === 'win32'
    ? await writePrivateClipboard(text)
    : false;
  if (!privateMode) clipboard.writeText(text);

  if (!autoClear) return { autoClear: false, privateMode };

  clipboardPendingText = text;
  clipboardClearTimeout = setTimeout(() => {
    clipboardClearTimeout = null;
    clipboardPendingText = null;
    const cleared = clearClipboardIfUnchanged(text);
    console.log(cleared
      ? `Clipboard cleared after ${delaySeconds}s`
      : `Clipboard left untouched after ${delaySeconds}s — content changed since the copy`);
  }, delaySeconds * 1000);

  return { autoClear: true, delaySeconds, privateMode };
});

// Quitter avant l'échéance laisserait le secret dans le presse-papiers.
app.on('will-quit', () => {
  if (clipboardPendingText === null) return;
  clearClipboardIfUnchanged(clipboardPendingText);
  cancelClipboardClear();
});

ipcMain.handle('window:minimize', () => mainWindow.minimize());
ipcMain.handle('window:maximize', () => mainWindow.isMaximized() ? mainWindow.unmaximize() : mainWindow.maximize());
ipcMain.handle('window:close', () => mainWindow.close());

// ─── Crypto / Setup ──────────────────────────────────────────
ipcMain.handle('crypto:getSetupStatus', () => ({
  initialized: isSetupDone(),
  portable: IS_PORTABLE,
}));

// Portable mode only: the user explicitly chooses to store credentials
// without encryption (e.g. USB stick used across machines, where the
// OS keychain would lock the data to a single Windows session).
ipcMain.handle('crypto:skipSetup', () => {
  if (isSetupDone()) {
    return { success: false, error: 'La configuration a déjà été effectuée.' };
  }
  if (!IS_PORTABLE) {
    return { success: false, error: "Le mode sans chiffrement n'est disponible qu'en version portable." };
  }
  const meta = getMetaStore();
  meta.set('encryptionDisabled', true);
  meta.set('initialized', true);
  initConnStore(null);
  return { success: true };
});

ipcMain.handle('crypto:generateMnemonic', () => {
  const bip39 = require('bip39');
  return bip39.generateMnemonic();
});

ipcMain.handle('crypto:completeSetup', async (_, { mnemonic }) => {
  // Guard: never overwrite an existing configuration (would destroy the key/salt).
  if (isSetupDone()) {
    return { success: false, error: 'La configuration a déjà été effectuée.' };
  }
  // Guard: without OS-level key protection, the derived key cannot be persisted
  // and the encrypted store would become permanently unreadable on next launch.
  if (!safeStorage.isEncryptionAvailable()) {
    return {
      success: false,
      error: "Le chiffrement sécurisé du système d'exploitation n'est pas " +
        "disponible sur cette machine. Impossible de protéger vos données en " +
        "toute sécurité. La configuration a été annulée.",
    };
  }

  const bip39 = require('bip39');
  if (!bip39.validateMnemonic(mnemonic.trim().toLowerCase())) {
    return { success: false, error: 'Phrase invalide' };
  }
  const meta = getMetaStore();
  const salt = crypto.randomBytes(32).toString('hex');
  const key = deriveKey(mnemonic, salt);
  const verifyHash = makeVerifyHash(mnemonic, salt);

  // Protect key with OS keychain (guaranteed available — checked above)
  const encB64 = safeStorage.encryptString(key).toString('base64');
  meta.set('encryptedKey', encB64);
  meta.set('salt', salt);
  meta.set('verifyHash', verifyHash);
  meta.set('initialized', true);

  // Migrate existing unencrypted data → encrypted store
  const oldData = store ? store.get('connections', []) : [];
  initConnStore(key);
  store.set('connections', oldData);

  return { success: true };
});

ipcMain.handle('crypto:verifyMnemonic', (_, { mnemonic }) => {
  const meta = getMetaStore();
  const salt = meta.get('salt');
  const verifyHash = meta.get('verifyHash');
  if (!salt || !verifyHash) return { valid: false };
  const hash = makeVerifyHash(mnemonic, salt);
  return { valid: hash === verifyHash };
});

// ─── Connections store ────────────────────────────────────────
ipcMain.handle('connections:getAll', () => store ? store.get('connections', []) : []);
ipcMain.handle('connections:save', (_, connections) => {
  if (!store) return false;
  store.set('connections', connections);
  return true;
});

// ─── Export (encrypted) ───────────────────────────────────────
ipcMain.handle('connections:export', async (_, { connections, groups }) => {
  const result = await dialog.showSaveDialog(mainWindow, {
    title: 'Exporter les connexions',
    defaultPath: 'ssh-manager-backup.enc',
    filters: [
      { name: 'Sauvegarde chiffrée SSH Manager', extensions: ['enc'] },
      { name: 'JSON non chiffré', extensions: ['json'] },
    ],
  });
  if (result.canceled) return { success: false, canceled: true };

  try {
    const isEncrypted = result.filePath.endsWith('.enc');
    if (isEncrypted) {
      const meta = getMetaStore();
      const salt = meta.get('salt');
      const key = getStoredKey();
      if (!key || !salt) {
        return { success: false, error: 'Chiffrement non configuré' };
      }
      const iv = crypto.randomBytes(16);
      const cipher = crypto.createCipheriv('aes-256-gcm', Buffer.from(key, 'hex').slice(0, 32), iv);
      const json = JSON.stringify({ connections, groups });
      const encrypted = Buffer.concat([cipher.update(json, 'utf8'), cipher.final()]);
      const authTag = cipher.getAuthTag();
      const fileData = JSON.stringify({
        version: 2, encrypted: true,
        salt,
        iv: iv.toString('hex'),
        authTag: authTag.toString('hex'),
        data: encrypted.toString('hex'),
      });
      fs.writeFileSync(result.filePath, fileData, 'utf8');
    } else {
      // Plain JSON export stores passwords in clear text — require confirmation.
      const confirm = dialog.showMessageBoxSync(mainWindow, {
        type: 'warning',
        title: 'Export non chiffré',
        message: 'Exporter en clair ?',
        detail:
          'Le fichier .json contiendra vos mots de passe et identifiants EN CLAIR, ' +
          'lisibles par quiconque y a accès.\n\n' +
          'Préférez le format .enc (chiffré) sauf si vous savez ce que vous faites.',
        buttons: ['Annuler', 'Exporter en clair'],
        defaultId: 0,
        cancelId: 0,
      });
      if (confirm !== 1) return { success: false, canceled: true };
      fs.writeFileSync(result.filePath, JSON.stringify({ version: 1, connections, groups }, null, 2), 'utf8');
    }
    return { success: true, path: result.filePath };
  } catch (err) { return { success: false, error: err.message }; }
});

// ─── Import ───────────────────────────────────────────────────
ipcMain.handle('connections:import', async () => {
  const result = await dialog.showOpenDialog(mainWindow, {
    title: 'Importer des connexions',
    filters: [
      { name: 'Sauvegardes SSH Manager', extensions: ['enc', 'json'] },
    ],
    properties: ['openFile'],
  });
  if (result.canceled) return { success: false, canceled: true };
  try {
    const raw = fs.readFileSync(result.filePaths[0], 'utf8');
    const data = JSON.parse(raw);
    if (data.encrypted) {
      // Return the encrypted payload to renderer (needs mnemonic)
      return { success: false, requiresMnemonic: true, payload: data };
    }
    if (!data.connections || !Array.isArray(data.connections))
      return { success: false, error: 'Format de fichier invalide' };
    return { success: true, connections: data.connections, groups: data.groups ?? [] };
  } catch (err) { return { success: false, error: err.message }; }
});

ipcMain.handle('connections:importDecrypt', (_, { mnemonic, payload }) => {
  try {
    const key = deriveKey(mnemonic, payload.salt);
    const decipher = crypto.createDecipheriv(
      'aes-256-gcm',
      Buffer.from(key, 'hex').slice(0, 32),
      Buffer.from(payload.iv, 'hex')
    );
    decipher.setAuthTag(Buffer.from(payload.authTag, 'hex'));
    const decrypted = Buffer.concat([
      decipher.update(Buffer.from(payload.data, 'hex')),
      decipher.final(),
    ]);
    const { connections, groups } = JSON.parse(decrypted.toString('utf8'));
    return { success: true, connections, groups: groups ?? [] };
  } catch {
    return { success: false, error: 'Phrase incorrecte ou fichier corrompu' };
  }
});

// ─── SSH ─────────────────────────────────────────────────────
ipcMain.handle('ssh:connect', (_, { tabId, connection, cols, rows }) => {
  return new Promise((resolve, reject) => {
    const client = new Client();
    const port = connection.port || 22;
    const { verifier, state } = createHostVerifier(connection.host, port);
    const opts = {
      host: connection.host, port,
      username: connection.username, readyTimeout: 15000,
      hostVerifier: verifier,
    };
    if (connection.privateKeyPath) {
      try { opts.privateKey = fs.readFileSync(connection.privateKeyPath); }
      catch (e) { return reject('Impossible de lire la clé privée : ' + e.message); }
    } else {
      opts.password = connection.password || '';
    }

    client.on('ready', () => {
      // Open the remote PTY at the real size of the local xterm instance: a
      // mismatch makes the server draw the prompt at wrong coordinates,
      // overlapping previously displayed text.
      const initialCols = Number.isInteger(cols) && cols > 0 ? cols : 120;
      const initialRows = Number.isInteger(rows) && rows > 0 ? rows : 40;
      client.shell({ term: 'xterm-256color', cols: initialCols, rows: initialRows }, (err, stream) => {
        if (err) { client.end(); return reject(err.message); }
        sshSessions.set(tabId, { client, stream });
        // Forward raw bytes (Buffer → Uint8Array over IPC): xterm.js decodes them
        // as UTF-8 with a streaming decoder, so multi-byte characters split across
        // two network chunks render correctly. Decoding to a string here with a
        // single-byte encoding would garble accents and box-drawing characters.
        stream.on('data', data => {
          if (mainWindow && !mainWindow.isDestroyed())
            mainWindow.webContents.send('ssh:data:' + tabId, data);
        });
        stream.stderr.on('data', data => {
          if (mainWindow && !mainWindow.isDestroyed())
            mainWindow.webContents.send('ssh:data:' + tabId, data);
        });
        stream.on('close', () => {
          sshSessions.delete(tabId);
          if (mainWindow && !mainWindow.isDestroyed())
            mainWindow.webContents.send('ssh:close:' + tabId);
        });
        resolve({ success: true });
      });
    });
    client.on('error', err => reject(state.rejection || err.message));
    client.connect(opts);
  });
});

// Removes the pinned host key for a host:port (called when a connection is
// deleted, so a future server reinstall with the same address can re-pin).
ipcMain.handle('ssh:forgetHostKey', (_, { host, port }) => {
  const meta = getMetaStore();
  const knownHosts = meta.get('hostKeys', {});
  const hostId = `${host}:${port || 22}`;
  if (Object.prototype.hasOwnProperty.call(knownHosts, hostId)) {
    delete knownHosts[hostId];
    meta.set('hostKeys', knownHosts);
  }
  return { success: true };
});

// ─── Génération de clés SSH ──────────────────────────────────
// Le nom devient un nom de fichier dans ~/.ssh : il ne doit contenir ni
// séparateur de chemin ni séquence de remontée d'arborescence.
const KEY_NAME_PATTERN = /^[A-Za-z0-9._-]{1,64}$/;

const KEY_TYPES = {
  ed25519: { algorithm: 'ed25519', options: {} },
  rsa:     { algorithm: 'rsa',     options: { bits: 4096 } },
};

/**
 * Generates an SSH key pair in the user's ~/.ssh directory.
 * Never overwrites an existing key: losing a private key is unrecoverable.
 */
ipcMain.handle('ssh:generateKey', (_, { name, type }) => {
  if (!KEY_NAME_PATTERN.test(name ?? '')) {
    return { success: false, error: 'Nom de fichier invalide : lettres, chiffres, point, tiret et souligné uniquement.' };
  }
  const keyType = KEY_TYPES[type];
  if (!keyType) return { success: false, error: `Type de clé inconnu : ${type}` };

  const sshDir = path.join(os.homedir(), '.ssh');
  const privateKeyPath = path.join(sshDir, name);
  const publicKeyPath = privateKeyPath + '.pub';

  try {
    fs.mkdirSync(sshDir, { recursive: true, mode: 0o700 });

    if (fs.existsSync(privateKeyPath) || fs.existsSync(publicKeyPath)) {
      return { success: false, error: `Un fichier « ${name} » existe déjà dans ${sshDir}.` };
    }

    const comment = `${os.userInfo().username}@${os.hostname()}`;
    const pair = sshUtils.generateKeyPairSync(keyType.algorithm, { ...keyType.options, comment });

    // flag 'wx' : échoue si le fichier est apparu entre-temps, plutôt que d'écraser.
    fs.writeFileSync(privateKeyPath, pair.private, { mode: 0o600, flag: 'wx' });
    fs.writeFileSync(publicKeyPath, pair.public + '\n', { mode: 0o644, flag: 'wx' });

    console.log(`SSH key pair generated — type: ${type}, path: ${privateKeyPath}`);
    return { success: true, privateKeyPath, publicKey: pair.public };
  } catch (err) {
    console.error(`Failed to generate SSH key pair — type: ${type}, path: ${privateKeyPath}`, err);
    return { success: false, error: err.message };
  }
});

// Reads back the pinned host key fingerprint, so the connection inspector can
// show which key is trusted for a host. Returns null when nothing is pinned yet.
ipcMain.handle('ssh:getHostKeyFingerprint', (_, { host, port }) => {
  const knownHosts = getMetaStore().get('hostKeys', {});
  return knownHosts[`${host}:${port || 22}`] ?? null;
});

ipcMain.handle('ssh:write', (_, { tabId, data }) => { const s = sshSessions.get(tabId); if (s) s.stream.write(data); });
ipcMain.handle('ssh:resize', (_, { tabId, cols, rows }) => { const s = sshSessions.get(tabId); if (s) s.stream.setWindow(rows, cols, 0, 0); });
ipcMain.handle('ssh:disconnect', (_, tabId) => { const s = sshSessions.get(tabId); if (s) { try { s.client.end(); } catch {} sshSessions.delete(tabId); } });

function sanitizeForShell(value, maxLen = 128) {
  if (typeof value !== 'string') return '';
  return value.replace(/[^a-zA-Z0-9._@:\-]/g, '').slice(0, maxLen);
}

ipcMain.handle('ssh:openInPowerShell', (_, connection) => {
  const host = sanitizeForShell(connection.host);
  const username = sanitizeForShell(connection.username);
  const port = Number.isInteger(connection.port) && connection.port > 0 && connection.port < 65536 ? connection.port : 22;
  const portArg = port !== 22 ? `-p ${port} ` : '';
  spawn('powershell.exe', ['-NoExit', '-Command', `ssh ${portArg}${username}@${host}`], { detached: true, stdio: 'ignore' }).unref();
});
ipcMain.handle('ssh:openInCmd', (_, connection) => {
  const host = sanitizeForShell(connection.host);
  const username = sanitizeForShell(connection.username);
  const port = Number.isInteger(connection.port) && connection.port > 0 && connection.port < 65536 ? connection.port : 22;
  const portArg = port !== 22 ? `-p ${port} ` : '';
  spawn('cmd.exe', ['/K', `ssh ${portArg}${username}@${host}`], { detached: true, stdio: 'ignore' }).unref();
});

// ─── SFTP ────────────────────────────────────────────────────
ipcMain.handle('sftp:connect', async (_, { tabId, connection }) => {
  const client = new SftpClient();
  const port = connection.port || 22;
  const { verifier, state } = createHostVerifier(connection.host, port);
  try {
    const opts = {
      host: connection.host, port, username: connection.username,
      hostVerifier: verifier,
    };
    if (connection.privateKeyPath) opts.privateKey = fs.readFileSync(connection.privateKeyPath);
    else opts.password = connection.password || '';
    await client.connect(opts);
    sftpSessions.set(tabId, client);
    return { success: true };
  } catch (err) { return { success: false, error: state.rejection || err.message }; }
});
ipcMain.handle('sftp:list', async (_, { tabId, remotePath }) => {
  const c = sftpSessions.get(tabId);
  if (!c) return { success: false, error: 'Non connecté' };
  try { return { success: true, list: await c.list(remotePath) }; }
  catch (err) { return { success: false, error: err.message }; }
});
ipcMain.handle('sftp:mkdir', async (_, { tabId, remotePath }) => {
  const c = sftpSessions.get(tabId);
  if (!c) return { success: false, error: 'Non connecté' };
  try { await c.mkdir(remotePath, true); return { success: true }; }
  catch (err) { return { success: false, error: err.message }; }
});
ipcMain.handle('sftp:delete', async (_, { tabId, remotePath, isDir }) => {
  const c = sftpSessions.get(tabId);
  if (!c) return { success: false, error: 'Non connecté' };
  try { if (isDir) await c.rmdir(remotePath, true); else await c.delete(remotePath); return { success: true }; }
  catch (err) { return { success: false, error: err.message }; }
});
ipcMain.handle('sftp:download', async (_, { tabId, remotePath }) => {
  const result = await dialog.showSaveDialog(mainWindow, { defaultPath: path.basename(remotePath) });
  if (result.canceled) return { success: false };
  const c = sftpSessions.get(tabId);
  if (!c) return { success: false, error: 'Non connecté' };
  try { await c.fastGet(remotePath, result.filePath); return { success: true }; }
  catch (err) { return { success: false, error: err.message }; }
});
ipcMain.handle('sftp:upload', async (_, { tabId, remotePath }) => {
  const result = await dialog.showOpenDialog(mainWindow, { properties: ['openFile'] });
  if (result.canceled) return { success: false };
  const c = sftpSessions.get(tabId);
  if (!c) return { success: false, error: 'Non connecté' };
  const dest = remotePath.endsWith('/') ? remotePath + path.basename(result.filePaths[0]) : remotePath;
  try { await c.fastPut(result.filePaths[0], dest); return { success: true }; }
  catch (err) { return { success: false, error: err.message }; }
});
ipcMain.handle('sftp:disconnect', async (_, tabId) => {
  const c = sftpSessions.get(tabId);
  if (c) { try { await c.end(); } catch {} sftpSessions.delete(tabId); }
});

// ─── FTP ─────────────────────────────────────────────────────
ipcMain.handle('ftp:connect', async (_, { tabId, connection }) => {
  const client = new ftp.Client(10000);
  try {
    await client.access({ host: connection.host, port: connection.port || 21, user: connection.username, password: connection.password || '', secure: connection.ftpSecure || false });
    ftpSessions.set(tabId, client);
    return { success: true };
  } catch (err) { client.close(); return { success: false, error: err.message }; }
});
ipcMain.handle('ftp:list', async (_, { tabId, remotePath }) => {
  const c = ftpSessions.get(tabId);
  if (!c) return { success: false, error: 'Non connecté' };
  try { return { success: true, list: await c.list(remotePath) }; }
  catch (err) { return { success: false, error: err.message }; }
});
ipcMain.handle('ftp:mkdir', async (_, { tabId, remotePath }) => {
  const c = ftpSessions.get(tabId);
  if (!c) return { success: false, error: 'Non connecté' };
  try { await c.ensureDir(remotePath); return { success: true }; }
  catch (err) { return { success: false, error: err.message }; }
});
ipcMain.handle('ftp:delete', async (_, { tabId, remotePath, isDir }) => {
  const c = ftpSessions.get(tabId);
  if (!c) return { success: false, error: 'Non connecté' };
  try { if (isDir) await c.removeDir(remotePath); else await c.remove(remotePath); return { success: true }; }
  catch (err) { return { success: false, error: err.message }; }
});
ipcMain.handle('ftp:download', async (_, { tabId, remotePath }) => {
  const result = await dialog.showSaveDialog(mainWindow, { defaultPath: path.basename(remotePath) });
  if (result.canceled) return { success: false };
  const c = ftpSessions.get(tabId);
  if (!c) return { success: false, error: 'Non connecté' };
  try { await c.downloadTo(result.filePath, remotePath); return { success: true }; }
  catch (err) { return { success: false, error: err.message }; }
});
ipcMain.handle('ftp:upload', async (_, { tabId, remotePath }) => {
  const result = await dialog.showOpenDialog(mainWindow, { properties: ['openFile'] });
  if (result.canceled) return { success: false };
  const c = ftpSessions.get(tabId);
  if (!c) return { success: false, error: 'Non connecté' };
  const dest = remotePath.endsWith('/') ? remotePath + path.basename(result.filePaths[0]) : remotePath;
  try { await c.uploadFrom(result.filePaths[0], dest); return { success: true }; }
  catch (err) { return { success: false, error: err.message }; }
});
ipcMain.handle('ftp:disconnect', (_, tabId) => {
  const c = ftpSessions.get(tabId);
  if (c) { try { c.close(); } catch {} ftpSessions.delete(tabId); }
});

// ─── Dialogs ─────────────────────────────────────────────────
ipcMain.handle('dialog:pickKeyFile', async () => {
  const result = await dialog.showOpenDialog(mainWindow, {
    title: 'Sélectionner une clé privée SSH',
    properties: ['openFile'],
    filters: [{ name: 'Clé privée', extensions: ['pem', 'key', 'ppk', ''] }],
  });
  return result.canceled ? null : result.filePaths[0];
});
