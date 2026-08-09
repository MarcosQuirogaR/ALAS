# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-prop::cycle``: the on-design turbofan cycle end to end.

Exercises ``alas/physics/propulsion.py``'s public surface against the default
``PropulsionCycleConfig``:

* ``compute_turbofan_cycle`` across flight conditions chosen to reach each
  branch -- a cruise point and a low-Mach climb (both feasible), a static
  sea-level point (``M0=0``, so the propulsive/overall efficiency drops to the
  ``v0 <= 1e-6`` branch and the fan nozzle runs unchoked), a low-bypass point,
  and two infeasible points (a turbine inlet temperature below the compressor
  discharge, and one so high the combustor energy balance has no solution).
* ``anchor_mass_flow_kg_s``, tying the static specific thrust to a rated thrust.
* the four parametric sweeps (``compute_carpet_plot``,
  ``compute_bpr_sensitivity``, ``compute_efficiency_decomposition``,
  ``compute_altitude_sweep``), on small grids, so the Rust loops are checked
  against the reference's array layout and NaN/feasibility bookkeeping, not
  only the single-point kernel they call.
* ``classify_engine_by_bpr`` at bypass ratios either side of its two cut points.

Every cycle evaluation reads ``asb.Atmosphere(altitude=...)`` -- the fitted
"differentiable" model, not the closed-form ISA -- which the Rust side reaches
through ``Atmosphere::new``; the two agree well inside the ``closed`` tier.

