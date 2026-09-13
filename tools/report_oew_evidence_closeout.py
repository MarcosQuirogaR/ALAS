#!/usr/bin/env python3
"""Build the OEW evidence-closeout CSVs, metrics and self-contained HTML.

Inputs (all produced from one frozen tree):

* ``outputs/oew-evidence-closeout/matrix-after/``          experiment matrix, production inputs
* ``outputs/oew-evidence-closeout/matrix-before-engine/``  same binary with ``--engine-fallback``
* ``outputs/oew-evidence-closeout/aviary-parity/``         same-state NASA/Aviary equation replay
* ``outputs/oew-evidence-closeout/evidence/external-evidence.json``
* ``outputs/mass-model-consolidation/after/raw.json``      the previous session's matrix (payload before the passenger-mass authority)
* ``outputs/oew-evidence-closeout/logs/*.log``             cargo test logs

Outputs, under ``outputs/oew-evidence-closeout/``: ``oew-reference-registry.csv``,
``conditional-comparison.csv``, ``before-after-oew.csv``, ``before-after-components.csv``,
``payload-policy-before-after.csv``, ``validation-metrics.json``, ``test-summary.json``,
``frozen-state-manifest.json`` and ``.agent/reports/oew-evidence-closeout.html``.

Usage (repository root)::

    uv run python tools/report_oew_evidence_closeout.py
"""

from __future__ import annotations

import csv
import hashlib
import html
import json
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "outputs" / "oew-evidence-closeout"
AFTER = OUT / "matrix-after"
BEFORE = OUT / "matrix-before-engine"
PARITY = OUT / "aviary-parity"
EVIDENCE = OUT / "evidence" / "external-evidence.json"
CONSOLIDATION_AFTER = ROOT / "outputs" / "mass-model-consolidation" / "after" / "raw.json"
REPORT = ROOT / ".agent" / "reports" / "oew-evidence-closeout.html"
GROUPS = ["Wing", "H-Stab", "V-Stab", "Fuselage", "Gear", "Propulsion", "Systems", "Furnishings"]
SOURCE_FILES = [
    "crates/alas-config/src/oew_reference.rs",
    "crates/alas-config/src/oew_reference/records.rs",
    "crates/alas-config/src/oew_reference/sources.rs",
    "crates/alas-config/src/preset_flops/structure.rs",
    "crates/alas-config/src/preset_flops/architecture.rs",
    "crates/alas-config/src/presets/narrowbody.rs",
    "crates/alas-config/src/presets/widebody_parts/part_01.rs",
    "crates/alas-config/src/presets/widebody_parts/part_02.rs",
    "crates/alas-config/src/presets/regional.rs",
    "crates/alas-config/src/cabin.rs",
    "crates/alas-config/src/sizing_basis.rs",
    "crates/alas-payload/src/build.rs",
    "crates/alas-payload/src/build/presets.rs",
    "crates/alas-opt/src/mdo/mda.rs",
    "crates/alas-opt/src/mdo/residuals.rs",
    "crates/alas-opt/src/mdo/sizing.rs",
    "crates/alas-opt/src/objective_model.rs",
    "crates/alas-mass/src/flops_transport/propulsion.rs",
    "crates/alas-mass/src/flops_transport/airframe.rs",
    "crates/alas-mass/src/flops_transport/structure.rs",
    "crates/alas-mass/src/flops_transport/equations.rs",
    "crates/alas-mass/src/breakdown/flops_methods.rs",
    "crates/alas-pipeline/src/full_analysis/cabin_sync.rs",
    "crates/alas-pipeline/src/full_analysis_parts/part_01.rs",
    "crates/alas-report/src/families/performance/lto.rs",
    "crates/alas-pipeline/examples/mass_experiment_matrix/main.rs",
    "crates/alas-pipeline/examples/mass_experiment_matrix/fixed.rs",
    "crates/alas-pipeline/examples/mass_experiment_matrix/mission.rs",
    "crates/alas-pipeline/examples/mass_experiment_matrix/rows.rs",
    "crates/alas-pipeline/examples/mass_experiment_matrix/support.rs",
    "crates/alas-mass/examples/flops_audit_inputs.rs",
    "crates/alas-mass/examples/flops_preset_comparison.rs",
    "crates/alas-config/tests/parity_aircraft_presets.rs",
    "crates/alas-acceptance/tests/acceptance_matrix.rs",
    "crates/alas-opt/tests/mission_sized.rs",
    "crates/alas-payload/tests/passenger_mass_authority.rs",
    "docs/mass-model-architecture.md",
    "docs/pure-flops-aircraft-evidence.md",
]


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


def esc(text) -> str:
    return html.escape("" if text is None else str(text))


