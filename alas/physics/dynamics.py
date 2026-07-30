# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Longitudinal/lateral-directional dynamic-mode analysis.

Distinct from ``stability.py`` (static trim/CG/neutral-point analysis): this
module answers "how does the aircraft respond over time to a disturbance",
via the classical small-perturbation eigenmodes (phugoid, short-period,
dutch roll, roll subsidence, spiral) computed from VLM stability derivatives.

AeroSandbox already implements the eigenmode solve itself
(``aerosandbox.dynamics.flight_dynamics.airplane.get_modes``); this module
supplies the two inputs ALAS doesn't otherwise compute -- the VLM
stability-derivative dict and an inertia estimate -- and wraps the result in
a small, consistently-shaped dataclass.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Dict, Tuple

import aerosandbox as asb


def estimate_inertia(plane: asb.Airplane, mass_kg: float) -> Tuple[float, float, float]:
    """Radius-of-gyration estimate of (Ixx, Iyy, Izz) [kg.m^2].

        rx = 0.25 * span,  ry = 0.38 * fuselage_length,  rz = 0.40 * fuselage_length

    A well-established conceptual-design approximation for transport
    aircraft (used before a real structural mass distribution exists to
    estimate moments of inertia from gross geometry alone) -- not a
    substitute for a real mass-properties model. Same disclaimer convention
    as ``config/engines.py``'s OPR/FPR/turbine-inlet-temperature estimates.
    """
    fus = plane.fuselages[0]
    fus_len = float(fus.xsecs[-1].xyz_c[0] - fus.xsecs[0].xyz_c[0])
    span = float(plane.wings[0].span())
    rx, ry, rz = 0.25 * span, 0.38 * fus_len, 0.40 * fus_len
    return mass_kg * rx**2, mass_kg * ry**2, mass_kg * rz**2


@dataclass
class DynamicMode:
    """One eigenmode of the linearized small-perturbation dynamics."""

    name: str
    eigenvalue_real: float
    eigenvalue_imag: float
    damping_ratio: float
    period_s: float  # 2*pi / |eigenvalue|; 0.0 for a purely real (aperiodic) mode
    stable: bool  # eigenvalue_real < 0


def compute_dynamic_modes(
    plane: asb.Airplane,
    op_point: asb.OperatingPoint,
    mass_props: asb.MassProperties,
) -> Dict[str, DynamicMode]:
    """Longitudinal (phugoid, short-period) and lateral-directional (dutch
    roll, roll subsidence, spiral) dynamic modes at ``op_point``.

    Runs a fresh VLM stability-derivative solve and hands the result to
    AeroSandbox's own ``dynamics.flight_dynamics.airplane.get_modes``, which
    needs exactly the derivative set ``run_with_stability_derivatives
    (alpha=True, beta=True, p=True, q=True, r=True)`` produces (CL, CD, Cma,
    Cmq, Clp, CYb, Cnb, CYr, Cnr, Clb, Clr). That call runs ~6 separate
    finite-difference VLM solves internally (one per perturbed axis), so
    resolution matters a lot here -- benchmarked directly: spanwise
    resolution 1 vs. 4 changes the phugoid/short-period eigenvalues by
    <1% and dutch-roll by ~12% (still well within conceptual-design
    accuracy) while cutting wall time from ~9s to ~0.8s, so this stays at
    the project's standard analysis-fidelity default rather than the
    higher resolution used for pure aerodynamic-shape figures like
    ``figure_vlm_flow``.
    """
    import aerosandbox.dynamics.flight_dynamics.airplane as flight_dynamics

    vlm = asb.VortexLatticeMethod(
        airplane=plane,
        op_point=op_point,
        spanwise_resolution=1,
        verbose=False,
    )
    aero = vlm.run_with_stability_derivatives(
        alpha=True, beta=True, p=True, q=True, r=True
    )

    raw = flight_dynamics.get_modes(
        airplane=plane,
        op_point=op_point,
        mass_props=mass_props,
        aero=aero,
    )

    modes: Dict[str, DynamicMode] = {}
    for key, m in raw.items():
        re, im = float(m["eigenvalue_real"]), float(m["eigenvalue_imag"])
        wn = math.hypot(re, im)
        period = (2.0 * math.pi / wn) if wn > 0 else 0.0
        damping = m["damping_ratio"]
        if hasattr(damping, "item"):
            damping = damping.item()
        modes[key] = DynamicMode(
            name=key,
            eigenvalue_real=re,
            eigenvalue_imag=im,
            damping_ratio=float(damping),
            period_s=period,
            stable=re < 0.0,
        )
    return modes
