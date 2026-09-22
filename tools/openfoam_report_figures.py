"""Create reproducible report-study figures without third-party plotting code.

The bundled runtime used for this project does not include matplotlib, so the
small renderer below uses Pillow for raster output and emits matching SVG
polylines.  It consumes only the report-study ``study.json``/``result.json``
files and native OpenFOAM raw surface samples.  Every plotted point is tagged
with its case status; finite provisional values remain visible.

    python tools/openfoam_report_figures.py CASE_ROOT

The output contains light, dark-accessible, and grey-accessible PNG/SVG files,
CSV summaries, and a concise Markdown evidence report.  These are numerical
post-processing views, not physical validation.
"""

from __future__ import annotations

import csv
import html
import json
import math
import re
import sys
from pathlib import Path
from typing import Iterable

from PIL import Image, ImageDraw, ImageFont


GAMMA = 1.4
R_AIR = 287.05287
T_INF_K = 242.65
P_INF_PA = 41060.35
RHO_INF_KG_M3 = 0.5895
CHORD_M = 6.0
REFERENCE_ALPHA0 = {
    0.0: {"CL": 0.42965, "CD": 0.00540, "CM": -0.09717, "LD": 79.5},
    0.6: {"CL": 0.52639, "CD": 0.00572, "CM": -0.11951, "LD": 92.0},
    0.85: {"CL": 0.34482, "CD": 0.05243, "CM": -0.22113, "LD": 6.6},
}

PALETTES = {
    "light": {
        "bg": "#ffffff", "panel": "#f4f5f7", "border": "#808080", "spine": "#333333",
        "tick": "#333333", "title": "#000000", "grid": "#dfe2e8", "accent": "#2563eb",
        "warning": "#7a4e00", "muted": "#555555",
    },
    "dark-accessible": {
        "bg": "#1e1e1e", "panel": "#262a31", "border": "#737881", "spine": "#737881",
        "tick": "#cccccc", "title": "#ffffff", "grid": "#363b44", "accent": "#4f8cff",
        "warning": "#ffd166", "muted": "#bbbbbb",
    },
    "grey-accessible": {
        "bg": "#3a3a3a", "panel": "#41454c", "border": "#969696", "spine": "#969696",
        "tick": "#dddddd", "title": "#ffffff", "grid": "#565b63", "accent": "#6aa2ff",
        "warning": "#ffd166", "muted": "#cccccc",
    },
}
THEMES = tuple(PALETTES)
SERIES_COLORS = ("#2563eb", "#d97706", "#15803d", "#9333ea", "#be123c")
FONT = ImageFont.load_default()


def numeric_time(path: Path) -> float:
    try:
        return float(path.name)
    except ValueError:
        return -math.inf


def finite(value: float) -> bool:
    return math.isfinite(value)


