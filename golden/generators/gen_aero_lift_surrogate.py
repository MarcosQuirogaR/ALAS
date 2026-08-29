# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-aero::lift_surrogate``: the spline the mission actually flies on.

``SUAVE.Analyses.Aerodynamics.Vortex_Lattice`` does not run a vortex lattice
per flight condition. It runs one, once, on a fixed grid of ten angles of
attack against eight Mach numbers, fits a bicubic spline through the result,
and every lift and induced-drag number the mission then sees is an evaluation
of that spline. So the surrogate is not an optimization of the solver -- it is
the model, and a knot placed differently is a different aeroplane.

That is why this fixture records the fitted splines themselves and not only
their values. ``RectBivariateSpline`` with its defaults is Dierckx's ``regrid``
at zero smoothing, and infinitely many bicubic surfaces pass through the same
grid; the knot vectors and coefficients are what pin down which one. The
values are then compared on top of them, including at query points outside the
training rectangle, where FITPACK clamps to the boundary knot and a
"reasonable" cubic extrapolation would diverge without bound.

The generator drives the real objects, as ``gen_aero_vorlax.py`` and
``gen_aero_drag_buildup.py`` do: it builds the vehicle the mission runner
builds, finalizes the ``Fidelity_Zero`` analysis -- which is what calls
``initialize``, which is what calls ``sample_training`` and ``build_surrogate``
-- and then reads the trained object.

It refuses to write a fixture in which the supersonic or transonic surrogates
have become non-``None``. Those select a different branch of
``evaluate_surrogate`` (a three-way Cubic_Spline_Blender between subsonic,
transonic and supersonic surfaces) and this row deliberately translates only
the branch ``Fidelity_Zero``'s subsonic-only training grid reaches.

Usage::

    & ".venv/Scripts/python.exe"       golden/generators/gen_aero_lift_surrogate.py --stage=build
    & ".suave-venv/Scripts/python.exe" golden/generators/gen_aero_lift_surrogate.py --stage=solve
