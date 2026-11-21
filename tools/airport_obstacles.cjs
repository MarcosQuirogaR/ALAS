#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez
//
// Builds crates/alas-config/data/airport_obstacles.json, the departure
// obstacle environment of the curated aerodromes (CAT.POL.A.210 / CS 25.115).
//
// Inputs (public, downloaded by this script or given by path):
//   * crates/alas-config/data/airports.json        aerodrome table (TODA, ARP)
//   * OurAirports runways.csv                       runway ends (public domain)
//   * Terrarium elevation tiles (Mapzen composite of SRTM/NED/..., zoom 12,
//     about 38 m per pixel at the equator), AWS open data
//   * FAA Digital Obstacle File DOF.CSV (US aerodromes), path in $DOF_CSV
//   * OpenStreetMap tall structures through the Overpass API (ODbL)
//
// Usage: node tools/airport_obstacles.cjs <runways.csv> [DOF.CSV] [cache-dir]
// Environment: OSM=1 queries Overpass; SURFACE=1 reads the Environment Agency 1 m
// first-return DSM for England (and validates the tiles against it with the Copernicus
// GLO-30 DSM; see tools/obstacle_surface.cjs); ONLY=EGLL,... restricts the aerodromes;
// OUT=path writes elsewhere; OSM_FROM=json reuses the obstacles of an earlier run.
// Unit tests: node --test tools/airport_obstacles.test.cjs
//
// One survey for a hand-entered aerodrome (terrain tiles only; set OSM=1 for
// OpenStreetMap structures), printed as the JSON of one `departure_surveys`
// entry to paste into the airport's JSON record:
//   node tools/airport_obstacles.cjs --custom <lat> <lon> <heading_deg> <toda_m> <elevation_m> <runway> [cache-dir]
// (lat/lon: the start of the take-off roll; heading: true heading of the take-off.)
//
// Frame: for each runway direction, s is the distance along the runway
// heading from the start of the take-off roll (threshold, displaced thresholds
// ignored), y the lateral offset positive to the right, elevations metres
// above mean sea level. The terrain grid holds, per cell, the highest ground
// elevation of the cell (ds along track by dy across); discrete obstacles are
// listed separately.

'use strict';
const fs = require('fs');
const path = require('path');
const zlib = require('zlib');
const https = require('https');
const surface = require('./obstacle_surface.cjs');

const ROOT = path.resolve(__dirname, '..');
const AIRPORTS = require(path.join(ROOT, 'crates/alas-config/data/airports.json')).airports;
const OUT = process.env.OUT || path.join(ROOT, 'crates/alas-config/data/airport_obstacles.json');

const DS = 250; // along-track cell, m
const DY = 30; // lateral cell width, m (under the smallest accountability half-width, 60 + b/2)
const LATERAL = Array.from({ length: 41 }, (_, i) => -600 + i * DY); // cell centres, m
const REACH = 40000; // beyond the end of TODA, m
const ZOOM = 12;
const FT = 0.3048;

