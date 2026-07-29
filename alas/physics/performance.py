# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Low-speed performance and matching chart calculations.

Implements:
  - Matching chart constraints in T/W₀ vs W/S space:
      * Cruise (drag polar at altitude, converted to SL with thrust lapse)
      * OEI second-segment climb (FAR 25.121 / CS 25.121)
      * Take-off field length (Raymer Ch.17 empirical formula)
      * Landing field length (reverse of landing-distance formula)
  - V speeds per FAR 25 (Vstall, Vmc, V1, VR, V2, VAPP, VTD)
  - Field performance estimates (TODR, BFL, ASD, LDR)

All formulas use SI inputs/outputs unless otherwise noted. Empirical
constants (37.7, K=0.60) are from Raymer, "Aircraft Design: A Conceptual
Approach", 5th ed., Chapters 17 & 21, originally expressed in imperial
units; the necessary unit-conversion factors are applied inline.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import TYPE_CHECKING, Dict, List

import numpy as np
import aerosandbox as asb

from ..config.airports import Airport

if TYPE_CHECKING:
    from ..config.performance_config import PerformanceConfig
from .mass import OEW_KEYS

_G = 9.81  # m/s²
_PA_TO_PSF = 0.020885  # Pa -> lb/ft^2
_M_TO_FT = 3.28084  # m -> ft
_MS_TO_KT = 1.94384  # m/s -> knots

# FAR 25.121 second-segment climb minimum gross gradient by engine count.
# Twin = 2.4%, tri-jet = 2.7%, quad = 3.0%.
FAR25_OEI_GRADIENT: Dict[int, float] = {2: 0.024, 3: 0.027, 4: 0.030}


# ---------------------------------------------------------------------------
# Atmospheric helpers
# ---------------------------------------------------------------------------


def density_ratio(elevation_m: float, delta_isa_c: float = 0.0) -> float:
    """Density ratio σ = ρ/ρ₀ at field elevation with an ISA offset."""
    atmo = asb.Atmosphere(altitude=elevation_m)
    temp_k = atmo.temperature() + delta_isa_c
    rho = atmo.pressure() / (287.05 * temp_k)
    return float(rho / 1.225)


# ---------------------------------------------------------------------------
# Matching chart constraint curves
# ---------------------------------------------------------------------------


def tw_cruise_constraint(
    ws_pa: np.ndarray,
    cd0: float,
    k: float,
    cruise_mach: float,
    cruise_altitude_m: float,
    thrust_lapse: float = 0.235,
) -> np.ndarray:
    """Cruise T/W₀ constraint.

    Solves level flight (L = W, T = D) at the cruise design point and converts
    the required altitude T/W to sea-level static via a fixed thrust lapse η:

        T/W₀ = [q·CD₀/(W/S) + k·(W/S)/q] / η
    """
    atmo = asb.Atmosphere(altitude=cruise_altitude_m)
    q = 0.5 * atmo.density() * (cruise_mach * atmo.speed_of_sound()) ** 2
    tw_alt = q * cd0 / ws_pa + k * ws_pa / q
    return tw_alt / thrust_lapse


def tw_oei_climb_constraint(
    cd0: float,
    k: float,
    n_engines: int,
    oei_gradient: float = 0.024,
    cl_climb: float = 1.2,
    delta_cd_to_config: float = 0.025,
) -> float:
    """OEI second-segment climb T/W₀ (constant, independent of W/S).

    FAR 25.121 / CS 25.121 requires N/(N-1) engines to sustain a minimum
    climb gradient with one engine inoperative.  Evaluated at ``cl_climb``
    (nominally 1.2·Vstall) in take-off configuration (flaps deployed, gear
    up); ``delta_cd_to_config`` is the flap/gear parasite-drag increment
    added to the clean ``cd0``. Pass ``PerformanceConfig.oei_climb_cl`` /
    ``.oei_climb_delta_cd`` so these are user-adjustable rather than fixed.
    """
    if n_engines < 2:
        return 0.0
    cd_to_config = cd0 + delta_cd_to_config + k * cl_climb**2
    ld_to = cl_climb / cd_to_config
    factor = n_engines / (n_engines - 1)
    return float(factor * (1.0 / ld_to + oei_gradient))


