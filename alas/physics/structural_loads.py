# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Shared spanwise load model for the wingbox.

One elliptic-lift (+ optional inertial-relief) distributed load, integrated
to shear/moment via a cantilever (tip -> root) numerical integral. Used by
BOTH :mod:`alas.physics.structural_sizing` (strength sizing, load only,
**no** relief -- the conservative choice, matching the reference scripts'
own ``00_sizing.py``, which didn't include relief either) and
:mod:`alas.physics.structural_analysis` (deflection estimate, **with**
relief -- matching the reference's ``05_validation.py``, which added relief
specifically to get a closer match to real NASTRAN deflections). Keeping
one shared load-integration primitive is what guarantees sizing, the
analytical solver, and the NASTRAN BDF FORCE cards never disagree about the
load model, unlike the reference scripts (whose sizing and simulation
scripts derived loads independently).
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import List, Tuple

import numpy as np

from ..config.geometry_config import EngineConfig
from ..config.mass_config import MassModelConfig
from ..config.requirements import DesignRequirements


@dataclass(frozen=True)
class LoadCase:
    """One structural design load case for the semi-wing."""

    name: str  # "pull-up" | "push-down" | "level"
    load_factor: float  # signed ultimate load factor n (already includes any additional safety factor)
    total_force_n: (
        float  # signed total aerodynamic force on this semi-wing = n * mtow_kg * g / 2
    )


def load_cases(
    req: DesignRequirements, additional_safety_factor: float = 1.0
) -> List[LoadCase]:
    """Pull-up ultimate / push-down ultimate / 1g level.

    Reuses :class:`DesignRequirements`' own ``ultimate_load_factor`` /
    ``limit_load_factor_neg`` fields with *exactly* the same derivation
    :func:`alas.physics.performance`'s V-n diagram already uses
    (``n_ult_pos = ultimate_load_factor``, ``n_ult_neg = limit_load_factor_neg
    * 1.5``) -- so the structural loads always match the V-n diagram already
    shown elsewhere in the app, not a second, independently-tuned load case.
    """
    g = req.gravity_m_s2
    w_n = req.mtow_kg * g
    n_ult_pos = req.ultimate_load_factor * additional_safety_factor
    n_ult_neg = req.limit_load_factor_neg * 1.5 * additional_safety_factor
    return [
        LoadCase("pull-up", n_ult_pos, n_ult_pos * w_n / 2.0),
        LoadCase("push-down", n_ult_neg, n_ult_neg * w_n / 2.0),
        LoadCase("level", 1.0, 1.0 * w_n / 2.0),
    ]


def elliptic_distributed_load(
    y: np.ndarray, semi_span: float, total_force_n: float
) -> np.ndarray:
    """Half-elliptic spanwise load distribution [N/m], integrating to
    ``total_force_n`` over ``[0, semi_span]``. A classic, well-precedented
    preliminary-design simplification for wing structural loads (the same
    one the reference scripts used, validated there to <20% vs. real
    NASTRAN deformations)."""
    q0 = 4.0 * total_force_n / (np.pi * semi_span)
    return q0 * np.sqrt(np.clip(1.0 - (y / semi_span) ** 2, 0.0, 1.0))


def cantilever_shear_moment(
    y: np.ndarray, q_net: np.ndarray
) -> Tuple[np.ndarray, np.ndarray]:
    """Shear V(y) and bending moment M(y) for a cantilever beam (free at the
    tip, fixed at the root) under a net distributed load ``q_net`` [N/m]
    sampled at ``y``, via cumulative trapezoidal integration from tip to
    root (root reaction is never referenced directly -- V/M at y=0 fall out
    of the integral, matching the reference's own approach)."""
    n = len(y)
    dy = np.diff(y)
    q_seg = 0.5 * (q_net[:-1] + q_net[1:]) * dy
    v = np.zeros(n)
    v[:-1] = np.cumsum(q_seg[::-1])[::-1]

    v_seg = 0.5 * (v[:-1] + v[1:]) * dy
    m = np.zeros(n)
    m[:-1] = np.cumsum(v_seg[::-1])[::-1]
    return v, m


def engine_point_loads_n(
    engine_cfg: EngineConfig,
    mass_cfg: MassModelConfig,
    req: DesignRequirements,
) -> List[Tuple[float, float]]:
    """Per-engine (y_position_m, dry_mass_kg) for every WING-mounted engine
    on the modeled (positive-Y, right) semi-wing.

    ``spanwise_positions_m`` lists BOTH wings' engines for the full
    aircraft (e.g. a symmetric twin is ``(9.8, -9.8)``); since the FEM only
    models one semi-wing, only ``y > 0`` stations are returned -- otherwise
    a symmetric pair would double-count one engine's mass onto a single
    semi-wing node (both +9.8 and -9.8 are the same distance from the
    root). A ``y == 0`` entry is a centerline/tail-mounted engine (e.g. a
    DC-10-style third engine), which doesn't load either wing and is
    skipped the same way.

    Reuses :mod:`alas.physics.mass`'s own per-engine dry-mass formula
    (thrust/TWR, scaled by the installation factor) rather than a new
    constant, so the wing FEM's engine point mass is never out of sync with
    the mass model's own propulsion-mass estimate.
    """
    thrust_n = engine_cfg.thrust_kn * 1000.0
    if thrust_n <= 0:
        return []
    m_engine = (
        thrust_n / (mass_cfg.propulsion_twr_factor * req.gravity_m_s2)
    ) * mass_cfg.propulsion_installation_factor
    return [
        (float(y_pos), m_engine)
        for y_pos in engine_cfg.spanwise_positions_m
        if y_pos > 1e-6
    ]
