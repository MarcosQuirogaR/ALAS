#!/usr/bin/env python3
"""Build the mass-model consolidation report and before/after CSVs.

Reads the experiment-matrix outputs written by
``cargo run -p alas-pipeline --release --example mass_experiment_matrix`` for
a baseline and an after-correction run, the Aviary equation replay, and the
narrowbody rerun, and writes:

* ``outputs/mass-model-consolidation/before-after-oew.csv``
* ``outputs/mass-model-consolidation/before-after-components.csv``
* ``outputs/mass-model-consolidation/sweep-invariance.csv``
* ``outputs/mass-model-consolidation/decision-table.csv``
* ``.agent/reports/mass-model-consolidation.html`` (self-contained, inline SVG)

Usage (from the repository root)::

    uv run python tools/report_mass_model_consolidation.py
"""

from __future__ import annotations

import csv
import hashlib
import html
import json
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "outputs" / "mass-model-consolidation"
BASELINE = OUT / "baseline"
AFTER = OUT / "after"
NARROW = OUT / "after-narrowbody"
REPORT = ROOT / ".agent" / "reports" / "mass-model-consolidation.html"
GROUPS = ["Wing", "H-Stab", "V-Stab", "Fuselage", "Gear", "Propulsion", "Systems", "Furnishings"]


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def read_csv(path: Path) -> list[dict]:
    with path.open(encoding="utf-8", newline="") as handle:
        return list(csv.DictReader(handle))


def write_csv(path: Path, rows: list[dict], columns: list[str]) -> None:
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        writer.writerows(rows)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def f(value, digits=1):
    if value is None or value == "":
        return "-"
    try:
        return f"{float(value):,.{digits}f}"
    except (TypeError, ValueError):
        return str(value)


def esc(text) -> str:
    return html.escape(str(text))


def before_after_oew(base: dict, after: dict) -> list[dict]:
    rows = []
    for name, entry in after["presets"].items():
        b = base["presets"].get(name, {}).get("fixed_design_weight", {})
        a = entry["fixed_design_weight"]
        if a.get("status") != "ok":
            rows.append({"preset": name, "status": a.get("status"), "reason": a.get("reason", "")})
            continue
        rows.append({
            "preset": name,
            "status": "ok",
            "reason": "",
            "design_gross_mass_kg": a["buildup"]["design_gross_mass_kg"],
            "design_landing_mass_before_kg": b.get("buildup", {}).get("design_landing_mass_kg"),
            "design_landing_mass_after_kg": a["buildup"]["design_landing_mass_kg"],
            "seated_pax": (a.get("layout") or {}).get("seated_pax"),
            "flops_split_before": "/".join(str(x) for x in b.get("buildup", {}).get("class_split", [])),
            "flops_split_after": "/".join(str(x) for x in a["buildup"]["class_split"]),
            "oew_before_kg": b.get("oew_kg"),
            "oew_after_kg": a["oew_kg"],
            "oew_delta_kg": (a["oew_kg"] - b["oew_kg"]) if b.get("oew_kg") is not None else None,
            "reference_oew_kg": a.get("reference_oew_kg"),
            "reference_note": "preset reference.oew_kg (mixed definitions; see docs/pure-flops-aircraft-evidence.md)",
            "payload_after_kg": a["buildup"]["masses_kg"]["Payload"],
            "actual_zfw_after_kg": a["actual_zfw_kg"],
            "reference_mzfw_kg": a.get("reference_mzfw_kg"),
            "signed_fuel_closure_after_kg": a["signed_fuel_closure_kg"],
            "usable_fuel_capacity_kg": a.get("usable_fuel_capacity_kg"),
        })
    return rows


