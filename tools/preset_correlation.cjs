// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Regenerate the evidence-aware preset audit from local reference datasets.
// Run from the repository root: node tools/preset_correlation.cjs
const fs = require('node:fs');
const path = require('node:path');

const median = values => {
  if (!values.length) return null;
  const a = [...values].sort((x, y) => x - y);
  const i = Math.floor(a.length / 2);
  return a.length % 2 ? a[i] : (a[i - 1] + a[i]) / 2;
};
const escape = value => String(value ?? '').replace(/[&<>"']/g,
  c => ({'&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;'}[c]));
const finite = value => typeof value === 'number' && Number.isFinite(value);

function applicability(preset, section, key, reference) {
  if (section === 'vspaero') return 'Historical solver diagnostic: geometry and mesh are not rerun here; fitted total-polar efficiency and point inviscid efficiency are different quantities.';
  if (section === 'route') return 'Routing diagnostic against an estimated fleet analogue, not a measured track for this flight.';
  if (key === 'cargo_volume_total_m3') return 'Total usable hold volume is unsupported; container internal volume is not hold volume. ATR main-deck baggage compartments are absent.';
  if (key === 'cl_alpha_per_rad') return 'Reference wing-alone/low-speed conditions differ from the model cruise aircraft and reported alpha axis.';
  if (section === 'aerodynamics' && key !== 'cruise_mach') return 'Literature/model estimate; Mach, Reynolds number, lift, trim and reference-area definitions must match before calibration.';
  if (section === 'geometry' && ['mac_m', 'lemac_from_nose_m', 'taper_ratio'].includes(key)) return 'Physical planform integral versus certified reference geometry or estimated equivalent planform; datum/planform equivalence is unverified.';
  if (preset === 'B787-9' && ['wing_area_m2', 'aspect_ratio', 'wing_loading_kg_m2'].includes(key)) return 'Reference uses 377 m²; preset uses NASA 360.464 m² convention. Do not tune coefficients or geometry across these area definitions.';
  if (preset === 'DC-10' && (section === 'mass' || key === 'uld_positions')) return 'Reference includes a different ACAP weight row or freighter layout; passenger 572k-option equivalence is unverified.';
  if (preset === 'A220-300' && key === 'fan_diameter_m') return 'Reference is fan-case diameter; model is fan-tip diameter.';
  if (reference.cite === 'alas_preset_input') return 'Reference copied from the preset; agreement is not independent evidence.';
  if (preset === 'A380-800' && key === 'usable_fuel_kg') return 'Fuel masses use different densities (0.785 versus 0.800 kg/L).';
  if (['A340-300','A380-800'].includes(preset) && ['oew_kg','max_payload_kg'].includes(key)) return 'Typical cabin-dependent OEW is not tied to the exact registered configuration; derived structural payload inherits that uncertainty.';
  if (preset === 'A320-200' && section === 'mass') return 'Reference OEW is a different cabin/sharklet configuration; fuel mass also uses a different density/modification state.';
  if (section === 'propulsion' && key === 'overall_pressure_ratio') return 'Catalogue LTO evidence anchor, not a predicted cruise-cycle pressure ratio.';
  if (section === 'propulsion' && key === 'cruise_tsfc_kg_kgf_h') return 'Cruise engine-family/condition equivalence is unverified; retained as a diagnostic.';
  if (reference.evidence === 'estimated' || /LOW CONFIDENCE|unverified provenance/i.test(reference.note || '')) return 'Estimated or low-confidence reference; not independent physical validation.';
  return '';
}

function makeRow(preset, section, key, model, reference) {
  const reason = applicability(preset, section, key, reference);
  const difference = finite(model) && finite(reference.value) && reference.value !== 0
    ? 100 * (model - reference.value) / reference.value : null;
  return {preset, section, key, model: finite(model) ? model : null,
    reference: reference.value, unit: reference.unit || '', evidence: reference.evidence || '',
    cite: reference.cite || '', condition: reference.condition || '', note: reference.note || '',
    status: !finite(model) ? 'unsupported' : reason ? 'diagnostic' : 'comparison',
    reason, difference_pct: difference,
    error_pct: reason || !finite(model) ? null : difference};
}

function main(dir = 'out/validation') {
  const read = file => JSON.parse(fs.readFileSync(path.join(dir, file), 'utf8'));
  const model = read('MODEL.json');
  const before = fs.existsSync(path.join(dir, 'MODEL.before-fixes.json')) ? read('MODEL.before-fixes.json') : {};
  const vsp = read('VSPAERO.json');
  const routes = read('ROUTES.json');
  const verification = fs.existsSync(path.join(dir, 'VERIFICATION.json')) ? read('VERIFICATION.json') : null;
  const rows = [];
  const remap = {
    'mass.max_payload_kg': m => m.payload_range?.max_payload_kg,
    'mass.usable_fuel_kg': m => m.payload_range?.fuel_capacity_kg,
    'cabin_payload.seats_typical_2class': m => m.cabin_payload?.seats_modelled,
    'cabin_payload.uld_positions': m => m.cabin_payload?.available_uld_positions,
    'cabin_payload.cargo_volume_total_m3': m => m.cabin_payload?.cargo_hold_volume_m3,
    'aerodynamics.cl_alpha_per_rad': m => m.aerodynamics?.cl_alpha_per_rad_reported_axis,
    'propulsion.max_cruise_thrust_kn': m => m.propulsion?.cruise_reference_thrust_kn,
    'propulsion.bypass_ratio': m => m.propulsion?.takeoff_bypass_ratio,
    'propulsion.engine_count': m => m.propulsion?.n_engines,
    'propulsion.overall_pressure_ratio': m => m.propulsion?.catalogue_lto_overall_pressure_ratio,
  };
  for (const [preset, m] of Object.entries(model)) {
    const filename = path.join(dir, `${preset}.json`);
    if (fs.existsSync(filename)) {
      const ref = read(`${preset}.json`);
      for (const section of ['geometry', 'mass', 'cabin_payload', 'aerodynamics', 'propulsion']) {
        for (const [key, reference] of Object.entries(ref[section] || {})) {
          if (!finite(reference?.value)) continue;
          const getter = remap[`${section}.${key}`];
          const value = getter ? getter(m) : m[section]?.[key];
          rows.push(makeRow(preset, section, key, value, reference));
        }
      }
    }
    if (vsp[preset]) {
      for (const [key, mv, rv] of [
        ['cl_alpha_per_rad', m.aerodynamics?.cl_alpha_per_rad_reported_axis, vsp[preset].vsp_cl_alpha_per_rad],
        ['oswald_e_at_cruise_cl', m.aerodynamics?.oswald_e, vsp[preset].vsp_oswald_e_at_cruise_cl],
      ]) rows.push(makeRow(preset, 'vspaero', key, mv, {value:rv, evidence:'solver', cite:'vspaero_run'}));
    }
    const route = (routes.routes || []).find(r => r.preset === preset);
    if (route && finite(route.great_circle_km?.value)) {
      const matchingPair = route.default_city_pair === `${m.route?.departure}-${m.route?.arrival}`;
      const row = makeRow(preset, 'route', 'track_to_great_circle_ratio',
        matchingPair ? m.route.planned_route_km / route.great_circle_km.value : null,
        route.track_to_great_circle_ratio);
      if (!matchingPair) row.reason = 'Model route is unavailable or its airport pair does not match this reference.';
      rows.push(row);
    }
  }
  fs.writeFileSync(path.join(dir, 'CORRELATION.json'), JSON.stringify(rows, null, 2));
  const fields = ['preset','section','key','model','reference','unit','difference_pct','error_pct','status','reason','evidence','cite'];
  const csv = v => `"${String(v ?? '').replaceAll('"','""')}"`;
  fs.writeFileSync(path.join(dir,'CORRELATION.csv'), [fields.join(','), ...rows.map(r => fields.map(k => csv(r[k])).join(','))].join('\n'));
  const scored = rows.filter(r => finite(r.error_pct));
  const fmt = v => finite(v) ? Number(v.toPrecision(5)).toString() : '—';
  const med = median(scored.map(r => Math.abs(r.error_pct)));
  let tables = '';
  for (const [preset, m] of Object.entries(model)) {
    tables += `<h2>${escape(preset)}</h2>`;
    if (m.error) tables += `<p class="open">Analysis failed: ${escape(m.error)}</p>`;
    const old = before[preset];
    if (old?.aerodynamics && m.aerodynamics) tables += `<p>Historical → current model: CD0 ${fmt(old.aerodynamics.cd0_cruise)} → ${fmt(m.aerodynamics.cd0_cruise)}; cruise L/D ${fmt(old.aerodynamics.l_over_d_cruise)} → ${fmt(m.aerodynamics.l_over_d_cruise)}. This includes current checkout changes; it is not an isolated calibration experiment.</p>`;
    if (m.cabin_payload) tables += `<p>Available ULD positions: ${fmt(m.cabin_payload.available_uld_positions)}; loaded: ${fmt(m.cabin_payload.container_positions_total)}; loose-bulk positions: ${fmt(m.cabin_payload.available_bulk_positions)}. Nominal container internal volume: ${fmt(m.cabin_payload.available_container_internal_volume_m3)} m³.</p>`;
    if (m.route) tables += `<p>Route: ${escape(m.route.departure)} → ${escape(m.route.arrival)}; source ${escape(m.route.route_source || 'unavailable')}; ${fmt(m.route.planned_route_km)} km. Mission: ${escape(m.route.mission_status || m.route.status || 'unavailable')}. ${escape(m.route.error)}</p>`;
    tables += '<table><thead><tr><th>Quantity / conditions</th><th>Model</th><th>Reference</th><th>Difference</th><th>Assessment / evidence</th></tr></thead><tbody>';
    for (const r of rows.filter(r => r.preset === preset)) tables += `<tr><td>${escape(r.section + '.' + r.key)}<small>${escape(r.condition)}</small></td><td>${fmt(r.model)} ${escape(r.unit)}</td><td>${fmt(r.reference)}</td><td>${finite(r.difference_pct) ? fmt(r.difference_pct) + '%' : '—'}</td><td><b>${escape(r.status)}</b><small>${escape(r.reason)} ${escape(r.note)}</small><small>${escape(r.evidence)} · ${escape(r.cite)}</small></td></tr>`;
    tables += '</tbody></table>';
  }
  const html = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Aircraft preset correlation — corrected audit</title><style>
  body{font:15px/1.5 system-ui,sans-serif;color:#192a3a;background:#f4f7fa;margin:0}main{max-width:1280px;margin:auto;padding:32px}h1{font-size:30px}h2{margin-top:36px}table{border-collapse:collapse;width:100%;background:white;font-size:13px}td,th{padding:9px;border:1px solid #ced8e2;text-align:left;vertical-align:top}th{background:#e5edf5}small{display:block;color:#536679;margin-top:5px}td:first-child{overflow-wrap:anywhere}td:nth-child(2),td:nth-child(3),td:nth-child(4){white-space:nowrap}.notice{padding:18px;background:#e7f1fb;border-left:4px solid #2674af}.open{padding:16px;background:#fff1d8}a{color:#145e9a}@media(max-width:800px){main{padding:14px}table{display:block;overflow-x:auto}}
  </style></head><body><main><h1>Aircraft preset correlation — corrected audit</h1><p>Regenerated ${new Date().toISOString()}. Reproduce with <code>cargo run -p alas-pipeline --example model_reference_dump</code>, then <code>node tools/preset_correlation.cjs</code>.</p>
  <div class="notice"><b>Comparison is not validation.</b> ${rows.length} reference rows are retained, including unsupported outputs and non-equivalent comparisons. ${scored.length} numerical comparisons enter summary statistics; median absolute difference ${fmt(med)}%. Within 2% / 5% / 10%: ${[2,5,10].map(n => scored.filter(r => Math.abs(r.error_pct) <= n).length).join(' / ')}. Copied preset inputs are data consistency checks, not independent model predictions. Diagnostic differences remain visible but do not enter the score. Zero references have no percentage error.</div>
  <h2>Defects corrected</h2><ul><li>Available cargo capacity is separate from loaded cargo. Bulk baggage is excluded from ULD counts, its mass is retained, and physical bulk positions cannot overlap ULD positions.</li><li>Parasite drag excludes main-wing area buried inside the fuselage. The legacy calculation remains selectable with <code>drag_model.exclude_buried_main_wing_area=false</code>. This geometric correction does not eliminate the remaining drag-model uncertainty.</li><li>The airway loader supports coordinate-bearing legacy records; a configurable detour limit rejects unsuitable automatic airway routes. Great-circle fallbacks are conceptual routes, not dispatch clearances.</li><li>ATR active wing area closes at 61 m²; A320 cross-section height and engine lateral stations, and A340 horizontal-tail span follow the cited dimensions.</li><li>The audit integrates the actual active planform, includes missing model quantities explicitly, uses takeoff engine fields for takeoff comparisons, and computes medians correctly for even samples.</li></ul>
  <h2>Open engineering limitations</h2><div class="open">ATR main-deck baggage compartments and total usable hold volumes are not implemented. Generic cargo geometry does not establish certified capacity. Remaining CD0 and L/D differences require matched Mach, Reynolds number, lift, trim and reference-area evidence. Historical VSPAERO results must be rerun after geometry changes; fitted total-polar Oswald efficiency is not point inviscid span efficiency, and there is no universal swept-wing ceiling at 0.968. Several certified MAC/datum and weight-variant comparisons remain unresolved. Structural external validation is not established by this audit.</div>
  <h2>Verification status</h2><p>${escape(verification?.summary || 'Verification record not supplied; do not infer test success from numeric agreement.')}</p>
  <p>Supporting documentation: <a href="../../docs/preset-correlation.md">corrections and reproducibility</a>, <a href="../../docs/cargo-capacity.md">cargo capacity</a>, <a href="../../docs/methods.md">drag method</a>, <a href="../../docs/route-model.md">routing</a>. All per-row citations refer to <a href="../validation/REFERENCES.bib">the reference bibliography</a>. Source notes remain visible so that estimates and configuration mismatches cannot be mistaken for defects. <a href="../validation/VERIFICATION.json">Detailed verification record</a>.</p>${tables}</main></body></html>`;
  fs.writeFileSync('out/reports/2026-09-04-preset-real-world-correlation.html', html);
  console.log(`${rows.length} rows; ${scored.length} scored comparisons; ${rows.filter(r => r.status === 'unsupported').length} unsupported outputs`);
}

module.exports = {median, escape, makeRow, applicability, main};
if (require.main === module) main(process.argv[2]);
