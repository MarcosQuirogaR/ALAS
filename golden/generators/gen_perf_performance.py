# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-perf::performance``: the closed-form point-performance surface.

Exercises the functions of ``alas/physics/performance.py`` whose inputs are
already available in phase P4 -- the atmospheric density ratio, the four
matching-chart constraint curves, the assembled matching chart, the FAR-25
V-speed schedule, the field-performance distances, the Breguet range equation
and the V-n (flight-envelope) diagram.

Left for a later change, and recorded in ``docs/PORTING.md`` rather than here:
``wing_fuel_volume_m3`` (reads a built ``asb.Wing``, so it needs the
``alas-geom`` edge), ``payload_range_diagram`` and ``fuel_volume_check`` (both
orchestrate a full analysis ``report`` object that only exists in P10), and
``static_thrust_to_weight`` (an ``ALASConfig`` accessor with an
exception-fallback, an app-layer concern).

The cases are chosen to reach each branch: ``density_ratio``'s ISA offset,
``tw_oei_climb_constraint``'s single-engine short circuit and the twin/tri/quad
gradients, ``compute_v_speeds``'s ``V1 >= VMC`` floor and the two ``VR`` floors,
and ``build_matching_chart``'s ``None`` keyword fallback onto a fresh
``PerformanceConfig``.
"""

from __future__ import annotations

import _framework

_framework.add_alas_to_path()

from types import SimpleNamespace  # noqa: E402

from alas.config.airports import Airport  # noqa: E402
from alas.config.performance_config import PerformanceConfig  # noqa: E402
from alas.config.requirements import DesignRequirements  # noqa: E402
from alas.physics import performance as perf  # noqa: E402


def _perf_config(overrides: dict) -> PerformanceConfig:
    pc = PerformanceConfig()
    for key, value in overrides.items():
        setattr(pc, key, value)
    return pc


def _airport(elevation_m: float, toda_m: float, lda_m: float, isa_deviation_c: float):
    # name/icao/lat/lon are irrelevant to every function under test; only the
    # elevation, ISA offset and runway distances feed the arithmetic.
    return Airport(
        name="Fixture",
        icao="FIX",
        elevation_m=elevation_m,
        toda_m=toda_m,
        lda_m=lda_m,
        isa_deviation_c=isa_deviation_c,
    )


def _fl(seq) -> list:
    return [float(v) for v in seq]


# --- density_ratio ---------------------------------------------------------

_DENSITY_RATIO = [
    {"elevation_m": 0.0, "delta_isa_c": 0.0},
    {"elevation_m": 1650.0, "delta_isa_c": 0.0},
    {"elevation_m": 2400.0, "delta_isa_c": 15.0},
    {"elevation_m": 4058.0, "delta_isa_c": 20.0},
]


def _density_ratio_cases() -> list:
    out = []
    for c in _DENSITY_RATIO:
        sigma = perf.density_ratio(c["elevation_m"], c["delta_isa_c"])
        out.append({**c, "sigma": float(sigma)})
    return out


# --- tw_cruise_constraint --------------------------------------------------

_TW_CRUISE = [
    {
        "ws_pa": [3000.0, 5000.0, 7000.0, 9000.0],
        "cd0": 0.019,
        "k": 0.045,
        "cruise_mach": 0.78,
        "cruise_altitude_m": 10668.0,
        "thrust_lapse": 0.235,
    },
    {
        "ws_pa": [4000.0, 6000.0, 8000.0],
        "cd0": 0.022,
        "k": 0.050,
        "cruise_mach": 0.82,
        "cruise_altitude_m": 11278.0,
        "thrust_lapse": 0.28,
    },
]


def _tw_cruise_cases() -> list:
    import numpy as np

    out = []
    for c in _TW_CRUISE:
        expected = perf.tw_cruise_constraint(
            np.array(c["ws_pa"]),
            c["cd0"],
            c["k"],
            c["cruise_mach"],
            c["cruise_altitude_m"],
            c["thrust_lapse"],
        )
        out.append({**c, "expected": _fl(expected)})
    return out


# --- tw_oei_climb_constraint ----------------------------------------------

_TW_OEI = [
    {"cd0": 0.019, "k": 0.045, "n_engines": 1, "oei_gradient": 0.024,
     "cl_climb": 1.2, "delta_cd_to_config": 0.025},
    {"cd0": 0.019, "k": 0.045, "n_engines": 2, "oei_gradient": 0.024,
     "cl_climb": 1.2, "delta_cd_to_config": 0.025},
    {"cd0": 0.021, "k": 0.048, "n_engines": 3, "oei_gradient": 0.027,
     "cl_climb": 1.3, "delta_cd_to_config": 0.030},
    {"cd0": 0.023, "k": 0.052, "n_engines": 4, "oei_gradient": 0.030,
     "cl_climb": 1.15, "delta_cd_to_config": 0.035},
]


def _tw_oei_cases() -> list:
    out = []
    for c in _TW_OEI:
        v = perf.tw_oei_climb_constraint(
            c["cd0"], c["k"], c["n_engines"], c["oei_gradient"],
            cl_climb=c["cl_climb"], delta_cd_to_config=c["delta_cd_to_config"],
        )
        out.append({**c, "expected": float(v)})
    return out


# --- tw_takeoff_constraint -------------------------------------------------

_TW_TAKEOFF = [
    {"ws_pa": [3000.0, 5000.0, 7000.0], "toda_m": 3902.0, "sigma": 1.0,
     "cl_max_to": 1.80},
    {"ws_pa": [4000.0, 6000.0, 8000.0], "toda_m": 3480.0, "sigma": 0.82,
     "cl_max_to": 2.00},
]


def _tw_takeoff_cases() -> list:
    import numpy as np

    out = []
    for c in _TW_TAKEOFF:
        expected = perf.tw_takeoff_constraint(
            np.array(c["ws_pa"]), c["toda_m"], c["sigma"], c["cl_max_to"]
        )
        out.append({**c, "expected": _fl(expected)})
    return out


# --- ws_landing_limit ------------------------------------------------------

_WS_LANDING = [
    {"lda_m": 3902.0, "sigma": 1.0, "cl_max_land": 2.60, "k_factor": 0.60},
    {"lda_m": 2900.0, "sigma": 0.78, "cl_max_land": 2.80, "k_factor": 0.55},
]


def _ws_landing_cases() -> list:
    out = []
    for c in _WS_LANDING:
        v = perf.ws_landing_limit(
            c["lda_m"], c["sigma"], c["cl_max_land"], c["k_factor"]
        )
        out.append({**c, "expected": float(v)})
    return out


# --- compute_v_speeds ------------------------------------------------------

_V_SPEEDS = [
    {
        "name": "sea_level_defaults",
        "mtow_kg": 79_000.0, "wing_area_m2": 122.0,
        "airport": {"elevation_m": 25.0, "toda_m": 3902.0, "lda_m": 3902.0,
                    "isa_deviation_c": 0.0},
        "cl_max_to": 1.80, "cl_max_land": 2.60, "config": {},
    },
    {
        "name": "hot_high_field",
        "mtow_kg": 230_000.0, "wing_area_m2": 361.0,
        "airport": {"elevation_m": 2400.0, "toda_m": 3300.0, "lda_m": 3300.0,
                    "isa_deviation_c": 15.0},
        "cl_max_to": 2.00, "cl_max_land": 2.90, "config": {},
    },
    {
        # A tight VR/VMC combination so v1_vr_factor*VR would dip below VMC,
        # exercising the max(v1_vr_factor*VR, v_mc) floor on V1.
        "name": "v1_floored_to_vmc",
        "mtow_kg": 79_000.0, "wing_area_m2": 122.0,
        "airport": {"elevation_m": 25.0, "toda_m": 3902.0, "lda_m": 3902.0,
                    "isa_deviation_c": 0.0},
        "cl_max_to": 1.80, "cl_max_land": 2.60,
        "config": {"vr_vmc_factor": 1.0, "vr_vstall_factor": 1.0,
                   "v1_vr_factor": 0.90},
    },
]


def _v_speeds_dict(vs) -> dict:
    return {k: float(v) for k, v in vs.as_ms().items()}


def _v_speeds_cases() -> list:
    out = []
    for c in _V_SPEEDS:
        pc = _perf_config(c["config"])
        vs = perf.compute_v_speeds(
            c["mtow_kg"], c["wing_area_m2"], _airport(**c["airport"]),
            c["cl_max_to"], c["cl_max_land"], pc,
        )
        out.append({**c, "expected": _v_speeds_dict(vs)})
    return out


# --- compute_field_performance --------------------------------------------

_FIELD = [
    {
        "name": "narrowbody",
        "mtow_kg": 79_000.0, "wing_area_m2": 122.0,
        "airport": {"elevation_m": 25.0, "toda_m": 3902.0, "lda_m": 3902.0,
                    "isa_deviation_c": 0.0},
        "cl_max_to": 1.80, "cl_max_land": 2.60, "tw_sl": 0.31,
        "k_land": 0.60, "bfl_factor": 1.15, "config": {},
    },
    {
        "name": "widebody_hot_high",
        "mtow_kg": 254_000.0, "wing_area_m2": 361.0,
        "airport": {"elevation_m": 1650.0, "toda_m": 4000.0, "lda_m": 4000.0,
                    "isa_deviation_c": 10.0},
        "cl_max_to": 2.10, "cl_max_land": 2.95, "tw_sl": 0.29,
        "k_land": 0.62, "bfl_factor": 1.18, "config": {},
    },
]


def _field_cases() -> list:
    out = []
    for c in _FIELD:
        pc = _perf_config(c["config"])
        fp = perf.compute_field_performance(
            c["mtow_kg"], c["wing_area_m2"], _airport(**c["airport"]),
            c["cl_max_to"], c["cl_max_land"], c["tw_sl"],
            k_land=c["k_land"], bfl_factor=c["bfl_factor"], perf_config=pc,
        )
        out.append({
            **c,
            "expected": {
                "todr_m": float(fp.todr_m),
                "bfl_m": float(fp.bfl_m),
                "asd_m": float(fp.asd_m),
                "ldr_m": float(fp.ldr_m),
                "to_margin_m": float(fp.to_margin_m),
                "land_margin_m": float(fp.land_margin_m),
                "to_feasible": bool(fp.to_feasible),
                "land_feasible": bool(fp.land_feasible),
                "v_speeds": _v_speeds_dict(fp.v_speeds),
            },
        })
    return out


# --- build_matching_chart --------------------------------------------------

# One case supplies every keyword; the other leaves the config-backed ones
# None so build_matching_chart falls back onto a fresh PerformanceConfig,
# which the Rust port must reproduce through PerformanceConfig::default().
_MATCHING = [
    {
        "name": "all_explicit",
        "cd0": 0.019, "k": 0.045, "cruise_mach": 0.78,
        "cruise_altitude_m": 10668.0, "mtow_kg": 79_000.0,
        "wing_area_m2": 122.0, "n_engines": 2,
        "airports": [
            {"elevation_m": 25.0, "toda_m": 3902.0, "lda_m": 3902.0,
             "isa_deviation_c": 0.0, "name": "London Heathrow (EGLL)"},
            {"elevation_m": 2400.0, "toda_m": 3300.0, "lda_m": 3300.0,
             "isa_deviation_c": 15.0, "name": "Mexico City (MMMX)"},
        ],
        "cl_max_to": 1.80, "cl_max_land": 2.60, "thrust_lapse": 0.235,
        "oei_gradient": 0.024, "k_land": 0.60, "oei_climb_cl": 1.2,
        "oei_climb_delta_cd": 0.025, "tw_design": 0.31, "n_ws_points": 5,
        "ws_min_pa": 2000.0, "ws_max_pa": 10000.0,
    },
    {
        "name": "config_fallback",
        "cd0": 0.021, "k": 0.048, "cruise_mach": 0.80,
        "cruise_altitude_m": 11278.0, "mtow_kg": 230_000.0,
        "wing_area_m2": 361.0, "n_engines": 4,
        "airports": [
            {"elevation_m": 1650.0, "toda_m": 4000.0, "lda_m": 4000.0,
             "isa_deviation_c": 10.0, "name": "Denver (KDEN)"},
        ],
        "cl_max_to": None, "cl_max_land": None, "thrust_lapse": None,
        "oei_gradient": None, "k_land": None, "oei_climb_cl": None,
        "oei_climb_delta_cd": None, "tw_design": None, "n_ws_points": 6,
        "ws_min_pa": None, "ws_max_pa": None,
    },
]


def _matching_cases() -> list:
    out = []
    for c in _MATCHING:
        airports = [
            Airport(name=a["name"], icao="FIX", elevation_m=a["elevation_m"],
                    toda_m=a["toda_m"], lda_m=a["lda_m"],
                    isa_deviation_c=a["isa_deviation_c"])
            for a in c["airports"]
        ]
        data = perf.build_matching_chart(
            c["cd0"], c["k"], c["cruise_mach"], c["cruise_altitude_m"],
            c["mtow_kg"], c["wing_area_m2"], c["n_engines"], airports,
            cl_max_to=c["cl_max_to"], cl_max_land=c["cl_max_land"],
            thrust_lapse=c["thrust_lapse"], oei_gradient=c["oei_gradient"],
            k_land=c["k_land"], oei_climb_cl=c["oei_climb_cl"],
            oei_climb_delta_cd=c["oei_climb_delta_cd"], tw_design=c["tw_design"],
            n_ws_points=c["n_ws_points"], ws_min_pa=c["ws_min_pa"],
            ws_max_pa=c["ws_max_pa"],
        )
        out.append({
            **c,
            "expected": {
                "ws_pa": _fl(data.ws_pa),
                "tw_cruise": _fl(data.tw_cruise),
                "tw_oei_climb": float(data.tw_oei_climb),
                "tw_takeoff": {k: _fl(v) for k, v in data.tw_takeoff.items()},
                "ws_land_limits": {k: float(v)
                                   for k, v in data.ws_land_limits.items()},
                "design_ws_pa": (None if data.design_ws_pa is None
                                 else float(data.design_ws_pa)),
                "design_tw": (None if data.design_tw is None
                              else float(data.design_tw)),
            },
        })
    return out


# --- breguet_range_m -------------------------------------------------------

_BREGUET = [
    # A normal cruise leg.
    {"tas_m_s": 231.0, "l_over_d": 18.5, "tsfc_si": 1.6e-5,
     "w_start_kg": 79_000.0, "w_end_kg": 63_000.0},
    # Each guard branch returns 0.0: end weight above start, non-positive
    # weights, and a non-positive TSFC.
    {"tas_m_s": 231.0, "l_over_d": 18.5, "tsfc_si": 1.6e-5,
     "w_start_kg": 63_000.0, "w_end_kg": 79_000.0},
    {"tas_m_s": 231.0, "l_over_d": 18.5, "tsfc_si": 1.6e-5,
     "w_start_kg": 79_000.0, "w_end_kg": 0.0},
    {"tas_m_s": 231.0, "l_over_d": 18.5, "tsfc_si": 0.0,
     "w_start_kg": 79_000.0, "w_end_kg": 63_000.0},
]


def _breguet_cases() -> list:
    out = []
    for c in _BREGUET:
        v = perf.breguet_range_m(
            c["tas_m_s"], c["l_over_d"], c["tsfc_si"],
            c["w_start_kg"], c["w_end_kg"],
        )
        out.append({**c, "expected": float(v)})
    return out


# --- build_vn_diagram ------------------------------------------------------

# build_vn_diagram reads only `plane.s_ref` off its airplane argument, so the
# fixture and the Rust port both pass the reference area on its own rather than
# a whole built airplane.
_VN = [
    {
        "name": "narrowbody",
        "s_ref": 122.0, "cruise_alt_m": 10668.0,
        "req": {"cruise_mach": 0.78, "mtow_kg": 79_000.0,
                "ultimate_load_factor": 3.75, "dive_speed_m_s": 190.0,
                "limit_load_factor_neg": -1.0},
        "perf": {"cl_max_clean": 1.50, "cl_min_clean": -1.00},
    },
    {
        "name": "widebody",
        "s_ref": 361.0, "cruise_alt_m": 11278.0,
        "req": {"cruise_mach": 0.85, "mtow_kg": 230_000.0,
                "ultimate_load_factor": 3.75, "dive_speed_m_s": 210.0,
                "limit_load_factor_neg": -1.0},
        "perf": {"cl_max_clean": 1.40, "cl_min_clean": -0.90},
    },
]


def _vn_cases() -> list:
    out = []
    for c in _VN:
        req = DesignRequirements()
        for k, v in c["req"].items():
            setattr(req, k, v)
        pc = _perf_config(c["perf"])
        plane = SimpleNamespace(s_ref=c["s_ref"])
        data = perf.build_vn_diagram(plane, req, pc, c["cruise_alt_m"])
        out.append({
            **c,
            "expected": {
                "n_lim_pos": float(data.n_lim_pos),
                "n_lim_neg": float(data.n_lim_neg),
                "n_ult_pos": float(data.n_ult_pos),
                "n_ult_neg": float(data.n_ult_neg),
                "v_s_kt": float(data.v_s_kt),
                "v_a_kt": float(data.v_a_kt),
                "v_c_kt": float(data.v_c_kt),
                "v_d_kt": float(data.v_d_kt),
                "v_cruise_op_kt": float(data.v_cruise_op_kt),
                "v_kt": _fl(data.v_kt),
                "n_stall_pos": _fl(data.n_stall_pos),
                "n_stall_neg": _fl(data.n_stall_neg),
            },
        })
    return out


def main() -> None:
    payload = {
        "density_ratio": _density_ratio_cases(),
        "tw_cruise": _tw_cruise_cases(),
        "tw_oei_climb": _tw_oei_cases(),
        "tw_takeoff": _tw_takeoff_cases(),
        "ws_landing_limit": _ws_landing_cases(),
        "v_speeds": _v_speeds_cases(),
        "field_performance": _field_cases(),
        "matching_chart": _matching_cases(),
        "breguet_range": _breguet_cases(),
        "vn_diagram": _vn_cases(),
    }
    _framework.write(
        "perf",
        "performance",
        payload,
        description=(
            "alas.physics.performance closed-form point-performance surface: "
            "density_ratio, the four matching-chart constraint curves, "
            "build_matching_chart, the FAR-25 V-speed schedule, "
            "compute_field_performance, breguet_range_m and build_vn_diagram, "
            "across branch-reaching cases"
        ),
    )


if __name__ == "__main__":
    main()
