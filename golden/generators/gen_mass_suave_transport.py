# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-mass::suave_transport``: SUAVE's ``Weights_Transport`` analysis.

Two stages, because the fixture needs both the AeroSandbox-side ALAS pipeline
(to build a vehicle request from the default design) and the vendored SUAVE
2.5.2 package (to build the ``SUAVE.Vehicle`` and evaluate the weight
analysis on it) -- the same split ``gen_aero_drag_buildup.py`` and
``gen_mission.py`` use.

Stage 1 (``.venv`` -- AeroSandbox environment):
    Runs the default design through ``FullAnalysis`` and builds the vehicle
    request exactly as the real mission runner does.

Stage 2 (``.suave-venv`` -- SUAVE 2.5.2 environment):
    Builds the ``SUAVE.Vehicle``, runs it through ``mission_builder``'s
    ``configs_setup``/``simple_sizing``/``finalize`` -- the same sizing pass
    the real mission runner applies before any analysis reads the vehicle --
    then constructs ``Weights_Transport``, points it at the sized base
    config (exactly as ``mission_builder.base_analysis`` does), and evaluates
    it. Records every field of the returned weight breakdown, the vehicle's
    post-evaluate ``mass_properties.operating_empty``, and the narrow set of
    geometry/mass inputs the port's ``TransportVehicle`` needs to reproduce
    it -- not a full serialization of the SUAVE vehicle object.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import _framework

_SCRATCH = _framework.GOLDEN_DIR / "mass" / "_suave_transport_request.json"


def _stage_build() -> None:
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
    print(f"wrote {_SCRATCH.relative_to(_framework.GOLDEN_DIR)}")


def _wing_to_dict(wing) -> dict:
    return {
        "span_m": float(wing.spans.projected),
        "sweep_quarter_chord_rad": float(wing.sweeps.quarter_chord),
        "area_m2": float(wing.areas.reference),
        "area_exposed_m2": float(wing.areas.exposed),
        "area_wetted_m2": float(wing.areas.wetted),
        "thickness_to_chord": float(wing.thickness_to_chord),
        "taper_ratio": float(wing.taper),
        "root_chord_m": float(wing.chords.root),
        "mean_aerodynamic_chord_m": float(wing.chords.mean_aerodynamic),
        "origin_x_m": float(wing.origin[0][0]),
        "aerodynamic_center_x_m": float(wing.aerodynamic_center[0]),
    }


def _stage_solve() -> None:
    _framework.add_suave_to_path()

    import SUAVE
    import vehicle_builder
    import mission_builder

    if not _SCRATCH.exists():
        raise SystemExit(
            f"{_SCRATCH} not found -- run stage 1 first:\n"
            '  & ".venv/Scripts/python.exe" golden/generators/gen_mass_suave_transport.py --stage=build'
        )

    vehicle_request = json.loads(_SCRATCH.read_text(encoding="utf-8"))
    vehicle = vehicle_builder.build_vehicle(vehicle_request)

    configs = mission_builder.configs_setup(vehicle)
    mission_builder.simple_sizing(configs)
    configs.finalize()

    base = configs.base

    weights = SUAVE.Analyses.Weights.Weights_Transport()
    weights.vehicle = base
    results = weights.evaluate()

    wings = {w.tag: w for w in base.wings}
    main_wing = wings["main_wing"]
    hstab = wings["horizontal_stabilizer"]
    vstab = wings["vertical_stabilizer"]
    fuse = list(base.fuselages)[0]
    turbofan = base.networks.turbofan

    def struct_of(data, *fields):
        return {name: float(data[name]) for name in fields}

    weight_breakdown = {
        "structures": struct_of(
            results.structures,
            "wing", "horizontal_tail", "vertical_tail", "fuselage",
            "main_landing_gear", "nose_landing_gear", "nacelle", "paint", "total",
        ),
        "propulsion_breakdown": struct_of(
            results.propulsion_breakdown,
            "engines", "thrust_reversers", "miscellaneous", "fuel_system", "total",
        ),
        "systems_breakdown": struct_of(
            results.systems_breakdown,
            "control_systems", "apu", "electrical", "avionics", "hydraulics",
            "furnish", "air_conditioner", "instruments", "total",
        ),
        "payload_breakdown": struct_of(
            results.payload_breakdown, "passengers", "baggage", "cargo", "total"
        ),
        "operational_items": struct_of(
            results.operational_items,
            "operating_items_less_crew", "flight_crew", "flight_attendants", "total",
        ),
        "empty": float(results.empty),
        "operating_empty": float(results.operating_empty),
        "zero_fuel_weight": float(results.zero_fuel_weight),
        "fuel": float(results.fuel),
        "max_takeoff": float(results.max_takeoff),
    }

    payload = {
        "inputs": {
            "mtow_kg": float(base.mass_properties.max_takeoff),
            "max_zero_fuel_kg": float(base.mass_properties.max_zero_fuel),
            "cargo_kg": float(base.mass_properties.cargo),
            "passenger_count": float(base.passengers),
            "reference_area_m2": float(base.reference_area),
            "ultimate_load_factor": float(base.envelope.ultimate_load),
            "limit_load_factor": float(base.envelope.limit_load),
            # Raw SUAVE strings, deliberately not pre-resolved to a category:
            # the port's own match against the literal upstream vocabulary is
            # what the parity test has to exercise, quirks included (see the
            # module doc on "long range" vs. "long-range").
            "control_type": str(base.systems.control),
            "accessories_type": str(base.systems.accessories),
            "engine_count": float(turbofan.number_of_engines),
            "sealevel_static_thrust_per_engine_n": float(turbofan.sealevel_static_thrust),
            "main_wing": _wing_to_dict(main_wing),
            "horizontal_tail": _wing_to_dict(hstab),
            "vertical_tail": _wing_to_dict(vstab),
            "fuselage": {
                "differential_pressure_pa": float(fuse.differential_pressure),
                "width_m": float(fuse.width),
                "height_max_m": float(fuse.heights.maximum),
                "length_total_m": float(fuse.lengths.total),
                "area_wetted_m2": float(fuse.areas.wetted),
            },
        },
        "weight_breakdown": weight_breakdown,
        "vehicle_mass_properties_operating_empty": float(base.mass_properties.operating_empty),
    }

    if weight_breakdown["structures"]["nacelle"] != 0.0:
        raise SystemExit(
            "expected structures.nacelle == 0 for method_type='New SUAVE' "
            "(wt_prop_data is only populated by the FLOPS branches); the port "
            "assumes this and does not model nacelle structural mass"
        )
    if weight_breakdown["empty"] != weight_breakdown["operating_empty"] - weight_breakdown["operational_items"]["total"]:
        raise SystemExit("empty/operating_empty/operational_items no longer add up as expected")
    if abs(payload["vehicle_mass_properties_operating_empty"] - weight_breakdown["empty"]) > 1e-9:
        raise SystemExit(
            "vehicle.mass_properties.operating_empty no longer equals "
            "results.empty -- Weights_Transport.evaluate()'s naming quirk "
            "(it assigns results.empty, not results.operating_empty) may "
            "have changed upstream"
        )

    _framework.write(
        "mass",
        "suave_transport",
        payload,
        description=(
            "SUAVE.Analyses.Weights.Weights_Transport.evaluate() (method_type="
            "'New SUAVE', the only value ever reached) on the default AVE "
            "design's sized base config: the full weight_breakdown, every "
            "intermediate correlation term, and vehicle.mass_properties."
            "operating_empty after evaluate()"
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
