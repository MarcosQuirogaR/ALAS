# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Capture the pinned Python contracts for W3.3 mission and route figures."""

from __future__ import annotations

import json
from pathlib import Path
from types import SimpleNamespace

import _framework

_framework.add_alas_to_path()

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402

from alas.reporting import visualization as viz  # noqa: E402
from alas.routing.route import Route, Waypoint  # noqa: E402


ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "golden" / "report" / "reference_render_w33.json"
THEMES = ("light", "dark")


def axis_contract(axis):
    return {
        "title": axis.get_title(),
        "title_left": axis.get_title(loc="left"),
        "title_right": axis.get_title(loc="right"),
        "xlabel": axis.get_xlabel(),
        "ylabel": axis.get_ylabel(),
        "aspect": str(axis.get_aspect()),
        "xlim": [float(value) for value in axis.get_xlim()],
        "ylim": [float(value) for value in axis.get_ylim()],
        "series": [line.get_label() for line in axis.lines],
        "legend": [text.get_text() for text in axis.get_legend().get_texts()]
        if axis.get_legend() is not None
        else [],
        "annotations": [text.get_text() for text in axis.texts],
    }


def capture(factory, mission, route, theme, *, route_inputs=False):
    figure = factory(
        route,
        np.array([68_000.0, 67_800.0, 67_600.0]) if route_inputs else None,
        np.array([0.0, 10_000.0, 11_000.0]) if route_inputs else None,
        theme=theme,
    ) if route_inputs else factory(mission, theme=theme)
    figure.canvas.draw()
    result = {
        "available": True,
        "theme": theme,
        "facecolor": [float(value) for value in figure.get_facecolor()],
        "panel_count": len(figure.axes),
        "suptitle": figure._suptitle.get_text() if figure._suptitle else "",
        "axes": [axis_contract(axis) for axis in figure.axes],
    }
    plt.close(figure)
    return result


def representative_mission():
    time_s = np.array([0.0, 600.0, 1200.0])
    columns = {
        "Time_s": time_s,
        "Altitude_m": np.array([10_000.0, 10_500.0, 11_000.0]),
        "TAS_m_s": np.array([230.0, 232.0, 234.0]),
        "Mass_kg": np.array([68_000.0, 67_800.0, 67_600.0]),
        "SFC_kg_kgf_hr": np.array([0.0001, 0.00011, 0.00012]),
        "EAS_m_s": np.array([135.0, 136.0, 137.0]),
        "Mach": np.array([0.78, 0.785, 0.79]),
        "Range_m": np.array([0.0, 138_000.0, 277_000.0]),
        "Pitch_deg": np.array([1.0, 1.1, 1.2]),
        "AoA_deg": np.array([1.7, 1.8, 1.9]),
        "CL": np.array([0.5, 0.51, 0.52]),
        "CD": np.array([0.025, 0.0255, 0.026]),
        "L_over_D": np.array([20.0, 20.0, 20.0]),
        "Throttle": np.array([0.7, 0.71, 0.72]),
        "Lift_N": np.array([700_000.0, 695_000.0, 690_000.0]),
        "Thrust_N": np.array([36_000.0, 36_200.0, 36_400.0]),
        "Drag_N": np.array([35_000.0, 35_500.0, 36_000.0]),
        "CD_parasite": np.array([0.015, 0.0153, 0.0156]),
        "CD_induced": np.array([0.0075, 0.00765, 0.0078]),
        "CD_compressible": np.array([0.00125, 0.001275, 0.0013]),
        "CD_miscellaneous": np.array([0.00125, 0.001275, 0.0013]),
        "CD_total": np.array([0.025, 0.0255, 0.026]),
    }
    return SimpleNamespace(
        columns=columns,
        time_s=time_s,
        altitude_m=columns["Altitude_m"],
        tas_m_s=columns["TAS_m_s"],
        mass_kg=columns["Mass_kg"],
        summary={"fuel_burned_kg": 400.0, "block_time_s": 1200.0},
    )


def main():
    mission = representative_mission()
    route = Route(
        waypoints=[
            Waypoint(40.47, -3.56, ident="LEMD"),
            Waypoint(45.0, -2.0, ident="FIX"),
            Waypoint(51.15, -0.19, ident="EGKK"),
        ],
        source="simbrief_api",
    )
    factories = {
        "mission_profile": viz.figure_mission_profile,
        "mission_velocities": viz.figure_mission_velocities,
        "mission_flight_path": viz.figure_mission_flight_path,
        "mission_aero_coefficients": viz.figure_mission_aero_coefficients,
        "mission_aero_forces": viz.figure_mission_aero_forces,
        "mission_drag_components": viz.figure_mission_drag_components,
    }
    figures = {}
    for figure_id, factory in factories.items():
        for theme in THEMES:
            figures[f"{figure_id}:{theme}"] = capture(factory, mission, route, theme)
    for theme in THEMES:
        figures[f"mission_route_2d:{theme}"] = capture(
            viz.figure_mission_route_2d, mission, route, theme, route_inputs=True
        )
    payload = {
        "schema": "reference-render-w33/v1",
        "reference": {
            "git_commit": _framework.alas_baseline(),
            "themes": list(THEMES),
            "mission": "SimpleNamespace with explicit export_data columns",
            "route": "LEMD-FIX-EGKK, source=simbrief_api",
        },
        "unavailable_reasons": {
            "mission": "Representative input has mission_result=None; the registry requires native mission-stage telemetry.",
            "route": "Representative input has route=None; the registry requires route-stage telemetry or a planned route.",
        },
        "route_acceptance": {
            "mass_coloring": True,
            "legend": ["Origin", "Destination", "Total Mass (kg)"],
        },
        "figures": figures,
    }
    OUT.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
