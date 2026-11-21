// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez
//
// Minimal GeoTIFF reader for the elevation rasters the obstacle generator reads
// (tools/airport_obstacles.cjs): classic TIFF, one band of 32-bit float or 16-bit
// integer samples, stripped or tiled, uncompressed or deflate (8, 32946), with
// predictor 1, 2 (integer) or 3 (floating point, libtiff fpAcc). No dependency
// beyond node's zlib.
//
// readGeoTiff(buffer) -> {width, height, nodata, originX, originY, dx, dy, data}
// where data is a Float32Array in row order (top row first) and pixel (i, j)
// has its north-west corner at (originX + i dx, originY + j dy), dy negative
// for a north-up raster (ModelPixelScale + ModelTiepoint, or ModelTransformation).

'use strict';
const zlib = require('zlib');

function readGeoTiff(b) {
  const le = b.toString('ascii', 0, 2) === 'II';
  if (!le && b.toString('ascii', 0, 2) !== 'MM') throw new Error('not a TIFF');
  const r16 = (o) => (le ? b.readUInt16LE(o) : b.readUInt16BE(o));
  const r32 = (o) => (le ? b.readUInt32LE(o) : b.readUInt32BE(o));
  const rf64 = (o) => (le ? b.readDoubleLE(o) : b.readDoubleBE(o));
  if (r16(2) !== 42) throw new Error('not a classic TIFF');
  const ifd = r32(4);
  const n = r16(ifd);
  const tags = new Map();
  const SIZE = { 1: 1, 2: 1, 3: 2, 4: 4, 5: 8, 12: 8, 16: 8 };
  for (let i = 0; i < n; i++) {
    const o = ifd + 2 + i * 12;
    const tag = r16(o), type = r16(o + 2), count = r32(o + 4);
    const bytes = SIZE[type] * count;
    const at = bytes <= 4 ? o + 8 : r32(o + 8);
    const vals = [];
    for (let k = 0; k < count; k++) {
      if (type === 3) vals.push(r16(at + 2 * k));
      else if (type === 4) vals.push(r32(at + 4 * k));
      else if (type === 12) vals.push(rf64(at + 8 * k));
      else if (type === 1) vals.push(b[at + k]);
    }
    tags.set(tag, type === 2 ? b.toString('ascii', at, at + count - 1) : vals);
  }
  const one = (t, d) => (tags.has(t) ? tags.get(t)[0] : d);
  const width = one(256), height = one(257), bits = one(258, 8), comp = one(259, 1);
  const predictor = one(317, 1), fmt = one(339, 1), spp = one(277, 1);
  if (spp !== 1) throw new Error('one sample per pixel expected');
  if (!(comp === 1 || comp === 8 || comp === 32946)) throw new Error('compression ' + comp);
  const bps = bits / 8;
  const data = new Float32Array(width * height);
  const tiled = tags.has(322);
  const tw = tiled ? one(322) : width;
  const th = tiled ? one(323) : one(278, height);
  const offsets = tiled ? tags.get(324) : tags.get(273);
  const counts = tiled ? tags.get(325) : tags.get(279);
  const tilesAcross = Math.ceil(width / tw);
  for (let t = 0; t < offsets.length; t++) {
    let raw = b.subarray(offsets[t], offsets[t] + counts[t]);
    if (comp !== 1) raw = zlib.inflateSync(raw);
    const tx = (t % tilesAcross) * tw, ty = Math.floor(t / tilesAcross) * th;
    // A strip holds `th` rows of full width; a tile holds `th` rows of `tw` columns.
    const rows = Math.min(th, height - ty), rowBytes = tw * bps;
    for (let y = 0; y < rows; y++) {
      const row = Buffer.from(raw.subarray(y * rowBytes, (y + 1) * rowBytes));
      let values;
      if (predictor === 3) {
        for (let k = 1; k < rowBytes; k++) row[k] = (row[k] + row[k - 1]) & 255;
        values = new Float32Array(tw);
        const view = Buffer.alloc(4);
        for (let c = 0; c < tw; c++) {
          view[3] = row[c]; view[2] = row[tw + c]; view[1] = row[2 * tw + c]; view[0] = row[3 * tw + c];
          values[c] = view.readFloatLE(0);
        }
      } else {
        values = new Float32Array(tw);
        let acc = 0;
        for (let c = 0; c < tw; c++) {
          let v;
          if (fmt === 3 && bps === 4) v = le ? row.readFloatLE(4 * c) : row.readFloatBE(4 * c);
          else if (bps === 2) v = fmt === 2 ? (le ? row.readInt16LE(2 * c) : row.readInt16BE(2 * c)) : le ? row.readUInt16LE(2 * c) : row.readUInt16BE(2 * c);
          else throw new Error('sample format ' + fmt + '/' + bits);
          if (predictor === 2) { acc += v; v = acc; }
          values[c] = v;
        }
      }
      const yy = ty + y;
      for (let c = 0; c < tw && tx + c < width; c++) data[yy * width + tx + c] = values[c];
    }
  }
  const scale = tags.get(33550), tie = tags.get(33922), mt = tags.get(34264);
  let originX, originY, dx, dy;
  if (mt) { dx = mt[0]; dy = mt[5]; originX = mt[3]; originY = mt[7]; }
  else { dx = scale[0]; dy = -scale[1]; originX = tie[3] - tie[0] * dx; originY = tie[4] - tie[1] * dy; }
  const nd = tags.get(42113);
  return { width, height, nodata: nd === undefined ? NaN : parseFloat(nd), originX, originY, dx, dy, data };
}

module.exports = { readGeoTiff };
