# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Capture the reference contracts used by the W3.9 figure parity tests."""

from __future__ import annotations

import json
from pathlib import Path
from types import SimpleNamespace

import _framework

_framework.add_alas_to_path()

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

from alas.config.design_variables import DesignVector  # noqa: E402
from alas.optimization.objective import OptimizationHistory  # noqa: E402
from alas.reporting import visualization as viz  # noqa: E402
from alas.reporting.airfoil_sweep_figures import (  # noqa: E402
    fig_mses_verification,
    fig_ranking_bars,
    fig_rerank_2d_3d,
    fig_section_shapes,
    fig_trade_map,
)


ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "golden" / "report" / "reference_render_w39.json"


def axis_contract(axis):
    return {
        "title": axis.get_title(),
        "xlabel": axis.get_xlabel(),
        "ylabel": axis.get_ylabel(),
        "legend": [text.get_text() for text in axis.get_legend().get_texts()]
        if axis.get_legend() is not None
        else [],
        "series": [line.get_label() for line in axis.lines],
        "patch_count": len(axis.patches),
        "collections": [type(collection).__name__ for collection in axis.collections],
        "annotations": [text.get_text() for text in axis.texts],
    }


def capture(factory, value, theme):
    try:
        figure = factory(value, theme=theme)
        if figure is None:
            return {"available": False, "reason": "reference factory returned None"}
        figure.canvas.draw()
        result = {
            "available": True,
            "theme": theme,
            "panel_count": len(figure.axes),
            "axes": [axis_contract(axis) for axis in figure.axes],
        }
        plt.close(figure)
        return result
    except Exception as error:
        plt.close("all")
        return {"available": False, "reason": f"{type(error).__name__}: {error}"}


def candidate(name, refined=False, verified=False):
    return SimpleNamespace(
        name=name,
        status="ok",
        l_over_d=16.0,
        l_over_d_3d=14.0,
        l_over_d_mses=13.5,
        cdw_mses=0.0012,
        tank_capacity_kg=12000.0,
        max_thickness_frac=0.12,
        refined=refined,
        mses_verified=verified,
        is_reference=False,
    )


def main():
    history = OptimizationHistory()
    values = DesignVector().to_array()
    for ld, span in ((12.0, 28.0), (15.0, 30.0), (13.0, 29.0)):
        history.record(
            values,
            True,
            0.0,
            ld=ld,
            span=span,
            alpha=0.0,
            area=0.0,
            trim_ih=0.0,
        )

    result = SimpleNamespace(
        baseline_airfoil="naca0012",
        candidates=[candidate("naca0012", True, True), candidate("naca2412", True)],
    )
    factories = {
        "optimization_history": lambda value, theme=None: viz.figure_optimization_history(
            value, theme=theme
        ),
        "trade_map": fig_trade_map,
        "rerank_2d_3d": fig_rerank_2d_3d,
        "ranking_bars": fig_ranking_bars,
        "section_shapes": fig_section_shapes,
        "mses_verification": fig_mses_verification,
    }
    values_by_id = {
        "optimization_history": history,
        "trade_map": result,
        "rerank_2d_3d": result,
        "ranking_bars": result,
        "section_shapes": result,
        "mses_verification": result,
    }

    entries = {}
    for figure_id, factory in factories.items():
        for theme in ("light", "dark"):
            entries[f"{figure_id}:{theme}"] = capture(
                factory, values_by_id[figure_id], theme
            )

    OUT.write_text(
        json.dumps(
            {
                "schema": "reference-render-w39/v1",
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
