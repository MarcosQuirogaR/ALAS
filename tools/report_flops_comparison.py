#!/usr/bin/env python3
"""Render the pure-FLOPS production comparison and its audit artifacts.

The input is the schema-version-2 JSON emitted by
``flops_preset_comparison``.  The report treats the pure FLOPS result as the
primary method, places the legacy control second, and keeps manufacturer
reference values as contextual anchors.  It performs arithmetic and schema
checks only; no calibration or physical-validation score is inferred from a
reference delta.

Usage::

    python tools/report_flops_comparison.py \
        outputs/pure-flops-production/raw.json \
        outputs/pure-flops-production

The HTML report is written to ``out/reports/pure-flops-production.html``
unless a third path is supplied.
"""

from __future__ import annotations

import csv
import hashlib
import html
import json
import math
import platform
import sys
from pathlib import Path
from typing import Any

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt


COMPONENTS = [
    ("Wing", "Wing"),
    ("H-Stab", "Horizontal tail"),
    ("V-Stab", "Vertical tail"),
    ("Fuselage", "Fuselage"),
    ("Gear", "Landing gear"),
    ("Propulsion", "Installed propulsion and nacelles"),
    ("Systems", "Systems less furnishings"),
    ("Furnishings", "Furnishings and operating items"),
    ("Payload", "Planning payload"),
    ("Fuel", "Signed fuel closure"),
]
COLORS = [
    "#2c7da0",
    "#59a5b8",
    "#78c091",
    "#f2b134",
    "#e07a5f",
    "#9b5de5",
    "#577590",
    "#f15bb5",
    "#7f8c8d",
    "#34495e",
]
EPS = 1.0e-6


def number(value: Any, scale: float = 1.0, digits: int = 2) -> str:
    """Format an optional finite number for a report table."""

    if value is None or not isinstance(value, (int, float)):
        return "—"
    if not math.isfinite(float(value)):
        return "non-finite"
    return f"{float(value) / scale:,.{digits}f}"


def percent(value: Any) -> str:
    if value is None or not isinstance(value, (int, float)):
        return "—"
    if not math.isfinite(float(value)):
        return "non-finite"
    return f"{float(value):+.2f}%"


def esc(value: Any) -> str:
    return html.escape(str(value))


def table(headers: list[str], rows: list[list[Any]], class_name: str = "") -> str:
    cls = f' class="{class_name}"' if class_name else ""
    head = "".join(f"<th>{esc(item)}</th>" for item in headers)
    body = "".join(
        "<tr>" + "".join(f"<td>{item}</td>" for item in row) + "</tr>"
        for row in rows
    )
    return f'<div class="scroll"><table{cls}><thead><tr>{head}</tr></thead><tbody>{body}</tbody></table></div>'


def mass_map(method: dict[str, Any] | None) -> dict[str, float] | None:
    if not method or not isinstance(method.get("masses_kg"), dict):
        return None
    return method["masses_kg"]


def oew(method: dict[str, Any] | None) -> float | None:
    if not method or not isinstance(method.get("ledger"), dict):
        return None
    value = method["ledger"].get("oew_kg")
    return float(value) if isinstance(value, (int, float)) else None


def reference_oew(row: dict[str, Any]) -> float | None:
    value = row.get("reference", {}).get("oew_kg")
    return float(value) if isinstance(value, (int, float)) else None


def reference_anchor(row: dict[str, Any], field: str) -> dict[str, Any] | None:
    evidence = row.get("reference", {}).get("mass_reference_evidence")
    anchor = evidence.get(field) if isinstance(evidence, dict) else None
    return anchor if isinstance(anchor, dict) else None


def reference_anchor_status(row: dict[str, Any], field: str) -> str | None:
    anchor = reference_anchor(row, field)
    value = anchor.get("status") if anchor else None
    return str(value) if value is not None else None


def reference_anchor_source(row: dict[str, Any], field: str) -> str | None:
    anchor = reference_anchor(row, field)
    source = anchor.get("source") if anchor else None
    if isinstance(source, dict):
        value = source.get("id")
        return str(value) if value is not None else None
    return str(source) if source is not None else None


