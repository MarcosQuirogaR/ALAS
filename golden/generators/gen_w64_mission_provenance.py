# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Capture W6.4 provenance and native-mission comparison checkpoints.

The generator runs in two pinned reference environments.  The first builds
the real ALAS report and request; the second builds and flies the real SUAVE
vehicle.  The output is written directly so this package does not modify the
shared family manifest while collecting evidence.

Usage::

    & ".venv/Scripts/python.exe" golden/generators/gen_w64_mission_provenance.py --stage=build
    & ".suave-venv/Scripts/python.exe" golden/generators/gen_w64_mission_provenance.py --stage=solve
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path

import _framework


_SCRATCH = _framework.GOLDEN_DIR / "mission" / "_w64_requests.json"
_OUTPUT = _framework.GOLDEN_DIR / "mission" / "w64_provenance.json"
_ROUTE_DISTANCE_M = 6_500_000.0
_SOURCE_FILES = (
    "alas/pipeline.py",
    "alas/analysis/full_analysis.py",
    "alas/integration/suave_mission.py",
    "alas/integration/suave_vehicle.py",
    "alas/physics/aerodynamics.py",
    "alas/geometry/aircraft_builder.py",
    "external tools/suave_runner/mission_builder.py",
    "external tools/suave_runner/vehicle_builder.py",
)