def f(value, digits=1):
    if value is None or value == "":
        return "-"
    try:
        return f"{float(value):,.{digits}f}"
    except (TypeError, ValueError):
        return str(value)


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


# --------------------------------------------------------------------------- registry
def registry_rows(registry: list[dict]) -> list[dict]:
    rows = []
    for record in registry:
        cfg = record["reference_configuration"]
        source = record.get("source") or {}
        anchor = record.get("case_anchor") or {}
        anchor_source = anchor.get("source") or {}
        inclusion = record["inclusion"]
        included = [k for k, v in inclusion.items() if v == "included"]
        unknown = [k for k, v in inclusion.items() if v == "unknown"]
        rows.append({
            "preset": record["preset"],
            "applicability": record["applicability"],
            "counts_toward_validation": record["counts_toward_validation"],
            "reference_oew_kg": record.get("reference_oew_kg"),
            "definition_label": record["definition_label"],
            "reference_model": cfg["model"],
            "reference_weight_variant": cfg["weight_variant"],
            "reference_mtow_kg": cfg.get("mtow_kg"),
            "reference_engine": cfg["engine"],
            "reference_modification_state": cfg["modification_state"],
            "reference_cabin": cfg["cabin"],
            "differences_from_preset": " | ".join(record.get("differences_from_preset", [])),
            "inclusion_included": ";".join(included),
            "inclusion_unknown": ";".join(unknown),
            "source_document": source.get("document", ""),
            "source_publisher": source.get("publisher", ""),
            "source_revision": source.get("revision", ""),
            "source_date": source.get("date", ""),
            "source_locator": source.get("locator", ""),
            "source_url": source.get("url", ""),
            "source_local_path": source.get("local_path", ""),
            "source_retrieved": source.get("retrieved", ""),
            "source_tier": source.get("tier", ""),
            "source_quote": source.get("quote", ""),
            "uncertainty_kg": record.get("uncertainty_kg"),
            "case_anchor_label": anchor.get("case_label", ""),
            "case_anchor_kg": anchor.get("value_kg"),
            "case_anchor_tier": anchor_source.get("tier", ""),
            "case_anchor_document": anchor_source.get("document", ""),
            "case_anchor_uncertainty_kg": anchor.get("uncertainty_kg"),
            "other_published_values": " | ".join(f"{v['label']}: {v['value_kg']:,.0f} kg ({'OEW' if v['is_operating_empty'] else 'not OEW'})" for v in record.get("other_published_values", []) if v["value_kg"]),
            "structural_payload_basis_oew_kg": record.get("structural_payload_basis_oew_kg"),
            "notes": record["notes"],
        })
    return rows


# --------------------------------------------------------------------------- comparisons
def conditional_rows(after: dict, recon: list[dict], registry: list[dict]) -> list[dict]:
    rows = []
    by_case = {r["label"]: r for r in recon}
    for record in registry:
        name = record["preset"]
        fixed = after["presets"].get(name, {}).get("fixed_design_weight", {})
        model_oew = fixed.get("oew_kg")
        visible = record.get("visible_value_kg")
        case = record.get("visible_comparison_case")
        compared_oew = model_oew
        case_label = "fixed_design_weight"
        if case and case != "fixed_design_weight":
            case_label = case
            compared_oew = float(by_case[case]["oew_kg"]) if case in by_case and by_case[case].get("oew_kg") else None
        residual = (compared_oew - visible) if (compared_oew is not None and visible) else None
        rows.append({
            "preset": name,
            "applicability": record["applicability"],
            "tier": record.get("visible_tier"),
            "counts_toward_validation": record["counts_toward_validation"],
            "model_status": fixed.get("status", "not_evaluated"),
            "model_fixed_design_weight_oew_kg": model_oew,
            "comparison_case": case_label if visible else "",
            "compared_model_oew_kg": compared_oew if visible else None,
            "published_value_kg": visible,
            "residual_kg": residual,
            "residual_pct": (100.0 * residual / visible) if residual is not None else None,
            "uncertainty_kg": record.get("uncertainty_kg") or ((record.get("case_anchor") or {}).get("uncertainty_kg")),
            "definition": record["definition_label"] if record.get("reference_oew_kg") else ((record.get("case_anchor") or {}).get("configuration", {}).get("cabin", "")),
            "class": classify(record, fixed),
        })
    return rows


def classify(record: dict, fixed: dict) -> str:
    if fixed.get("status") not in (None, "ok"):
        return "unsupported_model" if record["applicability"] == "unsupported_model" else "model_failed"
    if record["applicability"] == "not_applicable":
        return "notional_no_reference"
    if record["applicability"] == "source_gap":
        tier = record.get("visible_tier")
        if tier == "aggregator":
            return "reference_gap_aggregator_anchor_only"
        return "reference_gap_conditional_anchor_only"
    if record["applicability"] == "conditional_mismatch":
        return "conditional_primary_reference"
    return record["applicability"]