def reference_anchor_uncertainty(row: dict[str, Any], field: str) -> str | None:
    anchor = reference_anchor(row, field)
    value = anchor.get("uncertainty") if anchor else None
    return str(value) if value is not None else None


def percent_delta(value: float | None, baseline: float | None) -> float | None:
    if value is None or baseline is None or baseline == 0.0:
        return None
    return 100.0 * (value - baseline) / baseline


def svg(fig: Any, path: Path) -> str:
    fig.tight_layout()
    fig.savefig(path, format="svg", bbox_inches="tight")
    png = path.with_suffix(".png")
    fig.savefig(png, format="png", dpi=180, bbox_inches="tight")
    plt.close(fig)
    return path.name


def validate(data: dict[str, Any]) -> list[dict[str, Any]]:
    """Return machine-readable checks without hiding failed evidence."""

    checks: list[dict[str, Any]] = []

    def check(name: str, passed: bool, detail: str) -> None:
        checks.append({"check": name, "passed": bool(passed), "detail": detail})

    rows = data.get("aircraft")
    check("schema_version", data.get("schema_version") == 2, "schema_version must be 2")
    check("aircraft_count", isinstance(rows, list) and len(rows) == 8, "eight registered presets")
    check(
        "production_architecture_declaration",
        data.get("model", {}).get("production_architecture") == "pure_flops_transport_v1",
        "production architecture is pure FLOPS",
    )
    check(
        "legacy_architecture_declaration",
        data.get("model", {}).get("legacy_architecture")
        == "legacy_reference_compatible_comparison",
        "legacy is comparison-only",
    )
    if not isinstance(rows, list):
        return checks

    names = {row.get("preset") for row in rows if isinstance(row, dict)}
    check("unique_preset_names", len(names) == len(rows), "preset names are unique")
    evidence_rows = [
        row.get("reference", {}).get("mass_reference_evidence")
        for row in rows
        if isinstance(row, dict)
    ]
    check(
        "mass_reference_evidence_embedded",
        len(evidence_rows) == 8 and all(isinstance(value, dict) for value in evidence_rows),
        "each preset embeds the revision-locked mass anchor record",
    )
    for row in rows:
        name = row.get("preset", "unknown")
        production = row.get("production") or {}
        legacy = row.get("legacy_comparison") or {}
        if production.get("status") == "evaluated_declared_inputs":
            masses = mass_map(production)
            ledger = production.get("ledger") or {}
            if masses is None:
                check(f"{name}_production_mass_map", False, "evaluated result has no mass map")
                continue
            empty = sum(float(masses.get(component, 0.0)) for component, _ in COMPONENTS[:8])
            ledger_oew = ledger.get("oew_kg")
            check(
                f"{name}_production_oew_sum",
                isinstance(ledger_oew, (int, float)) and abs(empty - float(ledger_oew)) <= EPS,
                f"component OEW sum={empty:.9g}, ledger={ledger_oew}",
            )
            zfw = ledger.get("actual_zfw_kg")
            headroom = ledger.get("signed_fuel_headroom_kg")
            mtow = ledger.get("mtow_kg")
            check(
                f"{name}_production_mtow_closure",
                all(isinstance(value, (int, float)) for value in (zfw, headroom, mtow))
                and abs(float(zfw) + float(headroom) - float(mtow)) <= EPS,
                f"ZFW + headroom = MTOW ({zfw} + {headroom} vs {mtow})",
            )
            signed = ledger.get("signed_fuel_closure_kg")
            check(
                f"{name}_production_fuel_closure",
                isinstance(signed, (int, float))
                and isinstance(headroom, (int, float))
                and abs(float(signed) - float(headroom)) <= EPS,
                f"signed fuel closure={signed}, headroom={headroom}",
            )
            check(
                f"{name}_production_pure_architecture",
                production.get("mass_architecture") == "pure_flops_transport_v1",
                "production result must be pure FLOPS",
            )
        else:
            blockers = production.get("blockers") or []
            check(
                f"{name}_unsupported_is_explicit",
                production.get("masses_kg") is None
                and isinstance(blockers, list)
                and len(blockers) > 0,
                "unsupported/unverified result has no fabricated total and names blockers",
            )
        legacy_masses = mass_map(legacy)
        check(
            f"{name}_legacy_control_present",
            legacy.get("status") == "evaluated_comparison_control"
            and legacy.get("mass_architecture") == "legacy_reference_compatible_comparison"
            and legacy_masses is not None,
            "legacy result is explicitly selected and complete",
        )

    ave = next((row for row in rows if row.get("preset") == "AVE"), None)
    check(
        "ave_no_actual_aircraft_data",
        isinstance(ave, dict)
        and ave.get("reference", {}).get("actual_aircraft_data") is False,
        "AVE is explicitly marked as notional",
    )
    atr = next((row for row in rows if row.get("preset") == "ATR72-600"), None)
    atr_production = atr.get("production", {}) if isinstance(atr, dict) else {}
    check(
        "atr_unsupported_propulsion",
        atr_production.get("masses_kg") is None
        and "unsupported_propulsion_technology" in (atr_production.get("blockers") or []),
        "ATR has no fabricated jet-equivalent FLOPS mass",
    )
    dc10 = next((row for row in rows if row.get("preset") == "DC-10"), None)
    dc10_reference = dc10.get("reference", {}) if isinstance(dc10, dict) else {}
    dc10_anchor = reference_anchor(dc10, "oew_kg") if isinstance(dc10, dict) else None
    check(
        "dc10_oew_source_gap_is_honest",
        dc10_reference.get("oew_kg") is None
        and isinstance(dc10_anchor, dict)
        and dc10_anchor.get("status") == "source_gap",
        "DC-10 OEW remains null when the retained source definition is a gap",
    )
    return checks