// Uncertainty of terrain-minus-reference differences, per source of the Terrarium
// tiles at zoom 12. The sources of every tile are read from its X-Imagery-Sources
// response header (joerd docs/data-sources.md, "Source headers"), not assumed per
// aerodrome. The header lists the files that overlap the tile, not the pixels
// each supplied: joerd's composite.py paints the layers in order of increasing
// detail and a later layer overwrites an earlier one only where it has valid
// data, so a pixel comes from the finest source that has data at it. The pixel
// provenance is not in the tiles; a tile is therefore charged with the WORST of
// its listed non-coarse land sources (the coarse GMTED2010 and ETOPO1 only fill
// where nothing finer exists, which for a land corridor of a listed finer source is
// the void case and is reported in the source note, not charged). Every
// figure is on one confidence level, LE90 (vertical error, 90 %); an RMSE is
// converted with 1.645 (a normal error):
//  - srtm: USGS "closer to +-10 m" (LE90; mission specification 16 m absolute;
//    DLR VE90 6 m relative, whose sqrt(2) = 8.5 m is below 10 m): 10 m. A surface model.
//  - ned, ned13 (USGS 3DEP bare-earth): RMSE 1.55 m for the 2013 accuracy
//    assessment (the tiles were built in 2015-17: USGS FAQ 2013 figure; the 0.82 m
//    of 2022 is a later product); LE90 = 1.645 * 1.55; a difference of two
//    heights: sqrt(2) * that. BARE EARTH (DTM).
//  - uk_lidar: Environment Agency LIDAR Composite DTM 2 m (published 2015;
//    tiles built from LIDAR-DTM-2M-*.tif, last modified 2017): individual surveys
//    +-0.15 m RMSE (data.gov.uk metadata of the composite: "All individual LIDAR
//    surveys going into the production of the composite had a vertical accuracy
//    of +/-15cm RMSE"), LE90 = 1.645 * 0.15; sqrt(2) for a difference. BARE EARTH.
//  - eudem (EU-DEM v1.1, 25 m): vertical accuracy +-7 m RMSE (EEA SDI catalogue
//    record of v1.1, "vertical accuracy: +/- 7 meters RMSE"): LE90 = 1.645 * 7 = 11.5 m.
//    A first-surface model (DSM).
//  - austria (data.gv.at DGM), mx_lidar (INEGI), pgdc_5m: accuracy not characterised
//    on a page read for this table: the SRTM class (10 m) is assumed. Austria and
//    INEGI LiDAR terrain are bare earth.
//  - gmted, etopo1 alone (no finer source in the tile): not usable for an
//    obstacle margin, U = 100 m (never certifies).
// Components of a survey's required margin (all in metres):
//  - SYMMETRIC (vertical_uncertainty_m): the source term above, plus the 0.1 m
//    quantisation of the tile values, plus (in the engine) a datum disagreement;
//  - ONE-SIDED (one_sided_bias_m, it can only make the true height larger, so it
//    widens only the "clears" test): the mobile-object allowance of every
//    source (vehicles and trains are in no terrain model) and the sampling term;
//  - COMPLETENESS (completeness_allowance_m, one-sided): every source that is not a
//    lidar first-return surface, i.e. every Terrarium tile class (see below).
const QUANTISATION_M = 0.1;
const LE90_PER_RMSE = 1.645;
const SOURCE_CLASSES = {
  srtm: { name: 'SRTM 1 arc-second', u: 10, bare: false },
  ned13: { name: 'USGS 3DEP/NED 1/3 arc-second', u: Math.SQRT2 * LE90_PER_RMSE * 1.55, bare: true },
  ned: { name: 'USGS 3DEP/NED 1/9 arc-second', u: Math.SQRT2 * LE90_PER_RMSE * 1.55, bare: true },
  uk_lidar: { name: 'LIDAR Composite DTM 2 m, © Environment Agency 2015', u: Math.SQRT2 * LE90_PER_RMSE * 0.15, bare: true },
  austria: { name: 'Austrian DGM (data.gv.at), SRTM class assumed', u: 10, bare: true },
  mx_lidar: { name: 'INEGI LiDAR terrain, SRTM class assumed', u: 10, bare: true },
  eudem: { name: 'EU-DEM v1.1 (Copernicus)', u: LE90_PER_RMSE * 7, bare: false },
  pgdc_5m: { name: 'Polar Geospatial Center 5 m, SRTM class assumed', u: 10, bare: false },
  gmted: { name: 'GMTED2010', u: 100, bare: false, coarse: true },
  etopo1: { name: 'ETOPO1', u: 100, bare: false, coarse: true },
};
// Mobile objects on a traverse way the terrain data omit (one-sided, every source): one
// physical figure everywhere, the larger sourced one, 14 CFR 77.9(c) (eCFR Title 14 Part 77:
// a railroad 23 ft = 7.01 m, the largest of 17 ft interstate, 15 ft public road, 23 ft
// railroad). The CS-ADR-DSN 5.4 m railway figure is an approach light-plane rule for
// physically similar trains and catenary and is not used (audit G3e).
const MOBILE_OBJECT_M = 23 * FT;
// Completeness of the Terrarium tiles. Bare earth (3DEP, EA DTM, Austrian and Mexican
// LiDAR terrain) omits every tree and building without a height tag; the radar surface
// models (SRTM C-band, EU-DEM, and the Copernicus GLO-30 X-band used to validate them) read
// the highest object of a cell low. Both were measured at EGLL against the EA LIDAR
// Composite first-return DSM 1 m, cell maxima over the 6,560 cells (250 m x 30 m) of each
// direction (surface-EGLL-09L/27R.json of the generator's cache):
//   DSM less DTM (bare earth):  09L P50 15.0, P95 35.2, P99 73.5, max 176.7 m;
//                               27R P50 14.5, P95 25.4, P99 30.8, max 36.7 m;
//   DSM less GLO-30 (radar):    09L P50 11.3, P95 30.3, P99 69.1, max 167.8 m;
//                               27R P50 12.3, P95 21.0, P99 25.8, max 37.0 m.
// A corridor minimum is decided at one cell, and the governing cell is a local peak of
// the surface (a building, a mast, a tree line), where the under-read is in the tail; a
// per-cell statistic below the largest therefore does not bound the corridor minimum,
// and the largest (37 to 177 m) exceeds every margin computed. The same rule as for
// bare earth applies to every tile class: the allowance is the 100 m "never certifies"
// figure the coarse tiles use, so such a survey can be violated or undetermined, never
// clear. The lidar first-return surface (EA DSM, England) carries it too (see
// eaSurfaceUncertainty).
const COMPLETENESS_TILES_M = 100;
// Sampling validation at EGLL (tools/obstacle_surface.cjs against the EA LIDAR Composite
// 1 m DTM): the 24 m tile pixel (cos(51.5 deg) * 38.2 m) under-reads the footprint
// maximum by 4.69 m (09L) and 4.52 m (27R) at the 99th percentile (2.99 and 2.57 m at the
// 95th). The governing cell is a local peak, where the attenuation is largest, so the
// 99th percentile is taken (audit G3e M2) and scaled with the ground size of a pixel. It
// is one flat site: the term cannot decide a verdict while every tile survey carries
// COMPLETENESS_TILES_M (it widens only the "clears" test, which the completeness
// allowance already fails), so a rough-site validation would change no verdict.
const SAMPLING_P99_EGLL_M = 4.7;
const EGLL_PIXEL_M = 23.8;
// Take-off directions the AIP declares not available (TORA/TODA/ASDA "NU"): not surveyed,
// recorded in the output with their source. Only directions read on an AIP page are listed.
const TAKEOFF_NOT_AVAILABLE = {
  LEMD: {
    '18R': 'AIP Spain (ENAIRE) LE_AD_2_LEMD_AOC_3_en, declared distances: 18R TORA, TODA and ASDA "NU", LDA 3365 m; 36L TORA 4179, TODA 4609, ASDA 4179 m, LDA "NU"',
  },
};
// Environment Agency LIDAR Composite first-return DSM 1 m: a surface model, trees and
// buildings in the data. Individual surveys +-0.15 m RMSE; LE90 = 1.645 * 0.15 and a
// difference of two heights sqrt(2) times that. The cell maximum is read from the 1 m
// raster itself (no sampling term); mobile objects remain a one-sided allowance.
// The +-0.15 m RMSE is the figure of the surveys over open ground. Over canopy a first
// return reads a crown below its top, and the surveys (2016-2022) hold no object built
// since (a crane, a new building) and no growth. Neither is bounded by a source ALAS is
// licensed to use, so the DSM also carries the completeness allowance (fail closed, owner
// decision on audit G3e B3): it can be violated or undetermined, never clear, until a
// licensed current obstacle survey bounds it.
function eaSurfaceUncertainty(note) {
  const source = Math.SQRT2 * LE90_PER_RMSE * 0.15;
  return {
    name: note + ' [vertical accuracy of the individual surveys +-0.15 m RMSE (EA metadata), LE90 ' + source.toFixed(2) +
      ' m for a difference, quantisation ' + QUANTISATION_M + ' m (symmetric); one-sided: mobile objects ' + MOBILE_OBJECT_M.toFixed(2) +
      ' m (14 CFR 77.9(c) railroad); completeness ' + COMPLETENESS_TILES_M + ' m (canopy and objects built since the survey not bounded); cell maxima over the full 250 m x 30 m cell padded by 3 m; Terrarium tiles only as the fallback of a cell without DSM data; survey vintage 2016-2022, objects built since are in the data only if OSM-tagged]',
    symmetric: source + QUANTISATION_M,
    bias: MOBILE_OBJECT_M,
    completeness: COMPLETENESS_TILES_M,
  };
}

