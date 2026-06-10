/**
 * Converts assets/icon.png → assets/icon.ico
 *
 * The source PNG can be any size: it is first resized into the standard
 * Windows icon sizes (16–256 px), which are then bundled into a single
 * multi-resolution .ico. NSIS rejects icons containing entries above 256 px,
 * so feeding the source image directly would break the installer build.
 *
 * Usage: node scripts/convert-icon.js
 */
const pngToIco = require('png-to-ico');
const sharp = require('sharp');
const fs = require('fs');
const os = require('os');
const path = require('path');

const SRC  = path.join(__dirname, '..', 'assets', 'icon.png');
const DEST = path.join(__dirname, '..', 'assets', 'icon.ico');
const ICON_SIZES = [16, 24, 32, 48, 64, 128, 256];

async function main() {
  if (!fs.existsSync(SRC)) {
    console.error('❌  assets/icon.png introuvable.\n    Placez votre PNG dans assets/icon.png puis relancez.');
    process.exit(1);
  }

  console.log('🎨  Conversion assets/icon.png → assets/icon.ico …');

  // Resize the source into each standard size inside a temp directory
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ssh-manager-icon-'));
  const resizedPaths = [];
  try {
    for (const size of ICON_SIZES) {
      const outPath = path.join(tmpDir, `icon-${size}.png`);
      await sharp(SRC).resize(size, size, { fit: 'contain' }).png().toFile(outPath);
      resizedPaths.push(outPath);
    }

    const ico = await pngToIco.default(resizedPaths);
    fs.writeFileSync(DEST, ico);
    const sizeKb = Math.round(fs.statSync(DEST).size / 1024);
    console.log(`✅  assets/icon.ico généré (${ICON_SIZES.join(', ')} px — ${sizeKb} Ko).`);
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
}

main().catch(err => { console.error('Erreur :', err.message); process.exit(1); });
