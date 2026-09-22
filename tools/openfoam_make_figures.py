"""Build standalone figures from persisted native ALAS CFD results.

The script intentionally consumes only ``results.json`` values and never fills
missing angles by interpolation.  It produces one polar sheet and one
diagnostic sheet for each geometry represented below ``DATA_ROOT``.

    python tools/openfoam_make_figures.py CASE_ROOT OUTPUT_DIR [--theme THEME]

``--theme all`` (the default) writes light, dark-accessible, and
grey-accessible variants.  These names match ``alas-gui::AppTheme`` and the
shared ``alas-report`` palette definitions.

The PNG/SVG files are evidence views for the same result artifacts that the
Airfoil CFD Results tab displays.  They are not a validation or accuracy claim.
"""

from __future__ import annotations

import html
import json
import math
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


PALETTES = {
    # Keep these values synchronized with alas-report/src/theme.rs and the
    # egui adapter in alas-gui/src/theme.rs.  The accessible desktop variants
    # are the palettes selected by AppTheme::Dark and AppTheme::Grey.
    "light": {
        "bg": "#ffffff",
        "panel": "#f4f5f7",
        "border": "#808080",
        "spine": "#333333",
        "tick": "#333333",
        "title": "#000000",
        "accent": "#2563eb",
        "warning": "#7a4e00",
        "success": "#146939",
        "error": "#b42318",
        "grid": "#dfe2e8",
    },
    "dark-accessible": {
        "bg": "#1e1e1e",
        "panel": "#262a31",
        "border": "#737881",
        "spine": "#737881",
        "tick": "#cccccc",
        "title": "#ffffff",
        "accent": "#4f8cff",
        "warning": "#ffd166",
        "success": "#7ee7a5",
        "error": "#ffb4ab",
        "grid": "#363b44",
    },
    "grey-accessible": {
        "bg": "#3a3a3a",
        "panel": "#41454c",
        "border": "#969696",
        "spine": "#969696",
        "tick": "#dddddd",
        "title": "#ffffff",
        "accent": "#6aa2ff",
        "warning": "#ffd166",
        "success": "#7ee7a5",
        "error": "#ffb4ab",
        "grid": "#565b63",
    },
}
THEME_ORDER = ["light", "dark-accessible", "grey-accessible"]
FONT = ImageFont.load_default()


def rgb(hex_color):
    clean = hex_color.lstrip("#")
    return tuple(int(clean[index : index + 2], 16) for index in (0, 2, 4))


def series_colors(palette):
    return [
        palette["accent"],
        palette["warning"],
        palette["success"],
        palette["error"],
        "#d6632f",
        "#8d55b7",
    ]


def finite(value):
    return isinstance(value, (int, float)) and math.isfinite(float(value))


def load_groups(root: Path):
    groups = {}
    for result_path in sorted(root.rglob("results.json")):
        try:
            result = json.loads(result_path.read_text(encoding="utf-8"))
            provenance = result["provenance"]
            airfoil = provenance["airfoil"]
            name = airfoil["name"]
            key = (name, airfoil.get("coordinate_hash", "unknown"))
            result["_path"] = str(result_path)
            groups.setdefault(key, []).append(result)
        except (OSError, ValueError, KeyError, TypeError):
            continue
    return groups


def usable_force(result):
    if result.get("outcome") not in ("unconverged", "numerically_converged"):
        return None
    if not result.get("mesh_quality", {}).get("passed", False):
        return None
    forces = result.get("forces", [])
    if not forces:
        return None
    force = forces[-1]
    if not all(finite(force.get(key)) for key in ("time", "cl", "cd", "cm")):
        return None
    return force


def bounds(points, log_y=False):
    clean = [(float(x), float(y)) for x, y in points if finite(x) and finite(y) and (not log_y or y > 0)]
    if not clean:
        return (0.0, 1.0, 0.0, 1.0)
    xs = [point[0] for point in clean]
    ys = [math.log10(point[1]) if log_y else point[1] for point in clean]
    xmin, xmax = min(xs), max(xs)
    ymin, ymax = min(ys), max(ys)
    dx = xmax - xmin
    dy = ymax - ymin
    if dx <= 1e-12:
        dx = max(abs(xmin) * 0.1, 1.0)
    if dy <= 1e-12:
        dy = max(abs(ymin) * 0.1, 1.0)
    return xmin - 0.05 * dx, xmax + 0.05 * dx, ymin - 0.08 * dy, ymax + 0.08 * dy


