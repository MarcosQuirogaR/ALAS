# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Design reporting and data export.

Serialises an :class:`AnalysisReport` into:

* a console summary (human-readable design-point table),
* a JSON design database (machine-readable handoff for SUAVE and other tools),
* the optimized airfoil ``.dat`` file.

The JSON schema is intentionally explicit and flat so downstream tools can read
it without importing ALAS.
"""

from __future__ import annotations

import dataclasses
import json
from pathlib import Path
from typing import Any, Dict

import numpy as np

from ..analysis.full_analysis import AnalysisReport
from ..config.settings import ALASConfig
from ..data.airfoil_data import NAMED_COORDINATES
from ..geometry.airfoils import AirfoilLibrary, build_section
from ..physics.mass import OEW_KEYS


def report_to_dict(report: AnalysisReport, config: ALASConfig) -> Dict[str, Any]:
    """Build the serialisable design database from an analysis report."""
    req = config.requirements
    return {
        "metadata": {
            "name": "ALAS optimized design",
            "source": "alas.pipeline",
            "tool": "ALAS",
        },
        "design_vector": dataclasses.asdict(report.design),
        "geometry": report.geometry_summary,
        "aerodynamics": {
            "cruise_mach": req.cruise_mach,
            "cruise_altitude_m": req.cruise_altitude_m,
            "cd0_cruise": report.polar_fit.cd0,
            "k_factor": report.polar_fit.k,
            "oswald_efficiency": report.polar_fit.oswald_e,
            "aspect_ratio": report.polar_fit.aspect_ratio,
            "design_point": dataclasses.asdict(report.design_point),
            "static_margin": report.static_margin,
            "trimmed_design_point": (
                dataclasses.asdict(report.trimmed_design_point)
                if report.trimmed_design_point is not None
                else None
            ),
        },
        "weights": {
            "mtow_kg": req.mtow_kg,
            "physical_cg_m": report.physical_cg,
            "component_masses_kg": report.component_masses,
            "mass_coordinates_m": report.mass_coordinates,
        },
    }


def export_json(
    report: AnalysisReport, config: ALASConfig, path: str | Path
) -> Dict[str, Any]:
    """Write the design database to ``path`` as JSON and return the dict."""
    data = report_to_dict(report, config)
    path = Path(path)
    with path.open("w", encoding="utf-8") as f:
        json.dump(data, f, indent=4)
    print(f"  [file] design database written: {path}")
    return data


def export_airfoil_dat(
    report: AnalysisReport,
    config: ALASConfig,
    path: str | Path,
    name: str = "ALAS_Optimized",
) -> None:
    """Reconstruct and save the optimized root section as a Selig ``.dat`` file."""
    base_name = config.geometry.wing.root_airfoil
    base_coords = NAMED_COORDINATES.get(base_name)
    if base_coords is None:
        base_coords = AirfoilLibrary.get(base_name).coordinates
    section = build_section(report.design, base_coords)
    path = Path(path)
    np.savetxt(path, section.coordinates, fmt="%.6f", header=name, comments="")
    print(f"  [file] optimized airfoil written: {path}")


def format_summary(report: AnalysisReport, config: ALASConfig | None = None) -> str:
    """Return a concise design-point summary as a string (for console or GUI).

    When ``config`` is supplied the specified MTOW is shown alongside the
    model-computed mass total, making it obvious when the two diverge (e.g.
    when the fuel budget is negative).
    """
    dp = report.design_point
    pf = report.polar_fit
    g = report.geometry_summary

    m = report.component_masses
    m_oew = sum(m.get(k, 0.0) for k in OEW_KEYS)
    m_total = m_oew + m.get("Payload", 0.0) + m.get("Fuel", 0.0)

    mtow_specified = config.requirements.mtow_kg if config else None

    lines = [
        "=========== ALAS Design Summary ===========",
        f"  Cruise alpha     : {dp.alpha_deg:6.2f} deg",
        f"  Cruise CL        : {dp.cl:6.3f}",
        f"  Cruise CD        : {dp.cd:7.5f}",
        f"  L/D (cruise)     : {dp.l_over_d:6.2f}",
        f"  Aero static marg : {report.static_margin * 100:5.1f} %",
    ]
    tdp = report.trimmed_design_point
    if tdp is not None:
        lines.append(
            f"  Trimmed alpha    : {tdp.alpha_deg:6.2f} deg  (h-stab i_h = {tdp.trim_ih_deg:5.2f} deg, "
            f"L/D = {tdp.l_over_d:5.2f})"
        )
    lines.append("  ------------------ Weights -------------------")
    if mtow_specified is not None:
        lines.append(f"  MTOW (specified) : {mtow_specified:,.0f} kg")
    lines += [
        f"  MTOW (computed)  : {m_total:,.0f} kg",
        f"  OEW              : {m_oew:,.0f} kg",
        f"  Payload          : {m.get('Payload', 0.0):,.0f} kg",
        f"  Fuel             : {m.get('Fuel', 0.0):,.0f} kg",
        f"  Wing structure   : {m.get('Wing', 0.0):,.0f} kg",
        f"  Fuselage struct  : {m.get('Fuselage', 0.0):,.0f} kg",
        f"  Physical CG X    : {report.physical_cg[0]:6.2f} m"
        if report.physical_cg
        else "  Physical CG X    :    N/A",
        "  ----------------------------------------------",
        f"  CD0 (clean fit)  : {pf.cd0:7.5f}",
        f"  k factor         : {pf.k:6.4f}",
        f"  Oswald e         : {pf.oswald_e:6.3f}",
        f"  Aspect ratio     : {pf.aspect_ratio:6.2f}",
        "  ----------------------------------------------",
        f"  Span             : {g['span_m']:6.2f} m",
        f"  Wing area        : {g['wing_area_m2']:6.1f} m^2",
        f"  Taper ratio      : {g['taper_ratio']:6.3f}",
        f"  Sweep            : {g['sweep_deg']:6.1f} deg",
        f"  Fuselage length  : {g['fuselage_length_m']:6.2f} m",
        "================================================",
    ]
    return "\n".join(lines)


def print_summary(report: AnalysisReport, config: ALASConfig | None = None) -> None:
    """Print a concise design-point summary to the console."""
    print("\n" + format_summary(report, config))
