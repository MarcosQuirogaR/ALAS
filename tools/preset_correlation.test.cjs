// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

const {test} = require('node:test');
const assert = require('node:assert/strict');
const {median, escape, makeRow} = require('./preset_correlation.cjs');

test('median uses both middle observations without mutating inputs', () => {
  const values = [40, 1, 2, 10];
  assert.equal(median(values), 6);
  assert.deepEqual(values, [40, 1, 2, 10]);
  assert.equal(median([]), null);
  assert.equal(median([8, 2, 4]), 4);
});
test('missing model output is not counted as zero or agreement', () => {
  const row = makeRow('ATR72-600', 'cabin_payload', 'cargo_volume_total_m3', null, {value:8.58});
  assert.equal(row.status, 'unsupported');
  assert.equal(row.error_pct, null);
  assert.equal(row.difference_pct, null);
});
test('condition mismatch remains visible but is excluded from score', () => {
  const row = makeRow('A320-200', 'aerodynamics', 'cl_alpha_per_rad', 7.29, {value:4.87});
  assert.equal(row.status, 'diagnostic');
  assert.ok(row.difference_pct > 49);
  assert.equal(row.error_pct, null);
});
test('zero references do not generate infinite percentage errors', () => {
  assert.equal(makeRow('ATR72-600', 'cabin_payload', 'uld_positions', 0, {value:0}).error_pct, null);
});
test('source text cannot inject HTML into report', () => {
  assert.equal(escape('<b>"A&B"</b>'), '&lt;b&gt;&quot;A&amp;B&quot;&lt;/b&gt;');
});