def _source_hashes() -> dict[str, str]:
    result = {}
    for relative in _SOURCE_FILES:
        path = _framework.ALAS_ROOT / relative
        result[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
    return result


def _build() -> None:
    _framework.add_alas_to_path()
    from alas.analysis.full_analysis import FullAnalysis
    from alas.config.airports import get_airport
    from alas.config.design_variables import DesignVector
    from alas.config.settings import ALASConfig
    from alas.integration.suave_mission import build_mission_request
    from alas.integration.suave_vehicle import build_vehicle_request

    config = ALASConfig()
    report = FullAnalysis(config).run(DesignVector.default(), verbose=False)
    origin = get_airport("LEMD")
    destination = get_airport("HKJK")
    vehicle_request = build_vehicle_request(report, config)
    mission_request = build_mission_request(
        config, origin, destination, _ROUTE_DISTANCE_M
    )
    payload = {
        "alas_commit": _framework.alas_baseline(),
        "alas_environment": _framework._fixture_environment("alas"),
        "source_sha256": _source_hashes(),
        "vehicle_request": vehicle_request,
        "mission_request": mission_request,
        "analysis": {
            "physical_cg_m": [float(value) for value in report.physical_cg],
            "reference_area_m2": float(report.airplane.s_ref),
            "mean_aerodynamic_chord_m": float(report.airplane.c_ref),
            "reference_span_m": float(report.airplane.b_ref),
            "design_point": {
                "alpha_deg": float(report.design_point.alpha_deg),
                "cl": float(report.design_point.cl),
                "cd": float(report.design_point.cd),
                "l_over_d": float(report.design_point.l_over_d),
            },
            "wings": [
                {
                    "name": wing.name,
                    "area_m2": float(wing.area()),
                    "mean_aerodynamic_chord_m": float(
                        wing.mean_aerodynamic_chord()
                    ),
                    "aspect_ratio": float(wing.aspect_ratio()),
                    "quarter_chord_sweep_rad": float(
                        math.radians(wing.mean_sweep_angle(0.25))
                    ),
                }
                for wing in report.airplane.wings
            ],
        },
    }
    _SCRATCH.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {_SCRATCH.relative_to(_framework.GOLDEN_DIR)}")


def _column(array, row: int, column: int = 0) -> float:
    return float(array[row, column])


def _point(segment, row: int) -> dict[str, float]:
    conditions = segment.conditions
    breakdown = conditions.aerodynamics.drag_breakdown
    return {
        "altitude_m": _column(conditions.freestream.altitude, row),
        "temperature_k": _column(conditions.freestream.temperature, row),
        "pressure_pa": _column(conditions.freestream.pressure, row),
        "density_kg_m3": _column(conditions.freestream.density, row),
        "speed_of_sound_m_s": _column(conditions.freestream.speed_of_sound, row),
        "dynamic_viscosity_pa_s": _column(
            conditions.freestream.dynamic_viscosity, row
        ),
        "velocity_m_s": _column(conditions.freestream.velocity, row),
        "mach": _column(conditions.freestream.mach_number, row),
        "reynolds_number_per_m": _column(conditions.freestream.reynolds_number, row),
        "dynamic_pressure_pa": _column(conditions.freestream.dynamic_pressure, row),
        "angle_of_attack_rad": _column(conditions.aerodynamics.angle_of_attack, row),
        "body_angle_rad": _column(
            conditions.frames.body.inertial_rotations, row, 1
        ),
        "lift_coefficient": _column(conditions.aerodynamics.lift_coefficient, row),
        "drag_coefficient": _column(conditions.aerodynamics.drag_coefficient, row),
        "throttle": _column(conditions.propulsion.throttle, row),
        "thrust_n": _column(conditions.frames.body.thrust_force_vector, row),
        "mass_rate_kg_s": _column(conditions.weights.vehicle_mass_rate, row),
        "mass_kg": _column(conditions.weights.total_mass, row),
        "cd_parasite": _column(breakdown.parasite.total, row),
        "cd_induced": _column(breakdown.induced.total, row),
        "cd_compressible": _column(breakdown.compressible.total, row),
        "cd_miscellaneous": _column(breakdown.miscellaneous.total, row),
        "cd_total": _column(breakdown.total, row),
    }


def _segment(segment) -> dict:
    points = segment.conditions.freestream.altitude.shape[0]
    selected = sorted({0, points // 2, points - 1})
    return {
        "tag": segment.tag,
        "converged": bool(segment.state.numerics.converged),
        "number_control_points": int(segment.state.numerics.number_control_points),
        "tolerance_solution": float(segment.state.numerics.tolerance_solution),
        "air_speed_m_s": float(segment.air_speed),
        "selected_points": {str(row): _point(segment, row) for row in selected},
        "throttle": [float(value) for value in segment.state.unknowns.throttle[:, 0]],
        "body_angle_rad": [
            float(value) for value in segment.state.unknowns.body_angle[:, 0]
        ],
    }


def _solve() -> None:
    _framework.add_suave_to_path()
    import SUAVE
    import mission_builder
    import vehicle_builder

    if not _SCRATCH.exists():
        raise SystemExit("run the build stage first")
    request = json.loads(_SCRATCH.read_text(encoding="utf-8"))
    vehicle = vehicle_builder.build_vehicle(request["vehicle_request"])
    configs = mission_builder.configs_setup(vehicle)
    mission_builder.simple_sizing(configs)
    configs.finalize()
    configs_analyses = mission_builder.analyses_setup(configs)
    mission = mission_builder.mission_setup(configs_analyses, request["mission_request"])
    missions_analyses = mission_builder.missions_setup(mission)
    analyses = SUAVE.Analyses.Analysis.Container()
    analyses.configs = configs_analyses
    analyses.missions = missions_analyses
    analyses.finalize()
    analyses.configs.base.weights.evaluate()
    results = analyses.missions.base.evaluate()

    aero = configs_analyses.base.aerodynamics
    geometry = aero.geometry
    settings = aero.settings
    turbofan = geometry.networks.turbofan
    segments = [_segment(segment) for segment in results.segments.values()]
    cruise = results.segments["cruise_step_2"]
    cruise_row = cruise.conditions.freestream.altitude.shape[0] // 2
    payload = {
        "provenance": {
            "alas_commit": request["alas_commit"],
            "alas_environment": request["alas_environment"],
            "suave_environment": _framework._fixture_environment("suave"),
            "source_sha256": request["source_sha256"],
            "suave_version": str(getattr(SUAVE, "__version__", "2.5.2")),
        },
        "call_graph": {
            "reference": [
                "alas.pipeline -> alas.integration.suave_mission.build_mission_request",
                "external tools/suave_runner/vehicle_builder.build_vehicle",
                "external tools/suave_runner/mission_builder.mission_setup",
                "SUAVE.Analyses.Mission.Sequential_Segments.evaluate",
                "SUAVE.Methods.Missions.Segments.converge_root",
            ],
            "native": [
                "DesignPipeline.run_with_environment_and_route",
                "alas_pipeline::mission_stage::evaluate",
                "alas_pipeline::mission_stage::build_analyses",
                "alas_mission::Mission::evaluate",
                "alas_mission::converge_root",
            ],
        },
        "solver": {
            "reference": "scipy.optimize.fsolve / MINPACK hybrd",
            "native": "alas_math::hybrd",
            "segment_count": len(segments),
            "all_converged": all(segment["converged"] for segment in segments),
            "control_points": sorted(
                {segment["number_control_points"] for segment in segments}
            ),
            "tolerance_solution": sorted(
                {segment["tolerance_solution"] for segment in segments}
            ),
        },
        "inputs": {
            "mission_tag": request["mission_request"]["mission_tag"],
            "route_distance_m": request["mission_request"]["route_distance_m"],
            "cruise_altitude_m": request["mission_request"]["cruise_altitude_m"],
            "departure_elevation_m": request["mission_request"][
                "departure_elevation_m"
            ],
            "arrival_elevation_m": request["mission_request"]["arrival_elevation_m"],
        },
        "geometry": request["analysis"],
        "aero_settings": {
            "reference_area_m2": float(geometry.reference_area),
            "maximum_lift_coefficient": (
                None
                if math.isinf(float(settings.maximum_lift_coefficient))
                else float(settings.maximum_lift_coefficient)
            ),
            "fuselage_lift_correction": float(settings.fuselage_lift_correction),
            "drag_settings": {
                "wing_parasite_drag_form_factor": float(
                    settings.wing_parasite_drag_form_factor
                ),
                "fuselage_parasite_drag_form_factor": float(
                    settings.fuselage_parasite_drag_form_factor
                ),
                "viscous_lift_dependent_drag_factor": float(
                    settings.viscous_lift_dependent_drag_factor
                ),
                "trim_drag_correction_factor": float(
                    settings.trim_drag_correction_factor
                ),
                "drag_coefficient_increment": float(
                    settings.drag_coefficient_increment
                ),
                "spoiler_drag_increment": float(settings.spoiler_drag_increment),
                "lift_to_drag_adjustment": float(settings.lift_to_drag_adjustment),
            },
            "network_count": len(geometry.networks),
            "config_tags": list(configs.keys()),
            "turbofan_number_of_engines": float(turbofan.number_of_engines),
        },
        "operating_point": {
            "segment": "cruise_step_2",
            "row": cruise_row,
            **_point(cruise, cruise_row),
        },
        "segments": segments,
        "summary": {
            "initial_mass_kg": float(results.segments["takeoff"].conditions.weights.total_mass[0, 0]),
            "final_mass_kg": float(results.segments["final_landing"].conditions.weights.total_mass[-1, 0]),
            "fuel_burned_kg": float(
                results.segments["takeoff"].conditions.weights.total_mass[0, 0]
                - results.segments["final_landing"].conditions.weights.total_mass[-1, 0]
            ),
        },
    }
    _OUTPUT.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    _SCRATCH.unlink(missing_ok=True)
    print(f"wrote {_OUTPUT.relative_to(_framework.GOLDEN_DIR)}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--stage", choices=("build", "solve"), required=True)
    args = parser.parse_args()
    (_build if args.stage == "build" else _solve)()


if __name__ == "__main__":
    main()
