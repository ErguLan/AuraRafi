#!/usr/bin/env node
// Generate the small companion icons consumed by the Hub surface.
// Pure Node.js — emits handcrafted PNGs using the built-in `zlib` and CRC tables.

"use strict";

const fs = require("node:fs");
const path = require("node:path");
const zlib = require("node:zlib");

const ROOT = path.resolve(__dirname, "..", "..");
const ICON_DIR = path.join(ROOT, "editor", "assets", "ui_icons");
const SCALE = 4;
const CANVAS = 64;
const PIXELS = CANVAS * SCALE;

const WHITE = [245, 245, 245, 255];
const ORANGE = [224, 116, 24, 255];

function makeCanvas() {
  return new Uint8Array(PIXELS * PIXELS * 4);
}

function setPixel(buf, x, y, color) {
  if (x < 0 || y < 0 || x >= PIXELS || y >= PIXELS) return;
  const i = (y * PIXELS + x) * 4;
  buf[i] = color[0];
  buf[i + 1] = color[1];
  buf[i + 2] = color[2];
  buf[i + 3] = color[3];
}

function drawLine(buf, points, color, width) {
  for (let i = 0; i < points.length - 1; i++) {
    const [x0, y0] = points[i];
    const [x1, y1] = points[i + 1];
    drawLineSegment(buf, x0 * SCALE, y0 * SCALE, x1 * SCALE, y1 * SCALE, color, width * SCALE);
  }
}

function drawLineSegment(buf, x0, y0, x1, y1, color, width) {
  const dx = Math.abs(x1 - x0);
  const dy = Math.abs(y1 - y0);
  const sx = x0 < x1 ? 1 : -1;
  const sy = y0 < y1 ? 1 : -1;
  let err = dx - dy;
  let x = x0;
  let y = y0;
  const radius = Math.max(0, Math.floor(width / 2));
  while (true) {
    fillCircle(buf, x, y, radius, color);
    if (x === x1 && y === y1) break;
    const e2 = 2 * err;
    if (e2 > -dy) {
      err -= dy;
      x += sx;
    }
    if (e2 < dx) {
      err += dx;
      y += sy;
    }
  }
}

function fillCircle(buf, cx, cy, r, color) {
  if (r <= 0) {
    setPixel(buf, cx, cy, color);
    return;
  }
  const r2 = r * r;
  for (let y = -r; y <= r; y++) {
    for (let x = -r; x <= r; x++) {
      if (x * x + y * y <= r2) {
        setPixel(buf, cx + x, cy + y, color);
      }
    }
  }
}

function strokeCircle(buf, cx, cy, radius, color, width) {
  const r = radius * SCALE;
  const w = width * SCALE;
  const inner = Math.max(0, r - w / 2);
  const outer = r + w / 2;
  const outer2 = outer * outer;
  const inner2 = inner * inner;
  for (let y = -outer; y <= outer; y++) {
    for (let x = -outer; x <= outer; x++) {
      const d2 = x * x + y * y;
      if (d2 <= outer2 && d2 >= inner2) {
        setPixel(buf, cx + x, cy + y, color);
      }
    }
  }
}

function drawRoundedStroke(buf, x, y, w, h, r, color, half) {
  const outer = r + half;
  const inner = Math.max(0, r - half);
  for (let py = y - outer; py <= y + h + outer; py++) {
    for (let px = x - outer; px <= x + w + outer; px++) {
      const d = distanceToRoundedRect(px, py, x, y, w, h, r);
      if (d <= half) setPixel(buf, px, py, color);
    }
  }
}

function distanceToRoundedRect(px, py, x, y, w, h, r) {
  const left = x;
  const right = x + w;
  const top = y;
  const bottom = y + h;
  const cx = px < left + r ? left + r : px > right - r ? right - r : px;
  const cy = py < top + r ? top + r : py > bottom - r ? bottom - r : py;
  const dx = px - cx;
  const dy = py - cy;
  if (px >= left && px <= right && py >= top && py <= bottom) {
    const distLeft = px - left;
    const distRight = right - px;
    const distTop = py - top;
    const distBottom = bottom - py;
    return Math.max(-Math.min(distLeft, distRight, distTop, distBottom), Math.hypot(dx, dy) - r);
  }
  return Math.hypot(dx, dy) - r;
}