def tw_takeoff_constraint(
    ws_pa: np.ndarray,
    toda_m: float,
    sigma: float,
    cl_max_to: float,
) -> np.ndarray:
    """Take-off T/W₀ constraint (Raymer Ch.17, empirical).

        T/W = 37.7 · (W/S [psf]) / (σ · CL_max_TO · TODA [ft])

    The constant 37.7 is Raymer's regression coefficient for jet transports
    (originally calibrated in US customary units).
    """
    ws_psf = ws_pa * _PA_TO_PSF
    toda_ft = toda_m * _M_TO_FT
    return 37.7 * ws_psf / (sigma * cl_max_to * toda_ft)


def ws_landing_limit(
    lda_m: float,
    sigma: float,
    cl_max_land: float,
    k_factor: float = 0.60,
) -> float:
    """Maximum allowable W/S [Pa] set by the landing distance constraint.

    (W/S)_max = LDA · σ · CL_max_land / K
    """
    return float(lda_m * sigma * cl_max_land / k_factor)


# ---------------------------------------------------------------------------
# V speeds (FAR 25 definitions)
# ---------------------------------------------------------------------------


@dataclass
class VSpeeds:
    """Reference speeds [m/s] for one aircraft + airport combination."""

    v_stall_to_ms: float  # Stall speed in take-off config (1g)
    v_stall_land_ms: float  # Stall speed in landing config (1g)
    v_mc_ms: float  # Minimum control speed  (FAR 25.149, 1.13*VS_TO)
    v1_ms: float  # Decision speed         (~0.90*VR, simplified)
    v_r_ms: float  # Rotation speed         (FAR 25.107, >=1.1*VS_TO)
    v2_ms: float  # Take-off safety speed  (FAR 25.107, >=1.2*VS_TO)
    v_app_ms: float  # Approach speed         (FAR 25.125, 1.3·VS_land)
    v_td_ms: float  # Touchdown speed        (1.15·VS_land)

    def as_ms(self) -> Dict[str, float]:
        return {
            "Vstall_TO": self.v_stall_to_ms,
            "Vstall_land": self.v_stall_land_ms,
            "Vmc": self.v_mc_ms,
            "V1": self.v1_ms,
            "VR": self.v_r_ms,
            "V2": self.v2_ms,
            "VAPP": self.v_app_ms,
            "VTD": self.v_td_ms,
        }

    def as_knots(self) -> Dict[str, float]:
        return {k: v * _MS_TO_KT for k, v in self.as_ms().items()}


def compute_v_speeds(
    mtow_kg: float,
    wing_area_m2: float,
    airport: Airport,
    cl_max_to: float,
    cl_max_land: float,
    perf_config: "PerformanceConfig | None" = None,
) -> VSpeeds:
    """Compute FAR 25 V speeds at the given airport conditions.

    The multiplicative factors (VMC/VS, VR/VMC, V2/VS, V1/VR, ...) come from
    ``perf_config`` (:class:`PerformanceConfig`) rather than being hardcoded, so
    they can be calibrated to a real type. A ``None`` config uses a fresh
    ``PerformanceConfig()`` (realistic FAR-25 defaults).
    """
    from ..config.performance_config import PerformanceConfig

    pc = perf_config or PerformanceConfig()
    sigma = density_ratio(airport.elevation_m, airport.isa_deviation_c)
    rho = sigma * 1.225
    ws_pa = mtow_kg * _G / wing_area_m2

    v_stall_to = math.sqrt(2.0 * ws_pa / (rho * cl_max_to))
    v_stall_land = math.sqrt(2.0 * ws_pa / (rho * cl_max_land))

    v_mc = pc.vmc_vstall_factor * v_stall_to
    v_r = max(pc.vr_vmc_factor * v_mc, pc.vr_vstall_factor * v_stall_to)  # FAR 25.107
    # V1 just below VR on a dry balanced field, but FAR 25.107 also requires
    # V1 >= VMC (the decision speed can never sit below minimum control
    # speed) -- v1_vr_factor*VR alone can dip under VMC when VR is only
    # marginally above VMC (a tight vr_vmc_factor/vr_vstall_factor
    # combination), so floor it here rather than relying on the multiplier
    # always leaving enough margin.
    v1 = max(pc.v1_vr_factor * v_r, v_mc)
    v2 = max(pc.v2_vstall_factor * v_stall_to, v_r)  # FAR 25.107
    v_app = pc.vapp_vstall_land_factor * v_stall_land  # FAR 25.125
    v_td = pc.vtd_vstall_land_factor * v_stall_land

    return VSpeeds(
        v_stall_to_ms=v_stall_to,
        v_stall_land_ms=v_stall_land,
        v_mc_ms=v_mc,
        v1_ms=v1,
        v_r_ms=v_r,
        v2_ms=v2,
        v_app_ms=v_app,
        v_td_ms=v_td,
    )


