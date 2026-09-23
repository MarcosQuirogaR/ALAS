// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

'use strict';

const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {run, renderHtml, compareNumeric} = require('./aircraft_parity.cjs');

function fixture({check, modelValue, localValue, writeOutputs = false}) {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'alas-aircraft-parity-'));
  const contract = {
    schema: 'aircraft-real-parity-contract/test',
    scope: {tolerance_policy: {
      small: {absolute: 0.5, relative: 0.01, unit: 'm'},
      loose_absolute: {absolute: 5, relative: 0.01, unit: 'm'},
    }},
    sources: {test_source: {title: 'Test source', authority: 'Test authority', url: 'https://example.test/source'}},
    regulatory: [],
    aircraft: {TEST: {variant: 'TEST', evidence_file: 'evidence.json', checks: [check]}},
  };
  const model = {TEST: {geometry: {x: modelValue}}};
  fs.writeFileSync(path.join(cwd, 'contract.json'), `${JSON.stringify(contract)}\n`);
  fs.writeFileSync(path.join(cwd, 'model.json'), `${JSON.stringify(model)}\n`);
  fs.writeFileSync(path.join(cwd, 'evidence.json'), `${JSON.stringify({geometry: {x: localValue}})}\n`);
  return {cwd, writeOutputs};
}

function check(overrides = {}) {
  return {
    id: 'geometry.x',
    category: 'geometry',
    model_path: 'geometry.x',
    source_path: 'geometry.x',
    source_value: 10,
    unit: 'm',
    evidence: 'primary',
    condition: 'test condition',
    cite: 'test_source',
    tolerance: 'small',
    ...overrides,
  };
}

function one(overrides = {}, modelValue = 10, localValue = 10) {
  const f = fixture({check: check(overrides), modelValue, localValue});
  const result = run({cwd: f.cwd, modelPath: 'model.json', contractPath: 'contract.json', writeOutputs: false});
  return result.rows[0];
}

test('matched primary source within tolerance is eligible', () => {
  // 0.05 m absolute error on a 10 m source is 0.5% relative: inside both the 0.5 m
  // absolute and 1% relative bounds declared by the 'small' tolerance fixture.
  const row = one({}, 10.05);
  assert.equal(row.status, 'within_tolerance');
  assert.equal(row.comparison_eligible, true);
  assert.equal(row.within_tolerance, true);
  assert.ok(Math.abs(row.absolute_error - 0.05) < 1e-12);
});

test('matched primary source beyond tolerance is out of tolerance', () => {
  const row = one({}, 11);
  assert.equal(row.status, 'out_of_tolerance');
  assert.equal(row.comparison_eligible, true);
  assert.equal(row.within_tolerance, false);
});

test('secondary evidence remains diagnostic and is excluded from score', () => {
  const row = one({evidence: 'secondary'}, 10);
  assert.equal(row.status, 'diagnostic');
  assert.equal(row.comparison_eligible, false);
  assert.equal(row.absolute_error, 0);
});

test('condition mismatch remains diagnostic even when the value matches', () => {
  const row = one({condition_match: false}, 10);
  assert.equal(row.status, 'diagnostic');
  assert.equal(row.comparison_eligible, false);
});

test('missing model quantity is unsupported rather than zero', () => {
  const row = one({model_path: 'geometry.missing'}, 10);
  assert.equal(row.status, 'unsupported');
  assert.equal(row.model_value, null);
  assert.equal(row.absolute_error, null);
});

test('explicit public evidence gap is not converted into an agreement', () => {
  const row = one({source_available: false, source_value: undefined}, 10);
  assert.equal(row.status, 'evidence_gap');
  assert.equal(row.comparison_eligible, false);
  assert.equal(row.source_value, null);
});

test('local evidence conflict is visible and never scored', () => {
  const row = one({}, 10, 11);
  assert.equal(row.status, 'source_conflict');
  assert.equal(row.source_conflict, true);
  assert.equal(row.comparison_eligible, false);
  assert.equal(row.local_evidence_value, 11);
});

test('descriptive source evidence requires a normalized export', () => {
  const row = one({source_value: undefined, source_kind: 'descriptive', source_available: true}, {x: 1});
  assert.equal(row.status, 'diagnostic');
  assert.equal(row.comparison_eligible, false);
});

test('a generous absolute tolerance cannot rescue a check that misses its relative tolerance', () => {
  // Reproduces the ATR-72 mass.oew_kg defect: a 2 m miss against a 10 m source is 20%
  // relative error (well outside a 1% relative bound) even though it sits inside a
  // deliberately loose 5 m absolute bound. Both declared bounds must hold (AND), not either.
  const row = one({tolerance: 'loose_absolute'}, 12);
  assert.equal(row.status, 'out_of_tolerance');
  assert.equal(row.within_tolerance, false);
  assert.equal(row.comparison_eligible, true);
});

test('compareNumeric requires both declared bounds to hold, not either', () => {
  const atrLikeMiss = compareNumeric(12014.6, 13450, {absolute: 2000, relative: 0.03});
  assert.equal(atrLikeMiss.within, false);
  assert.ok(atrLikeMiss.absoluteError <= 2000, 'absolute bound alone would have passed');
  assert.ok(atrLikeMiss.relativeError > 0.03, 'relative bound is the one that must fail it');

  const genuineMatch = compareNumeric(13400, 13450, {absolute: 2000, relative: 0.03});
  assert.equal(genuineMatch.within, true);
});

