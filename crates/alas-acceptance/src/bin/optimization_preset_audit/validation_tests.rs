// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The validation block carries what the report path produces for the same
//! design, to the bit, on one jet (A320-200) and one turboprop (ATR72-600).
// The tests build curated presets, so a failed unwrap is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use alas_config::airports::Airport;
use alas_perf::performance::{compute_v_speeds_at_masses, far25_oei_gradient};
use alas_pipeline::field_reference::report_isa_sea_level_field_reference;
use alas_pipeline::report_config::config_for_report;
use alas_pipeline::FullAnalysis;

struct Case {
    config: AlasConfig,
    report: AnalysisReport,
    reference: Result<IsaSeaLevelFieldReference, String>,
    mtow_kg: f64,
    mlw_kg: f64,
    block: Value,
}

fn case(name: &str) -> Case {
    let config = AlasConfig::from_value(&json!({ "preset": name })).unwrap();
    let design = config.configured_nominal_design();
    let report = FullAnalysis::new(config.clone())
        .run(&design, true)
        .unwrap();
    let feasibility = alas_pipeline::assess_physical_feasibility(&config, &design, &report, None);
    // The masses of exposure::side_json for an unsized report.
    let mtow_kg = config.requirements.mtow_kg;
    let mlw_kg = alas_config::presets::get(name)
        .unwrap()
        .reference
        .mlw_kg
        .expect("preset MLW");
    let reference = report_isa_sea_level_field_reference(
        &config,
        &report,
        report.airplane.s_ref,
        mtow_kg,
        mlw_kg,
    );
    let block = validation_json(
        &config,
        &report,
        &feasibility,
        &FieldBasis {
            takeoff_mass_kg: mtow_kg,
            reference: &reference,
        },
    );
    Case {
        config,
        report,
        reference,
        mtow_kg,
        mlw_kg,
        block,
    }
}

fn f(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("not a number: {value}"))
}

#[test]
fn speeds_and_field_lengths_equal_the_report_config_path_to_the_bit() {
    let isa = Airport::custom("ISA sea level", 0.0, 10_000.0, 10_000.0, 0.0, 0.0, 0.0);
    for name in ["A320-200", "ATR72-600"] {
        let case = case(name);
        let area = case.report.airplane.s_ref;
        // The report's own speed basis, rebuilt independently of the block.
        let resolved =
            config_for_report(&case.config, &case.report, case.mtow_kg, case.mlw_kg, area);
        let perf = &resolved.performance;
        let speeds = compute_v_speeds_at_masses(
            case.mtow_kg,
            case.mlw_kg,
            area,
            &isa,
            perf.cl_max_to,
            perf.cl_max_land,
            perf,
        );
        let block = &case.block["takeoff_landing_speeds_isa_sea_level"];
        assert_eq!(
            f(&block["v2_m_s"]).to_bits(),
            speeds.v2_ms.to_bits(),
            "{name}"
        );
        assert_eq!(
            f(&block["vsr_takeoff_m_s"]).to_bits(),
            speeds.v_stall_to_ms.to_bits(),
            "{name}"
        );
        assert_eq!(
            f(&block["v2_over_vsr"]).to_bits(),
            (speeds.v2_ms / speeds.v_stall_to_ms).to_bits(),
            "{name}"
        );
        assert_eq!(
            f(&block["vref_m_s"]).to_bits(),
            (1.23 * speeds.v_stall_land_ms).to_bits(),
            "{name}"
        );
        // V2 respects the CS 25.107 floor of 1.13 VSR.
        assert!(f(&block["v2_over_vsr"]) >= 1.13 - 1e-12, "{name}");

        let reference = case.reference.as_ref().unwrap();
        let lengths = &case.block["field_lengths_isa_sea_level"];
        assert_eq!(
            f(&lengths["tofl_m"]).to_bits(),
            reference.takeoff_field_length_m.to_bits()
        );
        assert_eq!(
            f(&lengths["lfl_m"]).to_bits(),
            reference.landing_field_length_m.to_bits()
        );
        assert!(
            f(&lengths["tofl_m"]) > 500.0 && f(&lengths["lfl_m"]) > 500.0,
            "{name}"
        );
    }
}

