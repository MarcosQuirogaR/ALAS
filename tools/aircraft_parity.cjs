// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

/*
 * Evidence-backed parity harness for the real-aircraft presets.
 *
 * This tool deliberately treats the contract as an independent evidence
 * bundle.  It never derives or updates a source value from MODEL.json.  A
 * secondary or condition-mismatched source remains visible as diagnostic and
 * is excluded from the comparison score.
 *
 * Run from the repository root:
 *   node tools/aircraft_parity.cjs
 *
 * The model dump is generated separately, for example with:
 *   cargo run -p alas-pipeline --example model_reference_dump
 */
'use strict';

const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

const DEFAULTS = Object.freeze({
  modelPath: 'out/validation/MODEL.json',
  contractPath: 'golden/aircraft/real_aircraft_parity.json',
  outputDir: 'out/validation',
  reportPath: 'out/reports/2026-09-09-aircraft-parity.html',
});

function finite(value) {
  return typeof value === 'number' && Number.isFinite(value);
}

function own(object, key) {
  return object !== null && typeof object === 'object' &&
    Object.prototype.hasOwnProperty.call(object, key);
}

function getPath(root, dottedPath) {
  if (!dottedPath) return undefined;
  return String(dottedPath).split('.').reduce((value, key) => {
    if (value === null || value === undefined) return undefined;
    return value[key];
  }, root);
}