def before_after_rows(before: dict, after: dict) -> list[dict]:
    rows = []
    for name, entry in after["presets"].items():
        a = entry["fixed_design_weight"]
        b = before["presets"].get(name, {}).get("fixed_design_weight", {})
        if a.get("status") != "ok":
            rows.append({"preset": name, "status": a.get("status"), "reason": a.get("reason", "")})
            continue
        rows.append({
            "preset": name,
            "status": "ok",
            "reason": "",
            "engine_term_before": b.get("buildup", {}).get("sources", {}).get("baseline_engine_mass"),
            "engine_term_after": a["buildup"]["sources"]["baseline_engine_mass"],
            "declared_baseline_engine_mass_kg": a.get("declared_baseline_engine_mass_kg"),
            "engine_each_before_kg": (b.get("buildup", {}).get("propulsion") or {}).get("engine_each_kg"),
            "engine_each_after_kg": (a["buildup"].get("propulsion") or {}).get("engine_each_kg"),
            "oew_before_kg": b.get("oew_kg"),
            "oew_after_kg": a["oew_kg"],
            "oew_delta_kg": (a["oew_kg"] - b["oew_kg"]) if b.get("oew_kg") is not None else None,
            "reference_oew_kg": a.get("reference_oew_kg"),
            "residual_before_kg": (b["oew_kg"] - a["reference_oew_kg"]) if (b.get("oew_kg") is not None and a.get("reference_oew_kg")) else None,
            "residual_after_kg": a.get("oew_residual_kg"),
            "payload_kg": a["buildup"]["masses_kg"]["Payload"],
            "actual_zfw_kg": a["actual_zfw_kg"],
            "reference_mzfw_kg": a.get("reference_mzfw_kg"),
            "signed_fuel_closure_kg": a["signed_fuel_closure_kg"],
            "usable_fuel_capacity_kg": a.get("usable_fuel_capacity_kg"),
        })
    return rows


def component_rows(before: dict, after: dict) -> list[dict]:
    rows = []
    for name, entry in after["presets"].items():
        a = entry["fixed_design_weight"]
        b = before["presets"].get(name, {}).get("fixed_design_weight", {})
        if a.get("status") != "ok" or b.get("status") != "ok":
            continue
        for group in GROUPS:
            before_kg = b["buildup"]["masses_kg"][group]
            after_kg = a["buildup"]["masses_kg"][group]
            reason = ""
            if abs(after_kg - before_kg) > 1e-6:
                if group == "Propulsion":
                    reason = "FLOPS WENGB declared from the certified dry engine mass (EASA TCDS) instead of the equation 76 THRSO/5.5 correlation"
                elif group == "Wing":
                    reason = "wing pod inertia relief (eqs. 40-41) reads the declared engine mass"
                else:
                    reason = "consequence of the declared engine mass"
            rows.append({"preset": name, "group": group, "before_kg": before_kg, "after_kg": after_kg, "delta_kg": after_kg - before_kg, "reason": reason, "uncertainty": "starter overlap about 20 kg/engine, nozzle omission about 50 kg/engine" if group == "Propulsion" and reason else ""})
    return rows


def payload_rows(consolidation: dict, after: dict) -> list[dict]:
    rows = []
    for name, entry in after["presets"].items():
        a = entry["fixed_design_weight"]
        c = consolidation["presets"].get(name, {}).get("fixed_design_weight", {})
        if a.get("status") != "ok" or c.get("status") != "ok":
            continue
        layout = a.get("layout") or {}
        clayout = c.get("layout") or {}
        rows.append({
            "preset": name,
            "seated_pax": layout.get("seated_pax"),
            "classes_after": "/".join(f"{n}:{s}" for n, s in (layout.get("classes") or [])),
            "payload_before_kg": c["buildup"]["masses_kg"]["Payload"],
            "payload_after_kg": a["buildup"]["masses_kg"]["Payload"],
            "delta_kg": a["buildup"]["masses_kg"]["Payload"] - c["buildup"]["masses_kg"]["Payload"],
            "seat_mass_after_t": layout.get("seat_mass_t"),
            "bag_mass_after_t": layout.get("bag_mass_t"),
            "combined_per_seat_after_kg": (1000.0 * (float(layout.get("seat_mass_t", 0)) + float(layout.get("bag_mass_t", 0))) / layout["seated_pax"]) if layout.get("seated_pax") else None,
            "oew_unchanged": abs(a["oew_kg"] - c["oew_kg"]) < 1e-6 or "engine input changed" if name in ("A320-200", "A220-300", "A380-800") else abs(a["oew_kg"] - c["oew_kg"]) < 1e-6,
        })
    return rows


