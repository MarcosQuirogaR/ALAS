# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Analytical (no-NASTRAN) wingbox deformation, stress, and frequency solver.

Generalizes ``Reference Scripts/05_validation.py``'s Methods A/B/C (Castigliano
unit-load tip deflection, Euler-Bernoulli spanwise deflection curve, Rayleigh
quotient natural frequencies) to the sized wingbox from
:mod:`alas.physics.structural_sizing`. These are always available --
no NASTRAN install is required. Method D (Miles-equation RMS) is
deliberately **not** reproduced here: it needs a real NASTRAN sine-sweep
frequency-response function as input and has no analytical fallback -- it
only appears once :mod:`alas.integration.nastran_runner` completes a
real solve.

Unlike the sizing pass (which deliberately omits inertial relief for a
conservative strength check), this module **includes** relief -- the
sized structure's own distributed weight plus wing-mounted engine point
masses -- exactly as the reference's validation stage does, since that is
what makes the analytical deflection estimate track a real NASTRAN result
(the original work validated this to <20% error).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Dict, List, Tuple

import numpy as np

from ..config.materials import MaterialSpec
from ..config.geometry_config import EngineConfig
from ..config.mass_config import MassModelConfig
from ..config.requirements import DesignRequirements
from ..config.structures_config import StructuresConfig
from . import structural_loads as loads
from .structural_sizing import WingboxSizing
from ..geometry.wing_structure import WingStructureGeometry

_CANTILEVER_MODES = [
    (1.8751, 0.7341),
    (4.6941, 1.0185),
    (7.8548, 0.9992),
    (10.9955, 1.0000),
]


def _i_section(h: np.ndarray, bf: np.ndarray, tf: np.ndarray, tw: float) -> np.ndarray:
    """Area moment of inertia of a symmetric I-section about its own
    centroid, direct port of the reference's ``_I_section``."""
    hw = np.maximum(h - 2.0 * tf, 0.0)
    thin = h <= 2.0 * tf
    return np.where(
        thin,
        bf * h**3 / 12.0,
        tw * hw**3 / 12.0 + 2.0 * bf * tf * ((h - tf) / 2.0) ** 2,
    )


def _mass_per_length(
    sizing: WingboxSizing, cap_rho: float, web_rho: float, skin_rho: float
) -> np.ndarray:
    m_y = 2.0 * sizing.chord * sizing.t_skin * skin_rho
    for s in sizing.spars:
        m_y = m_y + s.t_web * s.h * web_rho + 2.0 * s.a_cap * cap_rho
    return m_y


def _ei_curve(
    sizing: WingboxSizing, cap_mat: MaterialSpec, skin_mat: MaterialSpec
) -> np.ndarray:
    """Combined bending stiffness EI(y) [N*m^2]: sum of each spar cap's
    I-section (CFRP/metal caps, ``cap_mat``) plus the skin torsion-box's
    parallel-axis contribution (always the skin material, matching the
    reference's ``_EI_curve``, generalized from a fixed front/rear pair to N
    spars)."""
    ei = np.zeros_like(sizing.y_stations)
    for s in sizing.spars:
        i_cap = _i_section(s.h, s.w_cap, s.t_cap, 0.0)
        ei = ei + cap_mat.e_pa * i_cap

    frac_min, frac_max = min(sizing.spar_fracs), max(sizing.spar_fracs)
    b_box = (frac_max - frac_min) * sizing.chord
    h_mean = np.mean([s.h for s in sizing.spars], axis=0)
    d_skin = h_mean / 2.0
    i_skin = 2.0 * b_box * sizing.t_skin * d_skin**2
    ei = ei + skin_mat.e_pa * i_skin
    return ei


@dataclass
class SparStressResult:
    chord_fraction: float
    stress_pa: np.ndarray  # bending stress in this spar's cap at each station [Pa]
    margin_of_safety: np.ndarray


@dataclass
class LoadCaseResult:
    name: str
    load_factor: float
    y: np.ndarray
    q_net: np.ndarray
    shear_n: np.ndarray
    moment_nm: np.ndarray
    deflection_m: np.ndarray  # Euler-Bernoulli spanwise deflection curve
    tip_deflection_m: float
    spar_stress: List[SparStressResult] = field(default_factory=list)


@dataclass
class ModalResult:
    frequencies_hz: np.ndarray
    mode_shapes: List[
        np.ndarray
    ]  # normalized (peak=1) shape per mode, sampled at the same y grid


@dataclass
class StructuralAnalysisReport:
    y: np.ndarray
    ei_nm2: np.ndarray  # static bending stiffness EI(y), independent of load case
    load_cases: Dict[str, LoadCaseResult]
    modal: ModalResult


def _deflection_curve(y: np.ndarray, m: np.ndarray, ei: np.ndarray) -> np.ndarray:
    """Spanwise deflection via the unit-load (virtual work) theorem at every
    station -- O(N^2), direct port of the reference's ``_deflection_curve``."""
    n = len(y)
    delta = np.zeros(n)
    integrand = m / ei
    for k in range(1, n):
        s_k = y[k]
        m_bar = np.where(y[: k + 1] <= s_k, s_k - y[: k + 1], 0.0)
        delta[k] = np.trapezoid(integrand[: k + 1] * m_bar, y[: k + 1])
    return delta


