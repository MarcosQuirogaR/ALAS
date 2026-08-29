# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Capture the W6.2 landing-and-take-off evidence surface.

This is an evidence fixture rather than a replacement for the P4 performance
fixture. It follows the application-facing inputs used by the LTO renderer:
the configured airport strings after resolution, atmosphere quantities,
weight/thrust inputs, high-lift limits, field distances, V-speeds, and the
derived runway/bar annotation data. The standalone output deliberately does
not update a family manifest because W6.2 evidence is owned by the physics
investigation, not the shared fixture registry.
"""

from __future__ import annotations

import json
from pathlib import Path

import _framework

_framework.add_alas_to_path()

import aerosandbox as asb  # noqa: E402

from alas.config.airports import get_airport  # noqa: E402
from alas.config.settings import ALASConfig  # noqa: E402
from alas.geometry.aircraft_builder import AircraftBuilder  # noqa: E402
from alas.config.design_variables import DesignVector  # noqa: E402
from alas.physics import performance as perf  # noqa: E402


ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "golden" / "w62_lto" / "lto.json"
MS_TO_KT = 1.94384
RHO_SL = 1.225
R_AIR = 287.05
G = 9.81


def _airport_record(configured: str) -> dict:
    airport = get_airport(configured)
    atmo = asb.Atmosphere(altitude=airport.elevation_m)
    base_temperature_k = float(atmo.temperature())
    pressure_pa = float(atmo.pressure())
    effective_temperature_k = base_temperature_k + airport.isa_deviation_c
    effective_density_kg_m3 = pressure_pa / (R_AIR * effective_temperature_k)
    sigma = effective_density_kg_m3 / RHO_SL
    return {
        "configured": configured,
        "name": airport.name,
        "icao": airport.icao,
        "elevation_m": float(airport.elevation_m),
        "toda_m": float(airport.toda_m),
        "lda_m": float(airport.lda_m),
        "isa_deviation_c": float(airport.isa_deviation_c),
        "atmosphere": {
            "base_temperature_k": base_temperature_k,
            "effective_temperature_k": effective_temperature_k,
            "pressure_pa": pressure_pa,
            "effective_density_kg_m3": effective_density_kg_m3,
            "sigma": sigma,
        },
        "airport": airport,
    }


def _json_airport(record: dict) -> dict:
    return {key: value for key, value in record.items() if key != "airport"}


def _renderer_inputs(field: perf.FieldPerformance) -> dict:
    """Record the numeric inputs consumed by the two LTO panels."""
    toda = float(field.toda_m)
    rw_h = toda * 0.06
    v2 = float(field.v_speeds.v2_ms)

    def marker_position(speed_ms: float) -> float:
        return float(field.todr_m * min((speed_ms / v2) ** 2, 1.0) * 0.75)

    markers = {
        name: {
            "speed_ms": float(speed),
            "speed_kt": float(speed * MS_TO_KT),
            "position_m": marker_position(speed),
        }
        for name, speed in [
            ("V1", field.v_speeds.v1_ms),
            ("VR", field.v_speeds.v_r_ms),
            ("V2", field.v_speeds.v2_ms),
        ]
    }
    labels = ["TODR", "BFL", "ASD", "LDR"]
    required = [field.todr_m, field.bfl_m, field.asd_m, field.ldr_m]
    available = [field.toda_m, field.toda_m, field.toda_m, field.lda_m]
    return {
        "runway_panel": {
            "toda_m": toda,
            "lda_m": float(field.lda_m),
            "runway_height_m": rw_h,
            "x_limits_m": [-toda * 0.05, toda * 1.05],
            "y_limits_m": [-rw_h * 4.3, rw_h * 5.2],
        },
        "v_markers": markers,
        "bars": {
            "labels": labels,
            "required_m": [float(value) for value in required],
            "available_m": [float(value) for value in available],
            "feasible": [
                bool(value <= limit) for value, limit in zip(required, available)
            ],
            "y_max_m": float(max(available) * 1.20),
        },
    }


def _case(config: ALASConfig, role: str, wing_area_m2: float) -> dict:
    configured = (
        config.departure_airport if role == "departure" else config.arrival_airport
    )
    airport_record = _airport_record(configured)
    airport = airport_record["airport"]
    n_engines = len(config.geometry.engine.spanwise_positions_m)
    total_thrust_kn = n_engines * config.geometry.engine.thrust_kn
    tw_sl = perf.static_thrust_to_weight(config)
    fp = perf.compute_field_performance(
        mtow_kg=config.requirements.mtow_kg,
        wing_area_m2=wing_area_m2,
        airport=airport,
        cl_max_to=config.performance.cl_max_to,
        cl_max_land=config.performance.cl_max_land,
        tw_sl=tw_sl,
        k_land=config.performance.k_land,
        bfl_factor=config.performance.bfl_factor,
        perf_config=config.performance,
    )
    return {
        "role": role,
        "inputs": {
            "configured_airport": configured,
            "mtow_kg": float(config.requirements.mtow_kg),
            "wing_area_m2": float(wing_area_m2),
            "engine_count": n_engines,
            "thrust_per_engine_kn": float(config.geometry.engine.thrust_kn),
            "total_thrust_kn": float(total_thrust_kn),
            "weight_n": float(config.requirements.mtow_kg * G),
            "tw_sl": float(tw_sl),
        },
        "airport": _json_airport(airport_record),
        "performance_limits": {
            "cl_max_to": float(config.performance.cl_max_to),
            "cl_max_land": float(config.performance.cl_max_land),
            "k_land": float(config.performance.k_land),
            "bfl_factor": float(config.performance.bfl_factor),
        },
        "v_speeds": {
            "ms": {key: float(value) for key, value in fp.v_speeds.as_ms().items()},
            "kt": {
                key: float(value) for key, value in fp.v_speeds.as_knots().items()
            },
        },
        "distances": {
            "TODR_m": float(fp.todr_m),
            "BFL_m": float(fp.bfl_m),
            "ASD_m": float(fp.asd_m),
            "LDR_m": float(fp.ldr_m),
            "TODA_m": float(fp.toda_m),
            "LDA_m": float(fp.lda_m),
            "takeoff_margin_m": float(fp.to_margin_m),
            "landing_margin_m": float(fp.land_margin_m),
            "takeoff_feasible": bool(fp.to_feasible),
            "landing_feasible": bool(fp.land_feasible),
        },
        "renderer_inputs": _renderer_inputs(fp),
    }


def main() -> None:
    config = ALASConfig()
    # The default design vector and builder are the same geometry source used
    # by the application's report geometry summary; no optimized result is
    # needed to expose the LTO calculation's exact wing-area input.
    wing = AircraftBuilder(config.geometry).build(
        DesignVector(), include_engines=False
    ).wings[0]
    wing_area_m2 = float(wing.area())
    payload = {
        "provenance": {
            "reference_commit": _framework.alas_baseline(),
            "reference_source": [
                "alas/physics/performance.py",
                "alas/config/airports.py",
                "alas/sidecar/figures_extra.py",
            ],
            "inputs": "ALASConfig() with its default route and default DesignVector wing area",
            "runtime": _framework.environment(),
        },
        "config": {
            "preset": config.preset,
            "departure_airport": config.departure_airport,
            "arrival_airport": config.arrival_airport,
            "mtow_kg": float(config.requirements.mtow_kg),
            "wing_area_m2": wing_area_m2,
        },
        "cases": [
            _case(config, "departure", wing_area_m2),
            _case(config, "arrival", wing_area_m2),
        ],
    }
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    with OUTPUT.open("w", encoding="utf-8", newline="\n") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print(f"wrote {OUTPUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