def sweep_rows(after_sweep: list[dict]) -> list[dict]:
    by_preset: dict[str, list[dict]] = {}
    for row in after_sweep:
        by_preset.setdefault(row["preset"], []).append(row)
    rows = []
    for preset, entries in by_preset.items():
        oews = [float(r["oew_kg"]) for r in entries if r.get("oew_kg")]
        rows.append({"preset": preset, "ranges_nmi": ";".join(f"{float(r['range_nmi']):.0f}" for r in entries), "oew_spread_kg": (max(oews) - min(oews)) if oews else None, "statuses": ";".join(r["status"] for r in entries)})
    return rows


# --------------------------------------------------------------------------- metrics
def metrics(cond: list[dict]) -> dict:
    validated = [r for r in cond if r["counts_toward_validation"] is True]
    conditional_primary = [r for r in cond if r["class"] == "conditional_primary_reference" and r["residual_kg"] is not None]
    conditional_all = [r for r in cond if r["residual_kg"] is not None and r["class"] not in ("notional_no_reference",)]

    def stats(rows: list[dict]) -> dict:
        if not rows:
            return {"count": 0}
        res = [r["residual_kg"] for r in rows]
        pct = [r["residual_pct"] for r in rows]
        return {
            "count": len(rows),
            "presets": [r["preset"] for r in rows],
            "mean_signed_residual_kg": sum(res) / len(res),
            "mean_absolute_residual_kg": sum(abs(x) for x in res) / len(res),
            "mean_signed_residual_pct": sum(pct) / len(pct),
            "mean_absolute_residual_pct": sum(abs(x) for x in pct) / len(pct),
            "max_absolute_residual_pct": max(abs(x) for x in pct),
        }

    return {
        "policy": "Only configuration-matched records with a stated inclusion list may enter a validation metric; none exists, so the validated set is empty. Conditional statistics are descriptive of mismatched definitions and are not accuracy claims. No correlation coefficient is reported: eight rows against mixed definitions would measure scale, not accuracy.",
        "validated": stats(validated),
        "conditional_primary_tier_same_weight_variant": stats(conditional_primary),
        "conditional_all_visible_anchors": stats(conditional_all),
    }


def test_summary() -> dict:
    logs = OUT / "logs"
    summary = {}
    for path in sorted(logs.glob("test-*.log")):
        text = path.read_text(encoding="utf-8", errors="replace")
        results = re.findall(r"^test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored", text, flags=re.M)
        failed = re.findall(r"^test (\S+) \.\.\. FAILED", text, flags=re.M)
        summary[path.name] = {
            "suites": len(results),
            "passed": sum(int(r[1]) for r in results),
            "failed": sum(int(r[2]) for r in results),
            "ignored": sum(int(r[3]) for r in results),
            "failed_tests": sorted(set(failed)),
        }
    return summary


def frozen_manifest() -> dict:
    files = {p: sha256(ROOT / p) for p in SOURCE_FILES if (ROOT / p).is_file()}
    artifacts = {}
    for path in sorted(OUT.rglob("*")):
        if path.is_file() and path.suffix in (".json", ".csv", ".py") and "logs" not in path.parts:
            artifacts[str(path.relative_to(ROOT)).replace("\\", "/")] = sha256(path)
    diff = subprocess.check_output(["git", "diff", "--binary"], cwd=ROOT)
    return {
        "generated_utc": datetime.now(timezone.utc).isoformat(),
        "head": git("rev-parse", "HEAD"),
        "git_diff_sha256": hashlib.sha256(diff).hexdigest(),
        "toolchain": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "source_files": files,
        "artifacts": artifacts,
    }


# --------------------------------------------------------------------------- html
def table(rows: list[dict], columns: list[str], digits: dict | None = None) -> str:
    digits = digits or {}
    head = "".join(f"<th>{esc(c)}</th>" for c in columns)
    body = []
    for row in rows:
        cells = []
        for c in columns:
            value = row.get(c, "")
            if isinstance(value, bool):
                cells.append(f"<td>{'yes' if value else 'no'}</td>")
            elif isinstance(value, (int, float)) or (isinstance(value, str) and re.fullmatch(r"-?\d+(\.\d+)?", value or "x")):
                cells.append(f'<td class="num">{f(value, digits.get(c, 1))}</td>')
            else:
                cells.append(f"<td>{esc(value)}</td>")
        body.append("<tr>" + "".join(cells) + "</tr>")
    return f'<div class="scroll"><table><thead><tr>{head}</tr></thead><tbody>{"".join(body)}</tbody></table></div>'