function strokeRoundedRect(buf, x, y, w, h, radius, color, width) {
  const cx0 = x * SCALE;
  const cy0 = y * SCALE;
  const cw = w * SCALE;
  const ch = h * SCALE;
  const cr = radius * SCALE;
  const cWidth = width * SCALE;
  const inner = Math.max(0, cWidth / 2);
  drawRoundedStroke(buf, cx0, cy0, cw, ch, cr, color, inner);
}

function drawIcon(name, painter) {
  const buf = makeCanvas();
  painter(buf);
  writePng(path.join(ICON_DIR, name), buf, PIXELS, PIXELS);
}

function writePng(filename, rgba, width, height) {
  const signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;

  const rowBytes = width * 4;
  const raw = Buffer.alloc((rowBytes + 1) * height);
  for (let y = 0; y < height; y++) {
    raw[y * (rowBytes + 1)] = 0;
    rgba.subarray(y * rowBytes, (y + 1) * rowBytes).forEach((value, index) => {
      raw[y * (rowBytes + 1) + 1 + index] = value;
    });
  }
  const idatData = zlib.deflateSync(raw);

  const chunks = [
    makeChunk("IHDR", ihdr),
    makeChunk("IDAT", idatData),
    makeChunk("IEND", Buffer.alloc(0)),
  ];

  fs.writeFileSync(filename, Buffer.concat([signature, ...chunks]));
}

function makeChunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, "ascii");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0);
  return Buffer.concat([length, typeBuf, data, crc]);
}

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) {
    c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  }
  return (c ^ 0xffffffff) >>> 0;
}

function sun(buf) {
  const cx = 32 * SCALE;
  const cy = 32 * SCALE;
  for (let i = 0; i < 8; i++) {
    const angle = (i * Math.PI) / 4;
    const x1 = cx + Math.round(Math.cos(angle) * 16 * SCALE);
    const y1 = cy + Math.round(Math.sin(angle) * 16 * SCALE);
    const x2 = cx + Math.round(Math.cos(angle) * 22 * SCALE);
    const y2 = cy + Math.round(Math.sin(angle) * 22 * SCALE);
    drawLine(buf, [[x1 / SCALE, y1 / SCALE], [x2 / SCALE, y2 / SCALE]], ORANGE, 3);
  }
  fillCircle(buf, cx, cy, 7 * SCALE, ORANGE);
  strokeCircle(buf, cx, cy, 11, ORANGE, 3);
}

function moon(buf) {
  const cx = 32 * SCALE;
  const cy = 32 * SCALE;
  fillCircle(buf, cx, cy, 20 * SCALE, WHITE);
  fillCircle(buf, cx + 9 * SCALE, cy - 6 * SCALE, 18 * SCALE, ORANGE);
  fillCircle(buf, cx, cy, 20 * SCALE, WHITE);
  fillCircle(buf, cx + 9 * SCALE, cy - 6 * SCALE, 18 * SCALE, ORANGE);
  strokeCircle(buf, cx, cy, 20, WHITE, 3);
}

function home(buf) {
  drawLine(buf, [[10, 30], [32, 12], [54, 30]], WHITE, 3);
  drawLine(buf, [[16, 28], [16, 50], [48, 50], [48, 28]], WHITE, 3);
  strokeRoundedRect(buf, 26, 36, 12, 14, 2, ORANGE, 3);
}

function arrowRight(buf) {
  drawLine(buf, [[14, 32], [46, 32]], WHITE, 3);
  drawLine(buf, [[38, 24], [46, 32], [38, 40]], WHITE, 3);
}

function pulse(buf) {
  drawLine(buf, [[8, 32], [20, 32], [26, 18], [34, 46], [40, 24], [56, 32]], ORANGE, 3);
}

function main() {
  fs.mkdirSync(ICON_DIR, { recursive: true });
  drawIcon("sun.png", sun);
  drawIcon("moon.png", moon);
  drawIcon("home.png", home);
  drawIcon("arrow_right.png", arrowRight);
  drawIcon("pulse.png", pulse);
  console.log("Hub companion icons written to", ICON_DIR);
}

if (require.main === module) {
  main();
}
