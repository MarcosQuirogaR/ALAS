# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-stab::suave_static``: SUAVE's ``Analyses.Stability.Fidelity_Zero``,
static branch only.

Two-stage, like ``gen_aero_drag_buildup.py``: stage 1 (under ``.venv``) builds
a ``vehicle_request`` off ``FullAnalysis``'s report on ``DesignVector.default()``;
stage 2 (under ``.suave-venv``) builds the SUAVE ``Vehicle`` from it and drives
``Fidelity_Zero`` directly -- *not* through ``mission_builder.configs_setup``/
``base_analysis``, because the only thing this row is scoped to is what
``Fidelity_Zero.__call__`` reads off an already-built vehicle and a synthetic
``conditions`` object, the same narrow scope
``external tools/suave_runner/mission_builder.py:89-91`` reaches it through.

The dynamic-stability branch (gated on a moments-of-inertia tensor this
program's SUAVE integration never populates) is out of scope; see this row's
module doc in ``crates/alas-stab/src/suave_static.rs``.
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path

import _framework

_SCRATCH = _framework.GOLDEN_DIR / "stab" / "_suave_static_request.json"


def _stage_build():
    _framework.add_alas_to_path()

    from alas.config.settings import ALASConfig
    from alas.integration.suave_vehicle import build_vehicle_request
    from alas.analysis.full_analysis import FullAnalysis
    from alas.config.design_variables import DesignVector

    config = ALASConfig()
    analysis = FullAnalysis(config)
    report = analysis.run(DesignVector.default(), verbose=False)
    vehicle_request = build_vehicle_request(report, config)

    _SCRATCH.parent.mkdir(parents=True, exist_ok=True)
    with _SCRATCH.open("w", encoding="utf-8", newline="\n") as f:
        json.dump(vehicle_request, f, indent=2)
        f.write("\n")


def _wing_common(w) -> dict:
    return {
        "aspect_ratio": float(w.aspect_ratio),
        "sweep_quarter_chord_rad": float(w.sweeps.quarter_chord),
        "taper": float(w.taper),
        "area_ref_m2": float(w.areas.reference),
        "origin_x_m": float(w.origin[0][0]),
        "dynamic_pressure_ratio": float(w.dynamic_pressure_ratio),
        "vertical": bool(w.vertical),
        "twist_root_rad": float(w.twists.root),
        "twist_tip_rad": float(w.twists.tip),
    }


def _stage_solve():
    _framework.add_suave_to_path()

    import numpy as np
    import vehicle_builder
    import SUAVE
    from SUAVE.Core import Data

    vehicle_request = json.loads(_SCRATCH.read_text(encoding="utf-8"))
    vehicle = vehicle_builder.build_vehicle(vehicle_request)

    stability = SUAVE.Analyses.Stability.Fidelity_Zero()
    stability.geometry = vehicle
    stability.finalize()

    main_wing = vehicle.wings["main_wing"]
    hstab = vehicle.wings["horizontal_stabilizer"]
    vstab = vehicle.wings["vertical_stabilizer"]
    fuselage = vehicle.fuselages["fuselage"]

    geometry = {
        "reference_area_m2": float(vehicle.reference_area),
        "main_wing": {
            **_wing_common(main_wing),
            "origin_z_m": float(main_wing.origin[0][2]),
            "chord_root_m": float(main_wing.chords.root),
            "span_m": float(main_wing.spans.projected),
            "mac_m": float(main_wing.chords.mean_aerodynamic),
        },
        "horizontal_stabilizer": _wing_common(hstab),
        "vertical_stabilizer": {
            **_wing_common(vstab),
            "origin_z_m": float(vstab.origin[0][2]),
            "chord_root_m": float(vstab.chords.root),
            "chord_tip_m": float(vstab.chords.tip),
            "span_m": float(vstab.spans.projected),
            "symmetric": bool(vstab.symmetric),
        },
        "fuselage": {
            "width_m": float(fuselage.width),
            "length_m": float(fuselage.lengths.total),
            "side_projected_area_m2": float(fuselage.areas.side_projected),
            "height_max_m": float(fuselage.heights.maximum),
            "height_at_quarter_length_m": float(fuselage.heights.at_quarter_length),
            "height_at_three_quarters_length_m": float(
                fuselage.heights.at_three_quarters_length
            ),
            "height_at_wing_root_quarter_chord_m": float(
                fuselage.heights.at_wing_root_quarter_chord
            ),
        },
    }

    atmo = SUAVE.Analyses.Atmospheric.US_Standard_1976()

    # Spread across the subsonic range this aircraft actually flies, plus a
    # spread of angles of attack (including a negative one) so CM's alpha
    # term is exercised in both directions.
    machs = [0.20, 0.40, 0.60, 0.78, 0.82, 0.85]
    alts_m = [0.0, 3000.0, 6000.0, 10000.0, 11000.0, 11000.0]
    alphas_deg = [0.0, 1.0, 2.0, 2.5, 3.0, -1.0]

    cases = {}
    for i, (mach, alt, alpha_deg) in enumerate(zip(machs, alts_m, alphas_deg)):
        atmo_vals = atmo.compute_values(alt, 0.0)
        velocity = mach * float(atmo_vals.speed_of_sound)
        density = float(atmo_vals.density)
        mu = float(atmo_vals.dynamic_viscosity)
        alpha_rad = math.radians(alpha_deg)

        conditions = Data()
        conditions.freestream = Data()
        conditions.freestream.dynamic_pressure = 0.5 * density * velocity**2
        conditions.freestream.mach_number = np.array([[mach]])
        conditions.freestream.velocity = velocity
        conditions.freestream.density = density
        conditions.freestream.dynamic_viscosity = mu
        conditions.aerodynamics = Data()
        conditions.aerodynamics.angle_of_attack = np.array([[alpha_rad]])
        conditions.weights = Data()
        # Unread by the static branch -- see the module doc's finding that
        # compute_mission_center_of_gravity's numerator is always the origin
        # in this program's SUAVE integration, independent of this value.
        conditions.weights.total_mass = float(vehicle_request["mtow_kg"])

        result = stability(conditions)

        cg_row = np.asarray(vehicle.mass_properties.center_of_gravity)
        cases[f"case_{i}"] = {
            "inputs": {
                "mach": float(mach),
                "alpha_rad": float(alpha_rad),
                "velocity_m_s": float(velocity),
                "density_kg_m3": float(density),
                "dynamic_viscosity_pa_s": float(mu),
                # `Fidelity_Zero.__call__`'s own `cg_x` -- `mass_properties
                # .center_of_gravity[0]`, always the row `[0, 0, 0]` because
                # this program's SUAVE integration never populates it (see
                # the module doc). Recorded as the scalar x-component so a
                # future nonzero CG is a formula this row already carries.
                "cg_x_m": float(cg_row[0][0]),
            },
            "cl_alpha": float(np.asarray(conditions.lift_curve_slope)[0, 0]),
            "cm_alpha": float(np.asarray(result.static.Cm_alpha)[0, 0]),
            "cm0": float(np.asarray(result.static.Cm0)[0, 0]),
            "cm": float(np.asarray(result.static.CM)[0, 0]),
            "cn_beta": float(np.asarray(result.static.Cn_beta).reshape(-1)[0]),
            "static_margin": float(np.asarray(result.static.static_margin).reshape(-1)[0]),
            # `neutral_point` broadcasts `cg_x` (shape (3,), always zero) against
            # a (1,1) array -- see the module doc. Every column of the (1,3)
            # result is numerically identical; column 0 is what this row's
            # `neutral_point` (a plain scalar `cg_x + mac*static_margin`) means.
            "neutral_point": float(np.asarray(result.static.neutral_point).reshape(-1)[0]),
        }

    _framework.write(
        "stab",
        "suave_static",
        {"geometry": geometry, "cases": cases},
        description=(
            "SUAVE.Analyses.Stability.Fidelity_Zero, static branch only, on the "
            "vehicle vehicle_builder.build_vehicle constructs for the default "
            "ALAS design: datcom lift-curve slopes, taw_cmalpha, taw_cnbeta, "
            "the resulting static margin and neutral point, across a spread of "
            "Mach numbers and angles of attack"
        ),
    )
    _SCRATCH.unlink(missing_ok=True)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--stage", choices=["build", "solve"], required=True)
    args = parser.parse_args()

    if args.stage == "build":
        _stage_build()
    else:
        _stage_solve()


if __name__ == "__main__":
    main()