def build_rows(data: dict[str, Any]) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    summary_rows: list[dict[str, Any]] = []
    component_rows: list[dict[str, Any]] = []
    for row in data["aircraft"]:
        production = row.get("production") or {}
        legacy = row.get("legacy_comparison") or {}
        pmap = mass_map(production)
        lmap = mass_map(legacy) or {}
        p_oew = oew(production)
        l_oew = oew(legacy)
        r_oew = reference_oew(row)
        summary_rows.append(
            {
                "preset": row.get("preset"),
                "model": row.get("reference", {}).get("identity", {}).get("model"),
                "reference_oew_kg": r_oew,
                "reference_mtow_kg": row.get("reference", {}).get("mtow_kg"),
                "reference_data_status": row.get("reference", {}).get("data_status"),
                "reference_oew_status": reference_anchor_status(row, "oew_kg"),
                "reference_oew_source": reference_anchor_source(row, "oew_kg"),
                "reference_oew_uncertainty": reference_anchor_uncertainty(row, "oew_kg"),
                "production_status": production.get("status"),
                "production_oew_kg": p_oew,
                "production_zfw_kg": (production.get("ledger") or {}).get("actual_zfw_kg"),
                "production_fuel_headroom_kg": (production.get("ledger") or {}).get(
                    "signed_fuel_headroom_kg"
                ),
                "legacy_oew_kg": l_oew,
                "legacy_zfw_kg": (legacy.get("ledger") or {}).get("actual_zfw_kg"),
                "legacy_fuel_headroom_kg": (legacy.get("ledger") or {}).get(
                    "signed_fuel_headroom_kg"
                ),
                "production_minus_reference_oew_kg": None
                if p_oew is None or r_oew is None
                else p_oew - r_oew,
                "production_minus_reference_oew_pct": percent_delta(p_oew, r_oew),
                "legacy_minus_reference_oew_kg": None
                if l_oew is None or r_oew is None
                else l_oew - r_oew,
                "legacy_minus_reference_oew_pct": percent_delta(l_oew, r_oew),
                "production_minus_legacy_oew_kg": None
                if p_oew is None or l_oew is None
                else p_oew - l_oew,
            }
        )
        for component, label in COMPONENTS:
            p_value = pmap.get(component) if pmap else None
            l_value = lmap.get(component)
            component_rows.append(
                {
                    "preset": row.get("preset"),
                    "component": component,
                    "label": label,
                    "production_kg": p_value,
                    "legacy_kg": l_value,
                    "production_minus_legacy_kg": None
                    if p_value is None or l_value is None
                    else float(p_value) - float(l_value),
                }
            )
    return summary_rows, component_rows