def project(points, frame, log_y=False):
    left, top, right, bottom = frame
    xmin, xmax, ymin, ymax = bounds(points, log_y)
    output = []
    for x, y in points:
        if not finite(x) or not finite(y) or (log_y and y <= 0):
            continue
        value = math.log10(y) if log_y else y
        px = left + (x - xmin) / (xmax - xmin) * (right - left)
        py = bottom - (value - ymin) / (ymax - ymin) * (bottom - top)
        output.append((px, py))
    return output, (xmin, xmax, ymin, ymax)


def tick_label(value):
    if abs(value) >= 1e4 or (0 < abs(value) < 1e-3):
        return f"{value:.2e}"
    return f"{value:.3g}"


def svg_panel(parts, x, y, width, height, title, x_label, y_label, series, log_y=False, palette=None):
    palette = palette or PALETTES["light"]
    left, top, right, bottom = x + 58, y + 34, x + width - 18, y + height - 48
    all_points = [point for _, points, _ in series for point in points]
    _, values = project(all_points, (left, top, right, bottom), log_y)
    xmin, xmax, ymin, ymax = values
    parts.append(f'<rect x="{x}" y="{y}" width="{width}" height="{height}" rx="12" fill="{palette["panel"]}" stroke="{palette["border"]}"/>')
    parts.append(f'<text x="{x + 14}" y="{y + 22}" font-size="15" font-family="sans-serif" font-weight="600" fill="{palette["title"]}">{html.escape(title)}</text>')
    for fraction in (0.0, 0.5, 1.0):
        gx = left + fraction * (right - left)
        gy = bottom - fraction * (bottom - top)
        parts.append(f'<line x1="{gx:.1f}" y1="{top}" x2="{gx:.1f}" y2="{bottom}" stroke="{palette["grid"]}"/>')
        parts.append(f'<line x1="{left}" y1="{gy:.1f}" x2="{right}" y2="{gy:.1f}" stroke="{palette["grid"]}"/>')
        parts.append(f'<text x="{gx:.1f}" y="{bottom + 17}" text-anchor="middle" font-size="10" font-family="sans-serif" fill="{palette["tick"]}">{html.escape(tick_label(xmin + fraction * (xmax - xmin)))}</text>')
        y_value = ymin + fraction * (ymax - ymin)
        parts.append(f'<text x="{left - 7}" y="{gy + 3:.1f}" text-anchor="end" font-size="10" font-family="sans-serif" fill="{palette["tick"]}">{html.escape(tick_label(y_value))}</text>')
    parts.append(f'<line x1="{left}" y1="{bottom}" x2="{right}" y2="{bottom}" stroke="{palette["spine"]}"/>')
    parts.append(f'<line x1="{left}" y1="{top}" x2="{left}" y2="{bottom}" stroke="{palette["spine"]}"/>')
    colors = series_colors(palette)
    for index, (name, points, dashed) in enumerate(series):
        projected, _ = project(points, (left, top, right, bottom), log_y)
        if len(projected) < 1:
            continue
        color = colors[index % len(colors)]
        attr = ' stroke-dasharray="5 4"' if dashed else ""
        coordinates = " ".join(f"{px:.1f},{py:.1f}" for px, py in projected)
        parts.append(f'<polyline points="{coordinates}" fill="none" stroke="{color}" stroke-width="2"{attr}/>')
        ly = top + 15 + (index % 8) * 14
        parts.append(f'<line x1="{right - 150}" y1="{ly}" x2="{right - 134}" y2="{ly}" stroke="{color}" stroke-width="2"{attr}/>')
        parts.append(f'<text x="{right - 129}" y="{ly + 3}" font-size="10" font-family="sans-serif" fill="{color}">{html.escape(name)}</text>')
    parts.append(f'<text x="{(left + right) / 2:.1f}" y="{y + height - 10}" text-anchor="middle" font-size="10" font-family="sans-serif" fill="{palette["tick"]}">{html.escape(x_label)}</text>')
    parts.append(f'<text x="{x + 14}" y="{(top + bottom) / 2:.1f}" text-anchor="middle" transform="rotate(-90 {x + 14} {(top + bottom) / 2:.1f})" font-size="10" font-family="sans-serif" fill="{palette["tick"]}">{html.escape(y_label)}</text>')