#[test]
fn the_second_segment_gradient_is_the_assessors_balance_and_meets_its_requirement() {
    for name in ["A320-200", "ATR72-600"] {
        let case = case(name);
        let oei = &case.block["oei_second_segment"];
        let reference = case.reference.as_ref().unwrap();
        let n_engines = case.config.geometry.engine.spanwise_positions_m.len() as i64;
        let required = f(&oei["required_gradient_cs25_121b"]);
        assert_eq!(required, far25_oei_gradient(n_engines).unwrap(), "{name}");
        let available = f(&oei["available_gradient"]);
        // gamma_avail - gamma_req = (N-1)/N r (T/W_installed - T/W_required_SLS):
        // the sign of the margin is the sign of the installed minus required T/W.
        let installed = f(&oei["installed_static_tw"]);
        let required_sls = f(&oei["required_installed_sls_tw"]);
        assert_eq!(available >= required, installed >= required_sls, "{name}");
        assert_eq!(
            oei["meets_requirement"],
            json!(available >= required),
            "{name}"
        );
        // A certified twin meets its second-segment requirement; a plausible
        // gross gradient is a few percent.
        assert!((0.0..0.15).contains(&available), "{name}: {available}");
        assert_eq!(f(&oei["margin_gradient"]), available - required);
        assert_eq!(
            reference.oei.available_gradient.unwrap().to_bits(),
            available.to_bits()
        );
    }
}

#[test]
fn the_payload_range_corners_are_the_report_chart_and_the_chart_mass_flight() {
    for name in ["A320-200", "ATR72-600"] {
        let case = case(name);
        let chart = payload_range_data(&case.report, &case.config).expect("payload-range data");
        let corners = &case.block["payload_range"]["model_masses"]["corners"];
        for (index, label) in ["A", "B", "C", "D"].iter().enumerate() {
            let corner = &corners[*label];
            assert_eq!(
                f(&corner["range_nmi"]).to_bits(),
                chart.points[index].range_nm.to_bits(),
                "{name} {label}"
            );
            assert_eq!(f(&corner["payload_kg"]), chart.points[index].payload_kg);
        }
        // A is zero range at maximum payload, D the ferry corner (no payload)
        // and the ranges grow from A to D.
        assert_eq!(f(&corners["A"]["range_nmi"]), 0.0);
        assert_eq!(f(&corners["D"]["payload_kg"]), 0.0);
        let ranges = ["A", "B", "C", "D"].map(|label| f(&corners[label]["range_nmi"]));
        assert!(
            ranges.windows(2).all(|pair| pair[0] < pair[1]),
            "{name}: {ranges:?}"
        );
    }
    // The published chart masses fly corners B and C: C (more fuel, less
    // payload) goes farther; ATR72-600 has no chart masses registered.
    let a320 = case("A320-200");
    let chart = &a320.block["payload_range"]["chart_mass_corners"];
    assert!(f(&chart["range_c_km"]) > f(&chart["range_b_km"]));
    assert!(f(&chart["range_b_km"]) > 1_000.0, "{chart}");
    assert_eq!(chart["tow_kg"], json!(78_000.0));
    let atr = case("ATR72-600");
    assert!(atr.block["payload_range"]["chart_mass_corners"]["unavailable"].is_string());
}

#[test]
fn the_registered_chart_masses_parse_for_every_listed_preset() {
    for name in [
        "A320-200", "A340-300", "B787-9", "A220-300", "A380-800", "DC-10", "B747-400", "E195-E2",
        "C919",
    ] {
        let masses = chart_masses(name).unwrap();
        assert!(
            masses.tow_kg > masses.tank_kg && masses.delta_payload_kg > 0.0,
            "{name}"
        );
    }
    assert!(chart_masses("AVE").is_err());
}