def write_csvs(
    output: Path, summary_rows: list[dict[str, Any]], component_rows: list[dict[str, Any]]
) -> None:
    summary_fields = list(summary_rows[0]) if summary_rows else []
    with (output / "comparison.csv").open("w", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=summary_fields)
        writer.writeheader()
        writer.writerows(summary_rows)
    component_fields = list(component_rows[0]) if component_rows else []
    with (output / "component-comparison.csv").open("w", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=component_fields)
        writer.writeheader()
        writer.writerows(component_rows)


def make_figures(output: Path, data: dict[str, Any], summary_rows: list[dict[str, Any]]) -> dict[str, str]:
    names = [row["preset"] for row in summary_rows]
    positions = list(range(len(names)))
    figures: dict[str, str] = {}

    fig, ax = plt.subplots(figsize=(11.0, 5.4))
    ref = [row["reference_oew_kg"] / 1000 if row["reference_oew_kg"] is not None else math.nan for row in summary_rows]
    prod = [row["production_oew_kg"] / 1000 if row["production_oew_kg"] is not None else math.nan for row in summary_rows]
    legacy = [row["legacy_oew_kg"] / 1000 if row["legacy_oew_kg"] is not None else math.nan for row in summary_rows]
    width = 0.24
    ax.bar([p - width for p in positions], ref, width, color="#7f8c8d", label="Reference OEW")
    ax.bar(positions, prod, width, color="#2c7da0", label="Pure FLOPS production")
    ax.bar([p + width for p in positions], legacy, width, color="#d68735", label="Legacy comparison")
    ax.set_xticks(positions, names, rotation=25, ha="right")
    ax.set_ylabel("Operating empty mass [t]")
    ax.set_title("Pure FLOPS production, reference anchors and legacy control", loc="left", pad=14)
    ax.grid(axis="y", alpha=0.22)
    ax.set_axisbelow(True)
    ax.legend(frameon=False, ncols=3, loc="upper left")
    figures["oew-reference-comparison.svg"] = svg(fig, output / "oew-reference-comparison.svg")

    fig, ax = plt.subplots(figsize=(11.0, 5.0))
    ref_prod = [
        row["production_minus_reference_oew_kg"] / 1000
        if row["production_minus_reference_oew_kg"] is not None
        else math.nan
        for row in summary_rows
    ]
    ref_legacy = [
        row["legacy_minus_reference_oew_kg"] / 1000
        if row["legacy_minus_reference_oew_kg"] is not None
        else math.nan
        for row in summary_rows
    ]
    ax.axhline(0.0, color="#34495e", linewidth=0.8)
    ax.bar([p - width for p in positions], ref_prod, width, color="#2c7da0", label="Pure FLOPS − reference")
    ax.bar([p + width for p in positions], ref_legacy, width, color="#d68735", label="Legacy − reference")
    ax.set_xticks(positions, names, rotation=25, ha="right")
    ax.set_ylabel("OEW difference [t]")
    ax.set_title("Reference deltas are descriptive evidence, not an accuracy score", loc="left", pad=14)
    ax.grid(axis="y", alpha=0.22)
    ax.set_axisbelow(True)
    ax.legend(frameon=False, ncols=2, loc="upper left")
    figures["oew-reference-deltas.svg"] = svg(fig, output / "oew-reference-deltas.svg")

    fig, ax = plt.subplots(figsize=(12.0, 6.4))
    for index, row in enumerate(data["aircraft"]):
        production = row.get("production") or {}
        legacy_method = row.get("legacy_comparison") or {}
        for offset, method, label, color in (
            (-0.19, production, "Pure FLOPS production", "#2c7da0"),
            (0.19, legacy_method, "Legacy comparison", "#d68735"),
        ):
            values = mass_map(method)
            if values is None:
                ax.text(0.0, index + offset, "unsupported / no fabricated total", va="center", fontsize=8, color="#9c3c36")
                continue
            left = 0.0
            for (component, _), color_component in zip(COMPONENTS, COLORS):
                value = float(values.get(component, 0.0)) / 1000.0
                ax.barh(index + offset, value, left=left, height=0.30, color=color_component, edgecolor="white", linewidth=0.4)
                left += value
            ax.text(left + 1.0, index + offset, f"{left:.1f} t", va="center", fontsize=8)
    handles = [plt.Rectangle((0, 0), 1, 1, color=color) for color in COLORS]
    labels = [label for _, label in COMPONENTS]
    ax.set_yticks(positions, names)
    ax.invert_yaxis()
    ax.set_xlabel("Component mass [t]")
    ax.set_title("Component ownership by mass architecture", loc="left", pad=14)
    ax.grid(axis="x", alpha=0.2)
    ax.set_axisbelow(True)
    ax.legend(handles, labels, frameon=False, ncols=2, bbox_to_anchor=(0, -0.16), loc="upper left", fontsize=8)
    figures["component-stacks.svg"] = svg(fig, output / "component-stacks.svg")
    return figures


