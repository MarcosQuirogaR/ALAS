# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-mission::segments`` and ``::solve``: the SUAVE mission network.

This generator writes **two** fixtures from one SUAVE run:

``golden/mission/mission.json``
    The end-to-end solve. Every segment, every control point, every column
    ``export_data.py`` writes, plus the resolved vehicle and the segment
    schedule the mission was built from -- everything a second implementation
    needs to fly the same aeroplane down the same profile.

``golden/mission/mission_segments.json``
    One *iteration* of three representative segments at a **fixed** unknown
    vector, with every update method's output recorded separately. The
    end-to-end fixture cannot localize a fault: a wrong residual there
    converges to a different throttle and every downstream column moves at
    once. This one holds the unknowns still, so a wrong orientation, a wrong
    freestream and a wrong thrust each print as their own line.

They are one generator rather than two because they must describe the same
aircraft. The vehicle is the expensive part (an ALAS analysis, then a
vortex-lattice surrogate over eighty conditions), and two generators would
mean two chances for the two fixtures to disagree about what was flown.

Two stages, because the mission needs two incompatible Python environments:

Stage 1 (``.venv``, the AeroSandbox environment)
    Runs the ALAS pipeline and builds the vehicle and mission request dicts,
    exactly as ``build_vehicle_request`` and ``build_mission_request`` do
    during a real analysis run.

Stage 2 (``.suave-venv``, the SUAVE 2.5.2 environment)
    Builds the SUAVE vehicle from those requests, configures and solves the
    mission, and records both fixtures.

Usage::

    & ".venv/Scripts/python.exe"       golden/generators/gen_mission.py --stage=build
    & ".suave-venv/Scripts/python.exe" golden/generators/gen_mission.py --stage=solve

