# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-aero::drag_buildup``: SUAVE's ``Fidelity_Zero`` drag buildup.

The buildup is not reachable as a free function. It is the ``compute.drag``
half of ``SUAVE.Analyses.Aerodynamics.Fidelity_Zero()``'s process chain, and
the only thing in the whole reference that builds one is the mission runner's
``mission_builder.py:85-87``, which attaches it to an already-assembled
vehicle and never touches its ``settings``. So this generator drives the real
analysis object rather than a hand-assembled stand-in: it builds the vehicle
the way the runner does, finalizes it (which trains the vortex-lattice lift
surrogate), and calls ``aerodynamics.evaluate(state)`` on a spread of flight
conditions.

That matters for more than tidiness. A previous version of this generator
hand-built a ``settings`` object and set ``wing_parasite_drag_form_factor``,
``fuselage_parasite_drag_form_factor`` and ``span_efficiency`` to values that
are *not* ``Fidelity_Zero``'s defaults, so the fixture recorded a
configuration the mission never runs. Reading the settings off the analysis
makes that class of mistake impossible.

What the fixture records as *inputs* is the whole of what the correlations
read: the freestream state SUAVE's own ``US_Standard_1976`` produced, and the
per-wing lift and inviscid induced drag the vortex-lattice surrogate supplied.
The second of those is ``alas-aero::lift_surrogate``'s output, taken here as
data -- the same arrangement ``alas-mass::suave_transport`` uses for
``sealevel_static_thrust``. Without it there is nothing to compare: with
``span_efficiency`` at its ``None`` default the inviscid induced drag *is* the
vortex lattice's, not a closed form.

Usage::

    & ".venv/Scripts/python.exe"       golden/generators/gen_aero_drag_buildup.py --stage=build
    & ".suave-venv/Scripts/python.exe" golden/generators/gen_aero_drag_buildup.py --stage=solve
