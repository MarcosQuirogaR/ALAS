# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Turbofan on-design propulsion cycle analysis.

A separate-flow (unmixed), two-spool turbofan parametric cycle model --
compressor/fan/turbine thermodynamics with polytropic component efficiencies,
following the same station-by-station structure (ram -> inlet -> LPC -> HPC
-> combustor -> HPT -> LPT -> core nozzle, with the fan as a parallel branch
off the same inlet stagnation state -> fan nozzle) that
``external tools/suave_runner/vehicle_builder.py`` already builds for SUAVE's
mission analysis, and reusing that file's exact component efficiency/pressure-
loss numbers as this module's defaults (:class:`PropulsionCycleConfig`) --
this is what keeps the two analyses' assumptions consistent even though they
are different fidelity levels (this module is a fast, closed-form, on-design
conceptual estimate; SUAVE's is a numerically-solved mission simulation),
the same relationship the "Model Comparison" tab already documents between
AeroSandbox/SUAVE/MSES.

Unlike the semi-empirical Mattingly-style installed-thrust-lapse tables (the
``EngineType``/``PowerSetting``/``ThrottleRatio`` machinery used for
matching-chart work -- ALAS already has its own matching chart, see
``physics/performance.py``), every equation here is first-principles
compressible-flow thermodynamics (isentropic + polytropic-efficiency
relations, energy/momentum conservation) with no curve-fit constants, so
altitude/Mach sensitivity is obtained by re-evaluating the same closed-form
cycle at different ambient conditions rather than an empirical lookup table.

All station temperatures are stagnation ("total") temperatures in Kelvin.
Station naming mirrors the standard two-spool notation: 0=freestream,
t2=post-inlet (fan/LPC face), t13=post-fan (bypass duct), t25=post-LPC,
t3=post-HPC (combustor inlet), t4=combustor exit (turbine inlet temperature,
the design input), t45=post-HPT, t5=post-LPT (core nozzle inlet), t6=core
nozzle exit.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import List, Optional, Tuple

import aerosandbox as asb
import numpy as np

from ..config.propulsion_config import PropulsionCycleConfig

__all__ = [
    "PropulsionCycleConfig",
    "TurbofanCycleInputs",
    "TurbofanCycleResult",
    "compute_turbofan_cycle",
    "anchor_mass_flow_kg_s",
    "CarpetPlotResult",
    "compute_carpet_plot",
    "BprSensitivityResult",
    "compute_bpr_sensitivity",
    "EfficiencyDecompositionResult",
    "compute_efficiency_decomposition",
    "AltitudeSweepResult",
    "compute_altitude_sweep",
    "classify_engine_by_bpr",
]


@dataclass
class TurbofanCycleInputs:
    """Design-point flight condition and cycle parameters for one evaluation."""

    mach: float
    altitude_m: float
    bypass_ratio: float
    overall_pressure_ratio: (
        float  # core (LPC*HPC) pressure ratio, i.e. EngineSpec.overall_pressure_ratio
    )
    fan_pressure_ratio: float
    turbine_inlet_temperature_k: float


@dataclass
class TurbofanCycleResult:
    """Output of one on-design cycle evaluation."""

    cycle_feasible: bool
    infeasibility_reason: str = ""

    specific_thrust_ms: float = float("nan")  # F / mdot_total  [m/s]
    tsfc_mg_ns: float = float("nan")  # mg fuel / (N.s)
    fuel_air_ratio: float = float("nan")

    thermal_efficiency: float = float("nan")
    propulsive_efficiency: float = float("nan")
    overall_efficiency: float = float("nan")

    temperature_t0_k: float = float("nan")
    temperature_t13_k: float = float("nan")
    temperature_t25_k: float = float("nan")
    temperature_t3_k: float = float("nan")
    temperature_t4_k: float = float("nan")
    temperature_t45_k: float = float("nan")
    temperature_t5_k: float = float("nan")
    temperature_t6_k: float = float("nan")

    exit_velocity_core_ms: float = float("nan")
    exit_velocity_fan_ms: float = float("nan")


def _infeasible(reason: str) -> TurbofanCycleResult:
    return TurbofanCycleResult(cycle_feasible=False, infeasibility_reason=reason)


def _expand_nozzle(
    tt: float, pt: float, p_ambient: float, gamma: float, cp: float, eta_n: float
) -> Tuple[float, float, float]:
    """Expand a stagnation state to ambient pressure (or choke), return (V, T, p) at exit."""
    r_gas = cp * (gamma - 1.0) / gamma
    p_star_over_pt = (2.0 / (gamma + 1.0)) ** (gamma / (gamma - 1.0))
    p_star = p_star_over_pt * pt
    choked = p_star >= p_ambient
    p_exit = p_star if choked else p_ambient
    t_exit_ideal = tt * (p_exit / pt) ** ((gamma - 1.0) / gamma)
    t_exit = tt - eta_n * (tt - t_exit_ideal)
    if choked:
        v_exit = math.sqrt(max(0.0, gamma * r_gas * t_exit))
    else:
        v_exit = math.sqrt(max(0.0, 2.0 * cp * (tt - t_exit)))
    return v_exit, t_exit, p_exit


def compute_turbofan_cycle(
    inputs: TurbofanCycleInputs,
    cfg: Optional[PropulsionCycleConfig] = None,
) -> TurbofanCycleResult:
    """Evaluate the on-design separate-flow turbofan cycle at one flight condition.

    Returns specific thrust [m/s, per unit TOTAL (core+bypass) mass flow, matching
    the convention ``S_Fn = F / mdot_total``], TSFC [mg/(N.s)], the thermal/
    propulsive/overall efficiency decomposition, and every station stagnation
    temperature -- or ``cycle_feasible=False`` with a reason if the requested
    combination of parameters has no physical solution (e.g. the turbine inlet
    temperature is below the compressor discharge temperature).
    """
    cfg = cfg or PropulsionCycleConfig()
    gc, cpc = cfg.gamma_cold, cfg.cp_cold_j_kgk
    gh, cph = cfg.gamma_hot, cfg.cp_hot_j_kgk

    atmo = asb.Atmosphere(altitude=inputs.altitude_m)
    t0 = float(atmo.temperature())
    p0 = float(atmo.pressure())
    a0 = float(atmo.speed_of_sound())
    m0 = max(0.0, float(inputs.mach))
    v0 = m0 * a0

    tt0 = t0 * (1.0 + 0.5 * (gc - 1.0) * m0**2)
    pt0_ideal = p0 * (1.0 + 0.5 * (gc - 1.0) * m0**2) ** (gc / (gc - 1.0))
    pt0 = pt0_ideal * cfg.inlet_pressure_recovery
    tt2, pt2 = tt0, pt0

    # -- Fan branch (parallel to the core compressors, same inlet state) -----
    pi_f = max(1.0, float(inputs.fan_pressure_ratio))
    tt13 = tt2 * pi_f ** ((gc - 1.0) / (gc * cfg.fan_polytropic_efficiency))
    pt13 = pt2 * pi_f * cfg.fan_nozzle_pressure_ratio

    # -- Core compressors (LPC then HPC) --------------------------------------
    pi_lpc = cfg.lpc_pressure_ratio_split
    pi_hpc = max(float(inputs.overall_pressure_ratio) / pi_lpc, 1.0)
    tt25 = tt2 * pi_lpc ** ((gc - 1.0) / (gc * cfg.lpc_polytropic_efficiency))
    tt3 = tt25 * pi_hpc ** ((gc - 1.0) / (gc * cfg.hpc_polytropic_efficiency))
    pt3 = pt2 * pi_lpc * pi_hpc

    # -- Combustor -------------------------------------------------------------
    tt4 = float(inputs.turbine_inlet_temperature_k)
    if tt4 <= tt3:
        return _infeasible(
            f"Turbine inlet temperature ({tt4:.0f} K) must exceed the compressor "
            f"discharge temperature ({tt3:.0f} K)."
        )
    pt4 = pt3 * cfg.combustor_pressure_ratio
    h_pr = cfg.fuel_heating_value_kj_kg * 1000.0
    denom = cfg.combustor_efficiency * h_pr - cph * tt4
    if denom <= 0:
        return _infeasible(
            "Turbine inlet temperature too high relative to the fuel heating value."
        )
    f_ratio = (cph * tt4 - cpc * tt3) / denom
    if f_ratio <= 0:
        return _infeasible("Computed fuel-air ratio is non-positive.")

    # -- HPT (drives HPC) -------------------------------------------------------
    hpc_work = cpc * (tt3 - tt25)
    d_tt_hpt = hpc_work / ((1.0 + f_ratio) * cph * cfg.turbine_mechanical_efficiency)
    tt45 = tt4 - d_tt_hpt
    if tt45 <= 0:
        return _infeasible(
            "HPT work required exceeds available combustor exit enthalpy."
        )
    pt45 = pt4 * (tt45 / tt4) ** (gh / ((gh - 1.0) * cfg.hpt_polytropic_efficiency))

    # -- LPT (drives LPC + fan) -------------------------------------------------
    lpc_work = cpc * (tt25 - tt2)
    fan_work = cpc * (tt13 - tt2)
    d_tt_lpt = (lpc_work + inputs.bypass_ratio * fan_work) / (
        (1.0 + f_ratio) * cph * cfg.turbine_mechanical_efficiency
    )
    tt5 = tt45 - d_tt_lpt
    if tt5 <= 0:
        return _infeasible(
            "LPT work required (LPC + fan) exceeds available HPT exit enthalpy."
        )
    pt5 = pt45 * (tt5 / tt45) ** (gh / ((gh - 1.0) * cfg.lpt_polytropic_efficiency))

    # -- Core nozzle -------------------------------------------------------------
    pt6 = pt5 * cfg.core_nozzle_pressure_ratio
    tt6 = tt5
    v9, t9, p9 = _expand_nozzle(tt6, pt6, p0, gh, cph, cfg.core_nozzle_efficiency)

    # -- Fan nozzle ----------------------------------------------------------------
    v19, t19, p19 = _expand_nozzle(tt13, pt13, p0, gc, cpc, cfg.fan_nozzle_efficiency)

    # -- Thrust (per unit CORE mass flow), including pressure-thrust if choked ----
    # Folded into an "equivalent" exit velocity (V9_eq = V9 + (p9-p0)/(rho9*V9))
    # rather than kept as a separate additive pressure term: algebraically
    # identical for the thrust equation itself ((1+f)*(V9_eq-V0) ==
    # (1+f)*(V9-V0) + (1+f)*(p9-p0)/(rho9*V9)), but it also lets the
    # efficiency energy-balance below use the SAME equivalent velocity for
    # its kinetic-energy term -- required for a choked/underexpanded nozzle,
    # where the exiting flow still carries recoverable pressure-energy a
    # longer nozzle would have converted to additional KE. Using the bare
    # (non-equivalent) V9/V19 in the KE-gain denominator instead (an earlier
    # version of this function did) let the momentum+pressure thrust in the
    # numerator exceed the KE-only accounting in the denominator whenever a
    # nozzle choked, producing propulsive efficiency > 1 -- impossible by
    # definition. This is the standard Mattingly "equivalent velocity"
    # treatment for exactly this case.
    r_hot = cph * (gh - 1.0) / gh
    r_cold = cpc * (gc - 1.0) / gc
    rho9 = p9 / max(r_hot * t9, 1e-9)
    rho19 = p19 / max(r_cold * t19, 1e-9)

    v9_eq = v9 + (p9 - p0) / max(rho9 * v9, 1e-9)
    v19_eq = v19 + (p19 - p0) / max(rho19 * v19, 1e-9)

    f_core = (1.0 + f_ratio) * (v9_eq - v0)
    f_bypass = inputs.bypass_ratio * (v19_eq - v0)
    f_per_mdot_core = f_core + f_bypass

    if f_per_mdot_core <= 0:
        return _infeasible(
            "Computed net thrust is non-positive at this flight condition."
        )

    bpr = float(inputs.bypass_ratio)
    sfn = f_per_mdot_core / (1.0 + bpr)
    tsfc_si = f_ratio / f_per_mdot_core
    tsfc_mg_ns = tsfc_si * 1.0e6

    ke_gain = (1.0 + f_ratio) * (v9_eq**2 - v0**2) / 2.0 + bpr * (
        v19_eq**2 - v0**2
    ) / 2.0
    fuel_power = f_ratio * h_pr
    eta_th = ke_gain / fuel_power if fuel_power > 0 else float("nan")
    if v0 > 1e-6 and ke_gain > 0:
        eta_p = (f_per_mdot_core * v0) / ke_gain
        eta_o = (f_per_mdot_core * v0) / fuel_power if fuel_power > 0 else float("nan")
    else:
        eta_p = 0.0
        eta_o = 0.0

    return TurbofanCycleResult(
        cycle_feasible=True,
        specific_thrust_ms=sfn,
        tsfc_mg_ns=tsfc_mg_ns,
        fuel_air_ratio=f_ratio,
        thermal_efficiency=eta_th,
        propulsive_efficiency=eta_p,
        overall_efficiency=eta_o,
        temperature_t0_k=tt0,
        temperature_t13_k=tt13,
        temperature_t25_k=tt25,
        temperature_t3_k=tt3,
        temperature_t4_k=tt4,
        temperature_t45_k=tt45,
        temperature_t5_k=tt5,
        temperature_t6_k=tt6,
        exit_velocity_core_ms=v9,
        exit_velocity_fan_ms=v19,
    )


# ---------------------------------------------------------------------------
# Design mass-flow anchor (ties the cycle's intensive/specific outputs to a
# real engine's published rated static thrust, so the tab can also show
# dimensional thrust, not just specific thrust).
# ---------------------------------------------------------------------------
def anchor_mass_flow_kg_s(
    thrust_kn: float,
    overall_pressure_ratio: float,
    fan_pressure_ratio: float,
    bypass_ratio: float,
    turbine_inlet_temperature_k: float,
    cfg: Optional[PropulsionCycleConfig] = None,
) -> Tuple[float, TurbofanCycleResult]:
    """Total design mass flow [kg/s] such that the on-design cycle's static
    (sea-level, M0=0) specific thrust reproduces ``thrust_kn`` exactly.

    A conceptual-design-level normalisation (not an independent validation):
    it lets the tab report a dimensional thrust at any flight condition
    (``specific_thrust_ms(condition) * mdot_total``) consistent with the
    engine's own rated static thrust, without needing a real corrected-flow/
    face-area schedule. Returns ``(mdot_total_kg_s, static_cycle_result)``.
    """
    static_inputs = TurbofanCycleInputs(
        mach=0.0,
        altitude_m=0.0,
        bypass_ratio=bypass_ratio,
        overall_pressure_ratio=overall_pressure_ratio,
        fan_pressure_ratio=fan_pressure_ratio,
        turbine_inlet_temperature_k=turbine_inlet_temperature_k,
    )
    static_result = compute_turbofan_cycle(static_inputs, cfg)
    if not static_result.cycle_feasible or static_result.specific_thrust_ms <= 0:
        return float("nan"), static_result
    mdot_total = (thrust_kn * 1000.0) / static_result.specific_thrust_ms
    return mdot_total, static_result


# ---------------------------------------------------------------------------
# Parametric sweeps (carpet plot, BPR sensitivity, efficiency decomposition,
# altitude/Mach sweep) -- each returns plain NumPy arrays ready for plotting.
# ---------------------------------------------------------------------------
@dataclass
class CarpetPlotResult:
    compressor_pressure_ratio_vector: np.ndarray
    tit_vector_k: np.ndarray
    specific_thrust_ms: np.ndarray  # shape (n_tit, n_pic)
    tsfc_mg_ns: np.ndarray  # shape (n_tit, n_pic)
    feasible_mask: np.ndarray  # shape (n_tit, n_pic)


def compute_carpet_plot(
    compressor_pressure_ratio_vector: np.ndarray,
    tit_vector_k: np.ndarray,
    mach: float,
    altitude_m: float,
    fan_pressure_ratio: float,
    bypass_ratio: float,
    cfg: Optional[PropulsionCycleConfig] = None,
) -> CarpetPlotResult:
    """Sweep (overall pressure ratio, turbine inlet temperature) at a fixed
    flight condition/BPR/FPR, for a carpet-plot-style visualisation of the
    specific-thrust/TSFC trade surface."""
    n_pic = len(compressor_pressure_ratio_vector)
    n_tit = len(tit_vector_k)
    sfn = np.full((n_tit, n_pic), np.nan)
    tsfc = np.full((n_tit, n_pic), np.nan)
    feasible = np.zeros((n_tit, n_pic), dtype=bool)

    for i, tit in enumerate(tit_vector_k):
        for j, pic in enumerate(compressor_pressure_ratio_vector):
            out = compute_turbofan_cycle(
                TurbofanCycleInputs(
                    mach=mach,
                    altitude_m=altitude_m,
                    bypass_ratio=bypass_ratio,
                    overall_pressure_ratio=float(pic),
                    fan_pressure_ratio=fan_pressure_ratio,
                    turbine_inlet_temperature_k=float(tit),
                ),
                cfg,
            )
            if out.cycle_feasible:
                sfn[i, j] = out.specific_thrust_ms
                tsfc[i, j] = out.tsfc_mg_ns
                feasible[i, j] = True

    return CarpetPlotResult(
        compressor_pressure_ratio_vector=np.asarray(
            compressor_pressure_ratio_vector, dtype=float
        ),
        tit_vector_k=np.asarray(tit_vector_k, dtype=float),
        specific_thrust_ms=sfn,
        tsfc_mg_ns=tsfc,
        feasible_mask=feasible,
    )


@dataclass
class BprSensitivityResult:
    bypass_ratio_vector: np.ndarray
    specific_thrust_ms: np.ndarray
    tsfc_mg_ns: np.ndarray
    feasible_mask: np.ndarray


def compute_bpr_sensitivity(
    bpr_vector: np.ndarray,
    overall_pressure_ratio: float,
    turbine_inlet_temperature_k: float,
    fan_pressure_ratio: float,
    mach: float,
    altitude_m: float,
    cfg: Optional[PropulsionCycleConfig] = None,
) -> BprSensitivityResult:
    """Sweep bypass ratio at fixed OPR/T4t/FPR/flight condition."""
    sfn = np.full(len(bpr_vector), np.nan)
    tsfc = np.full(len(bpr_vector), np.nan)
    feasible = np.zeros(len(bpr_vector), dtype=bool)
    for i, bpr in enumerate(bpr_vector):
        out = compute_turbofan_cycle(
            TurbofanCycleInputs(
                mach=mach,
                altitude_m=altitude_m,
                bypass_ratio=float(bpr),
                overall_pressure_ratio=overall_pressure_ratio,
                fan_pressure_ratio=fan_pressure_ratio,
                turbine_inlet_temperature_k=turbine_inlet_temperature_k,
            ),
            cfg,
        )
        if out.cycle_feasible:
            sfn[i] = out.specific_thrust_ms
            tsfc[i] = out.tsfc_mg_ns
            feasible[i] = True
    return BprSensitivityResult(
        bypass_ratio_vector=np.asarray(bpr_vector, dtype=float),
        specific_thrust_ms=sfn,
        tsfc_mg_ns=tsfc,
        feasible_mask=feasible,
    )


@dataclass
class EfficiencyDecompositionResult:
    compressor_pressure_ratio_vector: np.ndarray
    thermal_efficiency: np.ndarray
    propulsive_efficiency: np.ndarray
    overall_efficiency: np.ndarray
    feasible_mask: np.ndarray


def compute_efficiency_decomposition(
    pi_c_vector: np.ndarray,
    turbine_inlet_temperature_k: float,
    bypass_ratio: float,
    fan_pressure_ratio: float,
    mach: float,
    altitude_m: float,
    cfg: Optional[PropulsionCycleConfig] = None,
) -> EfficiencyDecompositionResult:
    """Sweep overall pressure ratio at fixed T4t/BPR/FPR/flight condition,
    decomposing overall efficiency into thermal x propulsive."""
    eta_th = np.full(len(pi_c_vector), np.nan)
    eta_p = np.full(len(pi_c_vector), np.nan)
    eta_o = np.full(len(pi_c_vector), np.nan)
    feasible = np.zeros(len(pi_c_vector), dtype=bool)
    for i, pic in enumerate(pi_c_vector):
        out = compute_turbofan_cycle(
            TurbofanCycleInputs(
                mach=mach,
                altitude_m=altitude_m,
                bypass_ratio=bypass_ratio,
                overall_pressure_ratio=float(pic),
                fan_pressure_ratio=fan_pressure_ratio,
                turbine_inlet_temperature_k=turbine_inlet_temperature_k,
            ),
            cfg,
        )
        if out.cycle_feasible:
            eta_th[i] = out.thermal_efficiency
            eta_p[i] = out.propulsive_efficiency
            eta_o[i] = out.overall_efficiency
            feasible[i] = True
    return EfficiencyDecompositionResult(
        compressor_pressure_ratio_vector=np.asarray(pi_c_vector, dtype=float),
        thermal_efficiency=eta_th,
        propulsive_efficiency=eta_p,
        overall_efficiency=eta_o,
        feasible_mask=feasible,
    )


@dataclass
class AltitudeSweepResult:
    altitude_m: np.ndarray
    specific_thrust_ms: np.ndarray  # shape (n_mach, n_altitude)
    tsfc_mg_ns: np.ndarray  # shape (n_mach, n_altitude)
    dimensional_thrust_kn: (
        np.ndarray
    )  # shape (n_mach, n_altitude), from the mass-flow anchor
    feasible_mask: np.ndarray  # shape (n_mach, n_altitude)
    mach_values: List[float] = field(default_factory=list)


def compute_altitude_sweep(
    altitude_vector_m: np.ndarray,
    mach_values: List[float],
    bypass_ratio: float,
    overall_pressure_ratio: float,
    fan_pressure_ratio: float,
    turbine_inlet_temperature_k: float,
    mdot_total_kg_s: float = float("nan"),
    cfg: Optional[PropulsionCycleConfig] = None,
) -> AltitudeSweepResult:
    """Specific thrust & TSFC vs altitude, for each of several Mach numbers,
    holding the cycle design parameters fixed -- the rigorous, closed-form
    stand-in for a semi-empirical installed-thrust-lapse table (see the
    module docstring)."""
    n_m, n_h = len(mach_values), len(altitude_vector_m)
    sfn = np.full((n_m, n_h), np.nan)
    tsfc = np.full((n_m, n_h), np.nan)
    thrust_kn = np.full((n_m, n_h), np.nan)
    feasible = np.zeros((n_m, n_h), dtype=bool)

    for i, mach in enumerate(mach_values):
        for j, alt in enumerate(altitude_vector_m):
            out = compute_turbofan_cycle(
                TurbofanCycleInputs(
                    mach=mach,
                    altitude_m=float(alt),
                    bypass_ratio=bypass_ratio,
                    overall_pressure_ratio=overall_pressure_ratio,
                    fan_pressure_ratio=fan_pressure_ratio,
                    turbine_inlet_temperature_k=turbine_inlet_temperature_k,
                ),
                cfg,
            )
            if out.cycle_feasible:
                sfn[i, j] = out.specific_thrust_ms
                tsfc[i, j] = out.tsfc_mg_ns
                feasible[i, j] = True
                if (
                    mdot_total_kg_s == mdot_total_kg_s and mdot_total_kg_s > 0
                ):  # not NaN
                    thrust_kn[i, j] = out.specific_thrust_ms * mdot_total_kg_s / 1000.0

    return AltitudeSweepResult(
        altitude_m=np.asarray(altitude_vector_m, dtype=float),
        specific_thrust_ms=sfn,
        tsfc_mg_ns=tsfc,
        dimensional_thrust_kn=thrust_kn,
        feasible_mask=feasible,
        mach_values=list(mach_values),
    )


def classify_engine_by_bpr(bypass_ratio: float) -> str:
    """Descriptive label from bypass ratio (informational only -- does not
    feed any lookup table, unlike the classic turbojet/LBR/HBR engine-deck
    buckets used for Mattingly-style installed-thrust-lapse tables)."""
    if bypass_ratio < 1.0:
        return "Turbojet / very-low-bypass"
    if bypass_ratio < 5.0:
        return "Low-bypass turbofan"
    return "High-bypass turbofan"
