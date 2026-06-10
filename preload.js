const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
  windowMinimize: () => ipcRenderer.invoke('window:minimize'),
  windowMaximize: () => ipcRenderer.invoke('window:maximize'),
  windowClose: () => ipcRenderer.invoke('window:close'),

  // ── Crypto / Setup ────────────────────────────────────────
  cryptoGetSetupStatus: () => ipcRenderer.invoke('crypto:getSetupStatus'),
  cryptoGenerateMnemonic: () => ipcRenderer.invoke('crypto:generateMnemonic'),
  cryptoCompleteSetup: (mnemonic) => ipcRenderer.invoke('crypto:completeSetup', { mnemonic }),
  cryptoSkipSetup: () => ipcRenderer.invoke('crypto:skipSetup'),
  cryptoVerifyMnemonic: (mnemonic) => ipcRenderer.invoke('crypto:verifyMnemonic', { mnemonic }),

  // ── Connections ───────────────────────────────────────────
  connectionsGetAll: () => ipcRenderer.invoke('connections:getAll'),
  connectionsSave: (c) => ipcRenderer.invoke('connections:save', c),
  connectionsExport: (data) => ipcRenderer.invoke('connections:export', data),
  connectionsImport: () => ipcRenderer.invoke('connections:import'),
  connectionsImportDecrypt: (p) => ipcRenderer.invoke('connections:importDecrypt', p),

  // ── SSH ───────────────────────────────────────────────────
  sshConnect: (p) => ipcRenderer.invoke('ssh:connect', p),
  sshWrite: (p) => ipcRenderer.invoke('ssh:write', p),
  sshResize: (p) => ipcRenderer.invoke('ssh:resize', p),
  sshDisconnect: (id) => ipcRenderer.invoke('ssh:disconnect', id),
  sshOpenInPowerShell: (c) => ipcRenderer.invoke('ssh:openInPowerShell', c),
  sshOpenInCmd: (c) => ipcRenderer.invoke('ssh:openInCmd', c),
  sshForgetHostKey: (p) => ipcRenderer.invoke('ssh:forgetHostKey', p),
  onSshData: (tabId, cb) => {
    const ch = 'ssh:data:' + tabId;
    ipcRenderer.on(ch, (_, d) => cb(d));
    return () => ipcRenderer.removeAllListeners(ch);
  },
  onSshClose: (tabId, cb) => {
    const ch = 'ssh:close:' + tabId;
    ipcRenderer.on(ch, () => cb());
    return () => ipcRenderer.removeAllListeners(ch);
  },

  // ── SFTP ──────────────────────────────────────────────────
  sftpConnect: (p) => ipcRenderer.invoke('sftp:connect', p),
  sftpList: (p) => ipcRenderer.invoke('sftp:list', p),
  sftpMkdir: (p) => ipcRenderer.invoke('sftp:mkdir', p),
  sftpDelete: (p) => ipcRenderer.invoke('sftp:delete', p),
  sftpDownload: (p) => ipcRenderer.invoke('sftp:download', p),
  sftpUpload: (p) => ipcRenderer.invoke('sftp:upload', p),
  sftpDisconnect: (id) => ipcRenderer.invoke('sftp:disconnect', id),

  // ── FTP ───────────────────────────────────────────────────
  ftpConnect: (p) => ipcRenderer.invoke('ftp:connect', p),
  ftpList: (p) => ipcRenderer.invoke('ftp:list', p),
  ftpMkdir: (p) => ipcRenderer.invoke('ftp:mkdir', p),
  ftpDelete: (p) => ipcRenderer.invoke('ftp:delete', p),
  ftpDownload: (p) => ipcRenderer.invoke('ftp:download', p),
  ftpUpload: (p) => ipcRenderer.invoke('ftp:upload', p),
  ftpDisconnect: (id) => ipcRenderer.invoke('ftp:disconnect', id),

  pickKeyFile: () => ipcRenderer.invoke('dialog:pickKeyFile'),
});
