// Convertit un WOFF 1 en TTF/OTF (sfnt) : un WOFF 1 n'est qu'un sfnt dont
// chaque table est éventuellement compressée en zlib. egui ne lit pas le WOFF.
//
// Usage : node woff-to-ttf.cjs <entrée.woff> <sortie.ttf>
// Référence du format : https://www.w3.org/TR/WOFF/

const fs = require('fs');
const zlib = require('zlib');

const [input, output] = process.argv.slice(2);
const woff = fs.readFileSync(input);

if (woff.readUInt32BE(0) !== 0x774f4646) throw new Error(`${input} n'est pas un WOFF 1`);
const flavor = woff.readUInt32BE(4);
const numTables = woff.readUInt16BE(12);

// En-tête sfnt : searchRange / entrySelector / rangeShift dérivés du nombre de tables
const maxPower = 2 ** Math.floor(Math.log2(numTables));
const header = Buffer.alloc(12);
header.writeUInt32BE(flavor, 0);
header.writeUInt16BE(numTables, 4);
header.writeUInt16BE(maxPower * 16, 6);
header.writeUInt16BE(Math.log2(maxPower), 8);
header.writeUInt16BE(numTables * 16 - maxPower * 16, 10);

const tables = [];
for (let i = 0; i < numTables; i++) {
  const entry = 44 + i * 20;
  const tag = woff.readUInt32BE(entry);
  const offset = woff.readUInt32BE(entry + 4);
  const compLength = woff.readUInt32BE(entry + 8);
  const origLength = woff.readUInt32BE(entry + 12);
  const checksum = woff.readUInt32BE(entry + 16);
  const raw = woff.subarray(offset, offset + compLength);
  const data = compLength < origLength ? zlib.inflateSync(raw) : raw;
  if (data.length !== origLength) throw new Error(`table ${tag.toString(16)} : taille inattendue`);
  tables.push({ tag, checksum, data });
}

const directory = Buffer.alloc(numTables * 16);
let offset = 12 + numTables * 16;
const bodies = [];
tables.forEach((table, i) => {
  directory.writeUInt32BE(table.tag, i * 16);
  directory.writeUInt32BE(table.checksum, i * 16 + 4);
  directory.writeUInt32BE(offset, i * 16 + 8);
  directory.writeUInt32BE(table.data.length, i * 16 + 12);
  const padded = Buffer.alloc((table.data.length + 3) & ~3);
  table.data.copy(padded);
  bodies.push(padded);
  offset += padded.length;
});

fs.writeFileSync(output, Buffer.concat([header, directory, ...bodies]));
console.log(`${output} — ${numTables} tables, ${offset} octets`);