def before_after_components(base: dict, after: dict) -> list[dict]:
    rows = []
    for name, entry in after["presets"].items():
        b = base["presets"].get(name, {}).get("fixed_design_weight", {})
        a = entry["fixed_design_weight"]
        if a.get("status") != "ok" or b.get("status") != "ok":
            continue
        for group in GROUPS:
            before = b["buildup"]["masses_kg"][group]
            now = a["buildup"]["masses_kg"][group]
            cause = ""
            if group == "Gear" and abs(now - before) > 1e-6:
                cause = "design landing mass: preset MLW in BaselineSandbox instead of 0.92 x MTOW (CleanSheet default mode)"
            elif group in ("Systems", "Furnishings") and abs(now - before) > 1e-6:
                cause = "FLOPS cabin synchronized with the seated layout (class split, passenger total)"
            rows.append({"preset": name, "group": group, "before_kg": before, "after_kg": now, "delta_kg": now - before, "attributed_to": cause})
    return rows


def sweep_invariance(base_rows: list[dict], after_rows: list[dict]) -> list[dict]:
    rows = []
    for tag, source in (("baseline", base_rows), ("after", after_rows)):
        by_preset: dict[str, list[dict]] = {}
        for row in source:
            by_preset.setdefault(row["preset"], []).append(row)
        for preset, entries in by_preset.items():
            oews = [float(r["oew_kg"]) for r in entries if r.get("oew_kg")]
            wings = [float(r["wing_kg"]) for r in entries if r.get("wing_kg")]
            if not oews:
                rows.append({"run": tag, "preset": preset, "ranges_nmi": ";".join(r["range_nmi"] for r in entries), "oew_min_kg": None, "oew_max_kg": None, "oew_spread_kg": None, "wing_spread_kg": None, "statuses": ";".join(r["status"] for r in entries)})
                continue
            rows.append({
                "run": tag,
                "preset": preset,
                "ranges_nmi": ";".join(f"{float(r['range_nmi']):.0f}" for r in entries),
                "oew_min_kg": min(oews),
                "oew_max_kg": max(oews),
                "oew_spread_kg": max(oews) - min(oews),
                "wing_spread_kg": (max(wings) - min(wings)) if wings else None,
                "statuses": ";".join(r["status"] for r in entries),
            })
    return rows