def svg_residuals(cond: list[dict]) -> str:
    items = [r for r in cond if r.get("residual_pct") is not None]
    if not items:
        return ""
    width, left, top, row_h = 760, 130, 30, 30
    height = top + row_h * len(items) + 30
    scale = (width - left - 60) / 2.0 / 22.0
    zero = left + (width - left - 60) / 2.0
    parts = [f'<svg viewBox="0 0 {width} {height}" width="100%" role="img" aria-label="OEW residuals" style="max-width:{width}px;font-family:system-ui;font-size:12px">']
    parts.append(f'<text x="{left}" y="18" font-weight="600">Modelled minus published OEW, percent of the published value (all conditional)</text>')
    parts.append(f'<line x1="{zero}" y1="{top}" x2="{zero}" y2="{height - 24}" stroke="#555"/>')
    for i, r in enumerate(items):
        y = top + row_h * i
        pct = float(r["residual_pct"])
        x0, x1 = (zero + pct * scale, zero) if pct < 0 else (zero, zero + pct * scale)
        colour = "#7a7f87" if r["tier"] == "aggregator" else ("#2f6fb5" if r["class"] == "conditional_primary_reference" else "#b7791f")
        parts.append(f'<text x="{left - 8}" y="{y + 18}" text-anchor="end">{esc(r["preset"])}</text>')
        parts.append(f'<rect x="{x0:.1f}" y="{y + 6}" width="{max(x1 - x0, 1):.1f}" height="16" fill="{colour}"/>')
        parts.append(f'<text x="{(x0 - 6) if pct < 0 else (x1 + 6):.1f}" y="{y + 18}" text-anchor="{"end" if pct < 0 else "start"}">{pct:+.1f} % ({esc(r["comparison_case"])})</text>')
    parts.append(f'<text x="{left}" y="{height - 6}" fill="#555">blue: primary manufacturer document, same weight variant; amber: anchor for another configuration (reconstructed case); grey: aggregator value</text>')
    parts.append("</svg>")
    return "".join(parts)


