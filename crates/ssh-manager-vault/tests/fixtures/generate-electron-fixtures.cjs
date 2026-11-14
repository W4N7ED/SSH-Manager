// Génère des coffres au format de l'application Electron, avec la bibliothèque
// `conf` qu'elle utilise réellement (via electron-store). Les tests Rust
// doivent les déchiffrer à l'identique : c'est la garantie que l'import
// automatique lira les vrais coffres des utilisateurs.
//
// Usage : node generate-electron-fixtures.cjs <dossier node_modules de l'app Electron>
// Les secrets employés sont fictifs et propres aux tests.

const path = require('path');
const fs = require('fs');
const crypto = require('crypto');

const nodeModules = process.argv[2];
const Conf = require(path.join(nodeModules, 'conf')).default;

const MNEMONIC = 'abandon ability able about above absent absorb abstract absurd abuse access accident';
const SALT_HEX = '00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff';
const OUT = __dirname;

// Même dérivation que deriveKey() et makeVerifyHash() dans main.js
const keyHex = crypto.pbkdf2Sync(MNEMONIC, Buffer.from(SALT_HEX, 'hex'), 210000, 32, 'sha512').toString('hex');
const verifyHash = crypto.createHmac('sha256', Buffer.from(SALT_HEX, 'hex')).update(MNEMONIC).digest('hex');

const connections = [
  { id: 'conn-1', name: 'prod-web-01', type: 'ssh', host: '10.20.0.11', port: 22, username: 'root',
    password: 'Pa55-wörd!', group: 'Production', favorite: true, color: '#ef4444', createdAt: 1755000000000,
    lastUsedAt: 1755650000000 },
  { id: 'conn-2', name: 'backup-nas', type: 'sftp', host: '192.168.1.40', port: 22, username: 'admin',
    privateKeyPath: 'C:\\Users\\test\\.ssh\\id_ed25519', group: 'Sauvegardes', createdAt: 1755000001000 },
  { id: 'conn-3', name: 'legacy-ftp', type: 'ftp', host: 'ftp.exemple.fr', port: 21, username: 'webmaster',
    ftpSecure: true, createdAt: 1755000002000 },
];

function writeStore(name, value) {
  const cwd = fs.mkdtempSync(path.join(require('os').tmpdir(), 'conf-fixture-'));
  const store = new Conf({ cwd, configName: 'store', encryptionKey: keyHex });
  store.store = value;
  fs.copyFileSync(path.join(cwd, 'store.json'), path.join(OUT, name));
  fs.rmSync(cwd, { recursive: true, force: true });
}

writeStore('electron-connections.json', { connections });

// Chaque écriture tire un IV aléatoire. Le sel PBKDF2 de conf est cet IV relu
// comme texte UTF-8 : des octets invalides deviennent U+FFFD. Une série de
// coffres couvre ces cas de remplacement, qui doivent être reproduits à l'octet près.
for (let i = 0; i < 24; i++) {
  writeStore(`electron-iv-${String(i).padStart(2, '0')}.json`, { connections: [], marker: i });
}

fs.writeFileSync(path.join(OUT, 'electron-reference.json'), JSON.stringify({
  mnemonic: MNEMONIC, saltHex: SALT_HEX, keyHex, verifyHash, connectionCount: connections.length,
}, null, 2));

console.log('fixtures écrites :', fs.readdirSync(OUT).filter(f => f.startsWith('electron-')).length);