# ---------------------------------------------------------------------------
# Field performance estimates
# ---------------------------------------------------------------------------


@dataclass
class FieldPerformance:
    """Estimated field-performance distances for one aircraft + airport."""

    airport: Airport
    v_speeds: VSpeeds

    todr_m: float  # Take-Off Distance Required (35 ft screen, m)
    bfl_m: float  # Balanced Field Length (m)
    asd_m: float  # Accelerate-Stop Distance (m)
    ldr_m: float  # Landing Distance Required (m)

    @property
    def toda_m(self) -> float:
        return self.airport.toda_m

    @property
    def lda_m(self) -> float:
        return self.airport.lda_m

    @property
    def to_margin_m(self) -> float:
        return self.toda_m - self.todr_m

    @property
    def land_margin_m(self) -> float:
        return self.lda_m - self.ldr_m

    @property
    def to_feasible(self) -> bool:
        return self.todr_m <= self.toda_m

    @property
    def land_feasible(self) -> bool:
        return self.ldr_m <= self.lda_m


def compute_field_performance(
    mtow_kg: float,
    wing_area_m2: float,
    airport: Airport,
    cl_max_to: float,
    cl_max_land: float,
    tw_sl: float,
    k_land: float = 0.60,
    bfl_factor: float = 1.15,
    perf_config: "PerformanceConfig | None" = None,
) -> FieldPerformance:
    """Estimate take-off and landing distances.

    Uses the same empirical constants as the matching chart constraint
    curves so distances are consistent with the design-space boundary.

    Parameters
    ----------
    tw_sl:
        Sea-level static T/W at MTOW (e.g. 2 x engine_thrust / (MTOW*g)).
    bfl_factor:
        BFL = bfl_factor * TODR.  Raymer Table 17.1: 1.15 for twin jets.
        Pass ``PerformanceConfig.bfl_factor`` to make this user-adjustable.
    """
    sigma = density_ratio(airport.elevation_m, airport.isa_deviation_c)
    ws_pa = mtow_kg * _G / wing_area_m2
    ws_psf = ws_pa * _PA_TO_PSF

    vs = compute_v_speeds(
        mtow_kg, wing_area_m2, airport, cl_max_to, cl_max_land, perf_config
    )

    # Take-off distance (Raymer reverse)
    toda_req_ft = 37.7 * ws_psf / (sigma * cl_max_to * max(tw_sl, 1e-6))
    todr_m = toda_req_ft / _M_TO_FT

    # Balanced field length
    bfl_m = todr_m * bfl_factor

    # Accelerate-stop distance at the balanced point equals BFL
    asd_m = bfl_m

    # Landing distance (reverse of landing limit formula)
    ldr_m = ws_pa * k_land / (sigma * cl_max_land)

    return FieldPerformance(
        airport=airport,
        v_speeds=vs,
        todr_m=todr_m,
        bfl_m=bfl_m,
        asd_m=asd_m,
        ldr_m=ldr_m,
    )