function resolveFrom(cwd, filePath) {
  return path.isAbsolute(filePath) ? filePath : path.resolve(cwd, filePath);
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function sha256(filePath) {
  return crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

function jsonValue(value) {
  if (value === undefined) return null;
  if (value === null || finite(value) || typeof value === 'string' || typeof value === 'boolean') return value;
  return value;
}

function stringifyValue(value) {
  if (value === undefined || value === null) return '';
  if (typeof value === 'string') return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function escape(value) {
  return String(value ?? '').replace(/[&<>"']/g,
    character => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[character]));
}

function csvCell(value) {
  return `"${stringifyValue(value).replaceAll('"', '""')}"`;
}

function valuesEqual(left, right) {
  if (finite(left) && finite(right)) {
    const scale = Math.max(1, Math.abs(left), Math.abs(right));
    return Math.abs(left - right) <= 1e-9 * scale;
  }
  if (left === right) return true;
  if (Array.isArray(left) && Array.isArray(right)) {
    return left.length === right.length && left.every((item, index) => valuesEqual(item, right[index]));
  }
  if (left && right && typeof left === 'object' && typeof right === 'object') {
    const leftKeys = Object.keys(left).sort();
    const rightKeys = Object.keys(right).sort();
    return leftKeys.length === rightKeys.length && leftKeys.every((key, index) =>
      key === rightKeys[index] && valuesEqual(left[key], right[key]));
  }
  return false;
}

function unwrapEvidence(value) {
  if (value && typeof value === 'object' && own(value, 'value')) {
    return {value: value.value, metadata: value};
  }
  return {value, metadata: null};
}

function unitsEquivalent(left, right) {
  if (!left || !right) return true;
  const normalise = unit => String(unit).trim().toLowerCase();
  const a = normalise(left);
  const b = normalise(right);
  if (!a || !b || ['-', '—', 'unknown', 'unspecified'].includes(a) || ['-', '—', 'unknown', 'unspecified'].includes(b)) return true;
  return a === b;
}

function compareNumeric(modelValue, sourceValue, tolerance) {
  if (!tolerance) return null;
  if (finite(modelValue) && finite(sourceValue)) {
    const difference = modelValue - sourceValue;
    const absoluteError = Math.abs(difference);
    const relativeError = sourceValue === 0 ? null : absoluteError / Math.abs(sourceValue);
    // Both bounds must hold when both are declared. An OR combination lets a
    // generous absolute bound (sized for a large aircraft in the same
    // category) mask a large relative error on a smaller one: e.g. a 2000 kg
    // absolute mass tolerance swallows a 10.7% error on a 13,450 kg airframe.
    const hasAbsolute = tolerance.absolute !== undefined && tolerance.absolute !== null;
    const hasRelative = tolerance.relative !== undefined && tolerance.relative !== null;
    const withinAbsolute = !hasAbsolute || absoluteError <= Number(tolerance.absolute);
    const withinRelative = !hasRelative || relativeError === null || relativeError <= Number(tolerance.relative);
    const within = (hasAbsolute || hasRelative) && withinAbsolute && withinRelative;
    return {difference, absoluteError, relativeError, within};
  }
  if (Array.isArray(modelValue) && Array.isArray(sourceValue) && modelValue.length === sourceValue.length) {
    const leaves = [];
    const visit = (modelItem, sourceItem, indexPath) => {
      if (finite(modelItem) && finite(sourceItem)) {
        const item = compareNumeric(modelItem, sourceItem, tolerance);
        leaves.push({...item, index: indexPath.join('.')});
        return;
      }
      if (Array.isArray(modelItem) && Array.isArray(sourceItem) && modelItem.length === sourceItem.length) {
        modelItem.forEach((item, index) => visit(item, sourceItem[index], [...indexPath, index]));
      }
    };
    visit(modelValue, sourceValue, []);
    if (!leaves.length) return null;
    return {
      difference: leaves,
      absoluteError: Math.max(...leaves.map(item => item.absoluteError)),
      relativeError: Math.max(...leaves.filter(item => item.relativeError !== null).map(item => item.relativeError), 0),
      within: leaves.every(item => item.within),
      elements: leaves.length,
    };
  }
  return null;
}

function loadEvidenceFile(cwd, aircraft) {
  if (!aircraft.evidence_file) return {status: 'not_declared', data: null, path: null};
  const filePath = resolveFrom(cwd, aircraft.evidence_file);
  try {
    return {status: 'loaded', data: readJson(filePath), path: filePath};
  } catch (error) {
    return {
      status: fs.existsSync(filePath) ? 'invalid' : 'missing',
      data: null,
      path: filePath,
      error: String(error.message || error),
    };
  }
}

function sourceAvailability(check) {
  if (check.source_available === false) return false;
  if (check.source_available === true) return true;
  return check.source_value !== undefined && check.source_value !== null;
}

function sourceHasNumericAnchor(check) {
  return check.source_value !== undefined && check.source_value !== null;
}

// A gear check id counts topology (wheel/strut counts) rather than a
// longitudinal/lateral station or span derived from one; only the latter is
// affected by the model's stations_source_scaled flag.
const GEAR_STATION_ID_PATTERN = /(\.x_|_x_m|wheelbase|track_width|wheel_track)/i;

// Surfaces provenance markers the model export already declares about itself
// (never invented here) so a within-tolerance row can be read correctly: it
// may confirm data retention rather than an independent prediction.
// Quantities that crates/alas-pipeline/examples/model_reference_dump.rs
// exports straight from the preset's design vector, requirements or gear
// layout definition. A match confirms the preset was entered correctly; it is
// not an output of any ALAS analysis.
const DECLARED_INPUT_PATTERN = /^(geometry\.(span_m|fuselage_length_m|engine_spanwise_position_m)|mass\.mtow_kg|aero\.cruise_mach|gear\.(n_nlg_wheels|n_mlg_struts|wheels_per_mlg_strut|mlg_wheels_per_strut|main_wheels_total))$/;

function modelProvenanceNote(check, modelPreset) {
  if (!modelPreset) return null;
  if (DECLARED_INPUT_PATTERN.test(check.id || '')) {
    return 'Model value is a declared preset input (design vector, requirement or gear layout definition), not an analysis output; a matching result confirms data entry, not a prediction.';
  }
  if (GEAR_STATION_ID_PATTERN.test(check.id || '') && modelPreset.gear && modelPreset.gear.stations_source_scaled === true) {
    const frame = modelPreset.gear.main_gear_station_frame || 'a registered source drawing frame';
    return `Model gear station is scaled from ${frame}, not independently placed by structural/turnover sizing; a matching result confirms scaling retention, not an independent station prediction.`;
  }
  if (check.model_path && /fuel_capacity/i.test(check.model_path) && modelPreset.payload_range &&
    modelPreset.payload_range.fuel_capacity_limit === 'published usable capacity') {
    return 'Model fuel capacity is the registered published usable capacity, not an independently predicted tank volume; a matching result confirms data retention, not a fuel-volume prediction.';
  }
  return null;
}

function evaluateCheck({preset, aircraft, modelPreset, evidence, contract, check}) {
  const modelValue = modelPreset ? getPath(modelPreset, check.model_path) : undefined;
  const sourceAvailable = sourceAvailability(check);
  const sourceAnchor = jsonValue(check.source_value);
  let localEvidenceValue;
  let localEvidenceMetadata;
  let localEvidenceStatus = evidence.status;
  let sourceConflict = false;
  let sourceConflictReason = '';

  if (evidence.data && check.source_path) {
    const local = unwrapEvidence(getPath(evidence.data, check.source_path));
    if (local.value !== undefined) {
      localEvidenceValue = jsonValue(local.value);
      localEvidenceMetadata = local.metadata;
      if (sourceHasNumericAnchor(check) && !valuesEqual(local.value, check.source_value)) {
        sourceConflict = true;
        sourceConflictReason = 'Local evidence bundle value differs from the immutable contract anchor.';
      } else if (local.metadata && check.unit && local.metadata.unit && !unitsEquivalent(local.metadata.unit, check.unit)) {
        sourceConflict = true;
        sourceConflictReason = `Local evidence unit ${local.metadata.unit} differs from contract unit ${check.unit}.`;
      }
    } else {
      localEvidenceStatus = 'missing_value';
    }
  } else if (check.source_path && evidence.status !== 'loaded') {
    localEvidenceStatus = evidence.status;
  }

  const sourceInfo = contract.sources?.[check.cite] || null;
  const tolerance = check.tolerance ? contract.scope?.tolerance_policy?.[check.tolerance] : null;
  const numericComparison = sourceHasNumericAnchor(check) ? compareNumeric(modelValue, sourceAnchor, tolerance) : null;
  const modelPresent = modelValue !== undefined && modelValue !== null;
  const conditionMatch = check.condition_match !== false;
  const descriptive = check.source_kind === 'descriptive';
  const comparisonEligible = sourceAvailable && !sourceConflict && modelPresent &&
    numericComparison !== null && check.evidence === 'primary' && conditionMatch && !descriptive;

  let status;
  let reason;
  if (!sourceAvailable) {
    status = 'evidence_gap';
    reason = check.note || 'No trustworthy public numeric target is available for this quantity.';
  } else if (sourceConflict) {
    status = 'source_conflict';
    reason = sourceConflictReason;
  } else if (!modelPresent) {
    status = 'unsupported';
    reason = modelPreset ? 'The model dump does not export this quantity.' : 'The contract preset is absent from the model dump.';
  } else if (numericComparison === null) {
    status = descriptive ? 'diagnostic' : 'unsupported';
    reason = descriptive
      ? (check.note || 'Source evidence is descriptive; a normalized comparable model value is required.')
      : 'Model and source values are not comparable numeric values.';
  } else if (!comparisonEligible) {
    status = 'diagnostic';
    reason = check.condition_match === false
      ? 'Source and model conditions, variant, datum, reference area, or metric definition are not identical.'
      : check.evidence === 'primary'
        ? 'A numeric value exists, but the source evidence is descriptive or the comparison is not condition-qualified.'
        : 'Source is secondary, estimated, solver-derived, or otherwise unsuitable for a release score.';
  } else {
    status = numericComparison.within ? 'within_tolerance' : 'out_of_tolerance';
    reason = numericComparison.within ? 'Matched primary evidence is within the declared tolerance.' : 'Matched primary evidence exceeds the declared tolerance.';
  }

  return {
    preset,
    variant: aircraft.variant || '',
    id: check.id,
    model_provenance_note: modelProvenanceNote(check, modelPreset),
    category: check.category || 'uncategorized',
    model_path: check.model_path || '',
    source_path: check.source_path || '',
    status,
    reason,
    model_value: modelPresent ? jsonValue(modelValue) : null,
    source_value: sourceHasNumericAnchor(check) ? sourceAnchor : null,
    local_evidence_value: localEvidenceValue === undefined ? null : localEvidenceValue,
    unit: check.unit || tolerance?.unit || '',
    tolerance: tolerance ? {
      absolute: tolerance.absolute,
      relative: tolerance.relative,
      unit: tolerance.unit,
    } : null,
    tolerance_name: check.tolerance || '',
    difference: numericComparison ? numericComparison.difference : null,
    absolute_error: numericComparison ? numericComparison.absoluteError : null,
    relative_error: numericComparison ? numericComparison.relativeError : null,
    within_tolerance: numericComparison ? numericComparison.within : null,
    compared_elements: numericComparison?.elements || null,
    comparison_eligible: comparisonEligible,
    condition_match: conditionMatch,
    condition: check.condition || '',
    evidence: check.evidence || (sourceAvailable ? 'unspecified' : 'unavailable'),
    source_kind: check.source_kind || 'numeric',
    source_available: sourceAvailable,
    source_conflict: sourceConflict,
    source_conflict_reason: sourceConflictReason,
    local_evidence_status: localEvidenceStatus,
    source_cite: check.cite || '',
    source_title: sourceInfo?.title || '',
    source_authority: sourceInfo?.authority || '',
    source_url: sourceInfo?.url || '',
    source_note: check.note || '',
    source_metadata: localEvidenceMetadata ? {
      evidence: localEvidenceMetadata.evidence,
      condition: localEvidenceMetadata.condition,
      cite: localEvidenceMetadata.cite,
      unit: localEvidenceMetadata.unit,
    } : null,
  };
}

function countBy(rows, field) {
  return rows.reduce((counts, row) => {
    const key = row[field] || 'unknown';
    counts[key] = (counts[key] || 0) + 1;
    return counts;
  }, {});
}

function buildResult({contract, model, rows, inputs, evidenceBundles}) {
  const eligible = rows.filter(row => row.comparison_eligible);
  const statuses = countBy(rows, 'status');
  return {
    schema: 'aircraft-real-parity-results/v1',
    generated_utc: new Date().toISOString(),
    contract_schema: contract.schema || '',
    inputs,
    summary: {
      contract_presets: Object.keys(contract.aircraft || {}).length,
      model_presets: Object.keys(model || {}).length,
      rows: rows.length,
      eligible_comparisons: eligible.length,
      scored_comparisons: eligible.length,
      statuses,
      // A scored row whose model value is a registered reference input (it
      // carries a model_provenance_note) confirms data retention, not an
      // independent prediction, so the two are counted separately.
      by_provenance: {
        independent: countBy(eligible.filter(row => !row.model_provenance_note), 'status'),
        reference_input: countBy(eligible.filter(row => row.model_provenance_note), 'status'),
      },
      by_category: countBy(rows, 'category'),
      by_preset: Object.fromEntries(Object.keys(contract.aircraft || {}).map(preset => [preset, countBy(rows.filter(row => row.preset === preset), 'status')])),
      model_only_presets: Object.keys(model || {}).filter(preset => !own(contract.aircraft || {}, preset)),
    },
    rows,
    evidence_bundles: evidenceBundles,
    sources: contract.sources || {},
    regulatory: contract.regulatory || [],
    notes: [
      'This is a source-backed comparison report, not certification validation.',
      'Expected values come from the immutable parity contract and optional local evidence files; no expected value is copied from MODEL.json.',
      'Diagnostic, unsupported, evidence-gap, and source-conflict rows are retained and excluded from the release comparison score.',
    ],
  };
}

function run(options = {}) {
  const cwd = path.resolve(options.cwd || process.cwd());
  const modelPath = resolveFrom(cwd, options.modelPath || DEFAULTS.modelPath);
  const contractPath = resolveFrom(cwd, options.contractPath || DEFAULTS.contractPath);
  const contract = readJson(contractPath);
  const model = readJson(modelPath);
  if (!contract || typeof contract !== 'object' || !contract.aircraft || typeof contract.aircraft !== 'object') {
    throw new Error(`Parity contract has no aircraft map: ${contractPath}`);
  }
  if (!model || typeof model !== 'object') throw new Error(`Model dump is not an object: ${modelPath}`);

  const inputs = {
    contract_path: path.relative(cwd, contractPath) || contractPath,
    model_path: path.relative(cwd, modelPath) || modelPath,
    contract_sha256: sha256(contractPath),
    model_sha256: sha256(modelPath),
  };
  const rows = [];
  const evidenceBundles = {};
  for (const [preset, aircraft] of Object.entries(contract.aircraft)) {
    const evidence = loadEvidenceFile(cwd, aircraft);
    evidenceBundles[preset] = {
      path: aircraft.evidence_file || null,
      status: evidence.status,
      sha256: evidence.status === 'loaded' ? sha256(evidence.path) : null,
      error: evidence.error || null,
    };
    const modelPreset = model[preset];
    for (const check of aircraft.checks || []) {
      rows.push(evaluateCheck({preset, aircraft, modelPreset, evidence, contract, check}));
    }
  }
  const result = buildResult({contract, model, rows, inputs, evidenceBundles});

  if (options.writeOutputs !== false) {
    const outputDir = resolveFrom(cwd, options.outputDir || DEFAULTS.outputDir);
    fs.mkdirSync(outputDir, {recursive: true});
    const reportPath = resolveFrom(cwd, options.reportPath || DEFAULTS.reportPath);
    result.outputs = {
      json: path.relative(cwd, path.join(outputDir, 'AIRCRAFT_PARITY.json')),
      csv: path.relative(cwd, path.join(outputDir, 'AIRCRAFT_PARITY.csv')),
      report: path.relative(cwd, reportPath),
    };
    fs.writeFileSync(path.join(outputDir, 'AIRCRAFT_PARITY.json'), `${JSON.stringify(result, null, 2)}\n`);
    const fields = [
      'preset', 'variant', 'id', 'category', 'status', 'reason', 'model_path', 'source_path',
      'model_value', 'source_value', 'local_evidence_value', 'unit', 'difference',
      'absolute_error', 'relative_error', 'within_tolerance', 'comparison_eligible',
      'condition_match', 'condition', 'evidence', 'source_kind', 'source_available',
      'source_conflict', 'local_evidence_status', 'source_cite', 'source_title',
      'source_authority', 'source_url', 'model_provenance_note',
    ];
    const csv = [fields.join(','), ...rows.map(row => fields.map(field => csvCell(row[field])).join(','))].join('\n');
    fs.writeFileSync(path.join(outputDir, 'AIRCRAFT_PARITY.csv'), `${csv}\n`);
    fs.mkdirSync(path.dirname(reportPath), {recursive: true});
    fs.writeFileSync(reportPath, renderHtml(result));
  }
  return result;
}

function formatNumber(value) {
  if (!finite(value)) return '—';
  return Number(value.toPrecision(6)).toString();
}

function displayValue(value) {
  if (value === null || value === undefined) return '—';
  if (finite(value)) return formatNumber(value);
  return stringifyValue(value);
}

function renderStatusTable(summary) {
  const order = ['within_tolerance', 'out_of_tolerance', 'diagnostic', 'unsupported', 'evidence_gap', 'source_conflict'];
  return `<table class="summary"><thead><tr><th>Status</th><th>Rows</th><th>Meaning</th></tr></thead><tbody>${order.map(status => `<tr><td><span class="status ${escape(status)}">${escape(status)}</span></td><td>${summary.statuses[status] || 0}</td><td>${escape({
    within_tolerance: 'Matched primary evidence is within the declared tolerance.',
    out_of_tolerance: 'Matched primary evidence exceeds the declared tolerance.',
    diagnostic: 'Values exist but source quality or conditions exclude a release score.',
    unsupported: 'A source target exists but the model export is missing the quantity.',
    evidence_gap: 'No trustworthy public numeric target is available.',
    source_conflict: 'Local evidence disagrees with the immutable contract anchor.',
  }[status])}</td></tr>`).join('')}</tbody></table>`;
}

function renderHtml(result) {
  const summary = result.summary;
  const rows = result.rows || [];
  const sourceCites = [...new Set(rows.map(row => row.source_cite).filter(Boolean))];
  const actionItems = [
    'Export a normalized aero polar with alpha, CL, CD, Mach, Reynolds number, altitude, configuration, trim state and reference-area metadata before treating CL-versus-AoA or L/D differences as calibration evidence.',
    'Export weight-dependent performance records: MMO/max operating altitude must remain distinct from a climb-limited service ceiling, with weight, CG, temperature, engine state and residual-climb criteria.',
    'Export subsystem masses with definitions and first moments, plus CG envelopes and cruise CG in a declared datum/LEMAC/MAC frame. Aggregate OEW alone cannot establish the envelope.',
    'Export landing-gear topology and wheel/axle coordinates with frame and units. Wheelbase or track alone cannot establish the requested gear parity.',
    'Keep SOL101 deformation rows disabled until a matching load case, materials, constraints and independent displacement/strain measurement reference are available.',
  ];
  const statusCounts = Object.entries(summary.statuses).map(([status, count]) => `${count} ${status}`).join(', ');
  const provenanceCounts = kind => Object.entries((summary.by_provenance || {})[kind] || {}).map(([status, count]) => `${count} ${status}`).join(', ') || 'none';
  const rowsHtml = rows.map(row => {
    const source = row.source_url ? `<a href="${escape(row.source_url)}">${escape(row.source_cite || row.source_title)}</a>` : escape(row.source_cite || row.source_title || '—');
    const diff = row.absolute_error === null ? '—' : `${formatNumber(row.absolute_error)}${row.relative_error === null ? '' : ` (${formatNumber(row.relative_error * 100)}%)`}`;
    return `<tr><td>${escape(row.preset)}<small>${escape(row.variant)}</small></td><td>${escape(row.id)}<small>${escape(row.category)}</small></td><td><span class="status ${escape(row.status)}">${escape(row.status)}</span><small>${escape(row.reason)}</small><small>${escape(row.source_note)}</small>${row.model_provenance_note ? `<small>⚠ ${escape(row.model_provenance_note)}</small>` : ''}</td><td>${escape(displayValue(row.model_value))}<small>${escape(row.unit)}</small></td><td>${escape(displayValue(row.source_value))}<small>${escape(row.unit)}</small></td><td>${escape(diff)}</td><td>${source}<small>${escape(row.evidence)} · ${escape(row.condition || '')}${row.condition_match ? '' : ' · condition mismatch'}</small></td></tr>`;
  }).join('');
  const sourceHtml = sourceCites.map(cite => {
    const source = result.sources[cite] || {};
    const title = source.url ? `<a href="${escape(source.url)}">${escape(source.title || cite)}</a>` : escape(source.title || cite);
    return `<tr><td>${escape(cite)}</td><td>${title}</td><td>${escape(source.authority || '—')}</td><td>${escape(source.url || 'No URL registered in contract')}</td></tr>`;
  }).join('');
  const regulatoryHtml = (result.regulatory || []).map(item => `<tr><td><a href="${escape(item.url || '')}">${escape(item.id || item.title || '')}</a></td><td>${escape(item.title || '')}</td><td>${escape(item.implication || '')}</td></tr>`).join('');
  const bundleHtml = Object.entries(result.evidence_bundles || {}).map(([preset, bundle]) => `<tr><td>${escape(preset)}</td><td>${escape(bundle.path || '—')}</td><td>${escape(bundle.status)}</td><td>${escape(bundle.sha256 || '—')}</td></tr>`).join('');
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Real-aircraft parity audit</title><style>
body{font:14px/1.45 system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;color:#1d2b3a;background:#f4f7fa;margin:0}main{max-width:1500px;margin:auto;padding:30px}h1{font-size:30px;margin:0 0 8px}h2{margin:34px 0 12px}p{max-width:1200px}.notice{padding:18px;background:#e6f1fb;border-left:4px solid #2674af}.warning{padding:18px;background:#fff2d8;border-left:4px solid #c37a00}table{border-collapse:collapse;width:100%;background:#fff;margin:10px 0 20px}th,td{border:1px solid #ccd7e2;padding:8px;text-align:left;vertical-align:top}th{background:#e5edf5}td small{display:block;color:#536679;margin-top:4px;overflow-wrap:anywhere}.status{font-weight:700}.within_tolerance{color:#176b3a}.out_of_tolerance{color:#a2351e}.diagnostic{color:#865e00}.unsupported,.evidence_gap{color:#5d6670}.source_conflict{color:#7b1fa2}code{background:#e7edf2;padding:1px 4px;border-radius:3px}a{color:#145e9a;overflow-wrap:anywhere}ul{max-width:1200px}td:nth-child(4),td:nth-child(5),td:nth-child(6){white-space:nowrap}@media(max-width:900px){main{padding:14px}table{display:block;overflow-x:auto}}
</style></head><body><main><h1>Real-aircraft parity audit</h1><p>Generated ${escape(result.generated_utc)} from <code>${escape(result.inputs.model_path)}</code> against <code>${escape(result.inputs.contract_path)}</code>.</p>
<div class="notice"><b>Comparison is not certification validation.</b> The contract contains independent source anchors and explicit conditions. No expected value is copied from <code>MODEL.json</code>, and no row is re-pinned to model output. ${escape(summary.rows)} rows: ${escape(statusCounts)}. ${escape(summary.eligible_comparisons)} matched primary comparisons enter the release comparison score; all diagnostic and evidence-gap rows remain visible. Of those, independent model outputs: ${escape(provenanceCounts('independent'))}; registered reference inputs (data retention, not prediction): ${escape(provenanceCounts('reference_input'))}.</div>
<h2>Inputs and integrity</h2><table><tbody><tr><th>Contract SHA-256</th><td><code>${escape(result.inputs.contract_sha256)}</code></td></tr><tr><th>Model SHA-256</th><td><code>${escape(result.inputs.model_sha256)}</code></td></tr><tr><th>Contract presets</th><td>${escape(summary.contract_presets)}</td></tr><tr><th>Model presets</th><td>${escape(summary.model_presets)}</td></tr><tr><th>Model-only presets</th><td>${escape(summary.model_only_presets.join(', ') || '—')}</td></tr></tbody></table>
<h2>Status summary</h2>${renderStatusTable(summary)}
<h2>Integration actions</h2><div class="warning"><ul>${actionItems.map(item => `<li>${escape(item)}</li>`).join('')}</ul></div>
<h2>Evidence bundles</h2><table><thead><tr><th>Preset</th><th>Path</th><th>Status</th><th>SHA-256</th></tr></thead><tbody>${bundleHtml}</tbody></table>
<h2>Per-check results</h2><table><thead><tr><th>Preset</th><th>Quantity</th><th>Status / interpretation</th><th>Model</th><th>Source anchor</th><th>Error</th><th>Evidence</th></tr></thead><tbody>${rowsHtml}</tbody></table>
<h2>Sources used by checks</h2><table><thead><tr><th>Citation key</th><th>Source</th><th>Authority</th><th>URL</th></tr></thead><tbody>${sourceHtml || '<tr><td colspan="4">No source citations registered.</td></tr>'}</tbody></table>
<h2>Regulatory design-space anchors</h2><table><thead><tr><th>Rule</th><th>Title</th><th>Parity implication</th></tr></thead><tbody>${regulatoryHtml || '<tr><td colspan="3">No regulatory entries registered.</td></tr>'}</tbody></table>
<h2>Reproduction</h2><p><code>cargo run -p alas-pipeline --example model_reference_dump</code> then <code>node tools/aircraft_parity.cjs</code>. Use <code>--fail-on-out-of-tolerance</code> only when a release gate intentionally requires matched primary checks to pass. The report records source conflicts and gaps without silently converting them to zeros or agreements.</p>
</main></body></html>`;
}

function parseArgs(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--help' || argument === '-h') {
      options.help = true;
    } else if (argument === '--fail-on-out-of-tolerance') {
      options.failOnOutOfTolerance = true;
    } else if (['--model', '--contract', '--out', '--report'].includes(argument)) {
      if (index + 1 >= argv.length) throw new Error(`${argument} requires a path`);
      const value = argv[++index];
      if (argument === '--model') options.modelPath = value;
      if (argument === '--contract') options.contractPath = value;
      if (argument === '--out') options.outputDir = value;
      if (argument === '--report') options.reportPath = value;
    } else {
      throw new Error(`Unknown argument: ${argument}`);
    }
  }
  return options;
}

function usage() {
  return [
    'Usage: node tools/aircraft_parity.cjs [options]',
    '',
    `  --model PATH       Model dump (default ${DEFAULTS.modelPath})`,
    `  --contract PATH    Independent parity contract (default ${DEFAULTS.contractPath})`,
    `  --out DIR          JSON/CSV output directory (default ${DEFAULTS.outputDir})`,
    `  --report PATH      HTML report (default ${DEFAULTS.reportPath})`,
    '  --fail-on-out-of-tolerance  exit 2 if an eligible primary comparison is outside tolerance',
  ].join('\n');
}

if (require.main === module) {
  try {
    const options = parseArgs(process.argv.slice(2));
    if (options.help) {
      console.log(usage());
    } else {
      const result = run(options);
      console.log(JSON.stringify({summary: result.summary, outputs: result.outputs || {}}, null, 2));
      if (options.failOnOutOfTolerance && (result.summary.statuses.out_of_tolerance || 0) > 0) process.exitCode = 2;
    }
  } catch (error) {
    console.error(`aircraft parity: ${error.message || error}`);
    process.exitCode = 1;
  }
}

module.exports = {
  DEFAULTS,
  compareNumeric,
  escape,
  evaluateCheck,
  getPath,
  renderHtml,
  run,
  sourceAvailability,
  valuesEqual,
};
