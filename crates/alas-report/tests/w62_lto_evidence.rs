// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! W6.2 evidence for the landing-and-take-off path.
//!
//! The fixture is generated from the pinned Python application without
//! updating the shared golden manifest. Checks proceed in the order used by
//! the renderer: effective airport, atmosphere, mass/thrust inputs, high-lift
//! limits, V-speeds, field distances, then runway/bar inputs. A failure is
//! intentionally local to the first differing stage so this test remains
//! evidence collection rather than a diagnosis of the cause.

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use alas_config::airports::get;
use alas_config::AlasConfig;
use alas_perf::performance::{compute_field_performance, density_ratio};
use alas_testkit::{agrees, load_json, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    config: ConfigInputs,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct ConfigInputs {
    departure_airport: String,
    arrival_airport: String,
    mtow_kg: f64,
    wing_area_m2: f64,
}

#[derive(Debug, Deserialize)]
struct Case {
    role: String,
    inputs: Inputs,
    airport: AirportEvidence,
    performance_limits: Limits,
    v_speeds: SpeedEvidence,
    distances: DistanceEvidence,
    renderer_inputs: RendererInputs,
}

#[derive(Debug, Deserialize)]
struct Inputs {
    configured_airport: String,
    mtow_kg: f64,
    wing_area_m2: f64,
    engine_count: usize,
    thrust_per_engine_kn: f64,
    total_thrust_kn: f64,
    weight_n: f64,
    tw_sl: f64,
}

#[derive(Debug, Deserialize)]
struct AirportEvidence {
    configured: String,
    name: String,
    icao: String,
    elevation_m: f64,
    toda_m: f64,
    lda_m: f64,
    isa_deviation_c: f64,
    atmosphere: AtmosphereEvidence,
}

#[derive(Debug, Deserialize)]
struct AtmosphereEvidence {
    sigma: f64,
}

#[derive(Debug, Deserialize)]
struct Limits {
    cl_max_to: f64,
    cl_max_land: f64,
    k_land: f64,
    bfl_factor: f64,
}

#[derive(Debug, Deserialize)]
struct SpeedEvidence {
    ms: std::collections::BTreeMap<String, f64>,
    kt: std::collections::BTreeMap<String, f64>,
}

#[derive(Debug, Deserialize)]
struct DistanceEvidence {
    #[serde(rename = "TODR_m")]
    todr_m: f64,
    #[serde(rename = "BFL_m")]
    bfl_m: f64,
    #[serde(rename = "ASD_m")]
    asd_m: f64,
    #[serde(rename = "LDR_m")]
    ldr_m: f64,
    #[serde(rename = "TODA_m")]
    toda_m: f64,
    #[serde(rename = "LDA_m")]
    lda_m: f64,
    takeoff_margin_m: f64,
    landing_margin_m: f64,
    takeoff_feasible: bool,
    landing_feasible: bool,
}

#[derive(Debug, Deserialize)]
struct RendererInputs {
    runway_panel: RunwayPanel,
    v_markers: std::collections::BTreeMap<String, Marker>,
    bars: Bars,
}

#[derive(Debug, Deserialize)]
struct RunwayPanel {
    toda_m: f64,
    lda_m: f64,
    runway_height_m: f64,
    x_limits_m: [f64; 2],
    y_limits_m: [f64; 2],
}

#[derive(Debug, Deserialize)]
struct Marker {
    speed_ms: f64,
    speed_kt: f64,
    position_m: f64,
}

#[derive(Debug, Deserialize)]
struct Bars {
    labels: Vec<String>,
    required_m: Vec<f64>,
    available_m: Vec<f64>,
    feasible: Vec<bool>,
    y_max_m: f64,
}

fn close(label: &str, actual: f64, expected: f64) {
    assert!(
        agrees(actual, expected, Tier::Closed),
        "{label}: got {actual:.17e}, reference {expected:.17e}"
    );
}

fn speed_ms(speeds: &SpeedEvidence, name: &str) -> f64 {
    *speeds
        .ms
        .get(name)
        .unwrap_or_else(|| panic!("fixture has no {name} speed"))
}

fn speed_kt(speeds: &SpeedEvidence, name: &str) -> f64 {
    *speeds
        .kt
        .get(name)
        .unwrap_or_else(|| panic!("fixture has no {name} speed"))
}

#[test]
fn w62_lto_matches_the_reference_intermediates_in_renderer_order() {
    let root: Fixture = serde_json::from_value(load_json("w62_lto", "lto"))
        .expect("W6.2 LTO fixture has its recorded schema");
    let config = AlasConfig::default();

    assert_eq!(root.config.departure_airport, config.departure_airport);
    assert_eq!(root.config.arrival_airport, config.arrival_airport);
    close(
        "config.mtow_kg",
        config.requirements.mtow_kg,
        root.config.mtow_kg,
    );

    for case in root.cases {
        assert_eq!(case.inputs.configured_airport, case.airport.configured);
        assert_eq!(
            case.role == "departure",
            case.inputs.configured_airport == config.departure_airport
        );

        // Effective airport and runway data are copied data, so compare them
        // before deriving any atmospheric or performance quantity.
        let airport = get(&case.inputs.configured_airport).expect("fixture airport is registered");
        assert_eq!(airport.name, case.airport.name);
        assert_eq!(airport.icao, case.airport.icao);
        assert_eq!(airport.elevation_m, case.airport.elevation_m);
        assert_eq!(airport.toda_m, case.airport.toda_m);
        assert_eq!(airport.lda_m, case.airport.lda_m);
        assert_eq!(airport.isa_deviation_c, case.airport.isa_deviation_c);

        close(
            &format!("{}: atmosphere.sigma", case.role),
            density_ratio(airport.elevation_m, airport.isa_deviation_c),
            case.airport.atmosphere.sigma,
        );

        // The renderer obtains the mass/thrust state from the effective
        // configuration and report geometry summary, not from a second model.
        assert_eq!(case.inputs.mtow_kg, root.config.mtow_kg);
        close(
            "inputs.wing_area_m2",
            case.inputs.wing_area_m2,
            root.config.wing_area_m2,
        );
        assert_eq!(
            case.inputs.engine_count,
            config.geometry.engine.spanwise_positions_m.len()
        );
        close(
            "inputs.thrust_per_engine_kn",
            config.geometry.engine.thrust_kn,
            case.inputs.thrust_per_engine_kn,
        );
        close(
            "inputs.total_thrust_kn",
            case.inputs.engine_count as f64 * case.inputs.thrust_per_engine_kn,
            case.inputs.total_thrust_kn,
        );
        close(
            "inputs.weight_n",
            case.inputs.mtow_kg * 9.81,
            case.inputs.weight_n,
        );
        let tw_sl = case.inputs.total_thrust_kn * 1000.0 / case.inputs.weight_n;
        close("inputs.tw_sl", tw_sl, case.inputs.tw_sl);

        close(
            "limits.cl_max_to",
            config.performance.cl_max_to,
            case.performance_limits.cl_max_to,
        );
        close(
            "limits.cl_max_land",
            config.performance.cl_max_land,
            case.performance_limits.cl_max_land,
        );
        close(
            "limits.k_land",
            config.performance.k_land,
            case.performance_limits.k_land,
        );
        close(
            "limits.bfl_factor",
            config.performance.bfl_factor,
            case.performance_limits.bfl_factor,
        );

        let field = compute_field_performance(
            case.inputs.mtow_kg,
            case.inputs.wing_area_m2,
            airport,
            case.performance_limits.cl_max_to,
            case.performance_limits.cl_max_land,
            case.inputs.tw_sl,
            case.performance_limits.k_land,
            case.performance_limits.bfl_factor,
            &config.performance,
        );

        for name in [
            "Vstall_TO",
            "Vstall_land",
            "Vmc",
            "V1",
            "VR",
            "V2",
            "VAPP",
            "VTD",
        ] {
            let actual = match name {
                "Vstall_TO" => field.v_speeds.v_stall_to_ms,
                "Vstall_land" => field.v_speeds.v_stall_land_ms,
                "Vmc" => field.v_speeds.v_mc_ms,
                "V1" => field.v_speeds.v1_ms,
                "VR" => field.v_speeds.v_r_ms,
                "V2" => field.v_speeds.v2_ms,
                "VAPP" => field.v_speeds.v_app_ms,
                "VTD" => field.v_speeds.v_td_ms,
                _ => unreachable!(),
            };
            close(
                &format!("{}: {name} [m/s]", case.role),
                actual,
                speed_ms(&case.v_speeds, name),
            );
            close(
                &format!("{}: {name} [kt]", case.role),
                actual * 1.94384,
                speed_kt(&case.v_speeds, name),
            );
        }

        for (label, actual, expected) in [
            ("TODR", field.todr_m, case.distances.todr_m),
            ("BFL", field.bfl_m, case.distances.bfl_m),
            ("ASD", field.asd_m, case.distances.asd_m),
            ("LDR", field.ldr_m, case.distances.ldr_m),
            ("TODA", field.toda_m(), case.distances.toda_m),
            ("LDA", field.lda_m(), case.distances.lda_m),
            (
                "takeoff_margin",
                field.to_margin_m(),
                case.distances.takeoff_margin_m,
            ),
            (
                "landing_margin",
                field.land_margin_m(),
                case.distances.landing_margin_m,
            ),
        ] {
            close(
                &format!("{}: distances.{label}", case.role),
                actual,
                expected,
            );
        }
        assert_eq!(field.to_feasible(), case.distances.takeoff_feasible);
        assert_eq!(field.land_feasible(), case.distances.landing_feasible);

        // Verify the renderer-input sidecar is derived from the same field
        // result, including the marker and available-runway branches.
        let runway = &case.renderer_inputs.runway_panel;
        close("renderer.toda_m", field.toda_m(), runway.toda_m);
        close("renderer.lda_m", field.lda_m(), runway.lda_m);
        close(
            "renderer.runway_height_m",
            field.toda_m() * 0.06,
            runway.runway_height_m,
        );
        close(
            "renderer.x_min",
            -field.toda_m() * 0.05,
            runway.x_limits_m[0],
        );
        close(
            "renderer.x_max",
            field.toda_m() * 1.05,
            runway.x_limits_m[1],
        );
        close(
            "renderer.y_min",
            -runway.runway_height_m * 4.3,
            runway.y_limits_m[0],
        );
        close(
            "renderer.y_max",
            runway.runway_height_m * 5.2,
            runway.y_limits_m[1],
        );

        let v2 = field.v_speeds.v2_ms;
        for (name, speed) in [
            ("V1", field.v_speeds.v1_ms),
            ("VR", field.v_speeds.v_r_ms),
            ("V2", field.v_speeds.v2_ms),
        ] {
            let marker = case
                .renderer_inputs
                .v_markers
                .get(name)
                .unwrap_or_else(|| panic!("fixture has no {name} marker"));
            close(&format!("renderer.{name}.speed_ms"), speed, marker.speed_ms);
            close(
                &format!("renderer.{name}.speed_kt"),
                speed * 1.94384,
                marker.speed_kt,
            );
            close(
                &format!("renderer.{name}.position_m"),
                field.todr_m * (speed / v2).powi(2).min(1.0) * 0.75,
                marker.position_m,
            );
        }

        let bars = &case.renderer_inputs.bars;
        assert_eq!(bars.labels, ["TODR", "BFL", "ASD", "LDR"]);
        for (i, (required, available)) in [
            (field.todr_m, field.toda_m()),
            (field.bfl_m, field.toda_m()),
            (field.asd_m, field.toda_m()),
            (field.ldr_m, field.lda_m()),
        ]
        .into_iter()
        .enumerate()
        {
            close(
                &format!("renderer.bar[{i}].required_m"),
                required,
                bars.required_m[i],
            );
            close(
                &format!("renderer.bar[{i}].available_m"),
                available,
                bars.available_m[i],
            );
            assert_eq!(required <= available, bars.feasible[i]);
        }
        close(
            "renderer.bars.y_max_m",
            field.toda_m().max(field.lda_m()) * 1.20,
            bars.y_max_m,
        );
    }
}
