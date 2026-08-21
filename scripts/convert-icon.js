/**
 * Builds the application icons: assets/icon.svg → icon.png → icon.ico
 *
 * The vector is the source of truth. It is rasterised once at 1024 px, then
 * resized into the standard Windows icon sizes (16–256 px), which are bundled
 * into a single multi-resolution .ico. NSIS rejects icons containing entries
 * above 256 px, so feeding a larger image directly would break the installer.
 *
 * Usage: node scripts/convert-icon.js
 */
const pngToIco = require('png-to-ico');
const sharp = require('sharp');
const fs = require('fs');
const os = require('os');
const path = require('path');

const VECTOR = path.join(__dirname, '..', 'assets', 'icon.svg');
const SMALL_VECTOR = path.join(__dirname, '..', 'assets', 'icon-small.svg');
const SRC    = path.join(__dirname, '..', 'assets', 'icon.png');
const DEST   = path.join(__dirname, '..', 'assets', 'icon.ico');
const MASTER_SIZE = 1024;
const ICON_SIZES = [16, 24, 32, 48, 64, 128, 256];
// En dessous de ce seuil, le dessin réduit se referme : on emploie la variante
// au trait épaissi plutôt que le grand format mis à l'échelle.
const SMALL_SIZE_THRESHOLD = 24;

async function main() {
  if (fs.existsSync(VECTOR)) {
    console.log('🎨  Rendu assets/icon.svg → assets/icon.png …');
    await sharp(VECTOR, { density: 384 })
      .resize(MASTER_SIZE, MASTER_SIZE)
      .png()
      .toFile(SRC);
  }

  if (!fs.existsSync(SRC)) {
    console.error('❌  assets/icon.png introuvable.\n    Placez votre PNG dans assets/icon.png puis relancez.');
    process.exit(1);
  }

  console.log('🎨  Conversion assets/icon.png → assets/icon.ico …');

  // Resize the source into each standard size inside a temp directory
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ssh-manager-icon-'));
  const resizedPaths = [];
  try {
    const hasSmallVariant = fs.existsSync(SMALL_VECTOR);
    for (const size of ICON_SIZES) {
      const outPath = path.join(tmpDir, `icon-${size}.png`);
      const source = hasSmallVariant && size <= SMALL_SIZE_THRESHOLD
        ? sharp(SMALL_VECTOR, { density: 384 })
        : sharp(SRC);
      await source.resize(size, size, { fit: 'contain' }).png().toFile(outPath);
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
