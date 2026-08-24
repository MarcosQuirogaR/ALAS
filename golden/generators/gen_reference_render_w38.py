# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Capture the source-derived W3.8 structural figure contract."""

from __future__ import annotations

import inspect

import _framework

_framework.add_alas_to_path()

from alas.reporting import visualization as viz  # noqa: E402


def _assert_reference_contract() -> None:
    functions = (
        viz.figure_structures_sizing,
        viz.figure_structures_loads,
        viz.figure_structures_stress,
        viz.figure_structures_modes,
        viz.figure_structures_vibration,
        viz.figure_structures_patran,
    )
    source = "\n".join(inspect.getsource(function) for function in functions)
    required = (
        "Spanwise position Y [m]",
        "Chordwise position X [m]",
        "Mass [kg]",
        "Bending moment M [MN.m]",
        "Margin of safety [-]",
        "Frequency [Hz]",
        "Normalized mode shape",
        "|H(f)| [m/N]",
        "RMS displacement [m]",
        "Requires a real NASTRAN sine/random-vibration solve",
        "Not run for this design",
    )
    missing = [value for value in required if value not in source]
    if missing:
        raise RuntimeError(f"reference structural contract changed: {missing}")


def main() -> None:
    _assert_reference_contract()
    structural_reason = viz._structures_unavailable_message(None)[1].split(
        " (", 1
    )[0]
    payload = {
        "schema": "reference-render-w38/v1",
        "reference": {
            "git_commit": _framework.alas_baseline(),
            "note": (
                "Axis and external-result contracts are source-derived from "
                "alas.reporting.visualization; delta and squared units are "
                "normalized to ASCII. The explicit missing-SOL-103 reason is "
                "native because the reference exposes only an unavailable panel."
            ),
        },
        "figures": {
            "structures_sizing": {
                "axis_labels": [
                    "Spanwise position Y [m]",
                    "Chordwise position X [m]",
                    "Estimate",
                    "Mass [kg]",
                ],
                "bars": 2,
            },
            "structures_loads": {
                "axis_labels": [
                    "Spanwise position Y [m]",
                    "Bending stiffness EI [GN.m^2]",
                    "Bending moment M [MN.m]",
                    "Deflection delta [m]",
                ]
            },
            "structures_stress": {
                "axis_labels": [
                    "Spanwise position Y [m]",
                    "Margin of safety [-]",
                ]
            },
            "structures_modes": {
                "axis_labels": [
                    "Mode",
                    "Frequency [Hz]",
                    "Spanwise position Y [m]",
                    "Normalized mode shape",
                ]
            },
            "structures_vibration": {
                "axis_labels": [
                    "Frequency [Hz]",
                    "|H(f)| [m/N]",
                    "Monitor grid",
                    "RMS displacement [m]",
                ]
            },
            "structures_patran": {
                "external_result": True,
                "ordered_images": True,
            },
        },
        "unavailable_reasons": {
            "structural": structural_reason,
            "modes_nastran": "NASTRAN SOL 103 was not run for this design",
            "vibration": "Requires a real NASTRAN sine/random-vibration solve",
            "patran": "Not run for this design",
        },
    }
    _framework.write(
        "report",
        "reference_render_w38",
        payload,
        description=(
            "Pinned W3.8 structural axis, external-result, and unavailable-state "
            "contracts derived from the reference visualization source, with the "
            "native missing-SOL-103 reason declared in the fixture."
        ),
    )


if __name__ == "__main__":
    main()
