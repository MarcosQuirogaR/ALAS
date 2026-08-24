# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Golden generator for `alas.analysis.full_analysis` (`FullAnalysis`).

Generates golden fixtures recording high-fidelity polar sweeps, design points,
parabolic polar fits, static margins, neutral points, and trimmed performance.
"""

from __future__ import annotations

import dataclasses
from pathlib import Path
import sys

import _framework

_framework.add_alas_to_path()

from alas.analysis.full_analysis import FullAnalysis  # noqa: E402
from alas.config.design_variables import DesignVector  # noqa: E402
from alas.config.settings import ALASConfig  # noqa: E402


def serialize_report(report) -> dict:
    return {
        "design": dataclasses.asdict(report.design),
        "design_point": {
            "alpha_deg": float(report.design_point.alpha_deg),
            "cl": float(report.design_point.cl),
            "cd": float(report.design_point.cd),
            "l_over_d": float(report.design_point.l_over_d),
        },
        "polar_fit": {
            "cd0": float(report.polar_fit.cd0),
            "k": float(report.polar_fit.k),
            "oswald_e": float(report.polar_fit.oswald_e),
            "aspect_ratio": float(report.polar_fit.aspect_ratio),
        },
        "polar": {
            "alpha_deg": [float(x) for x in report.polar["alpha"]],
            "cl": [float(x) for x in report.polar["CL"]],
            "cd": [float(x) for x in report.polar["CD"]],
            "cm": [float(x) for x in report.polar["Cm"]],
            "l_over_d": [float(x) for x in report.polar["L/D"]],
        },
        "static_margin": float(report.static_margin),
        "x_neutral_point": float(report.x_neutral_point),
        "physical_cg": [float(x) for x in report.physical_cg],
        "geometry_summary": {k: float(v) for k, v in report.geometry_summary.items()},
        "component_masses": {k: float(v) for k, v in report.component_masses.items()},
        "mass_coordinates": {k: [float(x) for x in v] for k, v in report.mass_coordinates.items()},
        "cg_envelope_ok": report.cg_envelope_ok,
        "trimmed_design_point": (
            {
                "alpha_deg": float(report.trimmed_design_point.alpha_deg),
                "trim_ih_deg": float(report.trimmed_design_point.trim_ih_deg),
                "cl": float(report.trimmed_design_point.cl),
                "cd": float(report.trimmed_design_point.cd),
                "l_over_d": float(report.trimmed_design_point.l_over_d),
                "cm_residual": float(report.trimmed_design_point.cm_residual),
            }
            if report.trimmed_design_point is not None
            else None
        ),
    }


def main():
    cases = {}

    # Case 1: Default configuration
    cfg_default = ALASConfig()
    dv_default = DesignVector()
    fa_default = FullAnalysis(cfg_default)
    rep_default = fa_default.run(dv_default, include_engines=True, verbose=False)
    cases["default"] = serialize_report(rep_default)

    # Case 2: Narrowbody / smaller sweep
    cfg_nb = ALASConfig()
    cfg_nb.requirements.cruise_mach = 0.78
    cfg_nb.requirements.cruise_altitude_m = 10668.0
    dv_nb = DesignVector(
        span_m=35.8,
        root_chord_m=6.5,
        break_chord_m=4.2,
        tip_chord_m=1.8,
        sweep_deg=25.0,
        fuselage_length_m=37.5,
    )
    fa_nb = FullAnalysis(cfg_nb)
    rep_nb = fa_nb.run(dv_nb, include_engines=True, verbose=False)
    cases["narrowbody"] = serialize_report(rep_nb)

    # Case 3: High aspect ratio / low sweep
    cfg_high_ar = ALASConfig()
    cfg_high_ar.requirements.cruise_mach = 0.75
    dv_high_ar = DesignVector(
        span_m=80.0,
        root_chord_m=12.0,
        break_chord_m=7.0,
        tip_chord_m=2.0,
        sweep_deg=22.0,
        fuselage_length_m=65.0,
    )
    fa_high_ar = FullAnalysis(cfg_high_ar)
    rep_high_ar = fa_high_ar.run(dv_high_ar, include_engines=True, verbose=False)
    cases["high_aspect_ratio"] = serialize_report(rep_high_ar)

    _framework.write(
        "pipeline",
        "full_analysis",
        cases,
        description="High-fidelity full aerodynamic and mass analysis runs across representative aircraft cases.",
    )


if __name__ == "__main__":
    main()