def load_study(root: Path) -> list[dict]:
    cases: list[dict] = []
    for study_path in sorted(root.glob("*/study.json")):
        case = study_path.parent
        try:
            study = json.loads(study_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        result_path = case / "result.json"
        if result_path.is_file():
            try:
                result = json.loads(result_path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                result = {}
        else:
            result = {}
        study["_case_dir"] = case
        study["_result"] = result
        point = study.get("operating_point", {})
        study["requested_mach"] = float(point.get("requested_mach", math.nan))
        study["actual_mach"] = float(point.get("actual_mach", math.nan))
        study["alpha_deg"] = float(study.get("alpha_deg", math.nan))
        cases.append(study)
    return sorted(cases, key=lambda item: (item["requested_mach"], item["alpha_deg"]))


def final_force(study: dict) -> dict[str, float] | None:
    rows = study.get("_result", {}).get("force_history", [])
    good = [row for row in rows if all(finite(float(row.get(key, math.nan))) for key in ("Cd", "Cl", "Cm"))]
    if not good:
        return None
    row = good[-1]
    cd, cl = float(row["Cd"]), float(row["Cl"])
    return {
        "time": float(row.get("time", math.nan)), "Cd": cd, "Cl": cl, "Cm": float(row["Cm"]),
        "LD": cl / cd if cd != 0.0 else math.nan,
    }


def series_label(study: dict) -> str:
    requested = study["requested_mach"]
    actual = study["actual_mach"]
    return f"M={requested:g} (actual {actual:.3f})" if requested == 0.0 else f"M={requested:g}"


def collect_summary(cases: list[dict]) -> list[dict]:
    summary: list[dict] = []
    for study in cases:
        metric = final_force(study)
        result = study.get("_result", {})
        summary.append(
            {
                "case": study.get("case", study["_case_dir"].name),
                "requested_mach": study["requested_mach"],
                "actual_mach": study["actual_mach"],
                "alpha_deg": study["alpha_deg"],
                "status": result.get("status", study.get("status", "unknown")),
                "solver_exit_code": result.get("solver_exit_code", ""),
                "last_time": "" if metric is None else metric["time"],
                "CL": "" if metric is None else metric["Cl"],
                "CD": "" if metric is None else metric["Cd"],
                "CM": "" if metric is None else metric["Cm"],
                "CL_over_CD": "" if metric is None else metric["LD"],
            }
        )
    return summary


def parse_raw_pressure(study: dict) -> list[tuple[float, float, float]]:
    case = study["_case_dir"]
    files = list((case / "postProcessing" / "surfacePressure").glob("*/p_airfoil.raw"))
    if not files:
        return []
    source = max(files, key=numeric_time)
    speed = float(study.get("operating_point", {}).get("velocity_m_s", math.nan))
    q_inf = 0.5 * RHO_INF_KG_M3 * speed * speed
    if not finite(q_inf) or q_inf <= 0.0:
        return []
    output: list[tuple[float, float, float]] = []
    for line in source.read_text(encoding="utf-8", errors="replace").splitlines():
        fields = line.split()
        if len(fields) < 4 or fields[0].startswith("#"):
            continue
        try:
            x, y, p = float(fields[0]), float(fields[1]), float(fields[3])
        except ValueError:
            continue
        cp = (p - P_INF_PA) / q_inf
        if all(finite(value) for value in (x, y, cp)):
            output.append((x / CHORD_M, y / CHORD_M, cp))
    return output


def rgb(value: str) -> tuple[int, int, int]:
    value = value.lstrip("#")
    return tuple(int(value[index : index + 2], 16) for index in (0, 2, 4))


def line(draw: ImageDraw.ImageDraw, points: list[tuple[float, float]], color: str, width: int = 3) -> None:
    if len(points) >= 2:
        draw.line(points, fill=rgb(color), width=width, joint="curve")
    elif points:
        x, y = points[0]
        draw.ellipse((x - 3, y - 3, x + 3, y + 3), fill=rgb(color))


def bounds(values: Iterable[float], default: tuple[float, float]) -> tuple[float, float]:
    filtered = [value for value in values if finite(value)]
    if not filtered:
        return default
    lo, hi = min(filtered), max(filtered)
    if lo == hi:
        pad = max(1.0, abs(lo) * 0.1)
        return lo - pad, hi + pad
    pad = 0.08 * (hi - lo)
    return lo - pad, hi + pad


def svg_chart(
    path: Path,
    title: str,
    xlabel: str,
    ylabel: str,
    curves: list[tuple[str, list[tuple[float, float]], str]],
    xlim: tuple[float, float],
    ylim: tuple[float, float],
    palette: dict,
    note: str,
) -> None:
    width, height = 1400, 850
    left, top, right, bottom = 120, 90, 1300, 730

    def sx(value: float) -> float:
        return left + (value - xlim[0]) / (xlim[1] - xlim[0]) * (right - left)

    def sy(value: float) -> float:
        return bottom - (value - ylim[0]) / (ylim[1] - ylim[0]) * (bottom - top)

    paths: list[str] = []
    for label, values, color in curves:
        coords = [(sx(x), sy(y)) for x, y in values if finite(x) and finite(y)]
        if len(coords) < 1:
            continue
        points = " ".join(f"{x:.1f},{y:.1f}" for x, y in coords)
        element = "polyline" if len(coords) > 1 else "circle"
        if element == "polyline":
            paths.append(f'<polyline points="{points}" fill="none" stroke="{color}" stroke-width="3"/>')
        else:
            x, y = coords[0]
            paths.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="4" fill="{color}"/>')
    legend = []
    for index, (label, _, color) in enumerate(curves):
        y = top + 25 + index * 24
        legend.append(f'<line x1="{right-220}" y1="{y}" x2="{right-185}" y2="{y}" stroke="{color}" stroke-width="4"/><text x="{right-175}" y="{y+5}" fill="{palette["title"]}" font-size="16">{html.escape(label)}</text>')
    text_color = palette["title"]
    svg = f'''<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<rect width="100%" height="100%" fill="{palette["bg"]}"/><rect x="{left}" y="{top}" width="{right-left}" height="{bottom-top}" fill="{palette["panel"]}" stroke="{palette["border"]}"/>
<text x="{left}" y="48" fill="{text_color}" font-size="24" font-family="sans-serif">{html.escape(title)}</text>
<text x="{(left+right)/2}" y="805" text-anchor="middle" fill="{text_color}" font-size="18" font-family="sans-serif">{html.escape(xlabel)}</text>
<text x="28" y="{(top+bottom)/2}" transform="rotate(-90 28 {(top+bottom)/2})" text-anchor="middle" fill="{text_color}" font-size="18" font-family="sans-serif">{html.escape(ylabel)}</text>
<line x1="{left}" y1="{bottom}" x2="{right}" y2="{bottom}" stroke="{palette["spine"]}" stroke-width="2"/><line x1="{left}" y1="{top}" x2="{left}" y2="{bottom}" stroke="{palette["spine"]}" stroke-width="2"/>
{''.join(paths)}{''.join(legend)}
<text x="{left}" y="{height-18}" fill="{palette["muted"]}" font-size="14" font-family="sans-serif">{html.escape(note)}</text>
</svg>'''
    path.write_text(svg, encoding="utf-8")


def raster_chart(
    path: Path,
    title: str,
    xlabel: str,
    ylabel: str,
    curves: list[tuple[str, list[tuple[float, float]], str]],
    xlim: tuple[float, float],
    ylim: tuple[float, float],
    palette: dict,
    note: str,
) -> None:
    width, height = 1400, 850
    left, top, right, bottom = 120, 90, 1300, 730
    image = Image.new("RGB", (width, height), rgb(palette["bg"]))
    draw = ImageDraw.Draw(image)
    draw.rectangle((left, top, right, bottom), fill=rgb(palette["panel"]), outline=rgb(palette["border"]), width=2)
    draw.text((left, 40), title, fill=rgb(palette["title"]), font=FONT)
    draw.text((width // 2 - len(xlabel) * 3, 805), xlabel, fill=rgb(palette["title"]), font=FONT)
    draw.text((18, 390), ylabel, fill=rgb(palette["title"]), font=FONT)
    for fraction in (0.0, 0.25, 0.5, 0.75, 1.0):
        x = left + fraction * (right - left)
        y = bottom - fraction * (bottom - top)
        draw.line((x, top, x, bottom), fill=rgb(palette["grid"]), width=1)
        draw.line((left, y, right, y), fill=rgb(palette["grid"]), width=1)
    draw.line((left, bottom, right, bottom), fill=rgb(palette["spine"]), width=2)
    draw.line((left, top, left, bottom), fill=rgb(palette["spine"]), width=2)

    def sx(value: float) -> int:
        return round(left + (value - xlim[0]) / (xlim[1] - xlim[0]) * (right - left))

    def sy(value: float) -> int:
        return round(bottom - (value - ylim[0]) / (ylim[1] - ylim[0]) * (bottom - top))

    for label, values, color in curves:
        coords = [(sx(x), sy(y)) for x, y in values if finite(x) and finite(y)]
        line(draw, coords, color)
    for index, (label, _, color) in enumerate(curves):
        y = top + 20 + index * 22
        draw.line((right - 240, y, right - 205, y), fill=rgb(color), width=3)
        draw.text((right - 195, y - 6), label, fill=rgb(palette["title"]), font=FONT)
    draw.text((left, height - 28), note, fill=rgb(palette["muted"]), font=FONT)
    image.save(path)


def make_chart(
    output: Path,
    name: str,
    title: str,
    xlabel: str,
    ylabel: str,
    curves_by_theme: dict[str, list[tuple[str, list[tuple[float, float]], str]]],
    xlim: tuple[float, float],
    ylim: tuple[float, float],
    note: str,
) -> None:
    for theme, palette in PALETTES.items():
        curves = curves_by_theme.get(theme, curves_by_theme["light"])
        raster_chart(output / f"{name}-{theme}.png", title, xlabel, ylabel, curves, xlim, ylim, palette, note)
        svg_chart(output / f"{name}-{theme}.svg", title, xlabel, ylabel, curves, xlim, ylim, palette, note)


def curves_for_metric(cases: list[dict], x_key: str, metric: str) -> list[tuple[str, list[tuple[float, float]], str]]:
    grouped: dict[float, list[tuple[float, float]]] = {}
    labels: dict[float, str] = {}
    for study in cases:
        force = final_force(study)
        if force is None:
            continue
        x = float(study["alpha_deg"] if x_key == "alpha" else force["Cd"])
        y = float(force[metric])
        if not finite(x) or not finite(y):
            continue
        key = study["requested_mach"]
        grouped.setdefault(key, []).append((x, y))
        labels[key] = series_label(study)
    curves = []
    for index, key in enumerate(sorted(grouped)):
        curves.append((labels[key], sorted(grouped[key]), SERIES_COLORS[index % len(SERIES_COLORS)]))
    return curves


def cp_curves(cases: list[dict]) -> list[tuple[str, list[tuple[float, float]], str]]:
    curves = []
    alpha_zero = [study for study in cases if abs(study["alpha_deg"]) < 1e-9]
    for index, study in enumerate(sorted(alpha_zero, key=lambda item: item["requested_mach"])):
        samples = parse_raw_pressure(study)
        if not samples:
            continue
        # Upper and lower faces are retained as separate lines.  Face centres
        # are not ordered by OpenFOAM, so sorting by x/c is explicit.
        upper = sorted((x, cp) for x, y, cp in samples if y >= 0.0)
        lower = sorted((x, cp) for x, y, cp in samples if y < 0.0)
        color = SERIES_COLORS[index % len(SERIES_COLORS)]
        label = series_label(study)
        if upper:
            curves.append((label + " upper", upper, color))
        if lower:
            curves.append((label + " lower", lower, color))
    return curves


def residual_curves(cases: list[dict]) -> list[tuple[str, list[tuple[float, float]], str]]:
    curves = []
    selected = [study for study in cases if abs(study["alpha_deg"]) < 1e-9]
    for index, study in enumerate(sorted(selected, key=lambda item: item["requested_mach"])):
        rows = study.get("_result", {}).get("residual_history", [])
        grouped: dict[int, float] = {}
        for row in rows:
            try:
                iteration = int(row["iteration"])
                value = float(row["initial_residual"])
            except (KeyError, TypeError, ValueError):
                continue
            if finite(value) and value > 0.0:
                grouped[iteration] = max(grouped.get(iteration, 0.0), value)
        if grouped:
            curves.append((series_label(study), sorted((x, math.log10(y)) for x, y in grouped.items()), SERIES_COLORS[index % len(SERIES_COLORS)]))
    return curves


def write_report(output: Path, cases: list[dict], summary: list[dict]) -> None:
    quality_rows = []
    for study in cases:
        quality = study.get("_result", {}).get("mesh_quality", {})
        if quality:
            quality_rows.append(
                f"| {study['_case_dir'].name} | {quality.get('cells', '')} | {quality.get('max_non_orthogonality_deg', '')} | {quality.get('max_skewness', '')} | {quality.get('min_volume_m3', '')} | {quality.get('mesh_ok', '')} |"
            )
    statuses = {}
    for row in summary:
        statuses[row["status"]] = statuses.get(row["status"], 0) + 1
    lines = [
        "# KC-135 Winglet report-study OpenFOAM evidence",
        "",
        "This artifact is a numerical post-processing record for the private `informe_tecnico_P1.md` study. The report’s source values were generated with NeuralFoil/Karman–Tsien (report lines 82–85 and 113–127); they are included as comparison data and are not treated as CFD validation.",
        "",
        f"Source conditions: ISA at 7000 m (T={T_INF_K:g} K, p={P_INF_PA:g} Pa, rho={RHO_INF_KG_M3:g} kg/m³), c={CHORD_M:g} m, perfect-gas compressible `rhoSimpleFoam` v2606, SST RANS, adiabatic no-slip airfoil wall.",
        "The report does not define turbulence intensity, transition, wall thermal condition, or y+ target. This run assumes fully turbulent SST, TI=1%, length scale 0.07c, adiabatic wall, and a dimensional first-cell setting of 1.2e-4 m. These assumptions are in each case’s `study.json`.",
        "The report’s M=0 and U≈50 m/s reference is retained as `M0-reference`; at this temperature the case has actual M≈0.160. M=0.6 and M=0.85 use U=aM.",
        "",
        f"Case status counts: {json.dumps(statuses, sort_keys=True)}.",
        "",
        "## Alpha=0 comparison",
        "",
        "The following comparison preserves the report’s reference values alongside the last finite native CFD sample. The native values are transient or early-stop diagnostics because every case remains unconverged.",
        "",
        "| Requested M | Actual M | CFD CL | CFD CD | CFD CM | CFD CL/CD | Last iteration | CFD status | Report CL | Report CD | Report CM | Report L/D |",
        "|---:|---:|---:|---:|---:|---:|---:|---|---:|---:|---:|---:|",
    ]
    for mach in sorted(REFERENCE_ALPHA0):
        native = next((row for row in summary if abs(float(row["requested_mach"]) - mach) < 1e-9 and abs(float(row["alpha_deg"])) < 1e-9), None)
        reference = REFERENCE_ALPHA0[mach]
        if native is None:
            continue
        lines.append(
            "| {m:g} | {actual:.4f} | {cl} | {cd} | {cm} | {ld} | {time} | {status} | {rcl:.5f} | {rcd:.5f} | {rcm:.5f} | {rld:.1f} |".format(
                m=mach,
                actual=float(native["actual_mach"]),
                cl=native["CL"] or "",
                cd=native["CD"] or "",
                cm=native["CM"] or "",
                ld=native["CL_over_CD"] or "",
                time=native["last_time"] or "",
                status=native["status"],
                rcl=reference["CL"],
                rcd=reference["CD"],
                rcm=reference["CM"],
                rld=reference["LD"],
            )
        )
    lines += [
        "",
        "## paraFoam contours",
        "",
        "Representative alpha=0 contour exports were produced through the native `paraFoam -vtk -touch` route and the shared Turbo renderer:",
    ]
    for study in cases:
        if abs(study["alpha_deg"]) > 1e-9:
            continue
        contour_dir = study["_case_dir"] / "postProcessing" / "alas-field-figures"
        if (contour_dir / "mach-contour.png").is_file() and (contour_dir / "pressure-contour.png").is_file():
            relative = contour_dir.relative_to(output.parent).as_posix()
            lines.append(f"- `{series_label(study)}`: `{relative}/mach-contour.png`, `{relative}/pressure-contour.png`; see `render-provenance.txt` for ranges, units, and Turbo LUT.")
    lines += [
        "",
        "## Mesh quality",
        "",
        "| Case | Cells | Max non-orthogonality (deg) | Max skewness | Min volume (m³) | checkMesh |",
        "|---|---:|---:|---:|---:|---|",
        *quality_rows,
        "",
        "Finite force outputs remain visible even when a solver stops early or residuals are high. No case is labelled physically validated.",
        "",
        "Reference alpha=0 values from report lines 140–148 are in `reference_alpha0.csv`; native CFD summaries are in `summary.csv`.",
    ]
    (output / "report-study.md").write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("out/openfoam-report-study")
    root = root.resolve()
    output = root / "figures"
    output.mkdir(parents=True, exist_ok=True)
    cases = load_study(root)
    summary = collect_summary(cases)
    with (output / "summary.csv").open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(summary[0]) if summary else ["case"])
        writer.writeheader()
        writer.writerows(summary)
    with (output / "reference_alpha0.csv").open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=["requested_mach", "CL", "CD", "CM", "LD", "source"])
        writer.writeheader()
        for mach, row in REFERENCE_ALPHA0.items():
            writer.writerow({"requested_mach": mach, **row, "source": "informe_tecnico_P1.md:140-148; NeuralFoil/Karman-Tsien"})
    with (output / "cp_alpha0.csv").open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=["case", "requested_mach", "actual_mach", "x_over_c", "y_over_c", "Cp", "source"],
        )
        writer.writeheader()
        for study in cases:
            if abs(study["alpha_deg"]) > 1e-9:
                continue
            for x, y, cp_value in parse_raw_pressure(study):
                writer.writerow(
                    {
                        "case": study["_case_dir"].name,
                        "requested_mach": study["requested_mach"],
                        "actual_mach": study["actual_mach"],
                        "x_over_c": x,
                        "y_over_c": y,
                        "Cp": cp_value,
                        "source": "native OpenFOAM surface p; Cp=(p-p_inf)/(0.5*rho_inf*U_inf^2)",
                    }
                )

    note = "native rhoSimpleFoam output; finite provisional values shown; no physical validation"
    alpha_curves = curves_for_metric(cases, "alpha", "Cl")
    make_chart(output, "cl-alpha", "Lift coefficient versus angle of attack", "alpha (deg)", "CL [-]", {theme: alpha_curves for theme in THEMES}, (-5, 13), bounds((y for _, values, _ in alpha_curves for _, y in values), (-1, 1)), note)
    cd_curves = curves_for_metric(cases, "alpha", "Cd")
    make_chart(output, "cd-alpha", "Drag coefficient versus angle of attack", "alpha (deg)", "CD [-]", {theme: cd_curves for theme in THEMES}, (-5, 13), bounds((y for _, values, _ in cd_curves for _, y in values), (-0.1, 0.2)), note)
    cm_curves = curves_for_metric(cases, "alpha", "Cm")
    make_chart(output, "cm-alpha", "Quarter-chord pitch moment versus angle of attack", "alpha (deg)", "CM [-]", {theme: cm_curves for theme in THEMES}, (-5, 13), bounds((y for _, values, _ in cm_curves for _, y in values), (-1, 1)), note)
    ld_curves = curves_for_metric(cases, "alpha", "LD")
    make_chart(output, "efficiency-alpha", "Aerodynamic efficiency versus angle of attack", "alpha (deg)", "CL/CD [-]", {theme: ld_curves for theme in THEMES}, (-5, 13), bounds((y for _, values, _ in ld_curves for _, y in values), (-100, 200)), note)
    polar_curves = curves_for_metric(cases, "cd", "Cl")
    make_chart(output, "cl-cd", "Lift-drag polar", "CD [-]", "CL [-]", {theme: polar_curves for theme in THEMES}, bounds((x for _, values, _ in polar_curves for x, _ in values), (-0.1, 0.2)), bounds((y for _, values, _ in polar_curves for _, y in values), (-1, 1)), note)
    cp = cp_curves(cases)
    make_chart(output, "cp-alpha0", "Surface pressure coefficient at alpha=0", "x/c [-]", "Cp [-]", {theme: cp for theme in THEMES}, (0, 1), bounds((y for _, values, _ in cp for _, y in values), (-5, 2)), note + "; Cp=(p-p_inf)/(0.5 rho_inf U_inf²)")
    residuals = residual_curves(cases)
    make_chart(output, "residual-history", "Maximum initial residual by SIMPLE iteration", "iteration [-]", "log10(initial residual)", {theme: residuals for theme in THEMES}, (0, max([10] + [x for _, values, _ in residuals for x, _ in values])), bounds((y for _, values, _ in residuals for _, y in values), (-8, 1)), note)
    quality_curves = []
    for index, study in enumerate(cases):
        quality = study.get("_result", {}).get("mesh_quality", {})
        if quality.get("cells") is not None and quality.get("max_skewness") is not None:
            quality_curves.append((study["_case_dir"].name, [(float(quality["cells"]), float(quality["max_skewness"]))], SERIES_COLORS[index % len(SERIES_COLORS)]))
    make_chart(output, "mesh-quality", "Native checkMesh quality summary", "cells [-]", "max skewness [-]", {theme: quality_curves for theme in THEMES}, bounds((x for _, values, _ in quality_curves for x, _ in values), (0, 1)), bounds((y for _, values, _ in quality_curves for _, y in values), (0, 1)), note)
    write_report(output, cases, summary)
    manifest = {
        "case_root": str(root),
        "output": str(output),
        "themes": list(THEMES),
        "cases": len(cases),
        "charts": [path.name for path in sorted(output.glob("*.png"))],
        "status": "numerical outputs; no physical validation claim",
    }
    (output / "figure-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
