# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Collect W6.5 planform-area evidence from the pinned reference.

The fixture follows the causal order in the W6.5 handoff: preset/config,
engine resolution, builder stations, reference axes, projected/planform areas,
then renderer scale.  It is intentionally standalone and does not update a
family manifest; the integration owner decides how evidence is registered.
"""

from __future__ import annotations

import copy
import dataclasses
import json
from pathlib import Path
from typing import Any

import _framework

_framework.add_alas_to_path()

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

from alas.config.presets import get_preset  # noqa: E402
from alas.config.settings import ALASConfig  # noqa: E402
from alas.geometry.aircraft_builder import AircraftBuilder  # noqa: E402
from alas.reporting import visualization as visualization_module  # noqa: E402


ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "golden" / "report" / "w65_planform_area.json"
PRESETS = ("AVE", "A320-200")


def _float(value: Any) -> float:
    return float(value)


def _array(values: Any) -> list[float]:
    return [_float(value) for value in values]


def _station(xsec: Any) -> dict[str, Any]:
    return {
        "xyz_le_m": _array(xsec.xyz_le),
        "chord_m": _float(xsec.chord),
        "twist_deg": _float(xsec.twist),
    }


def _outline_area(xsecs: list[Any], symmetric: bool) -> float:
    """Area of the top-view polygon drawn by ``_draw_planform``."""
    le = [(float(xsec.xyz_le[1]), float(xsec.xyz_le[0])) for xsec in xsecs]
    te = [
        (float(xsec.xyz_le[1]), float(xsec.xyz_le[0] + xsec.chord))
        for xsec in xsecs
    ]
    half = le + list(reversed(te))

    def polygon_area(points: list[tuple[float, float]]) -> float:
        return 0.5 * abs(
            sum(
                x0 * y1 - x1 * y0
                for (x0, y0), (x1, y1) in zip(points, points[1:] + points[:1])
            )
        )

    if not symmetric:
        return polygon_area(half)
    mirrored = [(-y, x) for y, x in le] + [(-y, x) for y, x in reversed(te)]
    return polygon_area(half) + polygon_area(mirrored)


def _wing(wing: Any) -> dict[str, Any]:
    xsecs = list(wing.xsecs)
    return {
        "name": wing.name,
        "symmetric": bool(wing.symmetric),
        "xsecs": [_station(xsec) for xsec in xsecs],
        "areas_m2": {
            "planform": _float(wing.area(type="planform")),
            "projected": _float(wing.area(type="projected")),
            "side": _float(wing.area(type="side")),
            "rendered_top_outline": _outline_area(xsecs, bool(wing.symmetric)),
        },
    }


def _fuselage(fuselage: Any) -> dict[str, Any]:
    return {
        "name": fuselage.name,
        "xsecs": [
            {
                "xyz_c_m": _array(xsec.xyz_c),
                "width_m": _float(xsec.width),
                "height_m": _float(xsec.height),
            }
            for xsec in fuselage.xsecs
        ],
    }


def _plane(plane: Any) -> dict[str, Any]:
    return {
        "name": plane.name,
        "xyz_ref_m": _array(plane.xyz_ref),
        "s_ref_m2": _float(plane.s_ref),
        "c_ref_m": _float(plane.c_ref),
        "b_ref_m": _float(plane.b_ref),
        "wings": [_wing(wing) for wing in plane.wings],
        "fuselages": [_fuselage(fuselage) for fuselage in plane.fuselages],
    }


def _figure_contract(plane: Any) -> dict[str, Any]:
    figure = visualization_module.figure_planform_comparison(
        plane, plane, labels=("baseline", "optimized"), theme="light"
    )
    try:
        figure.canvas.draw()
        axis = figure.axes[0]
        origin = axis.transData.transform((0.0, 0.0))
        x_step = axis.transData.transform((1.0, 0.0))
        y_step = axis.transData.transform((0.0, 1.0))
        return {
            "figure_size_inches": _array(figure.get_size_inches()),
            "dpi": _float(figure.dpi),
            "panel_count": len(figure.axes),
            "axes": {
                "aspect": str(axis.get_aspect()),
                "xlabel": axis.get_xlabel(),
                "ylabel": axis.get_ylabel(),
                "title": axis.get_title(),
                "xlim_m": _array(axis.get_xlim()),
                "ylim_m": _array(axis.get_ylim()),
                "bbox_px": _array(axis.bbox.bounds),
                "origin_px": _array(origin),
                "x_scale_px_per_m": abs(_float(x_step[0] - origin[0])),
                "y_scale_px_per_m": abs(_float(y_step[1] - origin[1])),
            },
        }
    finally:
        plt.close(figure)


def _case(preset_name: str) -> dict[str, Any]:
    config = ALASConfig.from_dict({"preset": preset_name})
    preset = get_preset(preset_name)
    design_vector = copy.deepcopy(preset.design_vector)
    before_geometry = copy.deepcopy(config.geometry)
    before_builder = copy.deepcopy(config.geometry.engine)
    builder = AircraftBuilder(config.geometry)
    after_builder = copy.deepcopy(builder.geometry.engine)
    plane = builder.build(design_vector, include_engines=True)
    return {
        "preset": preset_name,
        "input": {
            "config_overlay": {"preset": preset_name},
            "design_vector": dataclasses.asdict(design_vector),
            "include_engines": True,
        },
        "effective_geometry_config_before_builder": dataclasses.asdict(before_geometry),
        "engine": {
            "before_builder": dataclasses.asdict(before_builder),
            "after_builder": dataclasses.asdict(after_builder),
        },
        "airplane": _plane(plane),
        "renderer": _figure_contract(plane),
    }


def main() -> None:
    payload = {
        "schema": "w65-planform-area-evidence/v1",
        "reference": {
            "git_commit": _framework.alas_baseline(),
            "environment": _framework.environment(),
            "input": "ALASConfig.from_dict({'preset': name}) + get_preset(name).design_vector + AircraftBuilder(config.geometry).build(..., include_engines=True)",
        },
        "cases": [_case(name) for name in PRESETS],
    }
    OUT.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