def static_thrust_to_weight(config, default: float = 0.30) -> float:
    """Sea-level static T/W at MTOW, from the live engine design parameters
    (:class:`~alas.config.geometry_config.EngineConfig`) and geometry config.

    Shared by ``MatchingChartWidget`` and ``LTOWidget`` (both display the same
    aircraft's matching-chart/field-performance data) so the two tabs can
    never silently diverge on this derived quantity. ``config`` is an
    :class:`~alas.config.settings.ALASConfig`.
    """
    try:
        n_eng = len(config.geometry.engine.spanwise_positions_m)
        return (
            n_eng
            * config.geometry.engine.thrust_kn
            * 1000.0
            / (config.requirements.mtow_kg * _G)
        )
    except Exception:
        return default


# ---------------------------------------------------------------------------
# Convenience: compute full matching chart data for a list of airports
# ---------------------------------------------------------------------------


@dataclass
class MatchingChartData:
    """Pre-computed constraint curves ready for plotting."""

    ws_pa: np.ndarray  # wing-loading axis (Pa)
    tw_cruise: np.ndarray  # cruise T/W₀ curve
    tw_oei_climb: float  # OEI climb T/W₀ (constant)
    tw_takeoff: Dict[str, np.ndarray]  # T/W₀ curve per airport
    ws_land_limits: Dict[str, float]  # max W/S (Pa) per airport
    design_ws_pa: float | None = None  # aircraft design point
    design_tw: float | None = None


def build_matching_chart(
    cd0: float,
    k: float,
    cruise_mach: float,
    cruise_altitude_m: float,
    mtow_kg: float,
    wing_area_m2: float,
    n_engines: int,
    airports: List[Airport],
    cl_max_to: float | None = None,
    cl_max_land: float | None = None,
    thrust_lapse: float | None = None,
    oei_gradient: float | None = None,
    k_land: float | None = None,
    oei_climb_cl: float | None = None,
    oei_climb_delta_cd: float | None = None,
    tw_design: float | None = None,
    n_ws_points: int = 150,
    ws_min_pa: float | None = None,
    ws_max_pa: float | None = None,
) -> MatchingChartData:
    """Build all matching-chart constraint curves for a set of airports.

    Every keyword defaults to ``None`` and falls back to a fresh
    :class:`~alas.config.performance_config.PerformanceConfig`'s field
    of the same name -- there is exactly one place these defaults live, so
    a caller that omits a kwarg can never silently drift from
    ``PerformanceConfig`` (pass the config's own values explicitly to
    reflect user edits). ``ws_min_pa`` / ``ws_max_pa`` set the wing-loading
    axis range [Pa]; widen them for light or very heavy aircraft.
    """
    from ..config.performance_config import PerformanceConfig

    _d = PerformanceConfig()
    cl_max_to = _d.cl_max_to if cl_max_to is None else cl_max_to
    cl_max_land = _d.cl_max_land if cl_max_land is None else cl_max_land
    thrust_lapse = _d.thrust_lapse if thrust_lapse is None else thrust_lapse
    oei_gradient = _d.oei_gradient if oei_gradient is None else oei_gradient
    k_land = _d.k_land if k_land is None else k_land
    oei_climb_cl = _d.oei_climb_cl if oei_climb_cl is None else oei_climb_cl
    oei_climb_delta_cd = (
        _d.oei_climb_delta_cd if oei_climb_delta_cd is None else oei_climb_delta_cd
    )
    ws_min_pa = _d.ws_min_pa if ws_min_pa is None else ws_min_pa
    ws_max_pa = _d.ws_max_pa if ws_max_pa is None else ws_max_pa

    ws_pa = np.linspace(ws_min_pa, ws_max_pa, n_ws_points)

    tw_cruise = tw_cruise_constraint(
        ws_pa, cd0, k, cruise_mach, cruise_altitude_m, thrust_lapse
    )
    tw_oei = tw_oei_climb_constraint(
        cd0,
        k,
        n_engines,
        oei_gradient,
        cl_climb=oei_climb_cl,
        delta_cd_to_config=oei_climb_delta_cd,
    )

    tw_to: Dict[str, np.ndarray] = {}
    ws_land: Dict[str, float] = {}
    for apt in airports:
        sigma = density_ratio(apt.elevation_m, apt.isa_deviation_c)
        tw_to[apt.name] = tw_takeoff_constraint(ws_pa, apt.toda_m, sigma, cl_max_to)
        ws_land[apt.name] = ws_landing_limit(apt.lda_m, sigma, cl_max_land, k_land)

    ws_design = mtow_kg * _G / wing_area_m2 if (mtow_kg and wing_area_m2) else None

    return MatchingChartData(
        ws_pa=ws_pa,
        tw_cruise=tw_cruise,
        tw_oei_climb=tw_oei,
        tw_takeoff=tw_to,
        ws_land_limits=ws_land,
        design_ws_pa=ws_design,
        design_tw=tw_design,
    )


