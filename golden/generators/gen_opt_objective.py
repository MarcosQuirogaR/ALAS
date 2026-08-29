# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-opt::objective``: Design objective function, CG envelope checks,
fuel tank volume, and evaluation history tracking."""

from __future__ import annotations

import _framework

_framework.add_alas_to_path()

from alas.config.design_variables import DesignVector  # noqa: E402
from alas.config.settings import ALASConfig  # noqa: E402
from alas.geometry.aircraft_builder import AircraftBuilder  # noqa: E402
from alas.optimization.objective import (  # noqa: E402
    DesignObjective,
    _check_cg_envelope,
)
from alas.physics.mass import run_mass_analysis  # noqa: E402
from alas.physics.performance import wing_fuel_volume_m3  # noqa: E402


def generate() -> dict:
    config = ALASConfig()
    dv_default = DesignVector()
    builder = AircraftBuilder(config.geometry)
    plane = builder.build(dv_default, include_engines=False)
    wing = plane.wings[0]

    # 1. Fuel volume tests
    fuel_volume_cases = []
    for usable in [0.0, 0.5, 0.85, 1.0, 1.2]:
        vol = wing_fuel_volume_m3(wing, usable)
        fuel_volume_cases.append({
            "usable_fraction": usable,
            "volume_m3": float(vol),
        })

    # 2. CG envelope tests
    masses, coords, cg = run_mass_analysis(
        plane, config.requirements, config.geometry, config.mass_model
    )
    x_np = float(plane.wings[0].aerodynamic_center()[0]) + 0.5
    mac = float(wing.mean_aerodynamic_chord())

    cg_envelope_cases = []
    for cg_x_offset in [-2.0, -0.5, 0.0, 0.5, 2.0]:
        test_cg_x = float(cg[0]) + cg_x_offset
        viol, exc = _check_cg_envelope(
            plane, masses, coords, test_cg_x, x_np, mac, config
        )
        cg_envelope_cases.append({
            "cg_x": test_cg_x,
            "x_np": x_np,
            "mac": mac,
            "violation": bool(viol),
            "worst_exceedance": float(exc),
        })

    # 3. Objective evaluations
    obj = DesignObjective(config)
    eval_cases = []

    # Case A: Nominal default vector
    x_arr = dv_default.to_array()
    cost_nominal = obj(x_arr)
    eval_cases.append({
        "label": "nominal",
        "vector": [float(v) for v in x_arr],
        "cost": float(cost_nominal),
        "valid": bool(obj.history.valid[-1]),
        "l_over_d": float(obj.history.l_over_d[-1]),
        "span_m": float(obj.history.span_m[-1]),
        "alpha_deg": float(obj.history.alpha_deg[-1]),
        "area_m2": float(obj.history.area_m2[-1]),
        "trim_ih_deg": float(obj.history.trim_ih_deg[-1]),
        "reject_reason": str(obj.history.reject_reason[-1]),
    })

    # Case B: Perturbed vectors
    for factor in [0.95, 1.05]:
        dv_pert = DesignVector(
            span_m=dv_default.span_m * factor,
            root_chord_m=dv_default.root_chord_m * factor,
            break_chord_m=dv_default.break_chord_m * factor,
            tip_chord_m=dv_default.tip_chord_m * factor,
            sweep_deg=dv_default.sweep_deg * factor,
            fuselage_length_m=dv_default.fuselage_length_m * factor,
        )
        x_pert = dv_pert.to_array()
        cost_pert = obj(x_pert)
        eval_cases.append({
            "label": f"perturbed_{factor}",
            "vector": [float(v) for v in x_pert],
            "cost": float(cost_pert),
            "valid": bool(obj.history.valid[-1]),
            "l_over_d": float(obj.history.l_over_d[-1]),
            "span_m": float(obj.history.span_m[-1]),
            "alpha_deg": float(obj.history.alpha_deg[-1]),
            "area_m2": float(obj.history.area_m2[-1]),
            "trim_ih_deg": float(obj.history.trim_ih_deg[-1]),
            "reject_reason": str(obj.history.reject_reason[-1]),
        })

    # History diagnostics
    reason_counts = dict(obj.history.reject_reason_counts)

    return {
        "fuel_volume_cases": fuel_volume_cases,
        "cg_envelope_cases": cg_envelope_cases,
        "evaluation_cases": eval_cases,
        "history_summary": {
            "n_evaluations": obj.history.n_evaluations,
            "n_valid": obj.history.n_valid,
            "reject_reason_counts": reason_counts,
        },
    }


if __name__ == "__main__":
    _framework.write(
        "opt",
        "objective",
        generate(),
        description="Design objective function, fuel volume, CG envelope checks, and history diagnostics",
    )