def png_panel(draw, x, y, width, height, title, x_label, y_label, series, log_y=False, palette=None):
    palette = palette or PALETTES["light"]
    left, top, right, bottom = x + 58, y + 34, x + width - 18, y + height - 48
    all_points = [point for _, points, _ in series for point in points]
    _, values = project(all_points, (left, top, right, bottom), log_y)
    xmin, xmax, ymin, ymax = values
    draw.rounded_rectangle((x, y, x + width, y + height), radius=12, fill=rgb(palette["panel"]), outline=rgb(palette["border"]), width=1)
    draw.text((x + 14, y + 10), title, fill=rgb(palette["title"]), font=FONT)
    for fraction in (0.0, 0.5, 1.0):
        gx = left + fraction * (right - left)
        gy = bottom - fraction * (bottom - top)
        draw.line((gx, top, gx, bottom), fill=rgb(palette["grid"]), width=1)
        draw.line((left, gy, right, gy), fill=rgb(palette["grid"]), width=1)
        draw.text((gx - 12, bottom + 5), tick_label(xmin + fraction * (xmax - xmin)), fill=rgb(palette["tick"]), font=FONT)
        draw.text((left - 52, gy - 5), tick_label(ymin + fraction * (ymax - ymin)), fill=rgb(palette["tick"]), font=FONT)
    draw.line((left, bottom, right, bottom), fill=rgb(palette["spine"]), width=1)
    draw.line((left, top, left, bottom), fill=rgb(palette["spine"]), width=1)
    colors = series_colors(palette)
    for index, (name, points, dashed) in enumerate(series):
        projected, _ = project(points, (left, top, right, bottom), log_y)
        if not projected:
            continue
        color = rgb(colors[index % len(colors)])
        for first, second in zip(projected, projected[1:]):
            draw.line((*first, *second), fill=color, width=2)
        ly = top + 8 + (index % 8) * 14
        draw.line((right - 150, ly + 5, right - 134, ly + 5), fill=color, width=2)
        draw.text((right - 129, ly), name, fill=color, font=FONT)
    draw.text(((left + right) // 2 - len(x_label) * 3, y + height - 17), x_label, fill=rgb(palette["tick"]), font=FONT)
    draw.text((x + 5, (top + bottom) // 2), y_label, fill=rgb(palette["tick"]), font=FONT)


def residual_series(result):
    grouped = {}
    for sample in result.get("residuals", []):
        field = sample.get("field")
        iteration = sample.get("iteration")
        initial = sample.get("initial")
        final = sample.get("final_residual")
        if not isinstance(field, str) or not isinstance(iteration, int) or not finite(initial) or not finite(final) or initial <= 0 or final <= 0:
            continue
        key = (field, iteration)
        if key not in grouped or initial >= grouped[key][0]:
            grouped[key] = (float(initial), float(final))
    output = []
    for field in sorted({key[0] for key in grouped}):
        initial = sorted((float(iteration), value[0]) for (name, iteration), value in grouped.items() if name == field)
        final = sorted((float(iteration), value[1]) for (name, iteration), value in grouped.items() if name == field)
        output.append((f"{field} initial", initial, False))
        output.append((f"{field} final", final, True))
    return output


def surface_series(result, value_key):
    upper = []
    lower = []
    for sample in result.get("surface", {}).get("samples", []):
        center = sample.get("center_m", [])
        value = sample.get(value_key)
        if len(center) < 2 or not finite(center[0]) or not finite(center[1]) or not finite(value):
            continue
        target = upper if center[1] >= 0.0 else lower
        target.append((float(center[0]), float(value)))
    upper.sort()
    lower.sort()
    return upper, lower


def make_svg(path, title, subtitle, panels, palette):
    width, height = 1500, 1040
    parts = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">', f'<rect width="100%" height="100%" fill="{palette["bg"]}"/>']
    parts.append(f'<text x="28" y="32" font-size="22" font-family="sans-serif" font-weight="700" fill="{palette["title"]}">{html.escape(title)}</text>')
    parts.append(f'<text x="28" y="54" font-size="12" font-family="sans-serif" fill="{palette["tick"]}">{html.escape(subtitle)}</text>')
    for panel in panels:
        svg_panel(parts, *panel, palette=palette)
    parts.append("</svg>")
    path.write_text("\n".join(parts), encoding="utf-8")


def make_png(path, title, subtitle, panels, palette):
    width, height = 1500, 1040
    image = Image.new("RGB", (width, height), rgb(palette["bg"]))
    draw = ImageDraw.Draw(image)
    draw.text((28, 12), title, fill=rgb(palette["title"]), font=FONT)
    draw.text((28, 37), subtitle, fill=rgb(palette["tick"]), font=FONT)
    for panel in panels:
        png_panel(draw, *panel, palette=palette)
    image.save(path)


def group_sheets(key, results, output, theme_names):
    name, coordinate_hash = key
    valid = [(result, usable_force(result)) for result in results]
    valid = [(result, force) for result, force in valid if force is not None]
    valid.sort(key=lambda item: item[0]["provenance"]["config"].get("angle_of_attack_deg", 0.0))
    if not valid:
        return
    alphas = [(result["provenance"]["config"]["angle_of_attack_deg"], force) for result, force in valid]
    cl_alpha = [(alpha, force["cl"]) for alpha, force in alphas]
    cd_alpha = [(alpha, force["cd"]) for alpha, force in alphas]
    cl_cd = [(force["cd"], force["cl"]) for _, force in alphas]
    efficiency = [(alpha, force["cl"] / force["cd"]) for alpha, force in alphas if force["cd"] != 0]
    representative = min(
        valid,
        key=lambda item: abs(item[0]["provenance"]["config"].get("angle_of_attack_deg", 0.0) - 2.0),
    )[0]
    polar_panels = [
        (20, 80, 470, 300, "CL versus alpha (native finite points)", "alpha [deg]", "CL [-]", [("CL", cl_alpha, False)], False),
        (510, 80, 470, 300, "CD versus alpha (native finite points)", "alpha [deg]", "CD [-]", [("CD", cd_alpha, False)], False),
        (1000, 80, 470, 300, "CL versus CD (native finite points)", "CD [-]", "CL [-]", [("CL", cl_cd, False)], False),
        (20, 420, 470, 300, "Efficiency CL/CD versus alpha", "alpha [deg]", "CL/CD [-]", [("CL/CD", efficiency, False)], False),
    ]
    cp_upper, cp_lower = surface_series(representative, "cp")
    cf_upper, cf_lower = surface_series(representative, "cf")
    polar_panels.extend(
        [
            (510, 420, 470, 300, "Pressure coefficient Cp (native wall faces)", "x [m]", "Cp [-]", [("upper", cp_upper, False), ("lower", cp_lower, True)], False),
            (1000, 420, 470, 300, "Skin friction coefficient Cf (native wall faces)", "x [m]", "Cf [-]", [("upper", cf_upper, False), ("lower", cf_lower, True)], False),
        ]
    )
    sample_text = ", ".join(f"{result['provenance']['config']['angle_of_attack_deg']:.1f} deg: {result['outcome']}" for result, _ in valid)
    label = f"{name}; coordinate hash {coordinate_hash}; low-Mach incompressible SST; {len(valid)} actual cases ({sample_text})"
    stem = name.replace("/", "_").replace("\\", "_")
    residuals = residual_series(representative)
    distributions = representative.get("mesh_quality", {}).get("distributions", [])
    quality_panels = []
    for index, distribution in enumerate(distributions[:4]):
        points = list(zip(distribution.get("percentiles", []), distribution.get("values", [])))
        quality_panels.append((20 + (index % 3) * 490, 80 + (index // 3) * 330, 470, 300, f"{distribution.get('label', distribution.get('field'))} percentiles", "percentile [%]", f"{distribution.get('label')} [{distribution.get('unit')}]", [(distribution.get("field", "native"), points, False)], distribution.get("field") == "cellVolume"))
    wall = representative.get("mesh_quality", {}).get("near_wall_distribution")
    if wall:
        points = list(zip(wall.get("percentiles", []), wall.get("values", [])))
        quality_panels.append((510, 410, 470, 300, "Solved wall y+ percentiles", "percentile [%]", "y+ [-]", [("yPlus", points, False)], False))
    residual_panel = (1000, 410, 470, 300, "Residual histories by outer SIMPLE iteration", "outer iteration", "log10 residual", residuals, False)
    quality_panels.append(residual_panel)
    subtitle = f"Representative alpha {representative['provenance']['config']['angle_of_attack_deg']:.1f} deg; native fields; {representative['outcome']}; cell volume uses log10 ordinate"
    for theme_name in theme_names:
        palette = PALETTES[theme_name]
        # Preserve the original light-theme filenames for reports that already
        # link them, while desktop variants carry a short theme suffix.
        suffix = "" if theme_name == "light" else f"-{theme_name.split('-')[0]}"
        make_svg(
            output / f"{stem}-polar{suffix}.svg",
            f"{name} aerodynamic polar",
            label,
            polar_panels,
            palette,
        )
        make_png(
            output / f"{stem}-polar{suffix}.png",
            f"{name} aerodynamic polar",
            label,
            polar_panels,
            palette,
        )
        make_svg(
            output / f"{stem}-diagnostics{suffix}.svg",
            f"{name} mesh and solver diagnostics",
            subtitle,
            quality_panels,
            palette,
        )
        make_png(
            output / f"{stem}-diagnostics{suffix}.png",
            f"{name} mesh and solver diagnostics",
            subtitle,
            quality_panels,
            palette,
        )
    (output / f"{stem}-provenance.txt").write_text(
        "\n".join(
            [
                f"geometry={name}",
                f"coordinate_hash={coordinate_hash}",
                "model=incompressible steady RANS kOmegaSST",
                "diagnostic_mach=U/sqrt(1.4*287.05287*T)",
                f"representative_case={representative['_path']}",
                f"representative_outcome={representative['outcome']}",
                f"angles={','.join(str(alpha) for alpha, _ in alphas)}",
                f"themes={','.join(theme_names)}",
                "curves=last finite native force sample from each mesh-valid completed case; no interpolation",
                "residuals=initial and final values grouped by parsed outer SIMPLE iteration; pressure corrections retain largest initial",
                "quality=percentiles from native checkMesh -writeAllFields fields; wall_y_plus is a solved face diagnostic",
            ]
        )
        + "\n",
        encoding="utf-8",
    )


def main():
    if len(sys.argv) not in (3, 5) or (len(sys.argv) == 5 and sys.argv[3] != "--theme"):
        raise SystemExit(
            "usage: openfoam_make_figures.py CASE_ROOT OUTPUT_DIR [--theme THEME|all]"
        )
    root = Path(sys.argv[1]).resolve()
    output = Path(sys.argv[2]).resolve()
    requested_theme = sys.argv[4] if len(sys.argv) == 5 else "all"
    if requested_theme == "all":
        theme_names = THEME_ORDER
    elif requested_theme in PALETTES:
        theme_names = [requested_theme]
    else:
        raise SystemExit(
            f"unknown theme {requested_theme!r}; choose one of {', '.join(THEME_ORDER)} or all"
        )
    output.mkdir(parents=True, exist_ok=True)
    for key, results in load_groups(root).items():
        group_sheets(key, results, output, theme_names)
    print(f"wrote {','.join(theme_names)} figure sheets to {output}")


if __name__ == "__main__":
    main()