# ---------------------------------------------------------------------------
# Breguet range, payload-range diagram, wing fuel-tank volume
# ---------------------------------------------------------------------------


def wing_fuel_volume_m3(wing: asb.Wing, usable_fraction: float) -> float:
    """Torenbeek geometric wing fuel-tank volume estimate [m^3].

        V = 0.54 * (S^2 / b) * (t/c)_root * (1 + lambda + lambda^2) / (1 + lambda)^2

    S/b/taper/root-t-over-c come straight from the built wing (``wing.area()``,
    ``wing.span()``, ``wing.taper_ratio()``, root xsec airfoil
    ``max_thickness()``), not static config, so the estimate tracks whatever
    geometry the optimizer/preset actually produced. The 0.54 coefficient
    already represents "usable tank fraction of the theoretical wing-box
    volume" (Torenbeek 1982); ``usable_fraction`` is an additional,
    user-adjustable derate on top of that for structure/ribs/systems/
    unusable-fuel allowance (``MassModelConfig.fuel_tank_usable_fraction``).
    """
    s = float(wing.area())
    b = float(wing.span())
    taper = float(wing.taper_ratio())
    t_over_c_root = float(wing.xsecs[0].airfoil.max_thickness())
    term_taper = (1.0 + taper + taper**2) / (1.0 + taper) ** 2
    v_geo = 0.54 * (s**2 / max(b, 1e-6)) * t_over_c_root * term_taper
    return v_geo * max(0.0, min(1.0, usable_fraction))


def breguet_range_m(
    tas_m_s: float, l_over_d: float, tsfc_si: float, w_start_kg: float, w_end_kg: float
) -> float:
    """Breguet range equation [m], SI throughout.

        R = (V / (tsfc_si * g)) * (L/D) * ln(W_start / W_end)

    ``tsfc_si`` is thrust-specific fuel consumption in kg fuel / (N.s)
    (mass flow per unit thrust force per second) -- convert from the more
    readable ``EngineSpec.cruise_tsfc_kg_kgf_hr`` via
    ``tsfc_si = tsfc_kg_kgf_hr / (g * 3600)``.
    """
    if w_end_kg <= 0.0 or w_start_kg <= 0.0 or w_end_kg > w_start_kg or tsfc_si <= 0.0:
        return 0.0
    return (tas_m_s / (tsfc_si * _G)) * l_over_d * math.log(w_start_kg / w_end_kg)


@dataclass
class PayloadRangePoint:
    """One corner of the payload-range diagram."""

    label: str  # "A" | "B" | "C" | "D"
    range_nm: float
    payload_kg: float
    fuel_kg: float
    tow_kg: float


@dataclass
class PayloadRangeResult:
    """The classic 4-point (A-B-C-D) payload-range envelope."""

    points: List[PayloadRangePoint]  # A, B, C, D in that order
    fuel_capacity_kg: float
    fuel_capacity_limit: str  # "MTOW budget" | "wing tank volume"
    max_payload_kg: float
    oew_kg: float
    mtow_kg: float