#[test]
fn each_chart_carries_the_reserve_rule_it_prints() {
    use alas_config::FuelScheme;
    // Printed on the charts (data/chart_corner_masses.json carries the page).
    for name in ["DC-10", "B747-400"] {
        let rule = chart_masses(name).unwrap().reserve_rule;
        assert_eq!(rule.scheme, Some(FuelScheme::FaaFlagSupplemental), "{name}");
        assert_eq!(rule.alternate_distance_nmi, Some(200.0), "{name}");
    }
    let a380 = chart_masses("A380-800").unwrap().reserve_rule;
    assert_eq!(a380.scheme, Some(FuelScheme::EasaBasic));
    let e195 = chart_masses("E195-E2").unwrap().reserve_rule;
    assert_eq!(
        (e195.alternate_distance_nmi, e195.final_reserve_hold_min),
        (Some(100.0), Some(45.0))
    );
    // Charts that print no rule fly the configured policy.
    for name in ["A320-200", "A340-300", "B787-9", "A220-300", "C919"] {
        let rule = chart_masses(name).unwrap().reserve_rule;
        assert_eq!(rule, ChartReserveRule::NOT_PRINTED, "{name}");
    }
    let a320 = case("A320-200");
    let chart = &a320.block["payload_range"]["chart_mass_corners"];
    assert_eq!(chart["reserve_scheme"], json!("easa_basic"));
}

#[test]
fn the_atr_reference_missions_are_the_report_route_pricing() {
    let atr = case("ATR72-600");
    let missions = &atr.block["reference_missions"];
    let mut previous = 0.0;
    for (id, range_nmi) in REFERENCE_MISSIONS[0].1 {
        let mission = &missions[*id];
        assert_eq!(f(&mission["range_nmi"]), *range_nmi);
        let block = f(&mission["block_fuel_kg"]);
        assert!(block > previous, "{id}: block fuel must grow with range");
        previous = block;
        // The block is taxi out + trip + taxi in and holds the trip.
        assert!(block > f(&mission["trip_fuel_kg"]), "{id}");
    }
    let direct = reference_route_fuel(&atr.config, &atr.report, 300.0 * M_PER_NMI).unwrap();
    assert_eq!(
        f(&missions["Q5.block300"]["block_fuel_kg"]).to_bits(),
        direct.block_fuel_kg.to_bits()
    );
    // ATR 72 block fuel at 300 NM is of the order of a tonne (brochure 861 kg).
    assert!(
        (500.0..2_000.0).contains(&direct.block_fuel_kg),
        "{direct:?}"
    );
    // A preset with no stated mission has none.
    let a320 = case("A320-200");
    assert!(a320.block["reference_missions"]["Q5.block300"].is_null());
}

#[test]
fn balance_and_cruise_checks_are_present_and_consistent() {
    let case = case("A320-200");
    let balance = &case.block["balance"];
    let margin = f(&balance["static_margin"]["static_margin_pct_mac"]);
    assert_eq!(
        margin.to_bits(),
        (case.report.static_margin * 100.0).to_bits()
    );
    assert_eq!(balance["static_margin"]["stable"], json!(margin > 0.0));
    // An unsized baseline report has no planning-curve comparison (the
    // fields are null, never a number from another basis); the loading
    // envelope is always there.
    let loading = &balance["cg_range_over_loading"];
    let (forward, aft) = (f(&loading["forward_pct_mac"]), f(&loading["aft_pct_mac"]));
    assert!(
        0.0 < forward && forward < aft && aft < 100.0,
        "{forward} {aft}"
    );
    assert_eq!(
        f(&loading["range_pct_mac"]).to_bits(),
        (aft - forward).to_bits()
    );
    let cruise = &case.block["cruise_and_ceiling"];
    let mmo = &cruise["mmo_check"];
    assert!(f(&mmo["max_operating_mach"]) > f(&cruise["requirement_cruise_mach"]));
    assert_eq!(mmo["cruise_mach_within_mmo"], json!(true));
    // Each cruise section is a number or states why not; never silent.
    for key in ["top_of_climb_initial_cruise_level", "thrust_limited"] {
        assert!(cruise[key].is_object(), "{key}");
    }
}

/// `ALAS_VALIDATION_DUMP=dir cargo test ... dump_blocks -- --ignored` writes the
/// unsized-baseline block of each preset for inspection.
#[test]
#[ignore = "diagnostic dump"]
fn dump_blocks() {
    let dir = std::env::var("ALAS_VALIDATION_DUMP").expect("ALAS_VALIDATION_DUMP");
    for name in ["A320-200", "ATR72-600"] {
        let text = serde_json::to_string_pretty(&case(name).block).unwrap();
        std::fs::write(format!("{dir}/{name}.json"), text).unwrap();
    }
}