def html_report(
    input_path: Path,
    output: Path,
    report_path: Path,
    data: dict[str, Any],
    checks: list[dict[str, Any]],
    summary_rows: list[dict[str, Any]],
    component_rows: list[dict[str, Any]],
    figures: dict[str, str],
) -> None:
    passing = sum(1 for check in checks if check["passed"])
    failed = [check for check in checks if not check["passed"]]
    summary_table = [
        [
            esc(row["preset"]),
            esc(row["production_status"]),
            number(row["production_oew_kg"], 1000),
            number(row["reference_oew_kg"], 1000),
            number(row["production_minus_reference_oew_kg"], 1000),
            percent(row["production_minus_reference_oew_pct"]),
            number(row["legacy_oew_kg"], 1000),
            number(row["legacy_minus_reference_oew_kg"], 1000),
            percent(row["legacy_minus_reference_oew_pct"]),
        ]
        for row in summary_rows
    ]
    reference_table = [
        [
            esc(row["preset"]),
            esc(row["model"]),
            esc(row["reference_data_status"]),
            esc(row["reference_oew_status"] or "—"),
            esc(row["reference_oew_source"] or "—"),
            number(row["reference_mtow_kg"]),
            number(row["reference_oew_kg"]),
            esc(row["reference_oew_uncertainty"] or "—"),
            esc(
                next(
                    (
                        aircraft.get("reference", {}).get("design_mission", {}).get("status")
                        for aircraft in data["aircraft"]
                        if aircraft.get("preset") == row["preset"]
                    ),
                    "—",
                )
            ),
        ]
        for row in summary_rows
    ]
    component_table = [
        [
            esc(row["preset"]),
            esc(row["label"]),
            number(row["production_kg"]),
            number(row["legacy_kg"]),
            number(row["production_minus_legacy_kg"]),
        ]
        for row in component_rows
    ]
    ledger_table = [
        [
            esc(row["preset"]),
            number(row["production_zfw_kg"]),
            number(row["production_fuel_headroom_kg"]),
            number(row["legacy_zfw_kg"]),
            number(row["legacy_fuel_headroom_kg"]),
        ]
        for row in summary_rows
    ]
    check_table = [
        [
            "PASS" if check["passed"] else "FAIL",
            esc(check["check"]),
            esc(check["detail"]),
        ]
        for check in checks
    ]
    rules = data.get("summary", {}).get("notes", [])
    figure_markup = "".join(
        f'<figure><img src="../outputs/pure-flops-production/{esc(name)}" alt="{esc(name)}"><figcaption>{esc(name)}</figcaption></figure>'
        for name in figures
    )
    # The report lives in out/reports, one directory above outputs/.
    # Relative links therefore make the artifact portable inside the checkout.
    figure_markup = figure_markup.replace("../outputs", "../../outputs")
    source_links = data.get("sources", {})
    source_html = "<ul>" + "".join(
        f'<li><strong>{esc(key)}:</strong> {esc(value)}</li>' for key, value in source_links.items()
    ) + "</ul>"
    failure_html = ""
    if failed:
        failure_html = '<div class="alert"><strong>Failed report checks.</strong> The numerical artifacts are retained for diagnosis; this report is not a clean verification result.</div>'
    css = """
    :root{font-family:Inter,ui-sans-serif,system-ui,sans-serif;color:#18323d;background:#edf3f5}
    body{margin:0;padding:24px}main{max-width:1360px;margin:auto;background:#fff;border:1px solid #d1e0e5;border-radius:12px;padding:36px;box-shadow:0 12px 38px #21434b18}
    h1{font-size:34px;line-height:1.15;margin:0 0 8px}h2{border-top:2px solid #d8e6ea;padding-top:24px;margin-top:44px}h3{margin-top:30px}.lede{font-size:18px;color:#45616c}.sub{color:#607983}.alert{background:#fff4df;border-left:4px solid #d68735;padding:14px;margin:18px 0}.good{background:#e9f6ef;border-left:4px solid #3a9d70;padding:14px;margin:18px 0}
    .scroll{overflow-x:auto}table{width:100%;border-collapse:collapse;margin:16px 0;font-size:13px}th,td{border:1px solid #cddde2;padding:8px;text-align:right;vertical-align:top}th:first-child,td:first-child{text-align:left}th{background:#e5f0f3}td:nth-child(2){text-align:left}figure{margin:28px 0}figure img{display:block;max-width:100%;height:auto;border:1px solid #d3e1e5;border-radius:6px}figcaption{font-size:12px;color:#607983;margin-top:6px}code,pre{background:#eef3f5;border-radius:4px}code{padding:2px 4px}pre{padding:14px;overflow:auto;font-size:12px}.small{font-size:12px;color:#607983}nav a{margin-right:14px;color:#006f88}@media(max-width:800px){body{padding:8px}main{padding:18px}h1{font-size:28px}}
    """
    source_text = json.dumps(source_links, indent=2, ensure_ascii=False)
    doc = f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Pure FLOPS production comparison</title><style>{css}</style></head><body><main>