def payload_range_diagram(report, config) -> PayloadRangeResult:
    """Classic 4-point (A-B-C-D) Breguet payload-range diagram.

    Fuel capacity is the binding minimum of the MTOW weight budget
    (``MTOW - OEW``) and the physical wing-tank volume
    (:func:`wing_fuel_volume_m3`, converted to mass via
    ``MassModelConfig.fuel_density_kg_m3``) -- whichever constraint actually
    limits how much fuel the aircraft can carry, rather than a single
    hardcoded tank-capacity number. ``max_payload_kg`` is the payload the
    analyzed design's detailed cabin/cargo layout actually carries
    (``report.component_masses["Payload"]``) -- the design mission's payload,
    not an independently-derived structural maximum.
    """
    req = config.requirements
    mm = config.mass_model
    masses = report.component_masses
    oew = sum(masses.get(k, 0.0) for k in OEW_KEYS)
    max_payload = masses.get("Payload", 0.0)
    mtow = req.mtow_kg

    wing = report.airplane.wings[0]
    tank_capacity_kg = (
        wing_fuel_volume_m3(wing, mm.fuel_tank_usable_fraction) * mm.fuel_density_kg_m3
    )
    structural_capacity_kg = max(0.0, mtow - oew)
    if tank_capacity_kg <= structural_capacity_kg:
        fuel_capacity, limit = tank_capacity_kg, "wing tank volume"
    else:
        fuel_capacity, limit = structural_capacity_kg, "MTOW budget"

    dp = report.trimmed_design_point or report.design_point
    l_over_d = dp.l_over_d
    atmo = asb.Atmosphere(altitude=req.cruise_altitude_m)
    tas = req.cruise_mach * atmo.speed_of_sound()

    tsfc_si = config.geometry.engine.cruise_tsfc_kg_kgf_hr / (_G * 3600.0)

    def _range_nm(w_start: float, w_end: float) -> float:
        return breguet_range_m(tas, l_over_d, tsfc_si, w_start, w_end) / 1852.0

    # A: max payload, no fuel -- can't fly (range is definitionally 0).
    pt_a = PayloadRangePoint("A", 0.0, max_payload, 0.0, oew + max_payload)

    # B: max payload, fuel up to whichever budget binds first.
    fuel_b = max(0.0, min(fuel_capacity, mtow - oew - max_payload))
    tow_b = oew + max_payload + fuel_b
    pt_b = PayloadRangePoint(
        "B", _range_nm(tow_b, tow_b - fuel_b), max_payload, fuel_b, tow_b
    )

    # C: full tank, whatever payload the remaining MTOW budget allows.
    payload_c = max(0.0, min(max_payload, mtow - oew - fuel_capacity))
    tow_c = oew + payload_c + fuel_capacity
    pt_c = PayloadRangePoint(
        "C", _range_nm(tow_c, tow_c - fuel_capacity), payload_c, fuel_capacity, tow_c
    )

    # D: full tank, no payload (ferry range).
    tow_d = oew + fuel_capacity
    pt_d = PayloadRangePoint("D", _range_nm(tow_d, oew), 0.0, fuel_capacity, tow_d)

    return PayloadRangeResult(
        points=[pt_a, pt_b, pt_c, pt_d],
        fuel_capacity_kg=fuel_capacity,
        fuel_capacity_limit=limit,
        max_payload_kg=max_payload,
        oew_kg=oew,
        mtow_kg=mtow,
    )


@dataclass
class FuelVolumeCheck:
    """Wing usable fuel-tank capacity vs. the design's required fuel mass."""

    tank_volume_m3: float
    tank_capacity_kg: float
    required_fuel_kg: float
    margin_kg: float
    sufficient: bool


@dataclass
class VnDiagramData:
    """CS-25-style V-n (flight envelope) diagram data. Speeds are equivalent
    airspeed (EAS) at MTOW, sea-level reference density -- the convention
    V-n diagrams are plotted against."""

    v_kt: np.ndarray  # velocity axis [kt], 0..~1.1*max(VD, cruise)
    n_stall_pos: np.ndarray  # positive stall boundary n(v)
    n_stall_neg: np.ndarray  # negative stall boundary n(v)
    n_lim_pos: float
    n_lim_neg: float
    n_ult_pos: float
    n_ult_neg: float
    v_s_kt: float  # 1g clean stall speed at MTOW
    v_a_kt: float  # maneuvering speed
    v_c_kt: float  # design cruise speed (derived VD/1.25)
    v_d_kt: float  # design dive speed
    v_cruise_op_kt: float  # actual operating cruise EAS (informational)


