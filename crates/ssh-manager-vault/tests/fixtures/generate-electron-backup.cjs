// Génère une sauvegarde .enc exactement comme le gestionnaire
// « connections:export » de l'application Electron (main.js) : le test Rust
// doit la restaurer avec la phrase, preuve que les anciennes sauvegardes des
// utilisateurs restent utilisables.
//
// Usage : node generate-electron-backup.cjs
// Les secrets employés sont fictifs et propres aux tests.

const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const MNEMONIC = 'abandon ability able about above absent absorb abstract absurd abuse access accident';
const SALT_HEX = '00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff';

// deriveKey() de main.js
const key = crypto.pbkdf2Sync(MNEMONIC.trim().toLowerCase(), Buffer.from(SALT_HEX, 'hex'), 210000, 32, 'sha512').toString('hex');

const connections = [
  { id: 'conn-1', name: 'prod-web-01', type: 'ssh', host: '10.20.0.11', port: 22, username: 'root',
    password: 'Pa55-wörd!', group: 'Production', favorite: true, createdAt: 1755000000000 },
  { id: 'conn-2', name: 'legacy-ftp', type: 'ftp', host: 'ftp.exemple.fr', port: 21, username: 'webmaster',
    ftpSecure: true, createdAt: 1755000002000 },
];
const groups = ['Production', 'Vide'];

// Corps du gestionnaire « connections:export », branche .enc
const iv = crypto.randomBytes(16);
const cipher = crypto.createCipheriv('aes-256-gcm', Buffer.from(key, 'hex').slice(0, 32), iv);
const json = JSON.stringify({ connections, groups });
const encrypted = Buffer.concat([cipher.update(json, 'utf8'), cipher.final()]);
const authTag = cipher.getAuthTag();
const fileData = JSON.stringify({
  version: 2, encrypted: true,
  salt: SALT_HEX,
  iv: iv.toString('hex'),
  authTag: authTag.toString('hex'),
  data: encrypted.toString('hex'),
});
fs.writeFileSync(path.join(__dirname, 'electron-backup.enc'), fileData, 'utf8');
console.log('electron-backup.enc written');
