# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Direct strength-based wingbox sizing.

Generalizes ``Reference Scripts/00_sizing.py`` -- which iteratively bisected
a cap-area scale factor to hit one assignment's specific Torenbeek mass
target (``MASS_TARGET_KG = 21,000``) -- to any number of spars, with **no**
mass-target bisection: ALAS has no equivalent external target for an
arbitrary generated aircraft, so caps are sized directly from strength
(margin of safety = 0 by construction at the root, the bending-critical
station), which is simpler and more physically defensible (genuinely
minimum-mass-for-given-safety-factor, not mass-matched-to-a-guess).

What's kept from the reference, because it's a legitimate structural/
manufacturing convention rather than an artifact of the mass-target search:
the spar-cap **taper law** (full root section up to ``cap_taper_eta_lock``,
then linear taper to ``cap_taper_tip_fraction`` at the tip -- locking the
highest-moment inboard region preserves most of the tip-deflection
stiffness) and the geometric cap-width/height limits (``w_cap <= 0.5*chord``
and ``<= 0.6*H``, ``t_cap <= H/3`` so the cap always fits inside the
airfoil).

Loads come from :mod:`alas.physics.structural_loads` (elliptic
distribution, **no** inertial relief -- the conservative choice for
strength sizing, matching the reference's own ``00_sizing.py``, which also
didn't include relief). Moment/shear are split across spars weighted by
each spar's local section depth (generalizes the reference's fixed 70/30
front/rear split to N spars of arbitrary relative depth).
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Dict, List

import numpy as np

from ..config.materials import MaterialSpec
from ..config.requirements import DesignRequirements
from ..config.structures_config import StructuresConfig
from . import structural_loads as loads
from .structural_loads import LoadCase
from ..geometry.wing_structure import WingStructureGeometry


@dataclass
class SparSizing:
    """Per-spar sizing result, sampled at ``WingboxSizing.y_stations``."""

    chord_fraction: float
    h: np.ndarray  # free web height at each station [m]
    w_cap: np.ndarray  # cap flange width (tapered) [m]
    t_cap: np.ndarray  # cap flange thickness (tapered) [m]
    a_cap: np.ndarray  # one-flange cap area [m^2]
    t_web: float  # uniform web thickness [m]
    frac_moment: np.ndarray  # bending-moment fraction this spar carries, per station
    margin_of_safety: (
        np.ndarray
    )  # MS at each station (>= 0 expected by construction near the root)


@dataclass
class WingboxSizing:
    y_stations: np.ndarray
    eta_stations: np.ndarray
    chord: np.ndarray
    spar_fracs: List[float]
    spars: List[SparSizing] = field(default_factory=list)
    t_skin: float = 0.0
    num_ribs: int = 0
    rib_spacing_m: float = 0.0
    mass_breakdown_kg: Dict[str, float] = field(default_factory=dict)
    total_mass_kg: float = 0.0
    sizing_load_case: str = ""


def _cap_taper(eta: np.ndarray, eta_lock: float, tip_fraction: float) -> np.ndarray:
    return np.where(
        eta <= eta_lock,
        1.0,
        1.0 - (1.0 - tip_fraction) * (eta - eta_lock) / max(1.0 - eta_lock, 1e-9),
    )