"""

from __future__ import annotations

import argparse
import json

import _framework

# What stage 1 writes and stage 2 reads. The two stages need different
# interpreters, so the vehicle request has to survive a process boundary.
_SCRATCH = _framework.GOLDEN_DIR / "aero" / "_drag_requests.json"

# The flight conditions the fixture is evaluated at: subsonic climb-out
# through the transonic cruise point, so that the compressibility term goes
# from numerically zero to the dominant one, and so that the fuselage's
# `Mc < 0.95` / `Mc >= 0.95` branch and the wing form factor's cubic-spline
# blend (which only moves between Mach 0.95 and 1.0) are both reached.
_CASES = [
    # (tag, altitude m, Mach, angle of attack deg)
    ("sea_level_low_speed", 0.0, 0.20, 4.0),
    ("climb_low", 3000.0, 0.40, 3.0),
    ("climb_mid", 6000.0, 0.60, 2.5),
    ("climb_high", 9000.0, 0.70, 2.5),
    ("cruise_light", 11000.0, 0.80, 1.5),
    ("cruise_nominal", 11000.0, 0.82, 2.5),
    ("cruise_heavy", 11000.0, 0.85, 3.5),
    ("transonic", 11000.0, 0.90, 2.5),
    ("blend_band", 11000.0, 0.97, 2.5),
    ("sonic", 11000.0, 1.00, 2.5),
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
#  Stage 2: evaluate the drag buildup (run under .suave-venv)
# ======================================================================


def _scalar(value) -> float:
    """Read one number out of whatever SUAVE handed back.

    Every quantity here is either a bare float or a one-row, one-column
    array, because each case is evaluated on a single-row state.
    """
    import numpy as np  # noqa: PLC0415

    array = np.asarray(value, dtype=float)
    return float(array.reshape(-1)[0])


def _build_state(altitude_m: float, mach: float, alpha_deg: float):
    """One single-row aerodynamic state at a flight condition.

    The freestream comes from the same ``US_Standard_1976`` analysis
    ``mission_builder.py:100`` attaches to the mission, so the Reynolds
    number the correlations see is the one a real segment would see.
    """
    import numpy as np  # noqa: PLC0415
    import SUAVE  # noqa: PLC0415
    from SUAVE.Core import Units  # noqa: PLC0415

    conditions_module = SUAVE.Analyses.Mission.Segments.Conditions
    state = conditions_module.State()
    state.conditions = conditions_module.Aerodynamics()

    atmosphere = SUAVE.Analyses.Atmospheric.US_Standard_1976()
    values = atmosphere.compute_values(altitude_m, 0.0)

    velocity = mach * values.speed_of_sound
    # `re` is a Reynolds number *per metre*: every consumer multiplies it by
    # its own reference length (`print_parasite_drag.py` names it the same way).
    reynolds = values.density * velocity / values.dynamic_viscosity

    freestream = state.conditions.freestream
    freestream.altitude = np.atleast_2d(altitude_m)
    freestream.mach_number = np.atleast_2d(mach)
    freestream.temperature = values.temperature
    freestream.pressure = values.pressure
    freestream.density = values.density
    freestream.speed_of_sound = values.speed_of_sound
    freestream.dynamic_viscosity = values.dynamic_viscosity
    freestream.velocity = velocity
    freestream.reynolds_number = reynolds
    freestream.dynamic_pressure = 0.5 * values.density * velocity**2

    state.conditions.aerodynamics.angle_of_attack = np.atleast_2d(alpha_deg * Units.deg)

    return state


def _stage_solve() -> None:
    """Run the real ``Fidelity_Zero`` drag chain and record it."""
    _framework.add_suave_to_path()

    import mission_builder  # noqa: PLC0415
    import vehicle_builder  # noqa: PLC0415

    if not _SCRATCH.exists():
        raise SystemExit(
            f"{_SCRATCH} not found -- run stage 1 first:\n"
            '  & ".venv/Scripts/python.exe" '
            "golden/generators/gen_aero_drag_buildup.py --stage=build"
        )

    vehicle_request = json.loads(_SCRATCH.read_text(encoding="utf-8"))

    print("building the SUAVE vehicle...")
    vehicle = vehicle_builder.build_vehicle(vehicle_request)
    configs = mission_builder.configs_setup(vehicle)
    mission_builder.simple_sizing(configs)

    print("finalizing (training the vortex-lattice surrogate)...")
    analyses = mission_builder.analyses_setup(configs)
    analyses.finalize()

    aerodynamics = analyses.base.aerodynamics
    geometry = aerodynamics.geometry
    settings = aerodynamics.settings

    # Read the settings off the analysis rather than asserting them here: the
    # point of driving the real object is that these are whatever
    # `Fidelity_Zero.__defaults__` says, and `mission_builder.py` overrides
    # none of them. `None` is a real value for the two efficiency factors and
    # selects a different branch of `induced_drag_aircraft`, so it is recorded
    # as `null` rather than coerced.
    recorded_settings = {
        "wing_parasite_drag_form_factor": float(settings.wing_parasite_drag_form_factor),
        "fuselage_parasite_drag_form_factor": float(
            settings.fuselage_parasite_drag_form_factor
        ),
        "viscous_lift_dependent_drag_factor": float(
            settings.viscous_lift_dependent_drag_factor
        ),
        "trim_drag_correction_factor": float(settings.trim_drag_correction_factor),
        "drag_coefficient_increment": float(settings.drag_coefficient_increment),
        "spoiler_drag_increment": float(settings.spoiler_drag_increment),
        "lift_to_drag_adjustment": float(settings.lift_to_drag_adjustment),
        "recalculate_total_wetted_area": bool(settings.recalculate_total_wetted_area),
        "oswald_efficiency_factor": (
            None
            if settings.oswald_efficiency_factor is None
            else float(settings.oswald_efficiency_factor)
        ),
        "span_efficiency": (
            None if settings.span_efficiency is None else float(settings.span_efficiency)
        ),
    }

    cases = []
    for tag, altitude_m, mach, alpha_deg in _CASES:
        print(f"  evaluating {tag}: M={mach} at {altitude_m:.0f} m")
        state = _build_state(altitude_m, mach, alpha_deg)
        aerodynamics.evaluate(state)

        conditions = state.conditions
        freestream = conditions.freestream
        aero = conditions.aerodynamics
        breakdown = aero.drag_breakdown

        cases.append(
            {
                "tag": tag,
                "altitude_m": altitude_m,
                "mach": mach,
                "angle_of_attack_deg": alpha_deg,
                # Inputs: the flow state, recorded rather than re-derived. A
                # parity test that recomputed it would be comparing two
                # atmospheres and calling the difference a drag disagreement.
                "freestream": {
                    "temperature_k": _scalar(freestream.temperature),
                    "density_kg_m3": _scalar(freestream.density),
                    "speed_of_sound_m_s": _scalar(freestream.speed_of_sound),
                    "dynamic_viscosity_pa_s": _scalar(freestream.dynamic_viscosity),
                    "velocity_m_s": _scalar(freestream.velocity),
                    "reynolds_number_per_m": _scalar(freestream.reynolds_number),
                },
                # Inputs: what the vortex-lattice surrogate supplied.
                "lift": {
                    "total": _scalar(aero.lift_coefficient),
                    "inviscid_wings": {
                        wing: _scalar(value)
                        for wing, value in aero.lift_breakdown.inviscid_wings.items()
                    },
                    "compressible_wings": {
                        wing: _scalar(value)
                        for wing, value in aero.lift_breakdown.compressible_wings.items()
                    },
                    "inviscid_induced_wings": {
                        wing: _scalar(value)
                        for wing, value in breakdown.induced.inviscid_wings.items()
                    },
                },
                # Outputs: every stage, so a wrong component prints as its own
                # line rather than hiding inside the total.
                "parasite": {
                    "components": {
                        component: _scalar(result.parasite_drag_coefficient)
                        for component, result in breakdown.parasite.items()
                        if component != "total"
                    },
                    "skin_friction": {
                        component: _scalar(result.skin_friction_coefficient)
                        for component, result in breakdown.parasite.items()
                        if component != "total"
                    },
                    "form_factor": {
                        component: _scalar(result.form_factor)
                        for component, result in breakdown.parasite.items()
                        if component != "total"
                    },
                    "compressibility_factor": {
                        component: _scalar(result.compressibility_factor)
                        for component, result in breakdown.parasite.items()
                        if component != "total"
                    },
                    "reynolds_factor": {
                        component: _scalar(result.reynolds_factor)
                        for component, result in breakdown.parasite.items()
                        if component != "total"
                    },
                    "total": _scalar(breakdown.parasite.total),
                },
                "induced": {
                    "total": _scalar(breakdown.induced.total),
                    "viscous": _scalar(breakdown.induced.viscous),
                    "viscous_wings": {
                        wing: _scalar(value)
                        for wing, value in breakdown.induced.viscous_wings_drag.items()
                    },
                },
                "compressible": {
                    "wings": {
                        wing: _scalar(result.compressibility_drag)
                        for wing, result in breakdown.compressible.items()
                        if wing != "total"
                    },
                    "crest_critical": {
                        wing: _scalar(result.crest_critical)
                        for wing, result in breakdown.compressible.items()
                        if wing != "total"
                    },
                    "divergence_mach": {
                        wing: _scalar(result.divergence_mach)
                        for wing, result in breakdown.compressible.items()
                        if wing != "total"
                    },
                    "total": _scalar(breakdown.compressible.total),
                },
                "miscellaneous": {
                    "total_wetted_area_m2": _scalar(
                        breakdown.miscellaneous.total_wetted_area
                    ),
                    "total": _scalar(breakdown.miscellaneous.total),
                },
                "untrimmed": _scalar(breakdown.untrimmed),
                "trim_corrected": _scalar(breakdown.trim_corrected_drag),
                "spoiler": _scalar(breakdown.spoiler_drag),
                "total": _scalar(breakdown.total),
            }
        )

    geometry_data = {
        "reference_area_m2": float(geometry.reference_area),
        "wings": [
            {
                "tag": wing.tag,
                "mean_aerodynamic_chord_m": float(wing.chords.mean_aerodynamic),
                "quarter_chord_sweep_rad": float(wing.sweeps.quarter_chord),
                "thickness_to_chord": float(wing.thickness_to_chord),
                "reference_area_m2": float(wing.areas.reference),
                "wetted_area_m2": float(wing.areas.wetted),
                "transition_x_upper": float(wing.transition_x_upper),
                "transition_x_lower": float(wing.transition_x_lower),
                "aspect_ratio": float(wing.aspect_ratio),
                "segment_count": len(wing.Segments.keys()),
            }
            for wing in geometry.wings.values()
        ],
        "fuselages": [
            {
                "tag": fuselage.tag,
                "length_m": float(fuselage.lengths.total),
                "effective_diameter_m": float(fuselage.effective_diameter),
                "front_projected_area_m2": float(fuselage.areas.front_projected),
                "wetted_area_m2": float(fuselage.areas.wetted),
            }
            for fuselage in geometry.fuselages.values()
        ],
        "nacelles": [
            {
                "tag": nacelle.tag,
                "length_m": float(nacelle.length),
                "diameter_m": float(nacelle.diameter),
                "wetted_area_m2": float(nacelle.areas.wetted),
                "origin_count": len(nacelle.origin),
            }
            for nacelle in geometry.nacelles.values()
        ],
        "network_count": len(geometry.networks),
    }

    _check(cases, geometry_data, recorded_settings)

    _framework.write(
        "aero",
        "drag_buildup",
        {"settings": recorded_settings, "geometry": geometry_data, "cases": cases},
        description=(
            "SUAVE Fidelity_Zero drag buildup on the default AVE vehicle: "
            "parasite (wings, fuselage, nacelles, pylon), induced, "
            "compressibility, miscellaneous, and the untrimmed/trim/total "
            "chain, with the freestream and the vortex-lattice lift solution "
            "recorded as inputs"
        ),
    )

    _SCRATCH.unlink(missing_ok=True)
    print("done")


def _check(cases, geometry_data, settings) -> None:
    """Refuse to write a fixture that does not reach what it claims to.

    Each of these is a branch the port has to take and that a fixture can
    silently fail to exercise, leaving a wrong translation passing.
    """
    if settings["span_efficiency"] is not None:
        raise SystemExit(
            "span_efficiency is not None; the fixture would record the "
            "closed-form inviscid induced drag rather than the vortex "
            "lattice's, which is not what the mission runs"
        )
    if settings["oswald_efficiency_factor"] is not None:
        raise SystemExit("oswald_efficiency_factor is not None; unreached branch")

    if any(wing["segment_count"] for wing in geometry_data["wings"]):
        raise SystemExit(
            "a wing carries Segments; the port translates only the "
            "unsegmented branch of parasite_drag_wing"
        )
    if not geometry_data["nacelles"]:
        raise SystemExit("no nacelles; the pylon and nacelle terms are unreached")

    if not any(case["mach"] < 0.95 for case in cases):
        raise SystemExit("no case below Mach 0.95; the fuselage's subsonic branch is unreached")
    if not any(case["mach"] >= 0.95 for case in cases):
        raise SystemExit("no case at or above Mach 0.95; the fuselage's sonic branch is unreached")
    if not any(0.95 < case["mach"] < 1.0 for case in cases):
        raise SystemExit(
            "no case strictly inside 0.95 < M < 1.0; the wing form factor's "
            "cubic-spline blend is never partially applied, so a port that "
            "got the blend wrong would still agree everywhere"
        )

    for case in cases:
        if "pylon" not in case["parasite"]["components"]:
            raise SystemExit(f"case {case['tag']} has no pylon parasite entry")
        if case["total"] <= 0.0:
            raise SystemExit(f"case {case['tag']} has a non-positive total drag")

    # The trim correction is the one factor applied to the whole buildup, so a
    # fixture in which it happened to be 1.0 would not distinguish a port that
    # dropped it.
    if settings["trim_drag_correction_factor"] == 1.0:
        raise SystemExit("trim_drag_correction_factor is 1.0; dropping trim would be invisible")


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate the drag buildup fixture")
    parser.add_argument(
        "--stage",
        choices=["build", "solve"],
        required=True,
        help="'build' under .venv, 'solve' under .suave-venv",
    )
    args = parser.parse_args()

    if args.stage == "build":
        _stage_build()
    else:
        _stage_solve()


if __name__ == "__main__":
    main()
