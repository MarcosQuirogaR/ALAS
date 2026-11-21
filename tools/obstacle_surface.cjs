// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez
//
// Surface-model rasters for tools/airport_obstacles.cjs: the highest surface
// elevation inside every cell of a runway-direction corridor, from
//   * the Environment Agency (England) LIDAR Composite, 1 m, through its WCS
//     (first-return DSM = trees and buildings included; last-return DTM = bare
//     earth), Open Government Licence, (c) Environment Agency;
//   * the Copernicus DEM GLO-30 (30 m DSM, EGM2008, COG tiles on AWS open data).
// The cell footprint is sampled at `STEP_M` over the full cell, padded by
// `PAD_M` on every side (the position of the rasters relative to the terrain
// tiles is known to a few metres: the EA server reprojects between WGS84 and
// OSGB36 on its own).

'use strict';
const fs = require('fs');
const path = require('path');
const https = require('https');
const { readGeoTiff } = require('./geotiff.cjs');

const UA = 'ALAS-obstacle-builder/1.5 (+https://alas.uvigo.es)';
const STEP_M = 2; // 30 m GLO-30 pixels; the 1 m EA rasters use 1 m (see cellMaxima's step)
const PAD_M = 3;
const EA = {
  dsm: {
    url: 'https://environment.data.gov.uk/spatialdata/lidar-composite-digital-surface-model-first-return-dsm-1m/wcs',
    id: 'df4e3ec3-315e-48aa-aaaf-b5ae74d7b2bb__Lidar_Composite_Elevation_FZ_DSM_1m',
  },
  dtm: {
    url: 'https://environment.data.gov.uk/spatialdata/lidar-composite-digital-terrain-model-dtm-1m/wcs',
    id: '13787b9a-26a4-4775-8523-806d13af58fc__Lidar_Composite_Elevation_DTM_1m',
  },
};
const CHUNK_LAT = 0.01;
const CHUNK_LON = 0.015;
const MARGIN_DEG = 0.0004;

function fetchBuffer(url, tries = 5) {
  return new Promise((resolve, reject) => {
    const attempt = (n) => {
      https
        .get(url, { headers: { 'User-Agent': UA, Accept: '*/*', 'Accept-Encoding': 'identity' } }, (res) => {
          if (res.statusCode !== 200) {
            res.resume();
            if (n > 1) return setTimeout(() => attempt(n - 1), 4000);
            return reject(new Error(url.slice(0, 120) + ' -> ' + res.statusCode));
          }
          const chunks = [];
          res.on('data', (c) => chunks.push(c));
          res.on('end', () => resolve(Buffer.concat(chunks)));
          res.on('error', (e) => (n > 1 ? setTimeout(() => attempt(n - 1), 4000) : reject(e)));
        })
        .on('error', (e) => (n > 1 ? setTimeout(() => attempt(n - 1), 4000) : reject(e)));
    };
    attempt(tries);
  });
}

// A sampler over rasters held in memory: (lat, lon) -> elevation or NaN.
function rasterSampler(lookup) {
  return (lat, lon) => {
    const r = lookup(lat, lon);
    if (!r) return NaN;
    const i = Math.floor((lon - r.originX) / r.dx), j = Math.floor((lat - r.originY) / r.dy);
    if (i < 0 || j < 0 || i >= r.width || j >= r.height) return NaN;
    const v = r.data[j * r.width + i];
    return v > -1e30 && Number.isFinite(v) && v !== r.nodata ? v : NaN;
  };
}

