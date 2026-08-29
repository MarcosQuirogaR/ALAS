# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Collect W6.3 evidence for the fuselage mass correlation.

The fixture follows the reference call path through ``AircraftBuilder`` and
``aerosandbox.library.weights.torenbeek_weights.mass_fuselage_simple``. It
keeps the aircraft geometry, cabin/load inputs, mass-model inputs and every
ordered correlation term together so a later parity failure can distinguish a
geometry/input difference from a mass-model difference.

This evidence fixture is deliberately not registered in the family manifest:
W6.3 owns only this generator, its fixture and its focused Rust test. The
integration owner can register it after the evidence package is reviewed.
"""

from __future__ import annotations

import dataclasses
import json
import math
from pathlib import Path

import _framework

_framework.add_alas_to_path()

import aerosandbox.numpy as np
from alas.config.design_variables import DesignVector
from alas.config.geometry_config import GeometryConfig
from alas.config.mass_config import MassModelConfig
from alas.config.requirements import DesignRequirements
from alas.geometry.aircraft_builder import AircraftBuilder
from alas.physics.mass import calculate_component_masses
from aerosandbox.library.weights.torenbeek_weights import mass_fuselage_simple


_OUTPUT = _framework.GOLDEN_DIR / "mass" / "fuselage_evidence.json"


def _requirements(overrides: dict) -> DesignRequirements:
    value = DesignRequirements()
    for key, item in overrides.items():
        setattr(value, key, item)
    return value


def _mass_model(overrides: dict) -> MassModelConfig:
    value = MassModelConfig()
    for key, item in overrides.items():
        setattr(value, key, item)
    return value


def _design_vector(overrides: dict) -> DesignVector:
    value = DesignVector.default()
    for key, item in overrides.items():
        setattr(value, key, item)
    return value


def _f64(value) -> float:
    return float(value)


def _vec3(value) -> list[float]:
    return [_f64(value[0]), _f64(value[1]), _f64(value[2])]


def _wing_geometry(wing) -> dict:
    return {
        "aerodynamic_center_m": _vec3(wing.aerodynamic_center()),
        "xsecs": [
            {
                "xyz_le_m": _vec3(xsec.xyz_le),
                "chord_m": _f64(xsec.chord),
                "twist_deg": _f64(xsec.twist),
            }
            for xsec in wing.xsecs
        ],
    }


def _fuselage_geometry(fuselage) -> dict:
    return {
        "area_wetted_m2": _f64(fuselage.area_wetted()),
        "xsecs": [
            {
                "xyz_c_m": _vec3(xsec.xyz_c),
                "width_m": _f64(xsec.width),
                "height_m": _f64(xsec.height),
                "shape": _f64(xsec.shape),
            }
            for xsec in fuselage.xsecs
        ],
    }


def _softmax_trace(values: list[float], softness: float) -> dict:
    scaled = [value / softness for value in values]
    maximum = max(scaled)
    exponentials = [math.exp(max(value - maximum, -500.0)) for value in scaled]
    exponential_sum = sum(exponentials)
    logsumexp = maximum + math.log(exponential_sum)
    return {
        "values": values,
        "softness": _f64(softness),
        "scaled_values": [_f64(value) for value in scaled],
        "scaled_max": _f64(maximum),
        "exponential_sum": _f64(exponential_sum),
        "logsumexp": _f64(logsumexp),
        "softmax": _f64(logsumexp * softness),
    }


def _correlation_trace(fuselage, never_exceed_airspeed: float, wing_to_tail_distance: float) -> dict:
    widths = [_f64(xsec.width) for xsec in fuselage.xsecs]
    heights = [_f64(xsec.height) for xsec in fuselage.xsecs]
    mean_width = _f64(np.mean(np.array(widths)))
    mean_height = _f64(np.mean(np.array(heights)))
    width = _softmax_trace(widths, mean_width * 0.01)
    height = _softmax_trace(heights, mean_height * 0.01)
    max_width = width["softmax"]
    max_height = height["softmax"]
    width_height_sum = max_width + max_height
    speed_distance_over_sum = never_exceed_airspeed * wing_to_tail_distance / width_height_sum
    square_root_term = math.sqrt(speed_distance_over_sum)
    wetted_area = _f64(fuselage.area_wetted())
    wetted_area_power = wetted_area**1.2
    coefficient = 0.23
    mass = coefficient * square_root_term * wetted_area_power
    return {
        "width": width,
        "height": height,
        "mean_width_m": mean_width,
        "mean_height_m": mean_height,
        "max_width_m": max_width,
        "max_height_m": max_height,
        "width_height_sum_m": _f64(width_height_sum),
        "never_exceed_airspeed_m_s": _f64(never_exceed_airspeed),
        "wing_to_tail_distance_m": _f64(wing_to_tail_distance),
        "speed_distance_over_sum_m_s": _f64(speed_distance_over_sum),
        "square_root_term": _f64(square_root_term),
        "area_wetted_m2": wetted_area,
        "area_wetted_power_1_2": _f64(wetted_area_power),
        "coefficient": coefficient,
        "fuselage_mass_kg": _f64(mass),
    }


def _case(builder: AircraftBuilder, name: str, design_overrides: dict, requirement_overrides: dict, mass_model_overrides: dict) -> dict:
    design = _design_vector(design_overrides)
    requirements = _requirements(requirement_overrides)
    mass_model = _mass_model(mass_model_overrides)
    plane = builder.build(design, include_engines=True)
    wing = next(wing for wing in plane.wings if wing.name == "Main Wing")
    hstab = next(wing for wing in plane.wings if wing.name == "Horizontal Stabilizer")
    fuselage = plane.fuselages[0]
    wing_ac = wing.aerodynamic_center()
    hstab_ac = hstab.aerodynamic_center()
    tail_distance = max(1.0, _f64(hstab_ac[0] - wing_ac[0]))
    masses = calculate_component_masses(plane, requirements, builder.geometry, mass_model)
    correlation = _correlation_trace(fuselage, requirements.dive_speed_m_s, tail_distance)
    direct_mass = _f64(
        mass_fuselage_simple(
            fuselage=fuselage,
            never_exceed_airspeed=requirements.dive_speed_m_s,
            wing_to_tail_distance=tail_distance,
        )
    )
    if direct_mass != correlation["fuselage_mass_kg"]:
        raise SystemExit(f"reference trace construction disagrees for {name}")
    return {
        "name": name,
        "design_vector": dataclasses.asdict(design),
        "requirements_overrides": requirement_overrides,
        "mass_model_overrides": mass_model_overrides,
        "requirements": dataclasses.asdict(requirements),
        "mass_model": dataclasses.asdict(mass_model),
        "geometry_config": dataclasses.asdict(builder.geometry),
        "aircraft_geometry": {
            "main_wing": _wing_geometry(wing),
            "horizontal_stabilizer": _wing_geometry(hstab),
            "fuselage": _fuselage_geometry(fuselage),
        },
        "cabin_load": {
            "aircraft_type": requirements.aircraft_type,
            "cabin_preset": requirements.cabin_preset,
            "num_passengers": requirements.num_passengers,
            "passenger_mass_kg": _f64(requirements.passenger_mass_kg),
            "cargo_payload_kg": _f64(requirements.cargo_payload_kg),
            "payload_kg": _f64(requirements.payload_kg),
            "cabin_start_x_m": _f64(builder.geometry.fuselage.cabin_start_x_m),
            "tailcone_length_m": _f64(builder.geometry.fuselage.tailcone_length_m),
        },
        "model_choice": {
            "correlation": "aerosandbox.library.weights.torenbeek_weights.mass_fuselage_simple",
            "upstream": "AeroSandbox 4.2.8",
            "equation": "Torenbeek Eq. 8-16",
            "material_input": "not used by this correlation",
        },
        "correlation": correlation,
        "direct_mass_fuselage_kg": direct_mass,
        "component_mass_fuselage_kg": _f64(masses["Fuselage"]),
    }


def main() -> None:
    builder = AircraftBuilder(GeometryConfig())
    cases = [
        _case(builder, "nominal", {}, {}, {}),
        _case(
            builder,
            "same_geometry_cargo_and_tuned_model",
            {},
            {
                "aircraft_type": "cargo",
                "cabin_preset": "Custom",
                "cargo_payload_kg": 52_000.0,
                "passenger_mass_kg": 95.0,
            },
            {
                "systems_mass_fraction": 0.09,
                "furnishings_mass_fraction": 0.12,
                "cabin_payload_density_kg_m": 650.0,
            },
        ),
        _case(
            builder,
            "shorter_fuselage_geometry",
            {"fuselage_length_m": 70.0, "tail_x_shift_m": 0.5},
            {"dive_speed_m_s": 205.0},
            {},
        ),
    ]
    payload = {
        "provenance": {
            "reference_commit": _framework.alas_baseline(),
            "runtime": "alas",
            "environment": _framework.environment(),
        },
        "rust_source_corrections": {
            # Python has no explicit switch for this load-case choice. Rust
            # does, so the evidence test removes it only from the shared
            # reference view and pins its deliberate default here.
            "requirements.optimize_passenger_capacity": True,
        },
        "model_choice": cases[0]["model_choice"],
        "cases": cases,
    }
    _OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    with _OUTPUT.open("w", encoding="utf-8", newline="\n") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print(f"wrote {_OUTPUT.relative_to(_framework.GOLDEN_DIR)}")


if __name__ == "__main__":
    main()
