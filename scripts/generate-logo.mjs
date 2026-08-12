// Generates assets/logo.png, the source image for `tauri icon`.
// Hand-rolled PNG writer so the repo needs no image dependency.
import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SIZE = 1024;
const BG = [24, 25, 28];
const ACCENT = [77, 124, 255];
const FG = [237, 238, 242];

const px = new Uint8Array(SIZE * SIZE * 4);

const set = (x, y, [r, g, b], a = 255) => {
  if (x < 0 || y < 0 || x >= SIZE || y >= SIZE) return;
  const i = (y * SIZE + x) * 4;
  const src = a / 255;
  px[i] = px[i] * (1 - src) + r * src;
  px[i + 1] = px[i + 1] * (1 - src) + g * src;
  px[i + 2] = px[i + 2] * (1 - src) + b * src;
  px[i + 3] = 255;
};

// Rounded-square background.
const radius = SIZE * 0.22;
for (let y = 0; y < SIZE; y++) {
  for (let x = 0; x < SIZE; x++) {
    const dx = Math.max(radius - x, x - (SIZE - radius), 0);
    const dy = Math.max(radius - y, y - (SIZE - radius), 0);
    const d = Math.hypot(dx, dy);
    if (d <= radius) set(x, y, BG, Math.min(255, (radius - d) * 255));
  }
}

// Two chevrons pointing right: "in becomes out".
const chevron = (cx, cy, half, thickness, color) => {
  for (let y = cy - half; y <= cy + half; y++) {
    const x = cx + half - Math.abs(y - cy);
    for (let t = 0; t < thickness; t++) {
      for (let s = -2; s <= 2; s++) set(x - t, y + s, color, 255 - Math.abs(s) * 40);
    }
  }
};

chevron(SIZE * 0.42, SIZE / 2, SIZE * 0.2, SIZE * 0.075, FG);
chevron(SIZE * 0.63, SIZE / 2, SIZE * 0.2, SIZE * 0.075, ACCENT);

// PNG encoding: filter byte 0 per scanline, then a single IDAT.
const raw = Buffer.alloc(SIZE * (SIZE * 4 + 1));
for (let y = 0; y < SIZE; y++) {
  raw[y * (SIZE * 4 + 1)] = 0;
  Buffer.from(px.buffer, y * SIZE * 4, SIZE * 4).copy(raw, y * (SIZE * 4 + 1) + 1);
}

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
};

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // RGBA

const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

const out = resolve(dirname(fileURLToPath(import.meta.url)), "../assets/logo.png");
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, png);
console.log(`wrote ${out} (${(png.length / 1024).toFixed(1)} KB)`);