test('compareNumeric falls back to whichever single bound is declared', () => {
  const relativeOnly = compareNumeric(105, 100, {relative: 0.1});
  assert.equal(relativeOnly.within, true);
  const absoluteOnly = compareNumeric(100.3, 100, {absolute: 0.5});
  assert.equal(absoluteOnly.within, true);
  const absoluteOnlyFails = compareNumeric(101, 100, {relative: 0});
  assert.equal(absoluteOnlyFails.within, false);
});

test('compareNumeric treats a zero source value as absolute-only, since relative error is undefined', () => {
  const result = compareNumeric(0.02, 0, {absolute: 0.05, relative: 0.01});
  assert.equal(result.relativeError, null);
  assert.equal(result.within, true);
});

test('a source-scaled gear station carries a model-declared provenance note', () => {
  const f = fixture({
    check: check({id: 'gear.x_nlg_m', category: 'gear', model_path: 'gear.x_nlg_m', source_path: 'gear.x_nlg_m'}),
    modelValue: 10,
  });
  const model = {TEST: {
    geometry: {x: 10},
    gear: {x_nlg_m: 10, stations_source_scaled: true, main_gear_station_frame: 'nose_tip_drawing_reference'},
  }};
  fs.writeFileSync(path.join(f.cwd, 'model.json'), `${JSON.stringify(model)}\n`);
  const result = run({cwd: f.cwd, modelPath: 'model.json', contractPath: 'contract.json', writeOutputs: false});
  assert.equal(result.rows[0].status, 'within_tolerance');
  assert.match(result.rows[0].model_provenance_note, /scaled from nose_tip_drawing_reference/);
});

test('a gear topology count is not mislabeled as a scaled station', () => {
  const f = fixture({
    check: check({id: 'gear.n_nlg_wheels', category: 'gear', model_path: 'gear.n_nlg_wheels', source_path: 'gear.n_nlg_wheels'}),
    modelValue: 10,
  });
  const model = {TEST: {
    geometry: {x: 10},
    gear: {n_nlg_wheels: 10, stations_source_scaled: true, main_gear_station_frame: 'nose_tip_drawing_reference'},
  }};
  fs.writeFileSync(path.join(f.cwd, 'model.json'), `${JSON.stringify(model)}\n`);
  const result = run({cwd: f.cwd, modelPath: 'model.json', contractPath: 'contract.json', writeOutputs: false});
  assert.doesNotMatch(result.rows[0].model_provenance_note, /scaled/);
  assert.match(result.rows[0].model_provenance_note, /declared preset input/);
});

test('an independently placed gear station carries no provenance note', () => {
  const f = fixture({
    check: check({id: 'gear.x_nlg_m', category: 'gear', model_path: 'gear.x_nlg_m', source_path: 'gear.x_nlg_m'}),
    modelValue: 10,
  });
  const model = {TEST: {
    geometry: {x: 10},
    gear: {x_nlg_m: 10, stations_source_scaled: false, main_gear_station_frame: 'aircraft_nose_relative_model'},
  }};
  fs.writeFileSync(path.join(f.cwd, 'model.json'), `${JSON.stringify(model)}\n`);
  const result = run({cwd: f.cwd, modelPath: 'model.json', contractPath: 'contract.json', writeOutputs: false});
  assert.equal(result.rows[0].model_provenance_note, null);
});

test('a published-capacity fuel check carries a model-declared provenance note', () => {
  const f = fixture({
    check: check({id: 'mass.usable_fuel_kg', model_path: 'payload_range.fuel_capacity_kg', source_path: 'payload_range.fuel_capacity_kg'}),
    modelValue: 10,
  });
  const model = {TEST: {
    geometry: {x: 10},
    payload_range: {fuel_capacity_kg: 10, fuel_capacity_limit: 'published usable capacity'},
  }};
  fs.writeFileSync(path.join(f.cwd, 'model.json'), `${JSON.stringify(model)}\n`);
  const result = run({cwd: f.cwd, modelPath: 'model.json', contractPath: 'contract.json', writeOutputs: false});
  assert.match(result.rows[0].model_provenance_note, /not an independently predicted tank volume/);
  assert.deepEqual(result.summary.by_provenance, {independent: {}, reference_input: {within_tolerance: 1}});
});

test('a declared preset input is marked as data entry, not a prediction', () => {
  const f = fixture({check: check({id: 'mass.mtow_kg'}), modelValue: 10, localValue: 10});
  const result = run({cwd: f.cwd, modelPath: 'model.json', contractPath: 'contract.json', writeOutputs: false});
  assert.match(result.rows[0].model_provenance_note, /declared preset input/);
  assert.equal(result.summary.by_provenance.independent.within_tolerance, undefined);
});

test('run writes hashed JSON, CSV and escaped HTML outputs', () => {
  const f = fixture({check: check({note: '<script>alert(1)</script>'}), modelValue: 10, localValue: 10});
  const outputDir = path.join(f.cwd, 'out');
  const reportPath = path.join(f.cwd, 'report.html');
  const result = run({cwd: f.cwd, modelPath: 'model.json', contractPath: 'contract.json', outputDir, reportPath});
  assert.equal(result.summary.eligible_comparisons, 1);
  assert.equal(fs.existsSync(path.join(outputDir, 'AIRCRAFT_PARITY.json')), true);
  assert.equal(fs.existsSync(path.join(outputDir, 'AIRCRAFT_PARITY.csv')), true);
  assert.equal(fs.existsSync(reportPath), true);
  const html = fs.readFileSync(reportPath, 'utf8');
  assert.equal(html.includes('<script>alert(1)</script>'), false);
  assert.equal(html.includes('&lt;script&gt;alert(1)&lt;/script&gt;'), true);
  assert.match(renderHtml(result), /Real-aircraft parity audit/);
});