"""

from __future__ import annotations

import argparse
import json

import _framework

_SCRATCH = _framework.GOLDEN_DIR / "aero" / "_lift_surrogate_request.json"

# Where the surrogate is asked for a number. The first block spans the
# training rectangle; the last four are deliberately outside it, because that
# is where the clamp lives and where a port that extrapolated instead would
# part company with the reference by an unbounded amount. A mission segment
# really does reach the low-Mach corner: the takeoff segment climbs out at
# about 70 m/s, which is Mach 0.21, and the surrogate's Mach axis starts at 0.
_QUERIES = [
    # (tag, angle of attack deg, Mach)
    ("grid_point", 2.0, 0.3),
    ("between_knots", 1.0, 0.25),
    ("cruise", 2.5, 0.82),
    ("climb", 4.0, 0.5),
    ("takeoff", 8.0, 0.21),
    ("descent", 0.5, 0.6),
    ("negative_alpha", -3.0, 0.4),
    ("high_alpha", 11.0, 0.15),
    ("post_stall_grid", 45.0, 0.75),
    ("below_mach_floor", 3.0, 0.0),
    ("above_mach_ceiling", 3.0, 0.95),
    ("above_alpha_ceiling", 80.0, 0.5),
    ("below_alpha_floor", -8.0, 0.5),
]


# ======================================================================
#  Stage 1: build the vehicle request (run under .venv)
# ======================================================================


def _stage_build() -> None:
    """Build the vehicle request dict through the ALAS pipeline."""
    _framework.add_alas_to_path()

    from alas.analysis.full_analysis import FullAnalysis  # noqa: PLC0415
    from alas.config.design_variables import DesignVector  # noqa: PLC0415
    from alas.config.settings import ALASConfig  # noqa: PLC0415
    from alas.integration.suave_vehicle import build_vehicle_request  # noqa: PLC0415

    config = ALASConfig()
    report = FullAnalysis(config).run(DesignVector.default(), verbose=False)
    vehicle_request = build_vehicle_request(report, config)

    _SCRATCH.parent.mkdir(parents=True, exist_ok=True)
    with _SCRATCH.open("w", encoding="utf-8", newline="\n") as handle:
        json.dump(vehicle_request, handle, indent=2)
        handle.write("\n")
    print(f"wrote {_SCRATCH.relative_to(_framework.GOLDEN_DIR)}")


# ======================================================================
#  Stage 2: train the surrogate and evaluate it (run under .suave-venv)
# ======================================================================


def _flat(array) -> list:
    import numpy as np  # noqa: PLC0415

    return [float(v) for v in np.asarray(array, dtype=float).reshape(-1)]


def _table(array) -> list:
    """A two-dimensional training table as a list of rows."""
    import numpy as np  # noqa: PLC0415

    return [[float(v) for v in row] for row in np.asarray(array, dtype=float)]


def _spline(surrogate) -> dict:
    """One `RectBivariateSpline`'s knots and coefficients.

    These are what make the interpolation problem well posed. Recording only
    the values would let a surface with different knots agree at the training
    points and disagree everywhere between them.
    """
    knots_x, knots_y = surrogate.get_knots()
    return {
        "knots_x": _flat(knots_x),
        "knots_y": _flat(knots_y),
        "coefficients": _flat(surrogate.get_coeffs()),
    }


def _stage_solve() -> None:
    _framework.add_suave_to_path()

    import numpy as np  # noqa: PLC0415
    import mission_builder  # noqa: PLC0415
    import vehicle_builder  # noqa: PLC0415
    import SUAVE  # noqa: PLC0415
    from SUAVE.Core import Units  # noqa: PLC0415

    if not _SCRATCH.exists():
        raise SystemExit(
            f"{_SCRATCH} not found -- run stage 1 first:\n"
            '  & ".venv/Scripts/python.exe" '
            "golden/generators/gen_aero_lift_surrogate.py --stage=build"
        )

    vehicle_request = json.loads(_SCRATCH.read_text(encoding="utf-8"))

    print("building the SUAVE vehicle...")
    vehicle = vehicle_builder.build_vehicle(vehicle_request)
    configs = mission_builder.configs_setup(vehicle)
    mission_builder.simple_sizing(configs)

    print("finalizing (this is the call that trains the surrogate)...")
    analyses = mission_builder.analyses_setup(configs)
    analyses.finalize()

    aerodynamics = analyses.base.aerodynamics
    vortex_lattice = aerodynamics.process.compute.lift.inviscid_wings
    geometry = aerodynamics.geometry
    training = vortex_lattice.training
    surrogates = vortex_lattice.surrogates

    # --- Refusals ---------------------------------------------------------
    if surrogates.lift_coefficient_sub is None:
        raise SystemExit("the subsonic surrogate was not built; nothing to record")
    for name in (
        "lift_coefficient_sup",
        "lift_coefficient_trans",
        "drag_coefficient_sup",
        "drag_coefficient_trans",
    ):
        if surrogates[name] is not None:
            raise SystemExit(
                f"surrogates.{name} is not None; the supersonic and transonic "
                "branches of evaluate_surrogate are deliberately untranslated "
                "because Fidelity_Zero's training grid is subsonic throughout"
            )
    mach = np.asarray(training.Mach, dtype=float).reshape(-1)
    if (mach >= 1.0).any():
        raise SystemExit("the training grid reaches Mach 1")
    if np.shape(training.lift_coefficient_sup)[1] != 0:
        raise SystemExit("the supersonic training block is not empty")

    wing_tags = list(geometry.wings.keys())

    # --- Evaluate ---------------------------------------------------------
    conditions_module = SUAVE.Analyses.Mission.Segments.Conditions
    state = conditions_module.State()
    state.conditions = conditions_module.Aerodynamics()
    state.conditions.aerodynamics.angle_of_attack = np.atleast_2d(
        [q[1] * Units.deg for q in _QUERIES]
    ).T
    state.conditions.freestream.mach_number = np.atleast_2d([q[2] for q in _QUERIES]).T

    vortex_lattice.evaluate(state, vortex_lattice.settings, geometry)

    aero = state.conditions.aerodynamics
    inviscid_lift = _flat(aero.lift_coefficient)
    inviscid_drag = _flat(aero.drag_breakdown.induced.inviscid)

    # The rest of `Fidelity_Zero`'s lift chain: `vortex` is `Methods.skip`,
    # `fuselage` scales by a constant, and `total` returns what it is handed.
    from SUAVE.Methods.Aerodynamics.Common.Fidelity_Zero.Lift.fuselage_correction import (  # noqa: PLC0415,E501
        fuselage_correction,
    )
    from SUAVE.Methods.Aerodynamics.Common.Fidelity_Zero.Lift.aircraft_total import (  # noqa: PLC0415
        aircraft_total,
    )

    fuselage_correction(state, aerodynamics.settings, geometry)
    aircraft_lift = _flat(aircraft_total(state, aerodynamics.settings, geometry))

    cases = [
        {
            "tag": tag,
            "angle_of_attack_deg": alpha,
            "mach": mach_number,
            "inviscid_lift_coefficient": inviscid_lift[i],
            "inviscid_induced_drag_coefficient": inviscid_drag[i],
            "aircraft_lift_coefficient": aircraft_lift[i],
            "wing_lift_coefficient": {
                tag: _flat(aero.lift_breakdown.inviscid_wings[tag])[i] for tag in wing_tags
            },
            "wing_induced_drag_coefficient": {
                tag: _flat(aero.drag_breakdown.induced.inviscid_wings[tag])[i]
                for tag in wing_tags
            },
        }
        for i, (tag, alpha, mach_number) in enumerate(_QUERIES)
    ]

    payload = {
        "settings": {
            "fuselage_lift_correction": float(
                aerodynamics.settings.fuselage_lift_correction
            ),
            "supersonic_surrogate_is_absent": True,
            "transonic_surrogate_is_absent": True,
        },
        "wing_tags": wing_tags,
        "training": {
            "angle_of_attack_rad": _flat(training.angle_of_attack),
            "mach": _flat(training.Mach),
            "lift_coefficient": _table(training.lift_coefficient_sub),
            "drag_coefficient": _table(training.drag_coefficient_sub),
            "wing_lift_coefficient": {
                tag: _table(training.wing_lift_coefficient_sub[tag]) for tag in wing_tags
            },
            "wing_drag_coefficient": {
                tag: _table(training.wing_drag_coefficient_sub[tag]) for tag in wing_tags
            },
        },
        "surrogates": {
            "lift_coefficient": _spline(surrogates.lift_coefficient_sub),
            "drag_coefficient": _spline(surrogates.drag_coefficient_sub),
            "wing_lift_coefficient": {
                tag: _spline(surrogates.wing_lift_coefficient_sub[tag]) for tag in wing_tags
            },
            "wing_drag_coefficient": {
                tag: _spline(surrogates.wing_drag_coefficient_sub[tag]) for tag in wing_tags
            },
        },
        "cases": cases,
    }

    _framework.write(
        "aero",
        "lift_surrogate",
        payload,
        description=(
            "SUAVE's Vortex_Lattice lift and induced-drag surrogate on the "
            "default ALAS aircraft: the ten-by-eight training tables, the "
            "eight fitted bicubic surfaces with their knots and "
            "coefficients, and thirteen evaluations including four outside "
            "the training rectangle."
        ),
    )

    _SCRATCH.unlink(missing_ok=True)
    print("done")


# ======================================================================


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate the lift surrogate fixture")
    parser.add_argument("--stage", choices=["build", "solve"], required=True)
    args = parser.parse_args()

    if args.stage == "build":
        _stage_build()
    else:
        _stage_solve()


if __name__ == "__main__":
    main()
