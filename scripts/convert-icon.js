/**
 * Converts assets/icon.png → assets/icon.ico (sizes: 16,24,32,48,64,128,256)
 * Usage: node scripts/convert-icon.js
 */
const pngToIco = require('png-to-ico');
const fs = require('fs');
const path = require('path');

const SRC  = path.join(__dirname, '..', 'assets', 'icon.png');
const DEST = path.join(__dirname, '..', 'assets', 'icon.ico');

async function main() {
  if (!fs.existsSync(SRC)) {
    console.error('❌  assets/icon.png introuvable.\n    Placez votre PNG dans assets/icon.png puis relancez.');
    process.exit(1);
  }

  console.log('🎨  Conversion assets/icon.png → assets/icon.ico …');
  // pngToIco.default accepte un tableau de chemins de fichiers
  const ico = await pngToIco.default([SRC]);
  fs.writeFileSync(DEST, ico);
  console.log('✅  assets/icon.ico généré avec succès.');
}

main().catch(err => { console.error('Erreur :', err.message); process.exit(1); });