A field the reference leaves as NaN (every output of an infeasible cycle, and
every masked-off cell of a sweep) is written as JSON ``null`` and mapped back
to a NaN on the Rust side, where a NaN is defined to agree with a NaN.
"""

from __future__ import annotations

import math

import _framework

_framework.add_alas_to_path()

import numpy as np  # noqa: E402

from alas.physics.propulsion import (  # noqa: E402
    TurbofanCycleInputs,
    anchor_mass_flow_kg_s,
    classify_engine_by_bpr,
    compute_altitude_sweep,
    compute_bpr_sensitivity,
    compute_carpet_plot,
    compute_efficiency_decomposition,
    compute_turbofan_cycle,
)


def _num(x) -> float | None:
    """A scalar for the fixture: NaN becomes ``null`` (mapped back to NaN)."""
    fx = float(x)
    return None if math.isnan(fx) else fx


def _grid(a: np.ndarray) -> list:
    """A NumPy array (any rank) as nested lists, NaN cells as ``null``."""
    if a.ndim == 0:
        return _num(a)
    return [_grid(row) for row in a]


def _cycle_record(result) -> dict:
    return {
        "cycle_feasible": bool(result.cycle_feasible),
        "infeasibility_reason": result.infeasibility_reason,
        "specific_thrust_ms": _num(result.specific_thrust_ms),
        "tsfc_mg_ns": _num(result.tsfc_mg_ns),
        "fuel_air_ratio": _num(result.fuel_air_ratio),
        "thermal_efficiency": _num(result.thermal_efficiency),
        "propulsive_efficiency": _num(result.propulsive_efficiency),
        "overall_efficiency": _num(result.overall_efficiency),
        "temperature_t0_k": _num(result.temperature_t0_k),
        "temperature_t13_k": _num(result.temperature_t13_k),
        "temperature_t25_k": _num(result.temperature_t25_k),
        "temperature_t3_k": _num(result.temperature_t3_k),
        "temperature_t4_k": _num(result.temperature_t4_k),
        "temperature_t45_k": _num(result.temperature_t45_k),
        "temperature_t5_k": _num(result.temperature_t5_k),
        "temperature_t6_k": _num(result.temperature_t6_k),
        "exit_velocity_core_ms": _num(result.exit_velocity_core_ms),
        "exit_velocity_fan_ms": _num(result.exit_velocity_fan_ms),
    }


# Each case's six inputs feed one TurbofanCycleInputs; the cfg is the default
# PropulsionCycleConfig, which the Rust parity test also constructs.
_CYCLE_CASES = [
    {
        "name": "cruise_nominal",
        "mach": 0.85,
        "altitude_m": 10668.0,
        "bypass_ratio": 10.0,
        "overall_pressure_ratio": 50.0,
        "fan_pressure_ratio": 1.5,
        "turbine_inlet_temperature_k": 1600.0,
    },
    {
        "name": "static_takeoff",
        "mach": 0.0,
        "altitude_m": 0.0,
        "bypass_ratio": 10.0,
        "overall_pressure_ratio": 45.0,
        "fan_pressure_ratio": 1.5,
        "turbine_inlet_temperature_k": 1700.0,
    },
    {
        "name": "climb_low_mach",
        "mach": 0.45,
        "altitude_m": 3000.0,
        "bypass_ratio": 8.0,
        "overall_pressure_ratio": 40.0,
        "fan_pressure_ratio": 1.6,
        "turbine_inlet_temperature_k": 1650.0,
    },
    {
        "name": "high_opr",
        "mach": 0.82,
        "altitude_m": 11000.0,
        "bypass_ratio": 12.0,
        "overall_pressure_ratio": 60.0,
        "fan_pressure_ratio": 1.45,
        "turbine_inlet_temperature_k": 1750.0,
    },
    {
        "name": "low_bypass",
        "mach": 0.90,
        "altitude_m": 9000.0,
        "bypass_ratio": 0.8,
        "overall_pressure_ratio": 30.0,
        "fan_pressure_ratio": 2.2,
        "turbine_inlet_temperature_k": 1600.0,
    },
    {
        "name": "infeasible_tit_below_compressor",
        "mach": 0.0,
        "altitude_m": 0.0,
        "bypass_ratio": 10.0,
        "overall_pressure_ratio": 50.0,
        "fan_pressure_ratio": 1.5,
        "turbine_inlet_temperature_k": 400.0,
    },
    {
        "name": "infeasible_tit_above_heating",
        "mach": 0.8,
        "altitude_m": 10000.0,
        "bypass_ratio": 10.0,
        "overall_pressure_ratio": 50.0,
        "fan_pressure_ratio": 1.5,
        "turbine_inlet_temperature_k": 40000.0,
    },
]


def _cycle_cases() -> list:
    out = []
    for case in _CYCLE_CASES:
        inputs = {k: v for k, v in case.items() if k != "name"}
        result = compute_turbofan_cycle(TurbofanCycleInputs(**inputs))
        out.append(
            {"name": case["name"], "inputs": inputs, "result": _cycle_record(result)}
        )
    return out


def _anchor_case() -> dict:
    thrust_kn = 470.0
    inputs = {
        "thrust_kn": thrust_kn,
        "overall_pressure_ratio": 50.0,
        "fan_pressure_ratio": 1.5,
        "bypass_ratio": 10.0,
        "turbine_inlet_temperature_k": 1700.0,
    }
    mdot, static_result = anchor_mass_flow_kg_s(**inputs)
    return {
        "inputs": inputs,
        "mdot_total_kg_s": _num(mdot),
        "static_result": _cycle_record(static_result),
    }


def _carpet_case() -> dict:
    inputs = {
        "compressor_pressure_ratio_vector": [40.0, 50.0, 60.0],
        "tit_vector_k": [1500.0, 1700.0],
        "mach": 0.8,
        "altitude_m": 10000.0,
        "fan_pressure_ratio": 1.5,
        "bypass_ratio": 10.0,
    }
    r = compute_carpet_plot(
        np.asarray(inputs["compressor_pressure_ratio_vector"]),
        np.asarray(inputs["tit_vector_k"]),
        inputs["mach"],
        inputs["altitude_m"],
        inputs["fan_pressure_ratio"],
        inputs["bypass_ratio"],
    )
    return {
        "inputs": inputs,
        "compressor_pressure_ratio_vector": _grid(r.compressor_pressure_ratio_vector),
        "tit_vector_k": _grid(r.tit_vector_k),
        "specific_thrust_ms": _grid(r.specific_thrust_ms),
        "tsfc_mg_ns": _grid(r.tsfc_mg_ns),
        "feasible_mask": [[bool(v) for v in row] for row in r.feasible_mask],
    }


def _bpr_case() -> dict:
    inputs = {
        "bpr_vector": [1.0, 5.0, 10.0],
        "overall_pressure_ratio": 50.0,
        "turbine_inlet_temperature_k": 1600.0,
        "fan_pressure_ratio": 1.5,
        "mach": 0.8,
        "altitude_m": 10000.0,
    }
    r = compute_bpr_sensitivity(
        np.asarray(inputs["bpr_vector"]),
        inputs["overall_pressure_ratio"],
        inputs["turbine_inlet_temperature_k"],
        inputs["fan_pressure_ratio"],
        inputs["mach"],
        inputs["altitude_m"],
    )
    return {
        "inputs": inputs,
        "bypass_ratio_vector": _grid(r.bypass_ratio_vector),
        "specific_thrust_ms": _grid(r.specific_thrust_ms),
        "tsfc_mg_ns": _grid(r.tsfc_mg_ns),
        "feasible_mask": [bool(v) for v in r.feasible_mask],
    }


def _efficiency_case() -> dict:
    inputs = {
        "pi_c_vector": [30.0, 40.0, 50.0],
        "turbine_inlet_temperature_k": 1600.0,
        "bypass_ratio": 10.0,
        "fan_pressure_ratio": 1.5,
        "mach": 0.8,
        "altitude_m": 10000.0,
    }
    r = compute_efficiency_decomposition(
        np.asarray(inputs["pi_c_vector"]),
        inputs["turbine_inlet_temperature_k"],
        inputs["bypass_ratio"],
        inputs["fan_pressure_ratio"],
        inputs["mach"],
        inputs["altitude_m"],
    )
    return {
        "inputs": inputs,
        "compressor_pressure_ratio_vector": _grid(r.compressor_pressure_ratio_vector),
        "thermal_efficiency": _grid(r.thermal_efficiency),
        "propulsive_efficiency": _grid(r.propulsive_efficiency),
        "overall_efficiency": _grid(r.overall_efficiency),
        "feasible_mask": [bool(v) for v in r.feasible_mask],
    }


def _altitude_case() -> dict:
    inputs = {
        "altitude_vector_m": [0.0, 5000.0, 11000.0],
        "mach_values": [0.0, 0.8],
        "bypass_ratio": 10.0,
        "overall_pressure_ratio": 50.0,
        "fan_pressure_ratio": 1.5,
        "turbine_inlet_temperature_k": 1650.0,
        "mdot_total_kg_s": 350.0,
    }
    r = compute_altitude_sweep(
        np.asarray(inputs["altitude_vector_m"]),
        inputs["mach_values"],
        inputs["bypass_ratio"],
        inputs["overall_pressure_ratio"],
        inputs["fan_pressure_ratio"],
        inputs["turbine_inlet_temperature_k"],
        mdot_total_kg_s=inputs["mdot_total_kg_s"],
    )
    return {
        "inputs": inputs,
        "altitude_m": _grid(r.altitude_m),
        "mach_values": [float(m) for m in r.mach_values],
        "specific_thrust_ms": _grid(r.specific_thrust_ms),
        "tsfc_mg_ns": _grid(r.tsfc_mg_ns),
        "dimensional_thrust_kn": _grid(r.dimensional_thrust_kn),
        "feasible_mask": [[bool(v) for v in row] for row in r.feasible_mask],
    }


def _classify_cases() -> list:
    return [
        {"bypass_ratio": bpr, "label": classify_engine_by_bpr(bpr)}
        for bpr in (0.5, 0.999, 1.0, 3.0, 4.999, 5.0, 12.0)
    ]


def main() -> None:
    payload = {
        "cycle_cases": _cycle_cases(),
        "anchor": _anchor_case(),
        "carpet_plot": _carpet_case(),
        "bpr_sensitivity": _bpr_case(),
        "efficiency_decomposition": _efficiency_case(),
        "altitude_sweep": _altitude_case(),
        "classify": _classify_cases(),
    }
    _framework.write(
        "prop",
        "cycle",
        payload,
        description=(
            "alas.physics.propulsion: compute_turbofan_cycle across feasible/"
            "infeasible/static branches, anchor_mass_flow_kg_s, the four "
            "parametric sweeps and classify_engine_by_bpr, on the default "
            "PropulsionCycleConfig"
        ),
    )


if __name__ == "__main__":
    main()