def main() -> int:
    after = read_json(AFTER / "raw.json")
    before = read_json(BEFORE / "raw.json")
    registry = read_json(AFTER / "oew-reference-registry.json")
    recon_after = read_csv(AFTER / "reconstructed-cases.csv")
    recon_before = read_csv(BEFORE / "reconstructed-cases.csv")
    parity = read_json(PARITY / "audit.json")
    parity_rows = read_csv(PARITY / "component-comparison.csv")
    evidence = read_json(EVIDENCE)
    consolidation = read_json(CONSOLIDATION_AFTER) if CONSOLIDATION_AFTER.is_file() else {"presets": {}}
    sweeps = read_csv(AFTER / "mission-sweep.csv")
    failures = read_csv(AFTER / "failures.csv")
    climb = read_csv(AFTER / "climb-diagnosis.csv")

    reg_rows = registry_rows(registry)
    cond = conditional_rows(after, recon_after, registry)
    ba = before_after_rows(before, after)
    comp = component_rows(before, after)
    pay = payload_rows(consolidation, after)
    sweep = sweep_rows(sweeps)
    met = metrics(cond)
    tests = test_summary()
    manifest = frozen_manifest()

    write_csv(OUT / "oew-reference-registry.csv", reg_rows, list(reg_rows[0]))
    write_csv(OUT / "conditional-comparison.csv", cond, list(cond[0]))
    write_csv(OUT / "before-after-oew.csv", ba, ["preset", "status", "reason", "engine_term_before", "engine_term_after", "declared_baseline_engine_mass_kg", "engine_each_before_kg", "engine_each_after_kg", "oew_before_kg", "oew_after_kg", "oew_delta_kg", "reference_oew_kg", "residual_before_kg", "residual_after_kg", "payload_kg", "actual_zfw_kg", "reference_mzfw_kg", "signed_fuel_closure_kg", "usable_fuel_capacity_kg"])
    write_csv(OUT / "before-after-components.csv", comp, ["preset", "group", "before_kg", "after_kg", "delta_kg", "reason", "uncertainty"])
    if pay:
        write_csv(OUT / "payload-policy-before-after.csv", pay, list(pay[0]))
    write_csv(OUT / "sweep-invariance.csv", sweep, ["preset", "ranges_nmi", "oew_spread_kg", "statuses"])
    (OUT / "validation-metrics.json").write_text(json.dumps(met, indent=2) + "\n", encoding="utf-8")
    (OUT / "test-summary.json").write_text(json.dumps(tests, indent=2) + "\n", encoding="utf-8")
    (OUT / "frozen-state-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    recon_pairs = []
    before_by = {r["label"]: r for r in recon_before}
    for r in recon_after:
        b = before_by.get(r["label"], {})
        recon_pairs.append({**r, "oew_before_engine_kg": b.get("oew_kg"), "residual_before_engine_kg": b.get("oew_residual_kg")})

    css = """
    :root{--fg:#1d2430;--bg:#fbfbf9;--muted:#5b6672;--line:#d9dee5;--accent:#2f6fb5}
    body{margin:0;padding:24px 20px;background:var(--bg);color:var(--fg);font:14px/1.5 system-ui,Segoe UI,Roboto,sans-serif;max-width:1200px}
    h1{font-size:24px;margin:0 0 4px}h2{font-size:18px;margin:32px 0 8px;border-bottom:1px solid var(--line);padding-bottom:4px}h3{font-size:15px;margin:20px 0 6px}
    p,li{max-width:85ch}.muted{color:var(--muted)}.scroll{overflow-x:auto;margin:8px 0}
    table{border-collapse:collapse;font-size:12.5px;min-width:520px}th,td{border:1px solid var(--line);padding:4px 7px;text-align:left;vertical-align:top}th{background:#eef1f5}td.num{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}
    .box{border:1px solid var(--line);border-radius:6px;padding:10px 14px;margin:10px 0;background:#fff}code{background:#eef1f5;padding:0 4px;border-radius:3px}
    """
    now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%MZ")
    doc = [f"<!DOCTYPE html><html lang='en'><head><meta charset='utf-8'><title>OEW evidence closeout</title><style>{css}</style></head><body>"]
    doc.append("<h1>ALAS OEW evidence closeout</h1>")
    doc.append(f"<p class='muted'>Generated {now}. HEAD {esc(manifest['head'])}, uncommitted tree preserved (diff sha256 {esc(manifest['git_diff_sha256'][:16])}...), {esc(manifest['toolchain'])}. Regenerate: see section 11.</p>")

    doc.append("<h2>1. Outcome</h2><div class='box'><ul>")
    doc.append("<li><b>One OEW reference registry.</b> <code>alas_config::oew_reference</code> holds one record per registered preset (applicability, source tier, inclusion list, exact source locator and quote, uncertainty). Every preset's <code>reference.oew_kg</code>, the experiment matrix, the FLOPS comparison example and the parity/acceptance tests read it. No record is configuration matched, so the validated set is empty and every comparison below is conditional.</li>")
    doc.append("<li><b>Reference corrections established from primary documents.</b> A320: 41,244 kg is absent from Airbus ACAP Rev 46 (45,000 kg is empty weight for maintenance; 41,000 kg a rescue-drawing configuration); the 41,052 kg sheet is a 77 t / 180Y / wingtip-fence aircraft and is compared only through that reconstructed case. A220: 37,149 kg is the Airbus recovery-publication planning OEW with a stated inclusion list (ACP: 37,081 kg for the same 140-seat cabin). A340: 131,215 kg is the OEW printed on the ACAP jacking figure, weight variant and cabin unstated. DC-10: the ACAP Series 30 passenger column with its 572,000 lb footnote gives exactly the preset's 120,914 kg, 190,962 kg MLW and 46,008 kg structural payload; the earlier source-gap classification was wrong. A380 and B787-9 have no retained primary OEW page (aggregator / superseded attribution).</li>")
    doc.append("<li><b>One sourced general input correction.</b> FLOPS <code>WENGB</code> is a user input by definition; the certified dry engine mass is declared where the type-certificate data sheet states a scope that matches the FLOPS engine term (A320 CFM56-5B4/3 2,454.8 kg; A220 PW1521G-3 2,177 kg; A380 Trent 970-84 6,246 kg). A340 (dry weight includes the reverser), B787 (reverser scope unstated) and DC-10 (no value) keep the equation 76 correlation. OEW moves +456 kg (A320), +730 kg (A220), -134 kg (A380); the A380 change worsens its conditional residual and is kept.</li>")
    doc.append("<li><b>One passenger-mass authority.</b> <code>requirements.passenger_mass_kg</code> (100 kg combined) prices every seated passenger on every product path; class premiums are gone from the report path (787-9 payload -252 kg, A380 -588 kg); tests cover mixed cabins, bag mass, combined mass, declared-count cabins.</li>")
    doc.append("<li><b>Fixed-aircraft mission basis.</b> The landing limit of a fixed aircraft is its design landing mass in every MtowSizing mode; <code>Unconstrained</code> and <code>SizedByMission</code> are proven equivalent for a fixed aircraft whose mission closes below MTOW (B787-9) and the AVE landing limit no longer follows the dispatch mass.</li>")
    doc.append(f"<li><b>Implementation parity, same state.</b> NASA/Aviary equation replay on a deck and an ALAS record generated from this tree: {parity['rows']} rows, overall <b>{esc(parity['overall'])}</b> (A320 max {parity['per_preset']['A320-200']['max_abs_delta_kg']:.1e} kg, A220 max {parity['per_preset']['A220-300']['max_abs_delta_kg']:.1e} kg). The earlier 0.231 kg A220 wing difference was a stale frozen record against a regenerated deck, not a drift.</li>")
    doc.append("<li><b>What is not established.</b> Physical predictive accuracy. The widebody residuals (-13 to -19 % against typical-configuration or aggregator values) are not attributable to a component with the retained evidence; no calibration was fitted; the A320 fixed-aircraft mission still fails on the generic climb schedule (preserved, section 8).</li>")
    doc.append("</ul></div>")

    doc.append("<h2>2. The registry (all eight presets)</h2>")
    doc.append(table(reg_rows, ["preset", "applicability", "counts_toward_validation", "reference_oew_kg", "definition_label", "reference_weight_variant", "reference_cabin", "source_document", "source_revision", "source_locator", "source_tier", "uncertainty_kg", "case_anchor_label", "case_anchor_kg", "case_anchor_tier", "other_published_values", "structural_payload_basis_oew_kg"], {"reference_oew_kg": 0, "case_anchor_kg": 0, "uncertainty_kg": 0, "structural_payload_basis_oew_kg": 0}))
    doc.append("<h3>Inclusion lists and quotes</h3>")
    doc.append(table(reg_rows, ["preset", "inclusion_included", "inclusion_unknown", "source_quote", "differences_from_preset", "notes"]))

    doc.append("<h2>3. Conditional comparison (after the corrections)</h2>")
    doc.append("<p>Model OEW is the fixed-design-weight ledger at the declared MTOW in <code>BaselineSandbox</code>; where the registry's value belongs to another configuration, the comparison uses the named reconstructed case. Nothing in this table is a validation result.</p>")
    doc.append(svg_residuals(cond))
    doc.append(table(cond, ["preset", "class", "applicability", "tier", "model_status", "model_fixed_design_weight_oew_kg", "comparison_case", "compared_model_oew_kg", "published_value_kg", "residual_kg", "residual_pct", "uncertainty_kg", "definition"], {"residual_pct": 2, "uncertainty_kg": 0}))
    doc.append("<h3>Metrics, kept separate</h3>")
    doc.append(f"<pre>{esc(json.dumps(met, indent=2))}</pre>")

    doc.append("<h2>4. Engine input: before and after, per preset and per component</h2>")
    doc.append(table(ba, ["preset", "status", "engine_term_before", "engine_term_after", "declared_baseline_engine_mass_kg", "engine_each_before_kg", "engine_each_after_kg", "oew_before_kg", "oew_after_kg", "oew_delta_kg", "reference_oew_kg", "residual_before_kg", "residual_after_kg"], {"declared_baseline_engine_mass_kg": 1}))
    doc.append(table([r for r in comp if abs(r["delta_kg"]) > 1e-6], ["preset", "group", "before_kg", "after_kg", "delta_kg", "reason", "uncertainty"]))
    doc.append("<p class='muted'>Source and scope: NASA/TM-2017-219627 eqs. 75-80 (WENGB user input; THRSO/5.5 only when not input; WNAC is the nacelle or air-induction system); EASA TCDS E.003 Issue 06 p.11 and p.17, IM.E.090 Issue 10 p.7 and p.14, E.012 Issue 12 p.9, GEnx TCDS p.8 and p.19. The A220 was the pre-declared holdout: the rule was fixed from the source definitions, then its response recorded (+730 kg, residual -8.2 % to -6.3 %).</p>")

    doc.append("<h2>5. Reconstructed reference cases</h2>")
    doc.append(table(recon_pairs, ["label", "preset", "status", "declared_mtow_kg", "design_landing_mass_kg", "seated_pax", "unseated_pax", "flops_first", "flops_business", "flops_tourist", "oew_before_engine_kg", "oew_kg", "reference_oew_kg", "reference_status", "reference_tier", "residual_before_engine_kg", "oew_residual_kg", "oew_residual_pct"], {"declared_mtow_kg": 0, "design_landing_mass_kg": 0, "seated_pax": 0, "unseated_pax": 0, "flops_first": 0, "flops_business": 0, "flops_tourist": 0, "oew_residual_pct": 2}))
    for case in after.get("reconstructed_cases", []):
        doc.append(f"<div class='box'><b>{esc(case.get('label'))}</b><ul>" + "".join(f"<li>{esc(c)}</li>" for c in case.get("changes", [])) + f"</ul><p class='muted'>{esc(case.get('reference_note', ''))}</p></div>")

    doc.append("<h2>6. Passenger-mass authority: payload before and after</h2>")
    doc.append("<p>Before: the previous session's matrix (report path with per-class occupant masses). After: this tree. Operating empty mass is unaffected by the policy; the A320/A220/A380 OEW differences in this table come from the engine input of section 4.</p>")
    doc.append(table(pay, ["preset", "seated_pax", "classes_after", "payload_before_kg", "payload_after_kg", "delta_kg", "combined_per_seat_after_kg"], {"seated_pax": 0, "combined_per_seat_after_kg": 2}) if pay else "<p>consolidation baseline unavailable</p>")

    doc.append("<h2>7. Fixed-aircraft invariance and mode equivalence</h2>")
    doc.append(table(sweep, ["preset", "ranges_nmi", "oew_spread_kg", "statuses"], {"oew_spread_kg": 6}))
    doc.append("<p>Mission-case rows and the B787-9 Unconstrained/SizedByMission pair are in <code>matrix-after/mission-cases.csv</code>; the regression tests are <code>unconstrained_and_sized_by_mission_agree_on_a_fixed_aircraft_whose_mission_closes_below_mtow</code> and <code>ave_landing_limit_is_the_declared_mtow_fraction_in_every_mtow_sizing_mode_not_the_dispatch_mass</code>.</p>")

    doc.append("<h2>8. Preserved failures</h2>")
    doc.append(table([{"preset": r["preset"], "case": r["case"], "reason": r["reason"][:220]} for r in failures], ["preset", "case", "reason"]))
    doc.append(table(climb, ["preset", "variant", "status", "takeoff_mass_kg", "speed_reference", "step_climb_1_air_speed_m_s", "thrust_kn_per_engine", "dispatch_status"]))

    doc.append("<h2>9. Same-state NASA/Aviary replay</h2>")
    doc.append(f"<pre>{esc(json.dumps({k: parity[k] for k in ('overall', 'rows', 'per_preset', 'inputs')}, indent=2))}</pre>")
    doc.append(table([r for r in parity_rows if r["pass"].lower() != "true"], ["aircraft", "component", "alas_kg", "aviary_equation_replay_kg", "delta_kg"]) if any(r["pass"].lower() != "true" for r in parity_rows) else "<p>every row within 1e-6 kg</p>")

    doc.append("<h2>10. Tests and checks</h2>")
    doc.append(f"<pre>{esc(json.dumps(tests, indent=2))}</pre>")
    doc.append("<p>Pre-existing failures reproduced with this task's payload authority disabled (bisect) and documented in the handoff: alas-pipeline lib <code>default_brief_seating_shortfall_is_a_reported_finding_not_a_valid_finalist</code>, four <code>finalist_selection_matches_report</code> tests and <code>fixed_aircraft_mass_basis::a_declared_count_cabin_is_seated_as_declared_and_priced_as_declared</code> (geometry-side); the acceptance matrix's four known pins (A220 model-frame CG, A380 envelope, A320 landing-mass finding, ATR execution). The A220 CG pin's actual value moved with the engine input and is reported, not re-pinned.</p>")

    doc.append("<h2>11. Evidence and reproducibility</h2>")
    doc.append(table(evidence["records"], ["id", "aircraft_or_engine", "quantity", "value", "unit", "status", "document", "revision", "locator", "url_or_local_path"]))
    doc.append("<pre>" + esc("\n".join([
        "CARGO_TARGET_DIR=.agent/mass-target cargo build --release -p alas-pipeline --example mass_experiment_matrix -p alas-mass --example flops_audit_inputs --example flops_preset_comparison",
        ".agent/mass-target/release/examples/mass_experiment_matrix.exe outputs/oew-evidence-closeout/matrix-after",
        ".agent/mass-target/release/examples/mass_experiment_matrix.exe outputs/oew-evidence-closeout/matrix-before-engine --engine-fallback",
        ".agent/mass-target/release/examples/flops_audit_inputs.exe outputs/oew-evidence-closeout/aviary-parity/alas-inputs.json",
        ".agent/mass-target/release/examples/flops_preset_comparison.exe outputs/oew-evidence-closeout/aviary-parity/raw.json",
        "uv run python outputs/oew-evidence-closeout/aviary-parity/replay_same_state.py",
        "uv run python tools/report_oew_evidence_closeout.py",
        "CARGO_TARGET_DIR=.agent/mass-target-dbg cargo test -p alas-config -p alas-payload -p alas-opt -p alas-mass; cargo test -p alas-pipeline --tests --no-fail-fast; cargo test -p alas-acceptance --test acceptance_matrix",
    ])) + "</pre>")
    doc.append("<div class='scroll'><table><thead><tr><th>File</th><th>SHA-256</th></tr></thead><tbody>" + "".join(f"<tr><td>{esc(k)}</td><td><code>{esc(v)}</code></td></tr>" for k, v in {**manifest['source_files'], **manifest['artifacts']}.items()) + "</tbody></table></div>")
    doc.append("</body></html>")
    REPORT.write_text("\n".join(doc), encoding="utf-8")
    print(json.dumps({"report": str(REPORT), "conditional_rows": len(cond), "metrics": met["validated"], "tests": {k: (v["passed"], v["failed"]) for k, v in tests.items()}}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