def build_vn_diagram(
    plane: asb.Airplane, req, perf_cfg, cruise_alt_m: float
) -> VnDiagramData:
    """CS-25-style V-n (flight envelope) diagram.

    ``n_lim_pos = req.ultimate_load_factor / 1.5`` (CS-25.303 factor of
    safety); ``n_lim_neg = req.limit_load_factor_neg`` (CS-25.337(c));
    ``VD = req.dive_speed_m_s``; ``VC = VD / 1.25`` (CS-25.335(b) minimum
    margin) -- reusing existing config instead of adding redundant new
    speed/load-factor fields. Stall boundaries use ``perf_cfg.cl_max_clean``/
    ``cl_min_clean`` (the true clean-configuration stall CL, distinct from
    the flaps-down ``cl_max_to``/``cl_max_land``).
    """
    _KT = 1.943844
    w = req.mtow_kg * _G
    s = max(float(plane.s_ref), 1e-6)
    rho_sl = 1.225

    n_ult_pos = req.ultimate_load_factor
    n_lim_pos = n_ult_pos / 1.5
    n_lim_neg = req.limit_load_factor_neg
    n_ult_neg = n_lim_neg * 1.5

    v_d = req.dive_speed_m_s
    v_c = v_d / 1.25
    v_s = math.sqrt(2.0 * w / (rho_sl * s * perf_cfg.cl_max_clean))
    v_a = math.sqrt(2.0 * n_lim_pos * w / (rho_sl * s * perf_cfg.cl_max_clean))

    atmo = asb.Atmosphere(altitude=cruise_alt_m)
    v_cruise_op = (
        req.cruise_mach * atmo.speed_of_sound() * math.sqrt(atmo.density() / rho_sl)
    )

    v_max = max(v_d, v_cruise_op) * 1.15
    v_ms = np.linspace(0.0, v_max, 400)
    n_stall_pos = 0.5 * rho_sl * v_ms**2 * s * perf_cfg.cl_max_clean / w
    n_stall_neg = 0.5 * rho_sl * v_ms**2 * s * perf_cfg.cl_min_clean / w

    return VnDiagramData(
        v_kt=v_ms * _KT,
        n_stall_pos=n_stall_pos,
        n_stall_neg=n_stall_neg,
        n_lim_pos=n_lim_pos,
        n_lim_neg=n_lim_neg,
        n_ult_pos=n_ult_pos,
        n_ult_neg=n_ult_neg,
        v_s_kt=v_s * _KT,
        v_a_kt=v_a * _KT,
        v_c_kt=v_c * _KT,
        v_d_kt=v_d * _KT,
        v_cruise_op_kt=v_cruise_op * _KT,
    )


def fuel_volume_check(report, config) -> FuelVolumeCheck:
    """Compare the wing's usable fuel-tank capacity against the fuel mass
    the analyzed design actually requires (``report.component_masses["Fuel"]``,
    already computed by the mass analysis)."""
    mm = config.mass_model
    wing = report.airplane.wings[0]
    tank_volume_m3 = wing_fuel_volume_m3(wing, mm.fuel_tank_usable_fraction)
    tank_capacity_kg = tank_volume_m3 * mm.fuel_density_kg_m3
    required_fuel_kg = max(0.0, report.component_masses.get("Fuel", 0.0))
    margin_kg = tank_capacity_kg - required_fuel_kg
    return FuelVolumeCheck(
        tank_volume_m3=tank_volume_m3,
        tank_capacity_kg=tank_capacity_kg,
        required_fuel_kg=required_fuel_kg,
        margin_kg=margin_kg,
        sufficient=margin_kg >= 0.0,
    )