DECISIONS = [
    ("MDA re-sized a registered aircraft's structure at the dispatch iterate in every design mode (B787-9 OEW 103.9-112.0 t over a 500-7,635 nmi sweep; A220-300 33.1-34.1 t)", "sizing-mode mismatch", "implemented: MassSizingBasis / AlasConfig::at_closure_mass; fixed-aircraft modes keep declared DG and WLDG (sweep spread now 0 kg)", "outputs/mass-model-consolidation/{baseline,after}/mission-sweep.csv; alas-opt tests/mission_sized.rs (8)"),
    ("MtowSizing::Unconstrained probe closures 19-34 % below published MTOW (A380 370,695 kg etc.)", "sizing-mode mismatch + mission-model issue", "reclassified: CleanSheet coupled closure on operational airport pairs with re-derived cabins; not a fixed-aircraft MTOW prediction. Reproduced exactly on the baseline (A380 370,694.85 kg)", "outputs/mass-model-consolidation/baseline/mission-cases.csv"),
    ("Report path priced FLOPS furnishings/service/crew for a different cabin than the payload it carried (A320 150-seat OEW under a 180-seat payload)", "accounting mismatch", "implemented: full_analysis/cabin_sync.rs; alas-pipeline tests/fixed_aircraft_mass_basis.rs", "outputs/mass-model-consolidation/before-after-components.csv"),
    ("Optimizer path charged checked baggage twice (116 kg per seat vs 100 kg on the report path; +10.6 t payload on a 663-seat A380)", "implementation error", "implemented: PassengerCabinConfig::set_passenger_mass_kg splits occupant and checked bag; alas-config cabin test", "outputs/mass-model-consolidation/{baseline,after}/mission-cases.csv payload_kg"),
    ("Fixed-aircraft mode unevaluable on A320/A340/A380/DC-10 (structural_sizing error: doubled semi-wing box exceeds the FLOPS wing, A320 12,533 vs 7,913 kg)", "implementation error (diagnostic was fatal) + unresolved structural-model bias", "implemented: reconciliation reports ReferenceExceededBySizedBox; the strength model's box mass on those aircraft stays an open structural finding", "crates/alas-mass/examples/wingbox_vs_flops.rs; wing_reconciliation/support.rs tests"),
    ("Report path used 0.92 x MTOW as the gear design landing mass for registered aircraft (presets load in CleanSheet)", "input mismatch", "documented and exposed: BaselineSandbox uses the certified MLW (A320 gear -272 kg); the experiment matrix records both modes", "outputs/mass-model-consolidation/before-after-components.csv Gear rows"),
    ("wing_loading residual penalised a fixed aircraft dispatched light (A220, AVE)", "sizing-mode mismatch", "implemented: residual evaluated on design_gross_mass_kg", "outputs/mass-model-consolidation/{baseline,after}/mission-cases.csv violated_hard_ids"),
    ("A320-200 climb-energy deficit", "mission-model issue (input)", "diagnosed: generic default profile step climbs at 250 m/s true airspeed (M0.78-0.85 at 5-9 km); persists at zero payload and 1.2 x thrust; the preset carries no narrowbody schedule. Not a mass result; not weakened", "outputs/mass-model-consolidation/after-narrowbody/climb-diagnosis.csv"),
    ("A320-214 WV017 Sharklet OEW residual vs 41,052 kg", "reference-data limitation", "no matched OEW; the 41,052 kg sheet is a 77 t / 180Y aircraft and is reproduced as an explicit case (-1,359 kg, -3.3 %, wingtip-fence span not applied, unknown inclusion list)", "outputs/mass-model-consolidation/after/reconstructed-cases.csv"),
    ("A220-300 OEW residual vs 37,149 kg (-8.2 % product cabin, -8.7 % declared 140Y)", "reference-data limitation + unresolved model bias", "holdout unchanged by retuning; residual not attributable to a component; no calibration fitted", "outputs/mass-model-consolidation/after/reconstructed-cases.csv"),
    ("A220-300 wing 0.23 kg drift against the frozen refinement record", "input mismatch (concurrent geometry work)", "traced to the VLM spanwise-mesh change resampling t/c 0.132207 -> 0.132220; A320 replay parity 1.1e-13 kg", "outputs/mass-model-consolidation/aviary-parity/component-comparison.csv"),
    ("Engine term WENGB described as an all-in installation", "accounting mismatch (documentation)", "boundary stated: includes inlet/nozzle only when undeclared; pylons, mounts, EBU, fluids are unresolved scope", "docs/mass-model-architecture.md crosswalk"),
    ("ATR72-600", "reference-data limitation (domain)", "unsupported: no propeller/shaft-power FLOPS branch; explicit row", "outputs/mass-model-consolidation/after/failures.csv"),
]


def svg_bar_chart(rows: list[dict], title: str) -> str:
    items = [r for r in rows if r.get("status") == "ok" and r.get("oew_before_kg") is not None]
    if not items:
        return ""
    width, height, left, top = 760, 40 + 34 * len(items), 110, 30
    max_value = max(max(float(r["oew_after_kg"]), float(r["oew_before_kg"]), float(r.get("reference_oew_kg") or 0)) for r in items)
    scale = (width - left - 40) / max_value
    parts = [f'<svg viewBox="0 0 {width} {height}" width="100%" role="img" aria-label="{esc(title)}" style="max-width:{width}px;font-family:system-ui;font-size:12px">']
    parts.append(f'<text x="{left}" y="18" font-weight="600">{esc(title)}</text>')
    for i, r in enumerate(items):
        y = top + 34 * i
        parts.append(f'<text x="{left - 8}" y="{y + 20}" text-anchor="end">{esc(r["preset"])}</text>')
        parts.append(f'<rect x="{left}" y="{y + 4}" width="{float(r["oew_before_kg"]) * scale:.1f}" height="9" fill="#9aa7b4"/>')
        parts.append(f'<rect x="{left}" y="{y + 15}" width="{float(r["oew_after_kg"]) * scale:.1f}" height="9" fill="#2f6fb5"/>')
        if r.get("reference_oew_kg"):
            x = left + float(r["reference_oew_kg"]) * scale
            parts.append(f'<line x1="{x:.1f}" y1="{y + 2}" x2="{x:.1f}" y2="{y + 26}" stroke="#c0392b" stroke-width="2"/>')
        parts.append(f'<text x="{left + float(r["oew_after_kg"]) * scale + 6:.1f}" y="{y + 23}" fill="#2f6fb5">{f(r["oew_after_kg"], 0)} kg</text>')
    parts.append(f'<text x="{left}" y="{height - 6}" fill="#555">grey: baseline (preset default mode); blue: after (BaselineSandbox, seated cabin); red tick: preset reference OEW (mixed definitions)</text>')
    parts.append("</svg>")
    return "".join(parts)