// Sources of one tile: ["srtm/N39W110.tif", ...] -> a list of class keys.
const tileSources = new Map();
const tileLand = new Map();
// The class of a tile: the worst of its listed land sources, ignoring the coarse
// ones when a finer one is listed (see above).
function classOf(keys) {
  const known = keys.filter((k) => {
    if (!SOURCE_CLASSES[k]) throw new Error('unknown terrain source class ' + k);
    return true;
  });
  const fine = known.filter((k) => !SOURCE_CLASSES[k].coarse);
  const use = fine.length ? fine : known;
  let worst = null;
  for (const k of use) if (!worst || SOURCE_CLASSES[k].u > worst.u) worst = { ...SOURCE_CLASSES[k], key: k };
  return worst;
}
function terrainUncertainty(tileKeys, relief) {
  const keys = new Set();
  let worst = null;
  for (const t of tileKeys) {
    if (!tileLand.get(t)) continue;
    for (const k of tileSources.get(t) || []) keys.add(k);
    const c = classOf(tileSources.get(t) || []);
    if (!worst || c.u > worst.u) worst = c;
  }
  const mobile = MOBILE_OBJECT_M;
  const symmetric = worst.u + QUANTISATION_M;
  const bias = mobile + relief;
  const name = worst.name + ' [tile sources ' + [...keys].sort().join('+') + '; source ' + worst.u.toFixed(2) +
    ' m LE90 (worst listed land source), quantisation ' + QUANTISATION_M + ' m (symmetric); one-sided: mobile objects ' + mobile.toFixed(2) +
    ' m (14 CFR 77.9(c) railroad), sampling ' + relief.toFixed(2) + ' m; completeness ' + COMPLETENESS_TILES_M + ' m (not a lidar first-return surface)]';
  return { name, symmetric, bias, completeness: COMPLETENESS_TILES_M };
}
// Land test of one decoded tile: any pixel off sea level (SRTM and GMTED fill water with
// exactly zero); every pixel is read, so low-lying land (a polder at -3 m, a delta at
// 0.5 m) counts. A tile with coarse sources only carries ETOPO1 bathymetry over water
// (negative), so there only a pixel above sea level is land; ETOPO1 alone is never land.
function isLand(png, sources) {
  const coarseOnly = sources.every((k) => SOURCE_CLASSES[k] && SOURCE_CLASSES[k].coarse);
  let land = false;
  for (let i = 0; i < png.width * png.height && !land; i++) {
    const z = png.data[i * png.bpp] * 256 + png.data[i * png.bpp + 1] + png.data[i * png.bpp + 2] / 256 - 32768;
    if (coarseOnly ? z > 0.05 : Math.abs(z) > 0.05) land = true;
  }
  return land && !sources.every((k) => k === 'etopo1');
}
// Sampling term of a tile corridor at latitude latDeg: the larger of the step statistic
// (relief) and the validated floor scaled with the ground size of a pixel.
function samplingTerm(relief, latDeg) {
  const pixelM = (156543.03392 * Math.cos(rad(latDeg))) / 2 ** ZOOM;
  return Math.max(relief, (SAMPLING_P99_EGLL_M * pixelM) / EGLL_PIXEL_M);
}

const customMode = process.argv[2] === '--custom';
const runwaysCsv = customMode ? null : process.argv[2];
const dofCsv = customMode ? null : process.argv[3];
const cacheDir = (customMode ? process.argv[9] : process.argv[4]) || path.join(ROOT, '.agent-cache-tiles');
if (require.main === module) fs.mkdirSync(cacheDir, { recursive: true });

function get(url, tries = 4) {
  return new Promise((resolve, reject) => {
    const attempt = (n) => {
      https
        .get(url, { headers: { 'User-Agent': 'ALAS-obstacle-builder/1.5 (+https://alas.uvigo.es)', Accept: '*/*' } }, (res) => {
          if (res.statusCode !== 200) {
            res.resume();
            if (n > 1) return setTimeout(() => attempt(n - 1), 3000);
            return reject(new Error(url + ' -> ' + res.statusCode));
          }
          const chunks = [];
          res.on('data', (c) => chunks.push(c));
          res.on('end', () => resolve(Buffer.concat(chunks)));
        })
        .on('error', (e) => (n > 1 ? setTimeout(() => attempt(n - 1), 3000) : reject(e)));
    };
    attempt(tries);
  });
}