def size_wingbox(
    wsg: WingStructureGeometry,
    cfg: StructuresConfig,
    req: DesignRequirements,
    skin_mat: MaterialSpec,
    web_mat: MaterialSpec,
    cap_mat: MaterialSpec,
    rib_mat: MaterialSpec,
) -> WingboxSizing:
    n = cfg.spanwise_stations
    y = np.linspace(0.0, wsg.semi_span, n)
    eta = y / wsg.semi_span
    chord = np.array([wsg.local_chord(e) for e in eta])

    cases = loads.load_cases(req, cfg.additional_safety_factor)
    # Strength sizing uses whichever case produces the larger |root moment| --
    # for a standard elliptic-load cantilever this is always the load case
    # with the largest |total_force_n|, but comparing moments directly is
    # robust to future load-model changes.
    worst_case: LoadCase | None = None
    worst_m0 = -1.0
    case_moments = {}
    for case in cases:
        q = loads.elliptic_distributed_load(y, wsg.semi_span, case.total_force_n)
        _, m = loads.cantilever_shear_moment(y, q)
        case_moments[case.name] = (q, m)
        if abs(m[0]) > worst_m0:
            worst_m0 = abs(m[0])
            worst_case = case
    assert worst_case is not None
    q_sizing, m_sizing = case_moments[worst_case.name]
    v_sizing, _ = loads.cantilever_shear_moment(y, q_sizing)

    h_all = np.array(
        [[wsg.spar_height(e, f) for e in eta] for f in wsg.spar_fracs]
    )  # (n_spars, n)
    # A partial-span spar (e.g. the optional center spar, see
    # WingStructureGeometry.spar_full_span/StructuresConfig.
    # center_spar_enabled) physically doesn't exist outboard of the break --
    # zeroing its height there (before the moment-share/cap/web sizing below,
    # all of which is driven by h_all) makes it correctly carry none of the
    # bending moment or shear past that station and contribute no mass
    # there, with no other special-casing needed: cap area, web mass, and
    # margin-of-safety all key off h_i, so a zeroed h_i naturally zeros all
    # of them in lockstep.
    for i, full_span in enumerate(wsg.spar_full_span):
        if not full_span:
            h_all[i] = np.where(eta <= wsg.break_eta + 1e-9, h_all[i], 0.0)
    h_sum = np.sum(h_all, axis=0)
    h_sum = np.where(h_sum > 1e-9, h_sum, 1e-9)
    frac_moment_all = (
        h_all / h_sum
    )  # (n_spars, n) -- deeper spar carries more of the bending moment

    tau_allow_web = web_mat.f_allow_pa / (2.0 * math.sqrt(3.0))
    taper = _cap_taper(eta, cfg.cap_taper_eta_lock, cfg.cap_taper_tip_fraction)

    spars: List[SparSizing] = []
    for i, frac_c in enumerate(wsg.spar_fracs):
        h_i = h_all[i]
        frac_m = frac_moment_all[i]
        h_eff = h_i * 0.85

        # -- Root (strength-critical) cap sizing: MS = 0 by construction ----
        m0 = abs(m_sizing[0])
        h_eff0 = max(h_eff[0], 1e-6)
        a_cap0 = (frac_m[0] * m0) / (cap_mat.f_allow_pa * h_eff0)
        w_cap0 = min(0.5 * chord[0], h_i[0] * 0.6)
        w_cap0 = max(w_cap0, 1e-6)
        t_cap0 = min(a_cap0 / w_cap0, h_i[0] * 0.20)

        # -- Taper outboard; keep bf >= tf and tf <= H_local/3 everywhere ----
        t_cap = np.minimum(t_cap0 * taper, h_i / 3.0)
        w_cap = np.maximum(w_cap0 * taper, t_cap)
        a_cap = w_cap * t_cap

        # -- Web: uniform thickness, sized from root shear -------------------
        v0 = abs(v_sizing[0])
        t_web = max(cfg.t_web_min_m, (frac_m[0] * v0) / (tau_allow_web * h_eff0))

        # -- Margin of safety at every station (diagnostic + health check) ---
        m_adm = a_cap * cap_mat.f_allow_pa * h_eff
        demand = np.abs(frac_m * m_sizing)
        with np.errstate(divide="ignore", invalid="ignore"):
            ms = np.where(demand > 1.0, m_adm / demand - 1.0, np.inf)

        spars.append(
            SparSizing(
                chord_fraction=frac_c,
                h=h_i,
                w_cap=w_cap,
                t_cap=t_cap,
                a_cap=a_cap,
                t_web=t_web,
                frac_moment=frac_m,
                margin_of_safety=ms,
            )
        )

    # -- Skin: fixed at cfg.t_skin_min_m (no torsional shear-flow upsizing --
    # the same fidelity level the reference's own analytical model uses; it
    # doesn't model closed-box GJ/torsion either, see structural_analysis.py).
    # This value directly drives the rib-spacing calc below (sig_panel ~
    # 1/t_skin), so it needs to be a realistic operative thickness for this
    # aircraft class, not a bare defensive minimum -- see the field's own
    # docstring in structures_config.py. ------------------------------------
    t_skin = cfg.t_skin_min_m

    # -- Rib spacing: Euler panel-buckling criterion on the skin between the
    # outermost two spars, generalizing 00_sizing.py's compute_rib_spacing. --
    frac_min, frac_max = min(wsg.spar_fracs), max(wsg.spar_fracs)
    b_box_root = (frac_max - frac_min) * chord[0]
    h_root_mid = wsg.spar_height(0.0, 0.5 * (frac_min + frac_max))
    nx = (abs(m_sizing[0]) / max(h_root_mid, 1e-6)) / max(b_box_root, 1e-6)
    sig_panel = max(nx / t_skin, 1e6)
    l_rib = max(
        cfg.rib_radius_of_gyration_m
        * math.sqrt(cfg.rib_buckling_coeff * math.pi**2 * skin_mat.e_pa / sig_panel),
        0.5,
    )
    if cfg.num_ribs_override is not None:
        num_ribs = int(cfg.num_ribs_override)
    else:
        num_ribs = max(10, int(math.ceil(wsg.semi_span / l_rib)) + 1)

    # -- Mass breakdown (semi-wing) -------------------------------------------
    dy = np.gradient(y)
    m_caps = sum(float(np.sum(2.0 * s.a_cap * cap_mat.rho_kg_m3 * dy)) for s in spars)
    m_webs = sum(float(np.sum(s.t_web * s.h * web_mat.rho_kg_m3 * dy)) for s in spars)
    m_skin = float(np.sum(2.0 * chord * t_skin * skin_mat.rho_kg_m3 * dy))

    xc_full = np.linspace(0.01, 0.99, 60)
    m_ribs = 0.0
    for eta_r in np.linspace(0.0, 1.0, max(num_ribs, 2)):
        c_r = wsg.local_chord(eta_r)
        heights = np.array(
            [
                (wsg.airfoil_zu_zl(eta_r, xc)[0] - wsg.airfoil_zu_zl(eta_r, xc)[1])
                * c_r
                for xc in xc_full
            ]
        )
        m_ribs += (
            float(np.trapezoid(heights, xc_full * c_r))
            * cfg.t_rib_m
            * rib_mat.rho_kg_m3
        )

    total = m_caps + m_webs + m_skin + m_ribs

    return WingboxSizing(
        y_stations=y,
        eta_stations=eta,
        chord=chord,
        spar_fracs=list(wsg.spar_fracs),
        spars=spars,
        t_skin=t_skin,
        num_ribs=num_ribs,
        rib_spacing_m=l_rib,
        mass_breakdown_kg={
            "Spar caps": m_caps,
            "Spar webs": m_webs,
            "Skin": m_skin,
            "Ribs": m_ribs,
        },
        total_mass_kg=total,
        sizing_load_case=worst_case.name,
    )