def _rayleigh_frequencies(
    y: np.ndarray, ei: np.ndarray, m_y: np.ndarray, n_modes: int
) -> Tuple[np.ndarray, List[np.ndarray]]:
    """First ``n_modes`` cantilever bending-mode frequencies via the Rayleigh
    quotient with classical cantilever trial mode shapes -- direct port of
    the reference's ``_rayleigh_frequencies``."""
    length = y[-1]
    n_modes = min(n_modes, len(_CANTILEVER_MODES))
    freqs = np.zeros(n_modes)
    shapes: List[np.ndarray] = []
    for i in range(n_modes):
        beta_l, sigma = _CANTILEVER_MODES[i]
        beta = beta_l / max(length, 1e-9)
        by = beta * y
        phi = np.cosh(by) - np.cos(by) - sigma * (np.sinh(by) - np.sin(by))
        phi_pp = beta**2 * (
            np.cosh(by) + np.cos(by) - sigma * (np.sinh(by) + np.sin(by))
        )
        num = np.trapezoid(ei * phi_pp**2, y)
        den = np.trapezoid(m_y * phi**2, y)
        freqs[i] = (np.sqrt(num / den) / (2.0 * np.pi)) if den > 1e-30 else 0.0
        norm = np.max(np.abs(phi)) or 1.0
        shapes.append(phi / norm)
    return freqs, shapes


def analyze_structure(
    wsg: WingStructureGeometry,
    sizing: WingboxSizing,
    cfg: StructuresConfig,
    req: DesignRequirements,
    engine_cfg: EngineConfig,
    mass_cfg: MassModelConfig,
    skin_mat: MaterialSpec,
    web_mat: MaterialSpec,
    cap_mat: MaterialSpec,
) -> StructuralAnalysisReport:
    y = sizing.y_stations
    semi_span = wsg.semi_span
    g = req.gravity_m_s2

    ei = _ei_curve(sizing, cap_mat, skin_mat)
    m_y = _mass_per_length(
        sizing, cap_mat.rho_kg_m3, web_mat.rho_kg_m3, skin_mat.rho_kg_m3
    )
    engine_loads = loads.engine_point_loads_n(engine_cfg, mass_cfg, req)

    m_bar_tip = semi_span - y

    results: Dict[str, LoadCaseResult] = {}
    for case in loads.load_cases(req, cfg.additional_safety_factor):
        sign = 1.0 if case.total_force_n >= 0 else -1.0
        l_total = abs(case.total_force_n)
        n_factor = abs(case.load_factor)

        q_aero = loads.elliptic_distributed_load(y, semi_span, l_total)
        q_net = q_aero - n_factor * g * m_y
        v, m = loads.cantilever_shear_moment(y, q_net)

        for y_eng, m_eng in engine_loads:
            f_eng = n_factor * m_eng * g
            m = m + np.where(y <= y_eng, -f_eng * (y_eng - y), 0.0)

        tip_defl = float(np.trapezoid(m * m_bar_tip / ei, y)) * sign
        defl_curve = _deflection_curve(y, m, ei) * sign
        m_signed = m * sign

        spar_stress = []
        for s in sizing.spars:
            h_eff = s.h * 0.85
            with np.errstate(divide="ignore", invalid="ignore"):
                sigma = np.abs(s.frac_moment * m_signed) / np.maximum(
                    s.a_cap * h_eff, 1e-12
                )
                ms = np.where(
                    np.abs(s.frac_moment * m_signed) > 1.0,
                    cap_mat.f_allow_pa / np.maximum(sigma, 1e-9) - 1.0,
                    np.inf,
                )
            spar_stress.append(
                SparStressResult(
                    chord_fraction=s.chord_fraction,
                    stress_pa=sigma,
                    margin_of_safety=ms,
                )
            )

        results[case.name] = LoadCaseResult(
            name=case.name,
            load_factor=case.load_factor,
            y=y,
            q_net=q_net * sign,
            shear_n=v * sign,
            moment_nm=m_signed,
            deflection_m=defl_curve,
            tip_deflection_m=tip_defl,
            spar_stress=spar_stress,
        )

    # Modal: engine point masses smeared onto the nearest station (matches
    # the reference's own single-station injection, generalized to N engines).
    m_y_modal = m_y.copy()
    if len(y) > 1:
        dy_uniform = y[1] - y[0]
        for y_eng, m_eng in engine_loads:
            idx = int(np.clip(round(y_eng / max(dy_uniform, 1e-9)), 0, len(y) - 1))
            m_y_modal[idx] += m_eng / max(dy_uniform, 1e-9)
    freqs, shapes = _rayleigh_frequencies(y, ei, m_y_modal, cfg.n_modes)

    return StructuralAnalysisReport(
        y=y,
        ei_nm2=ei,
        load_cases=results,
        modal=ModalResult(frequencies_hz=freqs, mode_shapes=shapes),
    )