<h1>Pure FLOPS production mass comparison</h1>
<p class="sub">Schema v2 · Eight registered presets · Generated by the Rust production entry point</p>
<p class="lede">The production result is the complete NASA FLOPS conventional-transport buildup. The legacy reference-compatible buildup is shown as a secondary comparison control. Both use the same preset geometry, requirements and payload target.</p>
{failure_html}
<div class="good"><strong>Evidence boundary.</strong> Reference values are manufacturer or certification anchors when present. A reference delta describes the difference between two records; it is not a calibration score or a physical-validation result. Equation parity is numerical verification and does not establish aircraft accuracy.</div>
<h2>Production first</h2>
{table(["Preset","Production status","Pure FLOPS OEW [t]","Reference OEW [t]","Pure FLOPS − ref [t]","Pure FLOPS − ref [%]","Legacy OEW [t]","Legacy − ref [t]","Legacy − ref [%]"], summary_table)}
<p class="small">A blank production mass means the architecture refused to publish a total. ATR72-600 is expected to be present in that state because the pinned NASA memorandum has no propeller or shaft-power mass equation.</p>
<h2>Reference evidence and coverage</h2>
{table(["Preset","Model","Reference data status","OEW anchor status","OEW source","MTOW [kg]","OEW [kg]","OEW uncertainty","Design mission"], reference_table)}
<p>AVE is explicitly marked as notional and has no actual-aircraft dataset. Other rows retain the registered variant identity, source-document list, mission evidence status and partial mission gaps in the raw JSON. The OEW status and source columns come from the revision-locked mass-anchor artifact; a <code>source_gap</code> remains blank rather than inheriting a less-specific preset value. No missing design mission has been filled from an airport route, cruise Mach, payload target or marketing range.</p>
<h2>Ledger checks</h2>
{table(["Preset","Production ZFW [kg]","Production fuel headroom [kg]","Legacy ZFW [kg]","Legacy fuel headroom [kg]"], ledger_table)}
<p>The signed fuel closure is <code>MTOW − OEW − payload</code>. ZFW is <code>OEW + planning payload</code>; MZFW is retained as a separate published limit. A negative closure remains a diagnostic and is never converted into a physical negative fuel load.</p>
<h2>Figures</h2>
{figure_markup}
<h2>Component ownership</h2>
<p>The pure result maps FLOPS structural and transport groups into the ten ALAS slots. Nacelles are charged to propulsion exactly once; furnishings and operating items are carried in the furnishings slot; the systems slot excludes furnishings. The legacy column is kept for control evidence and does not feed production.</p>
{table(["Preset","Component","Pure FLOPS [kg]","Legacy [kg]","Pure FLOPS − legacy [kg]"], component_table)}
<h2>Automated arithmetic and contract checks</h2>
{table(["State","Check","Detail"], check_table)}
<p class="{'good' if not failed else 'alert'}"><strong>{passing}/{len(checks)} checks passed.</strong> Input SHA256: <code>{hashlib.sha256(input_path.read_bytes()).hexdigest()}</code>.</p>
<h2>Sources and reproducibility</h2>
<p>Primary source links and local evidence pointers:</p>{source_html}
<pre>cargo run -p alas-mass --example flops_preset_comparison -- outputs/pure-flops-production/raw.json &lt;path-to-local-evidence-copy&gt;/mass_reference_anchors.json
python tools/report_flops_comparison.py outputs/pure-flops-production/raw.json outputs/pure-flops-production
</pre>
<p class="small">Python {esc(platform.python_version())}; matplotlib {esc(matplotlib.__version__)}. The report performs schema, conservation and architecture-label checks. It does not claim physical validation, fit accuracy, or certification.</p>
<p class="small">Raw input: <code>{esc(input_path)}</code><br>Generated HTML: <code>{esc(report_path)}</code></p>
</main></body></html>"""
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(doc, encoding="utf-8")
    audit = {
        "checks": checks,
        "passed": passing,
        "failed": failed,
        "input_sha256": hashlib.sha256(input_path.read_bytes()).hexdigest(),
        "python": platform.python_version(),
        "matplotlib": matplotlib.__version__,
        "scope": "schema, arithmetic and architecture contract checks; equation parity and physical validation are separate evidence",
    }
    (output / "report-checks.json").write_text(json.dumps(audit, indent=2) + "\n", encoding="utf-8")


def render(input_path: Path, output: Path, report_path: Path) -> dict[str, Any]:
    data = json.loads(input_path.read_text(encoding="utf-8"))
    output.mkdir(parents=True, exist_ok=True)
    checks = validate(data)
    summary_rows, component_rows = build_rows(data)
    write_csvs(output, summary_rows, component_rows)
    figures = make_figures(output, data, summary_rows)
    html_report(input_path, output, report_path, data, checks, summary_rows, component_rows, figures)
    result = {
        "report": str(report_path),
        "output": str(output),
        "checks_passed": sum(1 for check in checks if check["passed"]),
        "checks_total": len(checks),
        "failed_checks": [check for check in checks if not check["passed"]],
        "figures": [str(output / name) for name in figures],
    }
    (output / "report-result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return result


def main() -> int:
    if len(sys.argv) not in (3, 4):
        print(__doc__, file=sys.stderr)
        return 2
    input_path = Path(sys.argv[1])
    output = Path(sys.argv[2])
    report_path = (
        Path(sys.argv[3])
        if len(sys.argv) == 4
        else Path(__file__).resolve().parents[1] / "out/reports/pure-flops-production.html"
    )
    result = render(input_path, output, report_path)
    print(json.dumps(result, indent=2))
    return 1 if result["failed_checks"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
