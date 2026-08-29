# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Capture reference Matplotlib contracts for the Results figure registry.

This is an evidence generator, not a renderer test. It runs the reference
factories against one real default AVE analysis, records the visible numeric
and layout contract, and saves the resulting light/dark images for review.
Factories whose required upstream artifact is unavailable are recorded with a
reason instead of being supplied synthetic input.
"""

from __future__ import annotations

import json
from pathlib import Path
import re
from types import SimpleNamespace

import _framework

_framework.add_alas_to_path()

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402

from alas.analysis.full_analysis import FullAnalysis  # noqa: E402
from alas.config.design_variables import DesignVector  # noqa: E402
from alas.config.settings import ALASConfig  # noqa: E402
from alas.reporting import visualization as viz  # noqa: E402
from alas.sidecar import figures as registry  # noqa: E402


ROOT = Path(__file__).resolve().parents[2]
IMAGE_DIR = ROOT / "reference_figures" / "c0"
CORPUS_PATH = ROOT / "golden" / "report" / "reference_render_corpus.json"
THEMES = ("light", "dark")

REPRESENTATIVE_INPUT = {
    "aircraft": {
        "name": "AVE",
        "identity": "AVE / ALASConfig defaults",
    },
    "configuration": "ALASConfig()",
    "design_vector": "DesignVector()",
    "analysis": "FullAnalysis(ALASConfig()).run(DesignVector(), include_engines=True, verbose=False)",
    "optional_artifacts": {
        "mission_result": None,
        "mses_pressure": None,
        "mses_result": None,
        "structural_result": None,
        "route": None,
    },
}


def _json_value(value):
    if isinstance(value, np.generic):
        return value.item()
    if isinstance(value, np.ndarray):
        return value.tolist()
    if isinstance(value, (list, tuple)):
        return [_json_value(item) for item in value]
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    return str(value)


def _line_contract(line):
    return {
        "label": line.get_label(),
        "x": _json_value(line.get_xdata()),
        "y": _json_value(line.get_ydata()),
        "color": _json_value(line.get_color()),
        "linestyle": line.get_linestyle(),
        "linewidth": float(line.get_linewidth()),
        "marker": line.get_marker(),
        "markersize": float(line.get_markersize()),
    }


def _axis_unit(label):
    match = re.search(r"\[([^\]]+)\]|\(([^)]+)\)", label)
    if match is None:
        return None
    return match.group(1) or match.group(2)


def _legend_contract(ax, figure):
    legend = ax.get_legend()
    if legend is None:
        return {
            "present": False,
            "labels": [],
            "location": None,
            "bbox_pixels": None,
        }
    bbox = legend.get_window_extent()
    return {
        "present": True,
        "labels": [text.get_text() for text in legend.get_texts()],
        "location": _json_value(getattr(legend, "_loc", None)),
        "bbox_pixels": [float(x) for x in bbox.bounds],
    }


def _collection_contract(collection):
    contract = {
        "kind": type(collection).__name__,
        "label": collection.get_label(),
        "offsets": _json_value(collection.get_offsets())
        if hasattr(collection, "get_offsets")
        else None,
        "facecolors": _json_value(collection.get_facecolors())
        if hasattr(collection, "get_facecolors")
        else None,
        "edgecolors": _json_value(collection.get_edgecolors())
        if hasattr(collection, "get_edgecolors")
        else None,
        "sizes": _json_value(collection.get_sizes())
        if hasattr(collection, "get_sizes")
        else None,
    }
    if hasattr(collection, "get_paths"):
        contract["paths"] = [
            {
                "vertices": _json_value(path.vertices),
                "codes": _json_value(path.codes),
            }
            for path in collection.get_paths()
        ]
    else:
        contract["paths"] = []
    return contract


def _axes_contract(ax, figure, panel_index):
    position = ax.get_position()
    bbox_pixels = ax.get_window_extent().bounds
    xlabel = ax.get_xlabel()
    ylabel = ax.get_ylabel()
    return {
        "kind": type(ax).__name__,
        "panel_index": panel_index,
        "bbox_fraction": [float(x) for x in position.bounds],
        "bbox_inches": [float(x) for x in ax.get_window_extent().transformed(figure.dpi_scale_trans.inverted()).bounds],
        "bbox_pixels": [float(x) for x in bbox_pixels],
        "title": ax.get_title(),
        "xlabel": xlabel,
        "ylabel": ylabel,
        "axis_labels": {"x": xlabel, "y": ylabel},
        "units": {"x": _axis_unit(xlabel), "y": _axis_unit(ylabel)},
        "xlim": [float(x) for x in ax.get_xlim()],
        "ylim": [float(x) for x in ax.get_ylim()],
        "aspect": _json_value(ax.get_aspect()),
        "adjustable": ax.get_adjustable(),
        "aspect_constraint": {
            "aspect": _json_value(ax.get_aspect()),
            "adjustable": ax.get_adjustable(),
        },
        "xticks": _json_value(ax.get_xticks()),
        "xticklabels": [label.get_text() for label in ax.get_xticklabels()],
        "yticks": _json_value(ax.get_yticks()),
        "yticklabels": [label.get_text() for label in ax.get_yticklabels()],
        "ticks": {
            "x": {
                "values": _json_value(ax.get_xticks()),
                "labels": [label.get_text() for label in ax.get_xticklabels()],
            },
            "y": {
                "values": _json_value(ax.get_yticks()),
                "labels": [label.get_text() for label in ax.get_yticklabels()],
            },
        },
        "legend": (
            [text.get_text() for text in ax.get_legend().get_texts()]
            if ax.get_legend() is not None
            else []
        ),
        "legend_contract": _legend_contract(ax, figure),
        "series": [_line_contract(line) for line in ax.lines],
        "patches": [
            {
                "label": patch.get_label(),
                "facecolor": _json_value(patch.get_facecolor()),
                "edgecolor": _json_value(patch.get_edgecolor()),
                "bbox_pixels": _json_value(patch.get_window_extent().bounds),
                "vertices": _json_value(patch.get_path().vertices),
                "codes": _json_value(patch.get_path().codes),
            }
            for patch in ax.patches
        ],
        "collections": [_collection_contract(collection) for collection in ax.collections],
        "annotations": [
            {
                "text": text.get_text(),
                "position": _json_value(text.get_position()),
                "transform": type(text.get_transform()).__name__,
            }
            for text in ax.texts
        ],
        "texts": [text.get_text() for text in ax.texts],
        "bounding_box": {
            "fraction": [float(x) for x in position.bounds],
            "inches": [
                float(x)
                for x in ax.get_window_extent()
                .transformed(figure.dpi_scale_trans.inverted())
                .bounds
            ],
            "pixels": [float(x) for x in bbox_pixels],
        },
    }


def _figure_contract(fig, figure_id, theme, image_path):
    fig.canvas.draw()
    width, height = fig.get_size_inches()
    return {
        "schema": "reference-render-contract/v1",
        "id": figure_id,
        "theme": theme,
        "available": True,
        "representative_input": REPRESENTATIVE_INPUT,
        "image": str(image_path.relative_to(ROOT)).replace("\\", "/"),
        "figure": {
            "size_inches": [float(width), float(height)],
            "dpi": float(fig.dpi),
            "facecolor": _json_value(fig.get_facecolor()),
            "panel_count": len(fig.axes),
            "axes": [_axes_contract(ax, fig, index) for index, ax in enumerate(fig.axes)],
            "annotations": [
                {
                    "text": text.get_text(),
                    "position": _json_value(text.get_position()),
                    "transform": type(text.get_transform()).__name__,
                }
                for text in fig.texts
            ],
            "bounding_box": {
                "inches": [0.0, 0.0, float(width), float(height)],
                "pixels": [0.0, 0.0, float(width * fig.dpi), float(height * fig.dpi)],
            },
        },
    }


def _unavailable_contract(figure_id, theme, reason):
    return {
        "schema": "reference-render-contract/v1",
        "id": figure_id,
        "theme": theme,
        "available": False,
        "representative_input": REPRESENTATIVE_INPUT,
        "image": None,
        "reason": reason,
        "figure": None,
    }


def _none_reason(figure_id):
    if figure_id in {"optimization_history", "design_evolution"}:
        return (
            "Representative FullAnalysis run did not produce optimization_result/history; "
            "the registry requires an optimization-stage artifact."
        )
    if figure_id == "mission_route_2d":
        return (
            "Representative input has route=None; the registry requires route-stage "
            "telemetry or a planned route."
        )
    if figure_id.startswith("mission_"):
        return (
            "Representative input has mission_result=None; the registry requires "
            "native mission-stage telemetry."
        )
    if figure_id in {"mses_pressure", "mses_mach_contours"}:
        return (
            "Representative input has mses_pressure=None; the registry requires a "
            "successful MSES pressure/flowfield export."
        )
    return "Reference registry returned None for the representative run."


def _result_bullet_names():
    text = (ROOT / "MISSING-THINGS.md").read_text(encoding="utf-8")
    section = text.split("## Results", 1)[1].split("## Physics discrepancies", 1)[0]
    return [match.group(1).strip() for match in re.finditer(r"^- ([^:]+):", section, re.M)]


def _contract_id(bullet):
    aliases = {
        "mses figures": "mses_pressure+mses_mach_contours",
        "wireframe figures": "wireframe_wing+wireframe_fuselage+wireframe_empennage",
        "asb_threeviews": "asb_threeview",
        "stability_sideview": "stability_side_view",
    }
    return aliases.get(bullet, bullet)


def _representative_input():
    return {
        "aircraft": dict(REPRESENTATIVE_INPUT["aircraft"]),
        "configuration": REPRESENTATIVE_INPUT["configuration"],
        "design_vector": REPRESENTATIVE_INPUT["design_vector"],
        "analysis": REPRESENTATIVE_INPUT["analysis"],
        "optional_artifacts": dict(REPRESENTATIVE_INPUT["optional_artifacts"]),
    }


def main():
    reference_commit = _framework.alas_baseline()
    # Calling the real analysis is intentional: a render corpus must not use
    # hand-written arrays that happen to make a chart look plausible.
    config = ALASConfig()
    report = FullAnalysis(config).run(
        DesignVector(), include_engines=True, verbose=False
    )
    result = SimpleNamespace(
        config=config,
        optimized_report=report,
        baseline_report=report,
        optimized_design=report.design,
        mses_pressure=None,
        mses_result=None,
        mission_result=None,
        structural_result=None,
    )

    # Use the reference registry so the corpus follows the public figure IDs
    # and ordering instead of maintaining a second list of factories.
    contracts = {}
    for figure_id, factory in registry.RESULT_FIGURES.items():
        for theme in THEMES:
            key = f"{figure_id}:{theme}"
            try:
                figure = factory(result, theme)
                if figure is None:
                    contracts[key] = _unavailable_contract(
                        figure_id,
                        theme,
                        _none_reason(figure_id),
                    )
                    continue
                image_path = IMAGE_DIR / f"{figure_id}__{theme}.png"
                IMAGE_DIR.mkdir(parents=True, exist_ok=True)
                figure.savefig(image_path, dpi=150)
                contracts[key] = _figure_contract(figure, figure_id, theme, image_path)
                plt.close(figure)
            except Exception as error:  # evidence must preserve the exact blocker
                contracts[key] = _unavailable_contract(
                    figure_id,
                    theme,
                    f"Reference factory raised {type(error).__name__}: {error}",
                )
                plt.close("all")

    named = {}
    for bullet in _result_bullet_names():
        contract_id = _contract_id(bullet)
        figure_ids = contract_id.split("+")
        theme_contracts = {}
        for theme in THEMES:
            entries = [contracts.get(f"{figure_id}:{theme}") for figure_id in figure_ids]
            missing = [
                f"{figure_id}:{theme}: {entry.get('reason', 'unknown reason')}"
                for figure_id, entry in zip(figure_ids, entries)
                if entry is None or not entry.get("available", False)
            ]
            theme_contracts[theme] = {
                "status": "captured" if not missing else "unavailable",
                "figure_contracts": [f"figures.{figure_id}:{theme}" for figure_id in figure_ids],
                "reasons": missing,
            }

        named[bullet] = {
            "schema": "results-bullet-contract/v1",
            "representative_input": _representative_input(),
            "contract": f"results.{contract_id}",
            "reference_figure_ids": figure_ids,
            "themes": theme_contracts,
            "status": (
                "captured"
                if all(item["status"] == "captured" for item in theme_contracts.values())
                else "unavailable"
            ),
        }

    payload = {
        "schema": "reference-render-corpus/v1",
        "reference": {
            "git_commit": reference_commit,
            "representative_input": _representative_input(),
            "environment": _framework.environment(),
            "themes": list(THEMES),
        },
        "results_bullets": named,
        "figures": contracts,
    }
    CORPUS_PATH.parent.mkdir(parents=True, exist_ok=True)
    with CORPUS_PATH.open("w", encoding="utf-8", newline="\n") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print(f"wrote {CORPUS_PATH.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