function decodePng(buf) {
  let pos = 8;
  let width = 0, height = 0, bitDepth = 0, colorType = 0;
  const idat = [];
  while (pos < buf.length) {
    const len = buf.readUInt32BE(pos);
    const type = buf.toString('ascii', pos + 4, pos + 8);
    const data = buf.subarray(pos + 8, pos + 8 + len);
    if (type === 'IHDR') {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colorType = data[9];
    } else if (type === 'IDAT') idat.push(data);
    pos += 12 + len;
  }
  if (bitDepth !== 8 || (colorType !== 2 && colorType !== 6)) throw new Error('unsupported PNG');
  const bpp = colorType === 6 ? 4 : 3;
  const raw = zlib.inflateSync(Buffer.concat(idat));
  const stride = width * bpp;
  const out = Buffer.alloc(height * stride);
  for (let y = 0; y < height; y++) {
    const f = raw[y * (stride + 1)];
    const src = y * (stride + 1) + 1;
    for (let x = 0; x < stride; x++) {
      const a = x >= bpp ? out[y * stride + x - bpp] : 0;
      const b = y > 0 ? out[(y - 1) * stride + x] : 0;
      const c = x >= bpp && y > 0 ? out[(y - 1) * stride + x - bpp] : 0;
      let v = raw[src + x];
      if (f === 1) v += a;
      else if (f === 2) v += b;
      else if (f === 3) v += (a + b) >> 1;
      else if (f === 4) {
        const p = a + b - c;
        const pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      out[y * stride + x] = v & 255;
    }
  }
  return { width, height, bpp, data: out };
}

// The X-Imagery-Sources header of a tile (carried as user metadata by the S3 object).
function head(url, tries = 4) {
  return new Promise((resolve, reject) => {
    const attempt = (n) => {
      https
        .request(url, { method: 'HEAD', headers: { 'User-Agent': 'ALAS-obstacle-builder/1.5 (+https://alas.uvigo.es)', Accept: '*/*' } }, (res) => {
          res.resume();
          const v = res.headers['x-amz-meta-x-imagery-sources'];
          if (res.statusCode === 200 && v) return resolve(v);
          if (n > 1) return setTimeout(() => attempt(n - 1), 3000);
          reject(new Error(url + ' -> ' + res.statusCode + ' without sources'));
        })
        .on('error', (e) => (n > 1 ? setTimeout(() => attempt(n - 1), 3000) : reject(e)))
        .end();
    };
    attempt(tries);
  });
}

const tiles = new Map();
async function tile(x, y) {
  const key = `${ZOOM}_${x}_${y}`;
  if (tiles.has(key)) return tiles.get(key);
  const file = path.join(cacheDir, key + '.png');
  const srcFile = path.join(cacheDir, key + '.src');
  let buf;
  if (fs.existsSync(file)) buf = fs.readFileSync(file);
  else {
    buf = await get(`https://s3.amazonaws.com/elevation-tiles-prod/terrarium/${ZOOM}/${x}/${y}.png`);
    fs.writeFileSync(file, buf);
  }
  if (!fs.existsSync(srcFile)) fs.writeFileSync(srcFile, await head(`https://s3.amazonaws.com/elevation-tiles-prod/terrarium/${ZOOM}/${x}/${y}.png`));
  tileSources.set(key, fs.readFileSync(srcFile, 'utf8').split(',').map((e) => e.trim().split('/')[0]).filter(Boolean));
  const png = decodePng(buf);
  tileLand.set(key, isLand(png, tileSources.get(key)));
  tiles.set(key, png);
  return png;
}

// Elevation lookup, nearest pixel; tiles must be preloaded.
function lonLatToPixel(lat, lon) {
  const n = 2 ** ZOOM;
  const xt = ((lon + 180) / 360) * n;
  const latR = (lat * Math.PI) / 180;
  const yt = ((1 - Math.log(Math.tan(latR) + 1 / Math.cos(latR)) / Math.PI) / 2) * n;
  return [xt, yt];
}
function elevation(lat, lon) {
  const [xt, yt] = lonLatToPixel(lat, lon);
  const tx = Math.floor(xt), ty = Math.floor(yt);
  const png = tiles.get(`${ZOOM}_${tx}_${ty}`);
  if (!png) throw new Error('tile not loaded');
  const px = Math.min(255, Math.floor((xt - tx) * 256));
  const py = Math.min(255, Math.floor((yt - ty) * 256));
  const i = (py * 256 + px) * png.bpp;
  return png.data[i] * 256 + png.data[i + 1] + png.data[i + 2] / 256 - 32768;
}

const R_EARTH = 6371008.8;
const rad = (d) => (d * Math.PI) / 180;
function bearing(lat1, lon1, lat2, lon2) {
  const p1 = rad(lat1), p2 = rad(lat2), dl = rad(lon2 - lon1);
  const y = Math.sin(dl) * Math.cos(p2);
  const x = Math.cos(p1) * Math.sin(p2) - Math.sin(p1) * Math.cos(p2) * Math.cos(dl);
  return ((Math.atan2(y, x) * 180) / Math.PI + 360) % 360;
}
function distance(lat1, lon1, lat2, lon2) {
  const dp = rad(lat2 - lat1), dl = rad(lon2 - lon1);
  const a = Math.sin(dp / 2) ** 2 + Math.cos(rad(lat1)) * Math.cos(rad(lat2)) * Math.sin(dl / 2) ** 2;
  return 2 * R_EARTH * Math.asin(Math.sqrt(a));
}
// Point at along-track s and right offset y from the start (lat0, lon0) on heading hdg.
function place(lat0, lon0, hdg, s, y) {
  const h = rad(hdg);
  const north = s * Math.cos(h) - y * Math.sin(h);
  const east = s * Math.sin(h) + y * Math.cos(h);
  return [lat0 + (north / R_EARTH) * (180 / Math.PI), lon0 + (east / (R_EARTH * Math.cos(rad(lat0)))) * (180 / Math.PI)];
}
// Inverse of place for nearby points.
function locate(lat0, lon0, hdg, lat, lon) {
  const north = rad(lat - lat0) * R_EARTH;
  const east = rad(lon - lon0) * R_EARTH * Math.cos(rad(lat0));
  const h = rad(hdg);
  return [north * Math.cos(h) + east * Math.sin(h), -north * Math.sin(h) + east * Math.cos(h)];
}

function parseCsv(text) {
  const rows = [];
  for (const line of text.split(/\r?\n/)) {
    if (!line) continue;
    const out = [];
    let cur = '', q = false;
    for (const ch of line) {
      if (ch === '"') q = !q;
      else if (ch === ',' && !q) { out.push(cur); cur = ''; }
      else cur += ch;
    }
    out.push(cur);
    rows.push(out);
  }
  return rows;
}

function longestRunway(rows, icao) {
  const header = rows[0];
  const ix = (n) => header.indexOf(n);
  const cand = rows
    .filter((r) => r[ix('airport_ident')] === icao && r[ix('closed')] !== '1')
    .sort((a, b) => +b[ix('length_ft')] - +a[ix('length_ft')]);
  const r = cand[0];
  if (!r) return null;
  const g = (n) => r[ix(n)];
  return {
    length_m: +g('length_ft') * FT,
    le: { ident: g('le_ident'), lat: +g('le_latitude_deg'), lon: +g('le_longitude_deg'), hdg: +g('le_heading_degT'), elev_m: g('le_elevation_ft') === '' ? NaN : +g('le_elevation_ft') * FT },
    he: { ident: g('he_ident'), lat: +g('he_latitude_deg'), lon: +g('he_longitude_deg'), hdg: +g('he_heading_degT'), elev_m: g('he_elevation_ft') === '' ? NaN : +g('he_elevation_ft') * FT },
  };
}

// The corridor is queried in groups of two points (10 km), each cached, so a dense
// urban site does not exceed the server's time limit in one request.
async function overpass(lat0, lon0, hdg, s1, s2) {
  const seen = new Map();
  for (let a = s1; a <= s2; a += 10000) {
    const pts = [];
    for (let s = a; s <= Math.min(s2, a + 5000); s += 5000) {
      const [la, lo] = place(lat0, lon0, hdg, s, 0);
      pts.push(la.toFixed(5), lo.toFixed(5));
    }
    for (const e of await overpassPoints(pts)) seen.set(e.type + '/' + e.id, e);
  }
  return [...seen.values()];
}
async function overpassPoints(pts) {
  const around = `(around:650,${pts.join(',')})`;
  const q =
    `[out:json][timeout:150];(` +
    `nwr${around}["man_made"~"^(tower|mast|chimney|communications_tower|crane|silo|water_tower|flare)$"]["height"];` +
    `nwr${around}["building"]["height"];` +
    `nwr${around}["building"]["building:levels"];` +
    `nwr${around}["natural"="tree"]["height"];` +
    `nwr${around}["power"="tower"]["height"];` +
        `);out geom tags;`;
  const body = 'data=' + encodeURIComponent(q);
  const cacheFile = path.join(cacheDir, 'op_' + require('crypto').createHash('sha1').update(q).digest('hex') + '.json');
  if (fs.existsSync(cacheFile)) return JSON.parse(fs.readFileSync(cacheFile, 'utf8'));
  const mirrors = ['https://overpass-api.de/api/interpreter', 'https://overpass.private.coffee/api/interpreter', 'https://overpass.kumi.systems/api/interpreter'];
  for (let attempt = 0; attempt < 12; attempt++) {
    const url = mirrors[attempt % mirrors.length];
    try {
      const els = await new Promise((resolve, reject) => {
        const req = https.request(url, { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded', 'User-Agent': 'ALAS-obstacle-builder/1.5 (+https://alas.uvigo.es)', Accept: '*/*' } }, (res) => {
          const chunks = [];
          res.on('data', (c) => chunks.push(c));
          res.on('end', () => {
            const t = Buffer.concat(chunks).toString();
            if (res.statusCode !== 200) return reject(new Error('overpass ' + res.statusCode));
            try { resolve(JSON.parse(t).elements || []); } catch (e) { reject(e); }
          });
        });
        req.on('error', reject);
        req.setTimeout(180000, () => req.destroy(new Error('timeout')));
        req.write(body);
        req.end();
      });
      fs.writeFileSync(cacheFile, JSON.stringify(els));
      return els;
    } catch (e) {
      console.error("overpass attempt", attempt, url, String(e).slice(0, 100));
      await new Promise((r) => setTimeout(r, 5000 + attempt * 3000));
    }
  }
  throw new Error('overpass unavailable');
}
// Dense urban sites carry tens of thousands of tagged buildings. The check is
// monotone in the lateral offset and the height (a smaller offset and a greater
// height are each worse), but NOT in the distance: a net path that descends in
// its first segment is lowest at the far end. Every 100 m x 40 m bin is replaced
// by ONE obstacle with its smallest lateral offset and greatest height, spanning
// the members' distances (s_m .. s_max_m); the check reads the path at its least
// over the span and the accountable strip at its far end.
const BIN_S = 100;
const BIN_Y = 40;
function mergeObstacles(list) {
  const bins = new Map();
  for (const o of list) {
    const key = Math.floor(o.s_m / BIN_S) + ':' + Math.floor(o.y_m / BIN_Y);
    const b = bins.get(key);
    if (!b) {
      bins.set(key, { ...o, n: 1, smax: o.s_max_m ?? o.s_m, ymin: Math.abs(o.y_m), ysign: Math.sign(o.y_m) || 1, hi: o });
    } else {
      b.n += 1;
      b.s_m = Math.min(b.s_m, o.s_m);
      b.smax = Math.max(b.smax, o.s_max_m ?? o.s_m);
      const ay = Math.abs(o.y_m);
      if (ay < b.ymin) { b.ymin = ay; b.ysign = Math.sign(o.y_m) || 1; }
      if (o.top_m > b.top_m) { b.top_m = o.top_m; b.hi = o; }
    }
  }
  return [...bins.values()].map((b) => ({
    s_m: b.s_m,
    ...(b.smax > b.s_m ? { s_max_m: b.smax } : {}),
    y_m: b.ysign * b.ymin,
    top_m: b.top_m,
    kind: b.hi.kind,
    source: b.n === 1 ? b.hi.source : b.hi.source + ' (+' + (b.n - 1) + ' merged, bin ' + BIN_S + ' x ' + BIN_Y + ' m)',
  }));
}

// Vertices of an Overpass element carrying 'out geom': a node, a way's outline or
// the outlines of a relation's members.
function osmPoints(e) {
  if (e.type === 'node') return [[e.lat, e.lon]];
  const pts = [];
  for (const g of e.geometry || []) if (g) pts.push([g.lat, g.lon]);
  for (const m of e.members || []) for (const g of m.geometry || []) if (g) pts.push([g.lat, g.lon]);
  return pts;
}
// Height of the top above the ground, m. A 'height' tag is the height to the top
// (OSM wiki Key:height: "the top edge of the building (including the roof...)"). Otherwise
// the storeys of a building at a sourced storey height plus the roof, which
// building:levels does not count (OSM wiki Key:building:levels: "height includes the roof,
// while building:levels does not"). The OSM default of 3 m per level is the usual value but
// below the CTBUH Height Calculator's mean height per floor of single-use offices, 4.3943 m
// over 7,343 buildings (skyscrapercenter.com/height-calculator, page data saved with the
// download log; residential 3.4228, hotel 3.836, office & residential 3.8991 m), the
// largest of its common functions, so that is used (an unconservative storey height was
// audit G3d M-e). A roof with no roof:height or roof:levels tag gets one more storey of
// height (an ALAS assumption: the wiki states no default roof height; its example roof is
// 2 m; the calculator's figure is height over floors, so it already carries some roof).
const STOREY_M = 4.39;
const ROOF_ALLOWANCE_M = STOREY_M;
function osmHeightM(tags) {
  if (tags.height !== undefined) return parseHeightM(tags.height);
  if (tags['building:height'] !== undefined) return parseHeightM(tags['building:height']);
  const levels = parseFloat(tags['building:levels']);
  if (!(Number.isFinite(levels) && levels > 0)) return null;
  const roofH = tags['roof:height'] !== undefined ? parseHeightM(tags['roof:height']) : null;
  const roofLevels = parseFloat(tags['roof:levels']);
  const roof = roofH !== null ? roofH : Number.isFinite(roofLevels) && roofLevels > 0 ? roofLevels * STOREY_M : ROOF_ALLOWANCE_M;
  return levels * STOREY_M + roof;
}
// Footprint of an object in the runway frame from its vertices [[s, y], ...]: the span of
// distances, the lateral offset of the vertex nearest the track (zero when the outline
// straddles it) and the index of the vertex nearest the track (its ground is the base).
function osmFootprint(loc) {
  const sMin = Math.min(...loc.map((p) => p[0])), sMax = Math.max(...loc.map((p) => p[0]));
  const yLo = Math.min(...loc.map((p) => p[1])), yHi = Math.max(...loc.map((p) => p[1]));
  const y = yLo <= 0 && yHi >= 0 ? 0 : Math.abs(yLo) < Math.abs(yHi) ? yLo : yHi;
  let near = 0;
  loc.forEach((p, i) => { if (Math.abs(p[1]) < Math.abs(loc[near][1])) near = i; });
  return { sMin, sMax, y, near };
}
function parseHeightM(tag) {
  const m = /^\s*([0-9]+(?:\.[0-9]+)?)\s*(m|ft|')?/.exec(String(tag));
  if (!m) return null;
  const v = parseFloat(m[1]);
  return m[2] === 'ft' || m[2] === "'" ? v * FT : v;
}

async function main() {
  const rows = customMode ? [] : parseCsv(fs.readFileSync(runwaysCsv, 'utf8'));
  const dofRows = dofCsv ? parseCsv(fs.readFileSync(dofCsv, 'utf8')) : [];
  const out = {
    source: {
      terrain: 'Mapzen/AWS Terrain Tiles (Terrarium), zoom 12, composite of SRTM, NED, and national DEMs; highest ground elevation per cell; England (EGLL): Environment Agency LIDAR Composite first-return DSM 1 m (WCS), highest surface per cell',
      terrain_url: 'https://registry.opendata.aws/terrain-tiles/',
      runways: 'OurAirports runways.csv (public domain), longest non-closed runway; thresholds as runway ends',
      us_obstacles: 'FAA Digital Obstacle File (DAILY_DOF_CSV.ZIP), https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/DailyDOF',
      other_obstacles: 'OpenStreetMap tall structures with a height tag (Overpass API), ODbL, (c) OpenStreetMap contributors',
      frame: 's metres along the runway heading from the start of the take-off roll (threshold), y metres to the right of the extended centreline, elevations metres above mean sea level',
      limitation: 'Derived from public terrain and obstacle data, not an AIP Type A obstacle chart; straight departure along the runway heading; obstacle heights from OSM/DOF where tagged. Bare-earth tiles omit untagged trees and buildings and radar surface models (SRTM, EU-DEM) read the highest object of a cell low; the Environment Agency first-return DSM (England) holds no object built since its 2016-2022 surveys and reads canopy low. No survey here can certify a clearance (completeness_allowance_m on every direction): the verdicts are violated or undetermined until a licensed current obstacle survey is loaded.',
      license: 'This file is a Derivative Database under the Open Database License (ODbL) 1.0 because it contains OpenStreetMap data: (c) OpenStreetMap contributors, https://www.openstreetmap.org/copyright, https://opendatacommons.org/licenses/odbl/1-0/. Terrain: Mapzen/AWS Terrain Tiles with the attributions of https://github.com/tilezen/joerd/blob/master/docs/attribution.md (EU-DEM Copernicus, USGS 3DEP/SRTM/GMTED2010, Geoscience Australia, INEGI, Austrian DGM, Environment Agency UK and others). FAA DOF and OurAirports are public domain. See THIRD-PARTY-NOTICES.md.',
      generated_utc: new Date().toISOString().slice(0, 10),
      cell_along_m: DS,
      cell_lateral_m: DY,
      lateral_centres_m: LATERAL,
      reach_beyond_toda_m: REACH,
    },
    airports: [],
  };
  const dofIx = (n) => dofRows[0].indexOf(n);

  let airportList = process.env.ONLY ? AIRPORTS.filter((a) => process.env.ONLY.split(',').includes(a.icao)) : AIRPORTS;
  let customRunway = null;
  if (customMode) {
    const [lat, lon, hdg, toda, elev, ident] = process.argv.slice(3, 9);
    const ap = { icao: 'CUST', name: 'custom', toda_m: +toda, elevation_m: +elev, latitude_deg: +lat, longitude_deg: +lon };
    airportList = [ap];
    const end = place(+lat, +lon, +hdg, +toda, 0);
    customRunway = { length_m: +toda, le: { ident, lat: +lat, lon: +lon, hdg: +hdg }, he: { ident: ident + 'R', lat: end[0], lon: end[1], hdg: (+hdg + 180) % 360 } };
  }
  for (const ap of airportList) {
    const rw = customRunway || longestRunway(rows, ap.icao);
    if (!rw) { console.error('no runway for', ap.icao); continue; }
    let ends = [rw.le, rw.he];
    if (!isFinite(ends[0].lat) || !isFinite(ends[1].lat)) {
      // Fall back on the aerodrome reference point and the ident heading.
      const hd = ends[0].hdg || parseInt(ends[0].ident, 10) * 10;
      const half = rw.length_m / 2;
      const a = place(ap.latitude_deg, ap.longitude_deg, hd, -half, 0);
      const b = place(ap.latitude_deg, ap.longitude_deg, hd, half, 0);
      ends = [
        { ...ends[0], lat: a[0], lon: a[1], hdg: hd },
        { ...ends[1], lat: b[0], lon: b[1], hdg: (hd + 180) % 360 },
      ];
    }
    const dirs = [];
    const notAvailable = [];
    for (const [from, to] of [[ends[0], ends[1]], [ends[1], ends[0]]]) {
      const declared = (TAKEOFF_NOT_AVAILABLE[ap.icao] || {})[from.ident];
      if (declared) { notAvailable.push({ runway: from.ident, source: declared }); continue; }
      const hdg = bearing(from.lat, from.lon, to.lat, to.lon);
      const length = distance(from.lat, from.lon, to.lat, to.lon);
      const s0 = ap.toda_m; // the grid starts at the end of the TODA: no runway or clearway inside a cell
      const n = Math.ceil((ap.toda_m + REACH - s0) / DS);
      // Load every tile the corridor touches.
      const need = new Set();
      for (let s = s0 - DS; s <= s0 + (n + 1) * DS; s += 250)
        for (const y of [-800, -400, 0, 400, 800]) {
          const [la, lo] = place(from.lat, from.lon, hdg, s, y);
          const [xt, yt] = lonLatToPixel(la, lo);
          need.add(`${Math.floor(xt)}_${Math.floor(yt)}`);
        }
      for (const k of need) { const [x, y] = k.split('_').map(Number); await tile(x, y); }
      const steps = [];
      const grid = [];
      for (let i = 0; i < n; i++) {
        const row = [];
        for (const yc of LATERAL) {
          let best = -Infinity;
          let previous = null;
          for (let ds = 5; ds < DS; ds += 10)
            for (const dy of [-12, -6, 0, 6, 12]) {
              const [la, lo] = place(from.lat, from.lon, hdg, s0 + i * DS + ds, yc + dy);
              const z = elevation(la, lo);
              best = Math.max(best, z);
              if (dy === 0) {
                if (previous !== null) steps.push(Math.abs(z - previous));
                previous = z;
              }
            }
          row.push(Math.round(best * 10) / 10);
        }
        grid.push(row);
      }
      // Ground elevation at the end of the take-off distance (median along the
      // last 300 m of the centreline), in the datum of the terrain tiles.
      const refs = [];
      for (let s = ap.toda_m - 300; s <= ap.toda_m; s += 30) {
        const [la, lo] = place(from.lat, from.lon, hdg, s, 0);
        refs.push(elevation(la, lo));
      }
      refs.sort((a, b) => a - b);
      let reference = Math.round(refs[Math.floor(refs.length / 2)] * 10) / 10;
      // Surface models (EA first-return DSM in England, Copernicus GLO-30 elsewhere)
      // for the omitted trees and buildings of a bare-earth corridor, and the validation
      // of the terrain tiles against them (SURFACE=1).
      let surfaceNote = null;
      let eaReference = null;
      if (process.env.SURFACE === '1' && !customMode && /^EG/.test(ap.icao)) {
        const here = (ss, yy) => place(from.lat, from.lon, hdg, ss, yy);
        const pts = surface.corridorPoints(here, s0, n, DS, LATERAL);
        const statsFile = path.join(cacheDir, `surface-${ap.icao}-${from.ident}.json`);
        const glo = await surface.gloSampler(pts, cacheDir);
        const gloGrid = surface.cellMaxima(glo, here, s0, n, LATERAL, DS, DY);
        let eaDsm = null, eaDtm = null;
        {
          const dsm = await surface.eaSampler('dsm', pts, cacheDir);
          eaDsm = surface.cellMaxima(dsm, here, s0, n, LATERAL, DS, DY, 1);
          const dtm = await surface.eaSampler('dtm', pts, cacheDir);
          eaDtm = surface.cellMaxima(dtm, here, s0, n, LATERAL, DS, DY, 1);
          // The ground at the end of the take-off distance in the same datum as the DSM.
          const ground = [];
          for (let ss = ap.toda_m - 300; ss <= ap.toda_m; ss += 30) { const [la, lo] = here(ss, 0); const z = dtm(la, lo); if (Number.isFinite(z)) ground.push(z); }
          ground.sort((a, b) => a - b);
          if (ground.length) eaReference = Math.round(ground[Math.floor(ground.length / 2)] * 10) / 10;
        }
        const flat = (g) => g.flat();
        const diff = (a, b) => { const out = []; const fa = flat(a), fb = flat(b); for (let k = 0; k < fa.length; k++) if (Number.isFinite(fa[k]) && Number.isFinite(fb[k])) out.push(fa[k] - fb[k]); return out; };
        const q = (arr, p) => { const v = [...arr].sort((x, y) => x - y); return v.length ? v[Math.min(v.length - 1, Math.floor(p * v.length))] : NaN; };
        const stat = (arr) => ({ n: arr.length, p05: q(arr, 0.05), p50: q(arr, 0.5), p90: q(arr, 0.9), p95: q(arr, 0.95), p99: q(arr, 0.99), max: q(arr, 1) });
        const stats = { icao: ap.icao, runway: from.ident, gloMinusTerrarium: stat(diff(gloGrid, grid)) };
        if (eaDsm) {
          stats.dtmMinusTerrarium = stat(diff(eaDtm, grid)); // resampling term of the bare-earth tile (cell max of the 1 m DTM less the tile's cell max)
          stats.dsmMinusDtm = stat(diff(eaDsm, eaDtm)); // trees, buildings, vehicles the DTM omits
          stats.dsmMinusGlo = stat(diff(eaDsm, gloGrid)); // what a 30 m DSM under-reads
          stats.dsmMinusTerrarium = stat(diff(eaDsm, grid));
          let fallback = 0;
          for (let i = 0; i < grid.length; i++) for (let j = 0; j < grid[i].length; j++) {
            if (Number.isFinite(eaDsm[i][j])) grid[i][j] = Math.round(eaDsm[i][j] * 10) / 10; else fallback++;
          }
          stats.fallbackCells = fallback;
          surfaceNote = 'EA LIDAR Composite first-return DSM 1 m';
        }
        fs.writeFileSync(statsFile, JSON.stringify(stats, null, 1));
        console.error('SURFACE', JSON.stringify(stats));
      }
      steps.sort((p, q) => p - q);
      const relief = steps.length ? steps[Math.min(steps.length - 1, Math.floor(0.95 * steps.length))] : 0;
      if (eaReference !== null) reference = eaReference;
      // Sampling term: the cell maximum the tile reads against the footprint maximum. The
      // step statistic above is a lower bound only (measured at EGLL against the 1 m EA DTM,
      // the 24 m tile pixel under-reads the footprint maximum by 3.0 m at the 95th percentile,
      // 4.7 m at the 99th, where the step statistic gave 0.8 m); the validated floor scales
      // with the ground size of a pixel.
      const sampling = samplingTerm(relief, ap.latitude_deg);
      const uncertainty = surfaceNote
        ? eaSurfaceUncertainty(surfaceNote)
        : terrainUncertainty([...need].map((k) => `${ZOOM}_${k}`), sampling);
      // Datum comparison against the elevation of the runway end where the take-off
      // distance ends (OurAirports), not the aerodrome reference point.
      const toEnd = from === ends[0] ? ends[1] : ends[0];
      const datumElevation = isFinite(toEnd.elev_m) ? toEnd.elev_m : ap.elevation_m;
      const obstacles = [];
      // FAA DOF for US aerodromes.
      if (dofRows.length && /^(K|PH|PA|PG)/.test(ap.icao)) {
        const ilat = dofIx('LATDEC'), ilon = dofIx('LONDEC'), iamsl = dofIx('AMSL'), itype = dofIx('TYPE'), iagl = dofIx('AGL'), iqty = dofIx('QUANTITY'), ioas = dofIx('OAS');
        for (const r of dofRows.slice(1)) {
          const la = +r[ilat], lo = +r[ilon];
          if (!isFinite(la) || Math.abs(la - ap.latitude_deg) > 0.6 || Math.abs(lo - ap.longitude_deg) > 0.9) continue;
          const [s, y] = locate(from.lat, from.lon, hdg, la, lo);
          if (s < s0 || s > ap.toda_m + REACH || Math.abs(y) > 650) continue;
          obstacles.push({ s_m: Math.round(s), y_m: Math.round(y), top_m: Math.round(+r[iamsl] * FT * 10) / 10, kind: r[itype].trim().toLowerCase(), source: 'FAA DOF ' + r[ioas] });
          void iagl; void iqty;
        }
      } else if (process.env.OSM_FROM) {
        // Reuse the Overpass records of an earlier run (same runway direction).
        const old = JSON.parse(fs.readFileSync(process.env.OSM_FROM, 'utf8')).airports.find((a) => a.icao === ap.icao);
        const prev = old && old.directions.find((d) => d.runway === from.ident);
        for (const o of prev ? prev.obstacles : []) if (o.s_m >= s0) obstacles.push(o);
      } else if (process.env.OSM === '1') {
        try {
          const els = await overpass(from.lat, from.lon, hdg, s0, s0 + n * DS);
          for (const e of els) {
            const h = osmHeightM(e.tags || {});
            const pts = osmPoints(e);
            if (!pts.length || h === null || !(h > 0)) continue;
            // The object is as close to the track as its nearest vertex (or the track
            // itself when its outline straddles it) and spans its vertices' distances.
            const loc = pts.map(([la, lo]) => locate(from.lat, from.lon, hdg, la, lo));
            const { sMin, sMax, y, near } = osmFootprint(loc);
            if (sMax < s0 || sMin > ap.toda_m + REACH || Math.abs(y) > 650) continue;
            // Ground at the vertex nearest the track: the top is ground + height.
            const [xt, yt] = lonLatToPixel(pts[near][0], pts[near][1]);
            await tile(Math.floor(xt), Math.floor(yt));
            const ground = elevation(pts[near][0], pts[near][1]);
            obstacles.push({ s_m: Math.round(Math.max(sMin, s0)), ...(sMax > sMin + 1 ? { s_max_m: Math.round(sMax) } : {}), y_m: Math.round(y), top_m: Math.round((ground + h) * 10) / 10, kind: e.tags['man_made'] || e.tags['power'] || e.tags['natural'] || 'building', source: `OSM ${e.type}/${e.id}` });
          }
          await new Promise((r) => setTimeout(r, 1500));
        } catch (e) {
          console.error('overpass failed for', ap.icao, from.ident, String(e).slice(0, 120));
          throw e;
        }
      }
      dirs.push({
        runway: from.ident,
        heading_deg: Math.round(hdg * 10) / 10,
        start_lat_deg: from.lat,
        start_lon_deg: from.lon,
        runway_length_m: Math.round(length),
        reference_elevation_m: reference,
        terrain_source: uncertainty.name,
        vertical_uncertainty_m: Math.round(uncertainty.symmetric * 100) / 100,
        one_sided_bias_m: Math.round(uncertainty.bias * 100) / 100,
        ...(uncertainty.completeness ? { completeness_allowance_m: uncertainty.completeness } : {}),
        reference_minus_aerodrome_elevation_m: Math.round((reference - datumElevation) * 10) / 10,
        terrain_s0_m: s0,
        terrain_max_elevation_m: grid,
        obstacles: mergeObstacles(obstacles),
      });
      console.error(ap.icao, from.ident, 'hdg', hdg.toFixed(1), 'len', Math.round(length), 'obst', obstacles.length);
    }
    if (customMode) {
      console.log(JSON.stringify({ ...dirs[0], cell_along_m: DS, cell_lateral_m: DY, lateral_centres_m: LATERAL, provenance: out.source.terrain + '; ' + out.source.limitation + ' ' + out.source.license }));
      return;
    }
    out.airports.push({ icao: ap.icao, runway_pair: `${ends[0].ident}/${ends[1].ident}`, directions: dirs, ...(notAvailable.length ? { takeoff_not_available: notAvailable } : {}) });
  }
  if (customMode) return;
  fs.writeFileSync(OUT, JSON.stringify(out));
  console.error('written', OUT, fs.statSync(OUT).size);
}
if (require.main === module) main().catch((e) => { console.error(e); process.exit(1); });
module.exports = { SOURCE_CLASSES, classOf, isLand, samplingTerm, terrainUncertainty, eaSurfaceUncertainty, osmHeightM, osmFootprint, tileSources, tileLand, MOBILE_OBJECT_M, COMPLETENESS_TILES_M, TAKEOFF_NOT_AVAILABLE };