The chain exercised is the whole of it: the VORLAX lift surrogate, the
``Fidelity_Zero`` drag buildup, the turbofan network's per-timestep
``evaluate_thrust``, the transport weight analysis and the MINPACK-backed
segment solver.
"""

from __future__ import annotations

import argparse
import json
import math

import _framework

# The scratch file that stage 1 writes and stage 2 reads.
_SCRATCH = _framework.GOLDEN_DIR / "mission" / "_requests.json"

# Which segments the per-method fixture holds still, and at what unknowns.
#
# One of each kind the mission builds -- a climb, a cruise and a descent --
# because the three differ in what their `initialize` computes and in which
# residual they form, and in nothing else. The unknown vectors are
# deliberately *not* the solved ones: a segment evaluated at its own root has
# residuals near zero, where a relative comparison says nothing, and every
# force term is in balance, which is exactly the state in which two different
# force models agree. These are displaced far enough to leave the residual a
# real number.
_HELD_SEGMENTS = (
    ("initial_climb", 0.72, 4.0),
    ("cruise_step_2", 0.45, 1.5),
    ("descent_2", 0.28, -1.0),
)


# ======================================================================
#  Stage 1: build requests (run under .venv)
# ======================================================================


def _stage_build() -> None:
    """Build vehicle + mission request dicts using the ALAS pipeline."""
    _framework.add_alas_to_path()

    from alas.analysis.full_analysis import FullAnalysis  # noqa: PLC0415
    from alas.config.airports import get_airport  # noqa: PLC0415
    from alas.config.design_variables import DesignVector  # noqa: PLC0415
    from alas.config.settings import ALASConfig  # noqa: PLC0415
    from alas.integration.suave_mission import build_mission_request  # noqa: PLC0415
    from alas.integration.suave_vehicle import build_vehicle_request  # noqa: PLC0415

    config = ALASConfig()

    # Run the analysis to get a report -- the same object build_vehicle_request
    # reads in the real pipeline.
    print("Running ALAS analysis pipeline...")
    analysis = FullAnalysis(config)
    report = analysis.run(DesignVector.default(), verbose=False)

    # The default route (LEMD -> HKJK, Madrid to Nairobi)
    origin = get_airport("LEMD")
    dest = get_airport("HKJK")
    route_distance_m = 6_500_000.0

    print("Building vehicle request...")
    vehicle_request = build_vehicle_request(report, config)

    print("Building mission request...")
    mission_request = build_mission_request(config, origin, dest, route_distance_m)

    _SCRATCH.parent.mkdir(parents=True, exist_ok=True)
    with _SCRATCH.open("w", encoding="utf-8", newline="\n") as handle:
        json.dump(
            {"vehicle": vehicle_request, "mission": mission_request},
            handle,
            indent=2,
        )
        handle.write("\n")
    print(f"wrote {_SCRATCH.relative_to(_framework.GOLDEN_DIR)}")


# ======================================================================
#  Stage 2 helpers
# ======================================================================


def _col(array, column: int = 0) -> list[float]:
    """One column of a SUAVE conditions array, as a list of floats."""
    import numpy as np  # noqa: PLC0415

    return [float(value) for value in np.asarray(array, dtype=float)[:, column]]


def _resolve_vehicle(analyses, configs) -> dict:
    """The narrow inputs a second implementation needs to fly this aeroplane.

    Read off the *built* analysis objects after ``simple_sizing`` and
    ``finalize`` have run, never off the request that produced them: the
    wetted areas are ``simple_sizing``'s, the engine's core-flow scale factor
    is ``turbofan_sizing``'s, and the settings are ``Fidelity_Zero``'s
    defaults. Recording the request instead would describe an aircraft
    adjacent to the one that flew.
    """
    aerodynamics = analyses.base.aerodynamics
    geometry = aerodynamics.geometry
    settings = aerodynamics.settings
    weights = analyses.base.weights
    turbofan = geometry.networks.turbofan
    thrust = turbofan.thrust

    return {
        "reference_area_m2": float(geometry.reference_area),
        # `np.inf` has no JSON spelling, and it is a real value: the clamp
        # `update_aerodynamics` applies is unreachable at it. Recorded as
        # null, which the port reads as "no clamp"; the check below refuses a
        # fixture in which it has become a number.
        "maximum_lift_coefficient": (
            None
            if math.isinf(float(settings.maximum_lift_coefficient))
            else float(settings.maximum_lift_coefficient)
        ),
        "takeoff_mass_kg": float(weights.vehicle.mass_properties.takeoff),
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
        "drag_settings": {
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
            "oswald_efficiency_factor": (
                None
                if settings.oswald_efficiency_factor is None
                else float(settings.oswald_efficiency_factor)
            ),
            "span_efficiency": (
                None
                if settings.span_efficiency is None
                else float(settings.span_efficiency)
            ),
        },
        "fuselage_lift_correction": float(settings.fuselage_lift_correction),
        # The engine, read off the assembled network. `evaluate_thrust` needs
        # the variable inputs, every fixed component efficiency (so the port's
        # own constants can be checked against them at `exact` before a single
        # thrust is compared) and the core-flow scale factor `turbofan_sizing`
        # solved -- the engine is already sized by the time a mission flies it.
        "turbofan": {
            "number_of_engines": float(turbofan.number_of_engines),
            "bypass_ratio": float(turbofan.bypass_ratio),
            "fan_pressure_ratio": float(turbofan.fan.pressure_ratio),
            "turbine_inlet_temperature_k": float(
                turbofan.combustor.turbine_inlet_temperature
            ),
            "lpc_pressure_ratio": float(turbofan.low_pressure_compressor.pressure_ratio),
            "hpc_pressure_ratio": float(turbofan.high_pressure_compressor.pressure_ratio),
            "inlet_pressure_ratio": float(turbofan.inlet_nozzle.pressure_ratio),
            "inlet_polytropic_efficiency": float(
                turbofan.inlet_nozzle.polytropic_efficiency
            ),
            "inlet_pressure_recovery": float(turbofan.inlet_nozzle.pressure_recovery),
            "lpc_polytropic_efficiency": float(
                turbofan.low_pressure_compressor.polytropic_efficiency
            ),
            "hpc_polytropic_efficiency": float(
                turbofan.high_pressure_compressor.polytropic_efficiency
            ),
            "fan_polytropic_efficiency": float(turbofan.fan.polytropic_efficiency),
            "combustor_pressure_ratio": float(turbofan.combustor.pressure_ratio),
            "combustor_efficiency": float(turbofan.combustor.efficiency),
            "turbine_mechanical_efficiency": float(
                turbofan.low_pressure_turbine.mechanical_efficiency
            ),
            "turbine_polytropic_efficiency": float(
                turbofan.low_pressure_turbine.polytropic_efficiency
            ),
            "core_nozzle_pressure_ratio": float(turbofan.core_nozzle.pressure_ratio),
            "core_nozzle_polytropic_efficiency": float(
                turbofan.core_nozzle.polytropic_efficiency
            ),
            "fan_nozzle_pressure_ratio": float(turbofan.fan_nozzle.pressure_ratio),
            "fan_nozzle_polytropic_efficiency": float(
                turbofan.fan_nozzle.polytropic_efficiency
            ),
            "design_thrust_total_n": float(thrust.total_design),
            "compressor_nondimensional_massflow": float(
                thrust.compressor_nondimensional_massflow
            ),
            "sfc_adjustment": float(thrust.SFC_adjustment),
            "reference_temperature_k": float(thrust.reference_temperature),
            "reference_pressure_pa": float(thrust.reference_pressure),
        },
        # Which configuration each segment flies is recorded so a reader can
        # confirm the claim the ledger makes: the six differ only in high-lift
        # deflections, which this aerodynamic model does not discretize, so
        # all six evaluate identically and the port builds one.
        "config_tags": list(configs.keys()),
    }


def _resolve_surrogate(analyses) -> dict:
    """The trained vortex-lattice tables, as `build_surrogate` received them.

    Handed to the port as data rather than resampled. `alas-aero::vorlax`'s
    agreement is its own row's claim, checked there; re-running the panel
    method here would make every mission column a function of it as well, and
    a disagreement in the kernel would surface as a disagreement in the
    mission -- the mistake `alas-aero::drag_buildup`'s row is a record of.
    """
    vlm = analyses.base.aerodynamics.process.compute.lift.inviscid_wings
    training = vlm.training
    return {
        "angle_of_attack_rad": [float(a) for a in training.angle_of_attack.flatten()],
        "mach": [float(m) for m in training.Mach.flatten()],
        # The order `evaluate_surrogate` iterates the wings in, which is the
        # order the per-wing lift solution reaches the drag buildup in.
        "wing_tags": list(vlm.geometry.wings.keys()),
        "lift_coefficient": [
            [float(v) for v in row] for row in training.lift_coefficient_sub
        ],
        "drag_coefficient": [
            [float(v) for v in row] for row in training.drag_coefficient_sub
        ],
        "wing_lift_coefficient": {
            tag: [[float(v) for v in row] for row in table]
            for tag, table in training.wing_lift_coefficient_sub.items()
        },
        "wing_drag_coefficient": {
            tag: [[float(v) for v in row] for row in table]
            for tag, table in training.wing_drag_coefficient_sub.items()
        },
        # Read off the built surrogates, not off the training arrays: this is
        # the exact quantity `evaluate_surrogate` branches on
        # (`if CL_surrogate_sup == None`), and it is what makes the whole
        # transonic blend unreachable. The training array is not `None` when
        # the surrogate is -- it is a (10, 0) slice -- so testing the array
        # would ask a different question.
        "supersonic_surrogate_is_absent": vlm.surrogates.lift_coefficient_sup is None,
        "transonic_surrogate_is_absent": vlm.surrogates.lift_coefficient_trans is None,
    }


def _segment_spec(segment) -> dict:
    """The schedule one segment was built with, as `mission_setup` set it."""
    keys = segment.keys()
    if "climb_rate" in keys:
        kind = "climb"
    elif "descent_rate" in keys:
        kind = "descent"
    else:
        kind = "cruise"

    spec = {
        "tag": segment.tag,
        "kind": kind,
        "air_speed_m_s": float(segment.air_speed),
        "true_course_rad": float(segment.true_course),
        "temperature_deviation_k": float(segment.temperature_deviation),
        "number_control_points": int(segment.state.numerics.number_control_points),
        "tolerance_solution": float(segment.state.numerics.tolerance_solution),
        # `max_evaluations` of 0 and an unset `step_size` are what SciPy
        # substitutes its own defaults for; the port reads them the same way.
        "max_evaluations": float(segment.state.numerics.max_evaluations),
        "step_size": (
            None
            if segment.state.numerics.step_size is None
            else float(segment.state.numerics.step_size)
        ),
    }
    if kind == "cruise":
        spec["altitude_m"] = None if segment.altitude is None else float(segment.altitude)
        spec["distance_m"] = float(segment.distance)
    else:
        spec["altitude_start_m"] = (
            None if segment.altitude_start is None else float(segment.altitude_start)
        )
        spec["altitude_end_m"] = float(segment.altitude_end)
        spec["rate_m_s"] = float(
            segment.climb_rate if kind == "climb" else segment.descent_rate
        )
    return spec


def _record_conditions(segment) -> dict:
    """Every array the iterate chain writes, one entry per update method.

    Grouped by the method that produced it, so a failure names the step that
    diverged rather than the column it showed up in. The transformation
    tensors are recorded flattened row-major per control point: they are what
    `update_forces` rotates through, and a wrong rotation order produces a
    plausible force with a wrong tensor behind it.
    """
    conditions = segment.state.conditions
    frames = conditions.frames
    freestream = conditions.freestream
    aerodynamics = conditions.aerodynamics
    breakdown = aerodynamics.drag_breakdown

    def _tensor(value) -> list[list[float]]:
        import numpy as np  # noqa: PLC0415

        array = np.asarray(value, dtype=float)
        return [[float(v) for v in matrix.reshape(-1)] for matrix in array]

    return {
        # initialize_conditions / update_differentials_altitude, and the
        # initialize_time shift applied on top of them.
        "time_s": _col(frames.inertial.time),
        "position_vector_x_m": _col(frames.inertial.position_vector, 0),
        "position_vector_y_m": _col(frames.inertial.position_vector, 1),
        "position_vector_z_m": _col(frames.inertial.position_vector, 2),
        "velocity_vector_x_m_s": _col(frames.inertial.velocity_vector, 0),
        "velocity_vector_z_m_s": _col(frames.inertial.velocity_vector, 2),
        "aircraft_range_m": _col(frames.inertial.aircraft_range),
        # update_acceleration (climb and descent only; cruise leaves it zero)
        "acceleration_vector_x_m_s2": _col(frames.inertial.acceleration_vector, 0),
        "acceleration_vector_z_m_s2": _col(frames.inertial.acceleration_vector, 2),
        # update_altitude / update_atmosphere / update_gravity
        "altitude_m": _col(freestream.altitude),
        "pressure_pa": _col(freestream.pressure),
        "temperature_k": _col(freestream.temperature),
        "density_kg_m3": _col(freestream.density),
        "speed_of_sound_m_s": _col(freestream.speed_of_sound),
        "dynamic_viscosity_pa_s": _col(freestream.dynamic_viscosity),
        "gravity_m_s2": _col(freestream.gravity),
        # update_freestream
        "velocity_m_s": _col(freestream.velocity),
        "mach": _col(freestream.mach_number),
        "reynolds_number_per_m": _col(freestream.reynolds_number),
        "dynamic_pressure_pa": _col(freestream.dynamic_pressure),
        # update_orientations
        "body_angle_rad": _col(frames.body.inertial_rotations, 1),
        "angle_of_attack_rad": _col(aerodynamics.angle_of_attack),
        "side_slip_angle_rad": _col(aerodynamics.side_slip_angle),
        "transform_body_to_inertial": _tensor(frames.body.transform_to_inertial),
        "transform_wind_to_inertial": _tensor(frames.wind.transform_to_inertial),
        # update_thrust
        "throttle": _col(conditions.propulsion.throttle),
        "thrust_force_x_n": _col(frames.body.thrust_force_vector, 0),
        "vehicle_mass_rate_kg_s": _col(conditions.weights.vehicle_mass_rate),
        # update_aerodynamics
        "lift_coefficient": _col(aerodynamics.lift_coefficient),
        "drag_coefficient": _col(aerodynamics.drag_coefficient),
        "lift_force_z_n": _col(frames.wind.lift_force_vector, 2),
        "drag_force_x_n": _col(frames.wind.drag_force_vector, 0),
        "drag_parasite": _col(breakdown.parasite.total),
        "drag_induced": _col(breakdown.induced.total),
        "drag_compressible": _col(breakdown.compressible.total),
        "drag_miscellaneous": _col(breakdown.miscellaneous.total),
        "drag_untrimmed": _col(breakdown.untrimmed),
        # update_weights
        "total_mass_kg": _col(conditions.weights.total_mass),
        "gravity_force_z_n": _col(frames.inertial.gravity_force_vector, 2),
        # update_forces
        "total_force_x_n": _col(frames.inertial.total_force_vector, 0),
        "total_force_y_n": _col(frames.inertial.total_force_vector, 1),
        "total_force_z_n": _col(frames.inertial.total_force_vector, 2),
        # update_planet_position
        "latitude_deg": _col(frames.planet.latitude),
        "longitude_deg": _col(frames.planet.longitude),
        # residual_total_forces
        "residual_horizontal": _col(segment.state.residuals.forces, 0),
        "residual_vertical": _col(segment.state.residuals.forces, 1),
    }


def _record_initials(segment) -> dict | None:
    """What this segment inherits from the one before it.

    A segment is not a closed problem: `initialize_time`,
    `initialize_weights`, `initialize_inertial_position` and
    `initialize_planet_position` each read the *last row* of the previous
    segment's converged state. Recording those four values is what makes one
    segment reproducible without re-solving the eleven before it.
    """
    if not segment.state.initials:
        return None
    conditions = segment.state.initials.conditions
    return {
        "time_s": float(conditions.frames.inertial.time[-1, 0]),
        "total_mass_kg": float(conditions.weights.total_mass[-1, 0]),
        "position_vector_x_m": float(conditions.frames.inertial.position_vector[-1, 0]),
        "position_vector_y_m": float(conditions.frames.inertial.position_vector[-1, 1]),
        "position_vector_z_m": float(conditions.frames.inertial.position_vector[-1, 2]),
        "aircraft_range_m": float(conditions.frames.inertial.aircraft_range[-1, 0]),
        "latitude_deg": float(conditions.frames.planet.latitude[-1, 0]),
        "longitude_deg": float(conditions.frames.planet.longitude[-1, 0]),
    }


# ======================================================================
#  Stage 2: run the SUAVE mission (run under .suave-venv)
# ======================================================================


def _stage_solve() -> None:
    """Run the SUAVE mission and write both fixtures."""
    _framework.add_suave_to_path()

    import numpy as np  # noqa: PLC0415
    import SUAVE  # noqa: PLC0415
    import mission_builder  # noqa: PLC0415
    import vehicle_builder  # noqa: PLC0415
    from SUAVE.Core import Units  # noqa: PLC0415

    if not _SCRATCH.exists():
        raise SystemExit(
            f"{_SCRATCH} not found -- run stage 1 first:\n"
            '  & ".venv/Scripts/python.exe" golden/generators/gen_mission.py --stage=build'
        )

    request = json.loads(_SCRATCH.read_text(encoding="utf-8"))
    vehicle_request = request["vehicle"]
    mission_request = request["mission"]

    print("Building SUAVE vehicle...")
    vehicle = vehicle_builder.build_vehicle(vehicle_request)

    print("Setting up configs...")
    configs = mission_builder.configs_setup(vehicle)
    mission_builder.simple_sizing(configs)

    print("Finalizing configs (building the VLM surrogate -- this takes a moment)...")
    configs.finalize()

    print("Setting up analyses...")
    configs_analyses = mission_builder.analyses_setup(configs)
    mission = mission_builder.mission_setup(configs_analyses, mission_request)
    missions_analyses = mission_builder.missions_setup(mission)

    analyses = SUAVE.Analyses.Analysis.Container()
    analyses.configs = configs_analyses
    analyses.missions = missions_analyses
    analyses.finalize()

    print("Evaluating weights...")
    analyses.configs.base.weights.evaluate()

    print("Evaluating mission (segment convergence)...")
    results = analyses.missions.base.evaluate()

    resolved_vehicle = _resolve_vehicle(configs_analyses, configs)
    surrogate = _resolve_surrogate(configs_analyses)

    # --- the end-to-end fixture -----------------------------------------
    _RHO_SL = 1.225
    _G0 = 9.80665
    segments_data = []
    t_offset = 0.0

    for segment in results.segments.values():
        conditions = segment.conditions
        time = conditions.frames.inertial.time[:, 0]
        cl = conditions.aerodynamics.lift_coefficient[:, 0]
        cd = conditions.aerodynamics.drag_coefficient[:, 0]
        breakdown = conditions.aerodynamics.drag_breakdown

        seg_start = float(time[0])
        rows = []
        for i in range(len(time)):
            def _at(array, row=i, column=0):
                return float(np.asarray(array, dtype=float)[row, column])

            l_over_d = float(cl[i] / cd[i]) if cd[i] else float("nan")
            tas = _at(conditions.freestream.velocity)
            density = _at(conditions.freestream.density)
            eas = tas * (density / _RHO_SL) ** 0.5
            thrust = _at(conditions.frames.body.thrust_force_vector)
            mdot = _at(conditions.weights.vehicle_mass_rate)
            sfc = (mdot * 3600.0) / (thrust / _G0) if thrust else float("nan")

            rows.append(
                {
                    "time_s": t_offset + (float(time[i]) - seg_start),
                    "segment": segment.tag,
                    "altitude_m": _at(conditions.freestream.altitude),
                    "tas_m_s": tas,
                    "eas_m_s": eas,
                    "mach": _at(conditions.freestream.mach_number),
                    "density_kg_m3": density,
                    "range_m": _at(conditions.frames.inertial.aircraft_range),
                    "pitch_deg": _at(conditions.frames.body.inertial_rotations, i, 1)
                    / Units.deg,
                    "aoa_deg": _at(conditions.aerodynamics.angle_of_attack) / Units.deg,
                    "cl": float(cl[i]),
                    "cd": float(cd[i]),
                    "l_over_d": l_over_d,
                    "throttle": _at(conditions.propulsion.throttle),
                    "lift_n": -_at(conditions.frames.wind.lift_force_vector, i, 2),
                    "drag_n": -_at(conditions.frames.wind.drag_force_vector, i, 0),
                    "thrust_n": thrust,
                    "cd_parasite": _at(breakdown.parasite.total),
                    "cd_induced": _at(breakdown.induced.total),
                    "cd_compressible": _at(breakdown.compressible.total),
                    "cd_miscellaneous": _at(breakdown.miscellaneous.total),
                    "cd_total": _at(breakdown.total),
                    "mass_kg": _at(conditions.weights.total_mass),
                    "mass_flow_rate_kg_s": mdot,
                    "sfc_kg_kgf_hr": sfc,
                }
            )
        t_offset += float(time[-1]) - seg_start
        segments_data.append(
            {
                "tag": segment.tag,
                "spec": _segment_spec(segment),
                "converged": bool(segment.state.numerics.converged),
                # The solved unknowns, so a port that lands on the same
                # trajectory by a different throttle schedule is still caught.
                "throttle": _col(segment.state.unknowns.throttle),
                "body_angle_rad": _col(segment.state.unknowns.body_angle),
                "points": rows,
            }
        )

    seg_list = list(results.segments.values())
    first_mass = float(seg_list[0].conditions.weights.total_mass[0, 0])
    last_mass = float(seg_list[-1].conditions.weights.total_mass[-1, 0])
    block_time_s = sum(
        float(seg.conditions.frames.inertial.time[-1, 0])
        - float(seg.conditions.frames.inertial.time[0, 0])
        for seg in seg_list
    )
    summary = {
        "initial_mass_kg": first_mass,
        "final_mass_kg": last_mass,
        "fuel_burned_kg": first_mass - last_mass,
        "block_time_s": block_time_s,
        "n_segments": len(seg_list),
    }

    _check_mission(summary, segments_data, resolved_vehicle, surrogate)

    print(f"  fuel burned: {summary['fuel_burned_kg']:.1f} kg")
    print(
        f"  block time:  {summary['block_time_s']:.0f} s "
        f"({summary['block_time_s'] / 3600:.2f} h)"
    )
    print(f"  segments:    {summary['n_segments']}")

    _framework.write(
        "mission",
        "mission",
        {
            "inputs": {
                "vehicle_request": vehicle_request,
                "mission_request": mission_request,
                "vehicle": resolved_vehicle,
                "surrogate_training": surrogate,
            },
            "summary": summary,
            "segments": segments_data,
        },
        description=(
            "End-to-end SUAVE mission: default AVE aircraft, LEMD to HKJK at "
            "6500 km, twelve segments of sixteen control points, with the "
            "resolved vehicle, the trained vortex-lattice tables and the "
            "segment schedule recorded as inputs"
        ),
    )

    # --- the per-update-method fixture ----------------------------------
    held = []
    for tag, throttle, body_angle_deg in _HELD_SEGMENTS:
        segment = results.segments[tag]
        unknowns = segment.state.unknowns
        points = int(segment.state.numerics.number_control_points)

        # Put the horizontal track back where every iteration inside the
        # solver saw it. `integrate_inertial_horizontal_position` runs once,
        # in `finalize`, *after* the search is over, so the converged segment
        # this is reached through is carrying a ground track no iteration ever
        # had; `expand_rows` leaves both columns at zero and
        # `initialize_inertial_position` then shifts them onto the previous
        # segment's end. Restoring the zeros is restoring the solver's state,
        # not inventing one.
        conditions = segment.state.conditions
        conditions.frames.inertial.position_vector[:, 0] = 0.0
        conditions.frames.inertial.position_vector[:, 1] = 0.0
        conditions.frames.inertial.aircraft_range[:, 0] = 0.0

        # Hold the unknowns at a fixed, deliberately-not-solved vector and run
        # exactly one pass of the iterate chain. `converge_root` would search;
        # this is the one evaluation the search is made of.
        unknowns.throttle[:, 0] = throttle
        unknowns.body_angle[:, 0] = body_angle_deg * Units.deg
        segment.process.iterate(segment)

        print(f"  held {tag} at throttle {throttle}, body angle {body_angle_deg} deg")
        held.append(
            {
                "tag": tag,
                "spec": _segment_spec(segment),
                "initials": _record_initials(segment),
                "unknowns": {
                    "throttle": [throttle] * points,
                    "body_angle_rad": [body_angle_deg * Units.deg] * points,
                },
                "conditions": _record_conditions(segment),
            }
        )

    _check_segments(held)

    _framework.write(
        "mission",
        "mission_segments",
        {
            "inputs": {
                "vehicle": resolved_vehicle,
                "surrogate_training": surrogate,
            },
            "segments": held,
        },
        description=(
            "One iteration of a climb, a cruise and a descent segment at a "
            "fixed, unsolved unknown vector, recording every update method's "
            "output separately so a wrong step names itself"
        ),
    )

    _SCRATCH.unlink(missing_ok=True)
    print("done")


# ======================================================================
#  Refusals
# ======================================================================


def _check_mission(summary, segments_data, vehicle, surrogate) -> None:
    """Refuse to write a mission fixture that does not reach what it claims."""
    if summary["fuel_burned_kg"] <= 0:
        raise SystemExit(
            f"fuel burned is non-positive ({summary['fuel_burned_kg']:.1f} kg); "
            "the mission did not converge or the turbofan is not producing thrust"
        )
    if not all(segment["converged"] for segment in segments_data):
        raise SystemExit(
            "a segment did not converge; the fixture would record MINPACK's "
            "best guess rather than a solved mission, and the port would be "
            "held to a number the reference does not stand behind"
        )
    kinds = {segment["spec"]["kind"] for segment in segments_data}
    if kinds != {"climb", "cruise", "descent"}:
        raise SystemExit(
            f"the mission flies {sorted(kinds)}; the port translates exactly "
            "the three kinds mission_setup builds"
        )
    if vehicle["maximum_lift_coefficient"] is not None:
        raise SystemExit(
            "maximum_lift_coefficient is a number; update_aerodynamics' clamp "
            "is reachable, and the port does not translate it"
        )
    if vehicle["drag_settings"]["span_efficiency"] is not None:
        raise SystemExit("span_efficiency is not None; unreached branch")
    if vehicle["drag_settings"]["oswald_efficiency_factor"] is not None:
        raise SystemExit("oswald_efficiency_factor is not None; unreached branch")
    if any(wing["segment_count"] for wing in vehicle["wings"]):
        raise SystemExit("a wing carries Segments; the port translates only the plain branch")
    if not vehicle["nacelles"]:
        raise SystemExit("no nacelles; the pylon and nacelle drag terms are unreached")
    if not surrogate["supersonic_surrogate_is_absent"]:
        raise SystemExit(
            "a supersonic surrogate was built; the port translates only the "
            "subsonic branch of evaluate_surrogate"
        )
    if not surrogate["transonic_surrogate_is_absent"]:
        raise SystemExit(
            "a transonic surrogate was built; the port has no Cubic_Spline_Blender"
        )
    if max(surrogate["mach"]) >= 1.0:
        raise SystemExit("the training grid reaches Mach 1; only the subsonic branch is ported")
    if abs(vehicle["turbofan"]["sfc_adjustment"]) > 0.0:
        raise SystemExit(
            "SFC_adjustment is nonzero; the port folds it out of the TSFC as a constant zero"
        )
    if vehicle["turbofan"]["compressor_nondimensional_massflow"] <= 0.0:
        raise SystemExit("the engine was never sized; every thrust would be zero")

    throttles = [
        value
        for segment in segments_data
        for value in segment["throttle"]
    ]
    if not throttles or min(throttles) <= 0.0:
        raise SystemExit("a solved throttle is non-positive; the mission is not flyable")


def _check_segments(held) -> None:
    """Refuse to write a per-method fixture that cannot localize a fault."""
    kinds = {segment["spec"]["kind"] for segment in held}
    if kinds != {"climb", "cruise", "descent"}:
        raise SystemExit(
            f"the held segments are {sorted(kinds)}; one of each kind is the "
            "point -- the three differ in their initialize and in their residual"
        )
    for segment in held:
        conditions = segment["conditions"]
        residuals = conditions["residual_horizontal"] + conditions["residual_vertical"]
        # A segment evaluated at its own root has residuals near zero, where a
        # relative comparison says nothing and every force is in balance --
        # which is exactly the state two different force models agree in.
        if max(abs(value) for value in residuals) < 1e-3:
            raise SystemExit(
                f"segment {segment['tag']} is at its root (residuals below 1e-3); "
                "hold it at an unknown vector further from the solution"
            )
        if segment["initials"] is None:
            raise SystemExit(
                f"segment {segment['tag']} has no initials; the four initialize_* "
                "methods would all be no-ops and go unchecked"
            )
        if segment["spec"]["kind"] == "cruise":
            if any(abs(v) > 0.0 for v in conditions["acceleration_vector_x_m_s2"]):
                raise SystemExit(
                    "the cruise segment has a nonzero acceleration; its iterate "
                    "chain omits update_acceleration and the port omits it too"
                )
        elif not any(abs(v) > 0.0 for v in conditions["acceleration_vector_z_m_s2"]):
            raise SystemExit(
                f"segment {segment['tag']} has zero vertical acceleration; "
                "update_acceleration is unexercised"
            )


# ======================================================================


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate the SUAVE mission fixtures")
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
