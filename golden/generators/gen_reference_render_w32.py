# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Capture the reference contracts used by the W3.2 geometry parity tests."""

from __future__ import annotations

import json
from pathlib import Path

import _framework

_framework.add_alas_to_path()

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

from alas.analysis.full_analysis import FullAnalysis  # noqa: E402
from alas.config.design_variables import DesignVector  # noqa: E402
from alas.config.settings import ALASConfig  # noqa: E402
from alas.geometry.aircraft_builder import AircraftBuilder  # noqa: E402
from alas.optimization.objective import OptimizationHistory  # noqa: E402
from alas.reporting import visualization as viz  # noqa: E402


ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "golden" / "report" / "reference_render_w32.json"


def axis_contract(axis):
    return {
        "kind": type(axis).__name__,
        "title": axis.get_title(),
        "xlabel": axis.get_xlabel(),
        "ylabel": axis.get_ylabel(),
        "aspect": str(axis.get_aspect()),
        "xlim": [float(v) for v in axis.get_xlim()],
        "ylim": [float(v) for v in axis.get_ylim()],
        "legend": [t.get_text() for t in axis.get_legend().get_texts()]
        if axis.get_legend() is not None
        else [],
        "series": [line.get_label() for line in axis.lines],
        "patches": [
            {
                "kind": type(patch).__name__,
                "label": patch.get_label(),
            }
            for patch in axis.patches
        ],
        "annotations": [text.get_text() for text in axis.texts],
        "texts": [text.get_text() for text in axis.texts],
    }


def capture(figure_id, factory, plane, theme):
    try:
        figure = factory(plane, theme=theme)
        if figure is None:
            return {"available": False, "reason": "reference factory returned None"}
        figure.canvas.draw()
        result = {
            "available": True,
            "theme": theme,
            "facecolor": [float(v) for v in figure.get_facecolor()],
            "panel_count": len(figure.axes),
            "suptitle": figure._suptitle.get_text() if figure._suptitle else "",
            "annotations": [text.get_text() for text in figure.texts],
            "axes": [axis_contract(axis) for axis in figure.axes],
        }
        plt.close(figure)
        return result
    except Exception as error:
        plt.close("all")
        return {"available": False, "reason": f"{type(error).__name__}: {error}"}


def main():
    config = ALASConfig()
    builder = AircraftBuilder(config.geometry)
    report = FullAnalysis(config).run(
        DesignVector(), include_engines=True, verbose=False
    )
    plane = report.airplane
    # The reference factory requires a real builder and a non-empty valid
    # optimization history. Reusing the pinned default design keeps this
    # evidence deterministic while exercising the public builder contract.
    history = OptimizationHistory()
    values = DesignVector().to_array()
    for cost in (1.0, 0.9, 0.8, 0.7):
        history.record(values, True, cost, ld=15.0, span=values[0], alpha=2.0, area=100.0, trim_ih=0.0)
    entries = {}
    factories = {
        "airfoil_evolution": viz.figure_airfoil_evolution,
        "threeview": viz.figure_asb_threeview,
        "design_evolution": lambda value, theme=None: viz.figure_design_evolution(
            history, builder, max_samples=4, theme=theme
        ),
        "planform_comparison": lambda value, theme=None: viz.figure_planform_comparison(
            value, value, theme=theme
        ),
        "wireframe_wing": viz.figure_wireframe_wing,
        "wireframe_fuselage": viz.figure_wireframe_fuselage,
        "wireframe_empennage": viz.figure_wireframe_empennage,
        "geometry": viz.figure_geometry,
    }
    for figure_id, factory in factories.items():
        for theme in ("light", "dark"):
            entries[f"{figure_id}:{theme}"] = capture(
                figure_id, factory, plane, theme
            )
    for theme in ("light", "dark"):
        entries[f"cabin_payload:{theme}"] = capture(
            "cabin_payload",
            lambda value, theme=None: viz.figure_cabin_payload(
                report.payload_layout, value, config, theme=theme
            ),
            plane,
            theme,
        )
    OUT.write_text(
        json.dumps(
            {
                "schema": "reference-render-w32/v1",
                "reference": {"git_commit": _framework.alas_baseline()},
                "figures": entries,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"wrote {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
