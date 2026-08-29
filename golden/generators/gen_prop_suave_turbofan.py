# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-prop::suave_turbofan``: SUAVE's ``Turbofan`` cycle, single flight
condition.

Two stages, like ``gen_aero_drag_buildup.py``. Stage 1 (``.venv``) builds an
``ALASConfig`` from a preset, runs ``FullAnalysis`` and turns the report into
a ``vehicle_request`` dict via ``alas.integration.suave_vehicle.build_vehicle_request``
-- the same request ``vehicle_builder.build_vehicle`` consumes in production.
Stage 2 (``.suave-venv``) calls ``vehicle_builder.build_vehicle(vehicle_request)``,
which builds the ``Turbofan`` network and, as its very last step, calls
``turbofan_sizing(turbofan, cruise_mach, cruise_altitude)``. That call is this
row's whole scope: it walks the network once at the cruise design point
(recording every intermediate station -- ram, inlet, both compressors, fan,
combustor, both turbines, both nozzles, the sizing pass through
``Thrust.size``) and once more at sea-level-static conditions through
``Turbofan.evaluate_thrust`` to report ``sealevel_static_thrust``, reusing the
core-flow scale factor (``compressor_nondimensional_massflow``) the cruise
pass solved.

``evaluate_thrust``'s repeated, vectorized-across-timesteps use by the mission
segment solver is out of scope for this row -- see
``crates/alas-prop/src/suave_turbofan.rs``'s module doc.
"""

from __future__ import annotations

import argparse
import json

import numpy as np

import _framework

_SCRATCH = _framework.GOLDEN_DIR / "prop" / "_turbofan_requests.json"

# Two presets with materially different engines/cruise points: AVE (GE9X,
# very high bypass, M0.84/11,887 m) and A320-200 (LEAP-1A, moderate bypass,
# a different cruise Mach/altitude and a much smaller thrust class).
_CASES = ["AVE", "A320-200"]


def _stage_build() -> None:
    _framework.add_alas_to_path()

    from alas.config.settings import ALASConfig
    from alas.config.presets import get_preset
    from alas.integration.suave_vehicle import build_vehicle_request
    from alas.analysis.full_analysis import FullAnalysis

    requests = {}
    for preset_name in _CASES:
        config = ALASConfig.from_dict({"preset": preset_name})
        design_vector = get_preset(preset_name).design_vector
        analysis = FullAnalysis(config)
        report = analysis.run(design_vector, verbose=False)
        vehicle_request = build_vehicle_request(report, config)
        requests[preset_name] = vehicle_request

    _SCRATCH.parent.mkdir(parents=True, exist_ok=True)
    with _SCRATCH.open("w", encoding="utf-8", newline="\n") as f:
        json.dump(requests, f, indent=2)
        f.write("\n")


def _val(x) -> float:
    """Pull the one scalar out of whatever shape SUAVE's Data arrays carry
    (1-D from ``np.atleast_1d`` at cruise, 2-D from ``np.atleast_2d`` at the
    sea-level-static point -- ``turbofan_sizing`` itself mixes the two)."""
    return float(np.ravel(np.asarray(x))[0])


def _outputs(component, keys: list[str]) -> dict:
    """Read the named ``component.outputs`` fields that exist, as floats.

    Not every field a component *can* produce is populated on every branch
    (e.g. ``Compression_Nozzle`` never sets ``static_pressure`` on the
    ``compressibility_effects=False`` path this network always takes), so a
    missing attribute is skipped rather than treated as an error.
    """
    out = {}
    for key in keys:
        if hasattr(component.outputs, key):
            out[key] = _val(getattr(component.outputs, key))
    return out


def _freestream(conditions, extra: list[str] | None = None) -> dict:
    keys = [
        "pressure",
        "temperature",
        "density",
        "dynamic_viscosity",
        "gravity",
        "isentropic_expansion_factor",
        "Cp",
        "R",
        "speed_of_sound",
        "velocity",
        "mach_number",
        "stagnation_temperature",
        "stagnation_pressure",
    ]
    if extra:
        keys = keys + extra
    fs = conditions.freestream
    out = {}
    for key in keys:
        if hasattr(fs, key):
            out[key] = _val(getattr(fs, key))
    return out


def _component_stations(turbofan) -> dict:
    """Every component's ``.outputs``, read straight off the network object.

    Deliberately does not touch ``conditions``/freestream: the components are
    mutated in place and shared between the cruise and sea-level-static
    passes, so a caller must combine this with a *separately* captured
    freestream snapshot from the pass it actually wants -- see
    ``_stage_solve``'s monkeypatching, which is what makes that possible for
    the cruise pass (`turbofan_sizing` never returns its cruise ``conditions``
    object, and by the time ``vehicle_builder.build_vehicle`` returns, every
    component has already been overwritten a second time by the
    sea-level-static replay it runs internally).
    """
    return {
        "ram": _outputs(
            turbofan.ram,
            [
                "stagnation_temperature",
                "stagnation_pressure",
                "isentropic_expansion_factor",
                "specific_heat_at_constant_pressure",
                "gas_specific_constant",
            ],
        ),
        "inlet_nozzle": _outputs(
            turbofan.inlet_nozzle,
            [
                "stagnation_temperature",
                "stagnation_pressure",
                "stagnation_enthalpy",
                "mach_number",
                "static_temperature",
                "static_enthalpy",
                "velocity",
            ],
        ),
        "low_pressure_compressor": _outputs(
            turbofan.low_pressure_compressor,
            ["stagnation_temperature", "stagnation_pressure", "stagnation_enthalpy", "work_done"],
        ),
        "high_pressure_compressor": _outputs(
            turbofan.high_pressure_compressor,
            ["stagnation_temperature", "stagnation_pressure", "stagnation_enthalpy", "work_done"],
        ),
        "fan": _outputs(
            turbofan.fan,
            ["stagnation_temperature", "stagnation_pressure", "stagnation_enthalpy", "work_done"],
        ),
        "combustor": _outputs(
            turbofan.combustor,
            ["stagnation_temperature", "stagnation_pressure", "stagnation_enthalpy", "fuel_to_air_ratio"],
        ),
        "high_pressure_turbine": _outputs(
            turbofan.high_pressure_turbine,
            ["stagnation_temperature", "stagnation_pressure", "stagnation_enthalpy"],
        ),
        "low_pressure_turbine": _outputs(
            turbofan.low_pressure_turbine,
            ["stagnation_temperature", "stagnation_pressure", "stagnation_enthalpy"],
        ),
        "core_nozzle": _outputs(
            turbofan.core_nozzle,
            [
                "stagnation_temperature",
                "stagnation_pressure",
                "stagnation_enthalpy",
                "mach_number",
                "static_temperature",
                "density",
                "static_enthalpy",
                "velocity",
                "static_pressure",
                "area_ratio",
            ],
        ),
        "fan_nozzle": _outputs(
            turbofan.fan_nozzle,
            [
                "stagnation_temperature",
                "stagnation_pressure",
                "stagnation_enthalpy",
                "mach_number",
                "static_temperature",
                "density",
                "static_enthalpy",
                "velocity",
                "static_pressure",
                "area_ratio",
            ],
        ),
        "thrust": _outputs(
            turbofan.thrust,
            [
                "thrust",
                "thrust_specific_fuel_consumption",
                "non_dimensional_thrust",
                "core_mass_flow_rate",
                "fuel_flow_rate",
                "power",
                "specific_impulse",
            ],
        ),
    }


def _stage_solve() -> None:
    _framework.add_suave_to_path()

    import vehicle_builder
    import SUAVE
    from SUAVE.Core import Data

    # `vehicle_builder.build_vehicle` calls `turbofan_sizing` internally as
    # its last step, and `turbofan_sizing` itself runs the network TWICE:
    # once manually (station by station) at the cruise design point, ending
    # in `thrust.size(conditions)`, and once more, entirely separately, at
    # sea-level-static through `turbofan.evaluate_thrust(state_sls)` to get
    # `sealevel_static_thrust`. Every component is a mutable object shared
    # between both passes, so by the time `build_vehicle` returns, every
    # component's `.outputs` holds the SECOND (sea-level-static) pass's
    # values -- reading them post-hoc, as a first version of this generator
    # did, silently records the SLS station values twice under both the
    # "cruise" and "sea_level_static" keys. `turbofan_sizing` never returns
    # its cruise `conditions` object either, so there is nothing to read the
    # real cruise freestream off after the fact.
    #
    # Two narrow monkeypatches recover both halves without reimplementing any
    # physics: `Thrust.size` is the cruise pass's last step and is called
    # with the actual cruise `conditions` object, so patching it captures
    # that reference before it goes out of scope. `Turbofan.evaluate_thrust`
    # is called only once, for the sea-level-static replay, so a snapshot
    # taken at its entry -- before delegating to the original -- is exactly
    # the network's state as the cruise pass left it.
    captured: dict = {}

    original_thrust_size = SUAVE.Components.Energy.Processes.Thrust.size

    def _patched_thrust_size(self, conditions):
        captured["cruise_conditions"] = conditions
        return original_thrust_size(self, conditions)

    SUAVE.Components.Energy.Processes.Thrust.size = _patched_thrust_size

    original_evaluate_thrust = SUAVE.Components.Energy.Networks.Turbofan.evaluate_thrust

    def _patched_evaluate_thrust(self, state):
        captured["cruise_component_stations"] = _component_stations(self)
        return original_evaluate_thrust(self, state)

    SUAVE.Components.Energy.Networks.Turbofan.evaluate_thrust = _patched_evaluate_thrust

    requests = json.loads(_SCRATCH.read_text(encoding="utf-8"))

    cases = []
    for preset_name, vehicle_request in requests.items():
        captured.clear()
        vehicle = vehicle_builder.build_vehicle(vehicle_request)
        turbofan = vehicle.networks.turbofan

        req = vehicle_request["requirements"]
        engine = vehicle_request["engine"]
        cruise_mach = float(req["cruise_mach"])
        cruise_altitude_m = float(req["cruise_altitude_m"])

        cruise_conditions = captured["cruise_conditions"]
        cruise_stations = {
            "freestream": _freestream(cruise_conditions, extra=["altitude"]),
            **captured["cruise_component_stations"],
        }

        # -- sea-level-static, replayed exactly as turbofan_sizing.py does it --
        atmosphere_sls = SUAVE.Analyses.Atmospheric.US_Standard_1976()
        atmo_sls = atmosphere_sls.compute_values(0.0, 0.0)
        planet = SUAVE.Attributes.Planets.Earth()
        conditions_sls = SUAVE.Analyses.Mission.Segments.Conditions.Aerodynamics()
        conditions_sls.freestream.altitude = np.atleast_2d(0.0)
        conditions_sls.freestream.mach_number = np.atleast_2d(0.01)
        conditions_sls.freestream.pressure = np.atleast_2d(atmo_sls.pressure)
        conditions_sls.freestream.temperature = np.atleast_2d(atmo_sls.temperature)
        conditions_sls.freestream.density = np.atleast_2d(atmo_sls.density)
        conditions_sls.freestream.dynamic_viscosity = np.atleast_2d(atmo_sls.dynamic_viscosity)
        conditions_sls.freestream.gravity = np.atleast_2d(planet.sea_level_gravity)
        conditions_sls.freestream.isentropic_expansion_factor = np.atleast_2d(
            turbofan.working_fluid.compute_gamma(atmo_sls.temperature, atmo_sls.pressure)
        )
        conditions_sls.freestream.Cp = np.atleast_2d(
            turbofan.working_fluid.compute_cp(atmo_sls.temperature, atmo_sls.pressure)
        )
        conditions_sls.freestream.R = np.atleast_2d(turbofan.working_fluid.gas_specific_constant)
        conditions_sls.freestream.speed_of_sound = np.atleast_2d(atmo_sls.speed_of_sound)
        conditions_sls.freestream.velocity = np.atleast_2d(atmo_sls.speed_of_sound * 0.01)
        conditions_sls.propulsion.throttle = np.atleast_2d(1.0)

        state_sls = Data()
        state_sls.numerics = Data()
        state_sls.conditions = conditions_sls
        results_sls = turbofan.evaluate_thrust(state_sls)

        sls_stations = {
            "freestream": _freestream(conditions_sls, extra=["altitude"]),
            **_component_stations(turbofan),
        }

        cases.append(
            {
                "label": preset_name,
                "n_engines": float(engine["n_engines"]),
                "bypass_ratio": float(engine["bypass_ratio"]),
                "overall_pressure_ratio": float(engine["overall_pressure_ratio"]),
                "fan_pressure_ratio": float(engine.get("fan_pressure_ratio", 1.5)),
                "turbine_inlet_temperature_k": float(engine.get("turbine_inlet_temp_k", 1650.0)),
                "cruise_mach": cruise_mach,
                "cruise_altitude_m": cruise_altitude_m,
                "design_thrust_n": _val(turbofan.design_thrust),
                "mass_flow_rate_design_kg_s": _val(turbofan.thrust.mass_flow_rate_design),
                "compressor_nondimensional_massflow": _val(
                    turbofan.thrust.compressor_nondimensional_massflow
                ),
                "sealevel_static_thrust_n_per_engine": _val(turbofan.sealevel_static_thrust),
                "cruise": cruise_stations,
                "sea_level_static": sls_stations,
                "sea_level_static_thrust_force_n": _val(results_sls.thrust_force_vector[0, 0]),
                "sea_level_static_vehicle_mass_rate_kg_s": _val(results_sls.vehicle_mass_rate),
            }
        )

    _framework.write(
        "prop",
        "suave_turbofan",
        {"cases": cases},
        description=(
            "SUAVE Turbofan.evaluate_thrust/size via turbofan_sizing(), at the cruise "
            "design point and replayed at sea-level-static, on the AVE and A320-200 presets"
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
