// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

'use strict';

const {test} = require('node:test');
const assert = require('node:assert/strict');
const g = require('./airport_obstacles.cjs');

// A decoded Terrarium tile of w x h pixels, every pixel at elevation z except one at z1.
function tile(z, w = 4, h = 4, z1 = z) {
  const bpp = 3;
  const data = Buffer.alloc(w * h * bpp);
  for (let i = 0; i < w * h; i++) {
    const v = (i === w * h - 1 ? z1 : z) + 32768;
    data[i * bpp] = Math.floor(v / 256);
    data[i * bpp + 1] = Math.floor(v % 256);
    data[i * bpp + 2] = Math.round((v % 1) * 256);
  }
  return {width: w, height: h, bpp, data};
}

test('a tile is charged with the worst of its listed fine sources', () => {
  assert.equal(g.classOf(['srtm', 'ned13']).key, 'srtm');
  assert.equal(g.classOf(['eudem', 'srtm', 'gmted']).key, 'eudem');
  // The coarse fill counts only when nothing finer is listed.
  assert.equal(g.classOf(['gmted', 'uk_lidar']).key, 'uk_lidar');
  assert.equal(g.classOf(['gmted', 'etopo1']).u, 100);
  assert.throws(() => g.classOf(['unknown']), /unknown terrain source class/);
});

test('every pixel decides land, and bathymetry is not land', () => {
  // A single low-lying pixel (a polder at -3 m) in a sea-level SRTM tile is land.
  assert.equal(g.isLand(tile(0, 4, 4, -3), ['srtm']), true);
  assert.equal(g.isLand(tile(0), ['srtm']), false);
  // Coarse-only tiles: ETOPO1 bathymetry (negative) is water, a pixel above sea level land.
  assert.equal(g.isLand(tile(-40), ['gmted', 'etopo1']), false);
  assert.equal(g.isLand(tile(-40, 4, 4, 2), ['gmted', 'etopo1']), true);
  // ETOPO1 alone is never land.
  assert.equal(g.isLand(tile(5), ['etopo1']), false);
});

test('every tile survey carries the completeness allowance and the one mobile figure', () => {
  for (const [key, sources] of [['a', ['srtm']], ['b', ['ned13']], ['c', ['eudem', 'gmted']]]) {
    g.tileSources.set(key, sources);
    g.tileLand.set(key, true);
    const u = g.terrainUncertainty([key], 2.0);
    assert.equal(u.completeness, g.COMPLETENESS_TILES_M, sources.join('+'));
    assert.equal(u.bias, g.MOBILE_OBJECT_M + 2.0);
    assert.equal(u.symmetric, g.classOf(sources).u + 0.1);
  }
  assert.ok(Math.abs(g.MOBILE_OBJECT_M - 7.0104) < 1e-9, '23 ft, 14 CFR 77.9(c)');
  // The lidar first-return surface fails closed too.
  const ea = g.eaSurfaceUncertainty('EA DSM');
  assert.equal(ea.completeness, g.COMPLETENESS_TILES_M);
  assert.equal(ea.bias, g.MOBILE_OBJECT_M);
});

test('the sampling floor scales with the pixel and never undercuts the step statistic', () => {
  // At EGLL (51.47 deg) the 23.8 m pixel gives the 99th-percentile figure itself.
  assert.ok(Math.abs(g.samplingTerm(0, 51.47) - 4.7) < 0.05);
  // A pixel twice as large (the equator: 38.2 m) gives a proportionally larger floor.
  assert.ok(Math.abs(g.samplingTerm(0, 0) - (4.7 * 38.22) / 23.8) < 0.05);
  assert.equal(g.samplingTerm(12, 51.47), 12);
});

test('OSM heights: a height tag is the top, storeys at 4.39 m plus a roof storey', () => {
  assert.equal(g.osmHeightM({height: '45'}), 45);
  assert.ok(Math.abs(g.osmHeightM({height: '100 ft'}) - 30.48) < 1e-9);
  assert.equal(g.osmHeightM({'building:height': '20 m'}), 20);
  assert.ok(Math.abs(g.osmHeightM({'building:levels': '10'}) - 11 * 4.39) < 1e-9);
  assert.ok(Math.abs(g.osmHeightM({'building:levels': '10', 'roof:height': '2'}) - (10 * 4.39 + 2)) < 1e-9);
  assert.ok(Math.abs(g.osmHeightM({'building:levels': '4', 'roof:levels': '1'}) - 5 * 4.39) < 1e-9);
  assert.equal(g.osmHeightM({building: 'yes'}), null);
});

test('an OSM footprint is placed at its vertex nearest the track', () => {
  // A building whose outline lies right of the track: the nearest vertex sets y and the base.
  const right = g.osmFootprint([[1000, 80], [1050, 40], [1100, 120]]);
  assert.deepEqual([right.sMin, right.sMax, right.y, right.near], [1000, 1100, 40, 1]);
  // An outline straddling the track is on it.
  const across = g.osmFootprint([[500, -30], [520, 25]]);
  assert.equal(across.y, 0);
  // Left of the track, the nearest is the smallest magnitude, kept negative.
  assert.equal(g.osmFootprint([[0, -90], [10, -60]]).y, -60);
});

test('LEMD 18R is declared not available for take-off', () => {
  assert.match(g.TAKEOFF_NOT_AVAILABLE.LEMD['18R'], /TORA, TODA and ASDA "NU"/);
});