def table(rows: list[dict], columns: list[str], digits: dict | None = None) -> str:
    digits = digits or {}
    head = "".join(f"<th>{esc(c)}</th>" for c in columns)
    body = []
    for row in rows:
        cells = []
        for c in columns:
            value = row.get(c, "")
            if isinstance(value, float) or (isinstance(value, str) and value.replace(".", "", 1).replace("-", "", 1).isdigit() and c not in ("preset", "label", "run")):
                cells.append(f'<td class="num">{f(value, digits.get(c, 1))}</td>')
            else:
                cells.append(f"<td>{esc(value)}</td>")
        body.append("<tr>" + "".join(cells) + "</tr>")
    return f'<div class="scroll"><table><thead><tr>{head}</tr></thead><tbody>{"".join(body)}</tbody></table></div>'


def main() -> int:
    base = read_json(BASELINE / "raw.json")
    after = read_json(AFTER / "raw.json")
    narrow = read_json(NARROW / "raw.json") if (NARROW / "raw.json").is_file() else None
    manifest = read_json(BASELINE / "manifest.json")

    oew_rows = before_after_oew(base, after)
    comp_rows = before_after_components(base, after)
    sweep_rows = sweep_invariance(read_csv(BASELINE / "mission-sweep.csv"), read_csv(AFTER / "mission-sweep.csv"))
    decision_rows = [{"discrepancy": d, "classification": c, "resolution": r, "evidence": e} for d, c, r, e in DECISIONS]
    oew_columns = ["preset", "status", "design_gross_mass_kg", "design_landing_mass_before_kg", "design_landing_mass_after_kg", "seated_pax", "flops_split_before", "flops_split_after", "oew_before_kg", "oew_after_kg", "oew_delta_kg", "reference_oew_kg", "payload_after_kg", "actual_zfw_after_kg", "reference_mzfw_kg", "signed_fuel_closure_after_kg", "usable_fuel_capacity_kg", "reference_note", "reason"]
    write_csv(OUT / "before-after-oew.csv", oew_rows, oew_columns)
    write_csv(OUT / "before-after-components.csv", comp_rows, ["preset", "group", "before_kg", "after_kg", "delta_kg", "attributed_to"])
    write_csv(OUT / "sweep-invariance.csv", sweep_rows, ["run", "preset", "ranges_nmi", "oew_min_kg", "oew_max_kg", "oew_spread_kg", "wing_spread_kg", "statuses"])
    write_csv(OUT / "decision-table.csv", decision_rows, ["discrepancy", "classification", "resolution", "evidence"])

    mission_after = read_csv(AFTER / "mission-cases.csv")
    mission_before = read_csv(BASELINE / "mission-cases.csv")
    recon = read_csv(AFTER / "reconstructed-cases.csv")
    climb = read_csv((NARROW if narrow else AFTER) / "climb-diagnosis.csv")
    parity = read_csv(OUT / "aviary-parity" / "component-comparison.csv")
    parity_fail = [r for r in parity if r["pass"].lower() != "true"]
    sens = after["presets"]["A320-200"].get("a320_sensitivities", {}).get("rows", [])
    now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%MZ")

    hashes = {p.name: sha256(p) for p in [BASELINE / "raw.json", AFTER / "raw.json", OUT / "aviary-parity" / "alas-inputs.json"] if p.is_file()}
    if narrow:
        hashes["after-narrowbody/raw.json"] = sha256(NARROW / "raw.json")

    css = """
    :root{--fg:#1d2430;--bg:#fbfbf9;--muted:#5b6672;--line:#d9dee5;--accent:#2f6fb5;--warn:#b7791f;--bad:#c0392b}
    body{margin:0;padding:24px 20px;background:var(--bg);color:var(--fg);font:14px/1.5 system-ui,Segoe UI,Roboto,sans-serif;max-width:1180px}
    h1{font-size:24px;margin:0 0 4px}h2{font-size:18px;margin:32px 0 8px;border-bottom:1px solid var(--line);padding-bottom:4px}h3{font-size:15px;margin:20px 0 6px}
    p,li{max-width:80ch}.muted{color:var(--muted)}.scroll{overflow-x:auto;margin:8px 0}
    table{border-collapse:collapse;font-size:12.5px;min-width:520px}th,td{border:1px solid var(--line);padding:4px 7px;text-align:left;vertical-align:top}th{background:#eef1f5}td.num{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}
    .box{border:1px solid var(--line);border-radius:6px;padding:10px 14px;margin:10px 0;background:#fff}.bad{color:var(--bad)}.warn{color:var(--warn)}code{background:#eef1f5;padding:0 4px;border-radius:3px}
    """
    doc = [f"<title>Mass-model consolidation</title><style>{css}</style><body>"]
    doc.append("<h1>ALAS mass-model consolidation</h1>")
    doc.append(f'<p class="muted">Generated {now}. Baseline HEAD {esc(manifest["head"])} with the uncommitted implementation preserved; toolchain {esc(manifest["toolchain"])}. Full definitions: <code>docs/mass-model-architecture.md</code>. Regenerate: <code>cargo run -p alas-pipeline --release --example mass_experiment_matrix -- outputs/mass-model-consolidation/&lt;label&gt;</code> then <code>uv run python tools/report_mass_model_consolidation.py</code>.</p>')

    doc.append("<h2>1. Outcome</h2><div class='box'><ul>")
    doc.append("<li><b>One architecture, one sizing basis.</b> Pure FLOPS transport v1 remains the only production mass model. The design gross and landing masses the equations size against are now an explicit state (<code>MassSizingBasis</code>): a registered aircraft keeps its declared weights under any mission; a clean-sheet design couples to its closure.</li>")
    doc.append("<li><b>Root cause of the light registered aircraft.</b> The MDA loop rewrote the takeoff-mass requirement to the dispatch iterate and re-sized every component at it in every design mode. Operating empty mass of a fixed aircraft rose with mission range (B787-9 103.9 t at 500 nmi to 112.0 t at 7,635 nmi). It is now invariant (spread 0 kg).</li>")
    doc.append("<li><b>Accounting corrections.</b> One cabin per case (FLOPS cabin terms priced for the seated layout); checked baggage no longer charged twice on the optimizer path; the strength-box diagnostic reports instead of aborting; wing loading on the design gross mass.</li>")
    doc.append("<li><b>Implementation correctness.</b> Independent NASA/Aviary equation replay: 34/34 A320 rows within 1.1e-13 kg on the regenerated deck.</li>")
    doc.append("<li><b>Physical accuracy: not established, and precisely bounded.</b> No configuration-matched OEW exists for the registered A320-214 WV017 Sharklet case. The closest record (77 t, 180Y, 41,052 kg, inclusion list unknown) is reproduced as an explicit case at -3.3 %. The frozen A220-300 holdout stays at -8.2 % (product cabin) / -8.7 % (declared 140Y) against a secondary planning value. No calibration was fitted.</li>")
    doc.append("</ul></div>")

    doc.append("<h2>2. Fixed design-weight ledger, before and after</h2>")
    doc.append("<p>Case A: every registered preset at its declared MTOW through the product report path. Baseline rows were evaluated in the preset's default <code>CleanSheet</code> mode (landing mass 0.92 x MTOW, FLOPS cabin from the preset record); after rows in <code>BaselineSandbox</code> (certified MLW, FLOPS cabin = seated cabin). Reference OEWs are the presets' mixed-definition anchors, shown for orientation only.</p>")
    doc.append(svg_bar_chart(oew_rows, "Operating empty mass by preset, kg"))
    doc.append(table(oew_rows, ["preset", "status", "design_gross_mass_kg", "design_landing_mass_before_kg", "design_landing_mass_after_kg", "seated_pax", "flops_split_before", "flops_split_after", "oew_before_kg", "oew_after_kg", "oew_delta_kg", "reference_oew_kg", "actual_zfw_after_kg", "reference_mzfw_kg", "signed_fuel_closure_after_kg", "usable_fuel_capacity_kg"], {"design_gross_mass_kg": 0, "seated_pax": 0}))
    doc.append("<h3>Component deltas and their cause</h3>")
    doc.append(table([r for r in comp_rows if abs(r["delta_kg"]) > 1e-6], ["preset", "group", "before_kg", "after_kg", "delta_kg", "attributed_to"]))

    doc.append("<h2>3. Fixed-aircraft mission sweeps: structural invariance</h2>")
    doc.append("<p>Same airframe, <code>BaselineSandbox</code>, <code>SizedByMission</code>, design range swept. The operating-empty spread across the sweep is the whole finding.</p>")
    doc.append(table(sweep_rows, ["run", "preset", "ranges_nmi", "oew_min_kg", "oew_max_kg", "oew_spread_kg", "wing_spread_kg", "statuses"]))

    doc.append("<h2>4. Mission cases by mode (after)</h2>")
    doc.append("<p>B rows are the fixed aircraft (declared DG/WLDG); C rows are the same design vector as a clean-sheet design (re-solved fuselage, coupled DG), i.e. a different aircraft. <code>hard_infeasible</code> lists the residuals the product would reject on; the masses are still the evaluated ledger.</p>")
    doc.append(table(mission_after, ["preset", "label", "status", "sizing_basis", "design_gross_mass_kg", "design_landing_mass_kg", "takeoff_mass_kg", "oew_kg", "zero_fuel_mass_kg", "payload_kg", "carried_passengers", "block_fuel_kg", "destination_landing_mass_kg", "landing_mass_limit_kg", "usable_capacity_kg", "violated_hard_ids"], {"design_gross_mass_kg": 0, "carried_passengers": 0}))
    doc.append("<h3>Baseline mission cases (for reference)</h3>")
    doc.append(table(mission_before, ["preset", "label", "status", "takeoff_mass_kg", "oew_kg", "zero_fuel_mass_kg", "payload_kg", "carried_passengers", "block_fuel_kg", "violated_hard_ids", "dispatch_status"], {"carried_passengers": 0}))

    doc.append("<h2>5. Reconstructed reference cases (A320 first, A220 holdout)</h2>")
    doc.append("<p>Each case is the registered preset plus an itemized list of declared changes, run through the product report path in <code>BaselineSandbox</code>; nothing in the registry was edited. Residuals are conditional on the reference's unknown inclusion list.</p>")
    doc.append(table(recon, ["label", "preset", "status", "declared_mtow_kg", "design_gross_mass_kg", "design_landing_mass_kg", "seated_pax", "flops_first", "flops_business", "flops_tourist", "flight_attendants", "wing_kg", "fuselage_kg", "gear_kg", "propulsion_kg", "systems_kg", "furnishings_kg", "oew_kg", "reference_oew_kg", "oew_residual_kg", "oew_residual_pct", "actual_zfw_kg"], {"declared_mtow_kg": 0, "design_gross_mass_kg": 0, "seated_pax": 0, "flops_first": 0, "flops_business": 0, "flops_tourist": 0, "flight_attendants": 0, "oew_residual_pct": 2}))
    for case in after.get("reconstructed_cases", []):
        doc.append(f"<div class='box'><b>{esc(case.get('label'))}</b><ul>" + "".join(f"<li>{esc(c)}</li>" for c in case.get("changes", [])) + f"</ul><p class='muted'>{esc(case.get('reference_note', ''))}</p></div>")
    doc.append("<h3>A320 wing sensitivities (low-level FLOPS wing equation, one input at a time)</h3>")
    doc.append(table([{"label": r["label"], "wing_kg": r["wing_kg"], "delta_wing_kg": r["delta_wing_kg"], "span_m": r["inputs"]["wing_span_m"], "DG_kg": r["inputs"]["design_gross_mass_kg"], "ULF": r["inputs"]["ultimate_load_factor"], "note": r["note"]} for r in sens], ["label", "wing_kg", "delta_wing_kg", "span_m", "DG_kg", "ULF", "note"], {"span_m": 2, "DG_kg": 0, "ULF": 3}))

    doc.append("<h2>6. A320 climb-energy deficit</h2>")
    doc.append("<p>Fixed aircraft, operational route, bounded variations. The deficit is not a mass result: it persists with zero payload and with 1.2 x rated thrust, and the flown step-climb speed is the generic default profile's 250 m/s true airspeed.</p>")
    doc.append(table(climb, ["preset", "variant", "status", "takeoff_mass_kg", "zero_fuel_mass_kg", "block_fuel_kg", "speed_reference", "step_climb_1_air_speed_m_s", "thrust_kn_per_engine", "dispatch_status", "note"]))

    doc.append("<h2>7. ALAS versus the pinned NASA/Aviary equations</h2>")
    doc.append(f"<p>Independent Python replay of the NASA/TM-2017-219627 equations from the regenerated ALAS deck (<code>outputs/mass-model-consolidation/aviary-parity/</code>): {len(parity) - len(parity_fail)} of {len(parity)} rows within 1e-6 kg. Failing rows:</p>")
    doc.append(table(parity_fail, ["aircraft", "component", "alas_kg", "aviary_equation_replay_kg", "delta_kg"], {"alas_kg": 3, "aviary_equation_replay_kg": 3, "delta_kg": 3}) if parity_fail else "<p>none</p>")
    doc.append("<p class='muted'>The A220-300 wing rows compare the current ALAS value with the frozen refinement record rather than with a fresh replay; the 0.23 kg difference is the lofted thickness ratio resampled by the concurrent vortex-lattice spanwise-mesh change (0.132207 to 0.132220), not an equation change. The A320-200 rows are a true same-input replay at 1.1e-13 kg.</p>")

    doc.append("<h2>8. Decision table</h2>")
    doc.append(table(decision_rows, ["discrepancy", "classification", "resolution", "evidence"]))

    doc.append("<h2>9. Remaining gaps and what would close them</h2><div class='box'><ul>")
    doc.append("<li>A configuration-matched OEW with an inclusion list for the registered A320-214 WV017 Sharklet aircraft, or an operator weight-and-balance record for the 77 t / 180Y aircraft behind the 41,052 kg sheet.</li>")
    doc.append("<li>Non-overlapping installed-engine masses (core, nacelle, pylon, mounts, starter, reverser, controls, fluids) for the CFM56-5B4/3 and PW1521G-3 installations; the FLOPS engine term is a boundary, not a pod.</li>")
    doc.append("<li>A narrowbody climb schedule for the A320 preset's operational defaults, and a review of the engine deck's thrust lapse at 5-9 km; until then the A320 fixed-aircraft mission does not close and no mission-level A320 comparison is possible.</li>")
    doc.append("<li>The strength-sized wing box on A320/A340/A380/DC-10 exceeds the empirical wing; a structural-model review (loads, materials, gauges) is a separate task. It no longer blocks the mass evaluation.</li>")
    doc.append("<li>One passenger-mass authority for the report and optimizer paths (class masses versus the 100 kg combined requirement).</li>")
    doc.append("<li>Calibration is deliberately not attempted: with two conditional OEW anchors there is no identifiable component parameter, and an OEW-level offset would be aircraft-specific.</li>")
    doc.append("</ul></div>")

    doc.append("<h2>10. Provenance</h2>")
    doc.append("<div class='scroll'><table><thead><tr><th>Artifact</th><th>SHA-256</th></tr></thead><tbody>" + "".join(f"<tr><td>{esc(k)}</td><td><code>{esc(v)}</code></td></tr>" for k, v in hashes.items()) + "</tbody></table></div>")
    doc.append("<p class='muted'>Baseline file hashes: <code>outputs/mass-model-consolidation/baseline/manifest.json</code>; frozen source tree copy: <code>" + esc(manifest["snapshot_dir"]) + "</code>.</p>")
    doc.append("</body>")
    REPORT.write_text("\n".join(doc), encoding="utf-8")
    print(json.dumps({"report": str(REPORT), "oew_rows": len(oew_rows), "component_rows": len(comp_rows), "parity_fail": len(parity_fail)}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