// EA composite chunks covering `points` ([lat, lon]), downloaded to cacheDir.
async function eaSampler(kind, points, cacheDir) {
  const keys = new Set(points.map(([la, lo]) => Math.floor(la / CHUNK_LAT) + ':' + Math.floor(lo / CHUNK_LON)));
  const rasters = new Map();
  for (const key of keys) {
    const [a, o] = key.split(':').map(Number);
    const file = path.join(cacheDir, `ea_${kind}_${key.replace(':', '_')}.tif`);
    if (!fs.existsSync(file)) {
      const s = EA[kind];
      const lat0 = a * CHUNK_LAT - MARGIN_DEG, lat1 = (a + 1) * CHUNK_LAT + MARGIN_DEG;
      const lon0 = o * CHUNK_LON - MARGIN_DEG, lon1 = (o + 1) * CHUNK_LON + MARGIN_DEG;
      const crs = 'http://www.opengis.net/def/crs/EPSG/0/4326';
      const url = `${s.url}?service=WCS&version=2.0.1&request=GetCoverage&coverageId=${s.id}&format=image/tiff` +
        `&subset=Lat(${lat0.toFixed(5)},${lat1.toFixed(5)})&subset=Long(${lon0.toFixed(5)},${lon1.toFixed(5)})` +
        `&subsettingCrs=${crs}&outputCrs=${crs}`;
      fs.writeFileSync(file, await fetchBuffer(url));
    }
    rasters.set(key, readGeoTiff(fs.readFileSync(file)));
  }
  return rasterSampler((la, lo) => rasters.get(Math.floor(la / CHUNK_LAT) + ':' + Math.floor(lo / CHUNK_LON)));
}

// Copernicus DEM GLO-30 tiles covering `points`.
async function gloSampler(points, cacheDir) {
  const names = new Set(points.map(([la, lo]) => tileName(Math.floor(la), Math.floor(lo))));
  const rasters = new Map();
  for (const name of names) {
    const file = path.join(cacheDir, `glo30_${name}.tif`);
    if (!fs.existsSync(file)) {
      const url = `https://copernicus-dem-30m.s3.amazonaws.com/Copernicus_DSM_COG_10_${name}_00_DEM/Copernicus_DSM_COG_10_${name}_00_DEM.tif`;
      try { fs.writeFileSync(file, await fetchBuffer(url, 3)); } catch (e) { fs.writeFileSync(file, ''); }
    }
    const buf = fs.readFileSync(file);
    rasters.set(name, buf.length ? readGeoTiff(buf) : null);
  }
  return rasterSampler((la, lo) => rasters.get(tileName(Math.floor(la), Math.floor(lo))));
}
function tileName(latDeg, lonDeg) {
  const ns = latDeg >= 0 ? 'N' : 'S', ew = lonDeg >= 0 ? 'E' : 'W';
  return `${ns}${String(Math.abs(latDeg)).padStart(2, '0')}_00_${ew}${String(Math.abs(lonDeg)).padStart(3, '0')}`;
}

// Highest surface in every cell of the corridor grid (rows: along-track cells from
// s0, columns: lateral centres), NaN where the raster has no valid sample.
// `place(s, y)` -> [lat, lon]; the cells are ds long and dy wide.
function cellMaxima(sample, place, s0, rows, lateral, ds, dy, step = STEP_M) {
  const grid = [];
  const half = dy / 2;
  for (let i = 0; i < rows; i++) {
    const row = [];
    for (const yc of lateral) {
      let best = -Infinity;
      for (let s = -PAD_M; s <= ds + PAD_M; s += step)
        for (let y = -half - PAD_M; y <= half + PAD_M; y += step) {
          const [la, lo] = place(s0 + i * ds + s, yc + y);
          const z = sample(la, lo);
          if (z > best) best = z;
        }
      row.push(best === -Infinity ? NaN : best);
    }
    grid.push(row);
  }
  return grid;
}

// Points of the corridor at a coarse spacing: the chunks to download.
function corridorPoints(place, s0, rows, ds, lateral) {
  const pts = [];
  const lo = Math.min(...lateral) - 40, hi = Math.max(...lateral) + 40;
  for (let i = 0; i <= rows; i++)
    for (const y of [lo, (lo + hi) / 2, hi]) {
      pts.push(place(s0 + i * ds, y));
      pts.push(place(s0 + i * ds + ds / 2, y));
    }
  return pts;
}

module.exports = { eaSampler, gloSampler, cellMaxima, corridorPoints, PAD_M, STEP_M };
