/* Generates the placeholder PWA icons: a gold "H" monogram on the panel
   color. Dependency-free by Constitution Art. V — raw PNG bytes written
   with node:zlib (deflate) and a hand-rolled CRC32.
   Deterministic: same script, same bytes — the icons are committed, this
   script exists so they are regenerable. Run: `node scripts/gen-icons.mjs`.
   Placeholder art, flagged swappable: replace the files in web/public/
   with real art any time — no code change (spec FR-7). */

/* global Buffer, console */

import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';

// The app palette — taken from web/src/app.css :root (plan: from :root,
// not from memory).
const PANEL = [0x1a, 0x1d, 0x23]; // --panel
const GOLD = [0xdc, 0xb2, 0x5b]; // --gold

const PNG_SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

// CRC-32 (ISO 3309), the polynomial PNG requires.
const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let bit = 0; bit < 8; bit += 1) {
    c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  }
  return c >>> 0;
});

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) {
    crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

/** Encode RGB pixel rows (array of Buffers, each 3*w bytes, no filter
    bytes) as a complete PNG file. */
function encodePng(width, height, rows) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr.writeUInt8(8, 8); // bit depth
  ihdr.writeUInt8(2, 9); // color type: truecolor RGB
  // compression 0, filter 0, interlace 0 are the zeros already in the buffer
  const raw = Buffer.concat(
    rows.map((row) => Buffer.concat([Buffer.from([0]), row])), // filter: none
  );
  return Buffer.concat([
    PNG_SIGNATURE,
    chunk('IHDR', ihdr),
    chunk('IDAT', zlib.deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

function makeCanvas(size) {
  const rows = [];
  for (let y = 0; y < size; y += 1) {
    const row = Buffer.alloc(3 * size);
    for (let x = 0; x < size; x += 1) {
      row[3 * x] = PANEL[0];
      row[3 * x + 1] = PANEL[1];
      row[3 * x + 2] = PANEL[2];
    }
    rows.push(row);
  }
  return { size, rows };
}

/** Fill an axis-aligned rectangle with gold. */
function fillRect(canvas, x, y, w, h) {
  for (let ry = y; ry < y + h; ry += 1) {
    for (let rx = x; rx < x + w; rx += 1) {
      canvas.rows[ry][3 * rx] = GOLD[0];
      canvas.rows[ry][3 * rx + 1] = GOLD[1];
      canvas.rows[ry][3 * rx + 2] = GOLD[2];
    }
  }
}

/** Draw a blocky "H" centered on the panel: two bars and a crossbar.
    `scale` shrinks the mark — the maskable variant stays inside the safe
    zone (the inner 80% circle mask), the plain one fills more of the tile. */
function drawMonogram(canvas, scale) {
  const { size } = canvas;
  const bar = Math.max(2, Math.round(size * 0.13 * scale));
  const height = Math.round(size * 0.58 * scale);
  const width = 3 * bar; // two bars + one gap
  const x0 = Math.round((size - width) / 2);
  const y0 = Math.round((size - height) / 2);
  const cross = Math.max(2, Math.round(bar));
  fillRect(canvas, x0, y0, bar, height);
  fillRect(canvas, x0 + 2 * bar, y0, bar, height);
  fillRect(canvas, x0, y0 + Math.round((height - cross) / 2), width, cross);
}

function writeIcon(name, size, scale) {
  const canvas = makeCanvas(size);
  drawMonogram(canvas, scale);
  const out = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'public', name);
  fs.writeFileSync(out, encodePng(size, size, canvas.rows));
  console.log(`wrote ${name} (${size}x${size})`);
}

writeIcon('icon-192.png', 192, 1.0);
writeIcon('icon-512.png', 512, 1.0);
writeIcon('icon-maskable-512.png', 512, 0.72); // content inside the safe zone
