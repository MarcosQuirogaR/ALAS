# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Builds the ``vehicle`` half of the SUAVE mission request from an
:class:`~alas.analysis.full_analysis.AnalysisReport` and the
:class:`~alas.config.settings.ALASConfig` it was produced from.

Reuses :func:`alas.reporting.design_report.report_to_dict`'s geometry
summary rather than re-deriving it, and passes ``config.geometry`` straight
through (its field names already match what
``external tools/suave_runner/vehicle_builder.py`` expects -- see
:mod:`alas.config.geometry_config`).
"""

from __future__ import annotations

import dataclasses
from typing import Any, Dict, Optional

from ..analysis.full_analysis import AnalysisReport
from ..config.settings import ALASConfig


def _cruise_thrust_kn_per_engine(
    report: AnalysisReport, config: ALASConfig
) -> Optional[float]:
    """Per-engine thrust AT the cruise design point -- what SUAVE's
    ``turbofan_sizing(turbofan, mach_number, altitude)`` actually expects as
    ``thrust.total_design`` when (as here) it's sized at the cruise Mach/
    altitude (confirmed against SUAVE's own Boeing 737 example vehicle, which
    sizes ``total_design`` to the cruise-thrust-required at 35,000 ft/M0.78 --
    about 24 kN/engine for a CFM56 rated near 120 kN static -- NOT the
    engine's sea-level-static rating).

    ``EngineConfig.thrust_kn`` is the STATIC (sea-level, M0=0) rated thrust
    (see physics/propulsion.py's module docstring and the Propulsion Analysis
    tab's "static (rated)" line) -- feeding that value in directly as
    ``total_design`` at the cruise condition told SUAVE the engine could
    produce its full static rating at cruise altitude/Mach too, which
    oversized the sized engine by roughly the static-to-cruise thrust lapse
    ratio (commonly ~4-5x for a high-BPR turbofan) and made every mission
    segment's solved throttle read far too low (e.g. ~0.2 at cruise).

    Computed directly from THIS aircraft's own trimmed cruise L/D and MTOW
    (thrust required = drag = weight / (L/D) in steady level flight) rather
    than routed through physics/propulsion.py's separate closed-form cycle
    model -- that module's own docstring flags it as "a different fidelity
    level... not an independent validation" of SUAVE's numerically-solved
    network, so using its cruise-point estimate here would size SUAVE's
    engine against a second approximate model instead of this aircraft's own
    (already-computed) aerodynamics. Sizing off MTOW (the heaviest, most
    demanding point) rather than a lighter mid-cruise weight leaves the
    expected, realistic margin for a cruise-climb profile: throttle starts
    near this reference and eases down as fuel burns off. Returns ``None``
    if no trimmed/untrimmed design point is available, so the caller can
    fall back safely.
    """
    dp = report.trimmed_design_point or report.design_point
    if dp is None or dp.l_over_d <= 0:
        return None
    n_engines = len(config.geometry.engine.spanwise_positions_m)
    thrust_required_n = config.requirements.mtow_kg * 9.81 / dp.l_over_d
    return thrust_required_n / n_engines / 1000.0


def build_vehicle_request(report: AnalysisReport, config: ALASConfig) -> Dict[str, Any]:
    req = config.requirements
    engine = config.geometry.engine
    n_engines = len(engine.spanwise_positions_m)
    cruise_thrust_kn = _cruise_thrust_kn_per_engine(report, config)

    return {
        "name": config.preset or "ALAS_Design",
        "design_vector": dataclasses.asdict(report.design),
        "geometry_summary": report.geometry_summary,
        "geometry_config": dataclasses.asdict(config.geometry),
        "mtow_kg": req.mtow_kg,
        "component_masses_kg": report.component_masses,
        # Read from the design's own live EngineConfig, not a fresh
        # ENGINE_DATABASE lookup by name -- so an Engine Designer edit (or a
        # hand-tuned thrust/BPR/OPR/FPR/TIT) reaches SUAVE's turbofan sizing
        # exactly like it reaches mass estimation and the Propulsion
        # Analysis tab, with no second copy of the data to drift out of sync.
        "engine": {
            "n_engines": n_engines,
            "thrust_kn": engine.thrust_kn,
            # Per-engine thrust AT the cruise design point (mach/altitude
            # below) -- what SUAVE's turbofan_sizing() actually needs as
            # thrust.total_design when sized at that reference point; see
            # _cruise_thrust_kn_per_engine's docstring. None (cycle
            # infeasible) falls back to the static rating in vehicle_builder.
            "cruise_thrust_kn": cruise_thrust_kn,
            "bypass_ratio": engine.bypass_ratio,
            "nacelle_length_m": engine.nacelle_length_m(),
            "nacelle_max_radius_m": engine.radius_scale_m,
            "overall_pressure_ratio": engine.overall_pressure_ratio,
            "turbine_inlet_temp_k": engine.turbine_inlet_temp_k,
            "fan_pressure_ratio": engine.fan_pressure_ratio,
        },
        "requirements": {
            "aircraft_type": req.aircraft_type,
            "num_passengers": req.num_passengers,
            "cruise_mach": req.cruise_mach,
            "cruise_altitude_m": req.cruise_altitude_m,
            "ultimate_load_factor": req.ultimate_load_factor,
        },
    }
