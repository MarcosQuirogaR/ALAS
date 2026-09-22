// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The product report path answers the fixed-aircraft questions coherently:
//! one cabin per case, and one design-weight basis per design mode.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, DesignMode};
use alas_payload::layout::{LayoutSummary, PassengerSummary};
use alas_pipeline::FullAnalysis;

/// The A320 planning cabin declared by count: twelve first at 36 in pitch,
/// one hundred thirty-eight economy at 31 in pitch (SI below), which is the
/// Airbus two-class 150-seat layout.
fn a320_declared_planning_cabin(first: i64, economy: i64) -> AlasConfig {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    let cabin = &mut config.cabin.passenger;
    cabin.class_mix_mode = "count".to_owned();
    cabin.first.count = first;
    // 36 in = 0.9144 m, 21.65 in = 0.55 m.
    cabin.first.pitch_m = 0.9144;
    cabin.first.width_m = 0.55;
    cabin.business.count = 0;
    cabin.premium.count = 0;
    cabin.economy.count = economy;
    // 31 in = 0.7874 m, 18.11 in = 0.46 m.
    cabin.economy.pitch_m = 0.7874;
    cabin.economy.width_m = 0.46;
    config.requirements.num_passengers = first + economy;
    config
}

/// Seats of one class, or zero when the class is not installed.
fn seats(summary: &PassengerSummary, class: &str) -> i64 {
    summary
        .classes
        .iter()
        .find(|(name, _)| *name == class)
        .map_or(0, |(_, seats)| *seats)
}

#[test]
fn the_report_flops_cabin_is_the_cabin_the_layout_seated() {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
    let preset = presets::get("A320-200").unwrap();
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .unwrap();
    let buildup = report
        .flops_mass_buildup
        .as_deref()
        .expect("the pure FLOPS report carries its grouped buildup");
    let layout = report.payload_layout.as_ref().expect("passenger layout");
    let LayoutSummary::Passenger(summary) = &layout.summary else {
        panic!("the A320 is a passenger aircraft");
    };
    let seated: i64 = summary.classes.iter().map(|(_, seats)| seats).sum();
    assert_eq!(seated, summary.seated_pax);
    assert_eq!(
        buildup.inputs.passenger_count(),
        usize::try_from(summary.seated_pax).unwrap(),
        "the FLOPS cabin must be the seated cabin"
    );
    // The registered seed count differs from the seated cabin on this
    // preset, so the synchronization above is load-bearing rather than a
    // coincidence of equal numbers.
    assert_ne!(config.requirements.num_passengers, summary.seated_pax);
    assert!(
        (buildup.masses.payload - layout.total_mass).abs() < 1.0e-6,
        "the ledger payload is the layout payload"
    );
}

#[test]
fn a_sized_report_of_a_fixed_aircraft_keeps_the_declared_design_weights() {
    let preset = presets::get("A320-200").unwrap();
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    let declared_mtow_kg = config.requirements.mtow_kg;
    let closure_mass_kg = 0.9 * declared_mtow_kg;

    let fixed = FullAnalysis::new(config.clone());
    let at_design = fixed.run(&preset.design_vector, true).unwrap();
    let at_closure = fixed
        .run_at_sized_takeoff_mass(&preset.design_vector, true, closure_mass_kg)
        .unwrap();
    let group = |report: &alas_pipeline::full_analysis::AnalysisReport, name: &str| {
        report.component_masses[name]
    };
    for name in [
        "Wing",
        "H-Stab",
        "V-Stab",
        "Fuselage",
        "Gear",
        "Propulsion",
        "Systems",
    ] {
        assert!(
            (group(&at_design, name) - group(&at_closure, name)).abs() < 1.0e-6,
            "{name}: a fixed aircraft must keep its component at a lower closure mass"
        );
    }
    assert!(group(&at_closure, "Fuel") < group(&at_design, "Fuel"));
    assert_eq!(
        at_closure.geometry_summary["analysis_design_gross_mass_kg"],
        declared_mtow_kg
    );
    assert_eq!(
        at_closure.geometry_summary["analysis_mass_basis_kg"],
        closure_mass_kg
    );

    // The same design vector as a clean-sheet design couples: its wing at
    // the lower closure mass is a lighter wing.
    config.optimizer.design_space.mode = DesignMode::CleanSheet;
    let coupled = FullAnalysis::new(config)
        .run_at_sized_takeoff_mass(&preset.design_vector, true, closure_mass_kg)
        .unwrap();
    assert!(group(&coupled, "Wing") < group(&at_design, "Wing"));
    assert_eq!(
        coupled.geometry_summary["analysis_design_gross_mass_kg"],
        closure_mass_kg
    );
}

#[test]
fn a_declared_count_cabin_is_seated_as_declared_and_priced_as_declared() {
    // The Airbus typical two-class 150-seat A320 cabin, declared by count:
    // the layout seats exactly those seats and the FLOPS cabin terms price
    // exactly that split, so a reconstructed reference case can be run
    // through the production path without reshaping the preset.
    let preset = presets::get("A320-200").unwrap();
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    // Declared by count with the planning seat geometry the diagram implies
    // (first 36 in pitch two-two abreast, economy 31 in pitch three-three);
    // the default first-class suite geometry would not seat this cabin.
    let cabin = &mut config.cabin.passenger;
    cabin.class_mix_mode = "count".to_owned();
    cabin.first.count = 12;
    cabin.first.pitch_m = 0.9144;
    cabin.first.width_m = 0.55;
    cabin.business.count = 0;
    cabin.premium.count = 0;
    cabin.economy.count = 138;
    cabin.economy.pitch_m = 0.7874;
    cabin.economy.width_m = 0.46;
    config.requirements.num_passengers = 150;
    let report = FullAnalysis::new(config)
        .run(&preset.design_vector, true)
        .unwrap();
    let layout = report.payload_layout.as_ref().expect("passenger layout");
    let LayoutSummary::Passenger(summary) = &layout.summary else {
        panic!("passenger aircraft");
    };
    // The declared cabin is the input: the layout tries to seat exactly it
    // and reports what did not fit, rather than re-solving a different
    // cabin from length shares. Whether all 138 economy seats fit at this
    // pitch is the layout engine's finding, not this contract's.
    assert_eq!(summary.total_pax, 150);
    assert_eq!(summary.seated_pax + summary.unseated_pax, 150);
    let seats = |class: &str| {
        summary
            .classes
            .iter()
            .find(|(name, _)| *name == class)
            .map_or(0, |(_, seats)| *seats)
    };
    assert_eq!(seats("First"), 12);
    assert!(seats("Economy") > 100 && seats("Economy") <= 138);
    assert_eq!(seats("Business"), 0);
    // What the layout engine actually finds on this fuselage is pinned by
    // `the_a320_planning_cabin_seats_every_declared_seat` below.
    // And the FLOPS cabin is priced as the seated declared cabin.
    let buildup = report.flops_mass_buildup.as_deref().unwrap();
    assert_eq!(buildup.inputs.first_class_passenger_count, 12);
    assert_eq!(buildup.inputs.business_class_passenger_count, 0);
    assert_eq!(
        buildup.inputs.tourist_class_passenger_count,
        usize::try_from(seats("Economy")).unwrap()
    );
}

#[test]
fn the_a320_planning_cabin_seats_every_declared_seat() {
    // The physical finding this pins: the A320 shell laid out by the product
    // row packer seats the whole declared planning layout, twelve first at
    // 0.9144 m pitch and one hundred thirty-eight economy at 0.7874 m, with
    // nothing left unseated. It seated one hundred thirty-two economy while
    // the pitch-stretch budget charged the door bays but not the bay the
    // packer carves at the first/economy boundary: the block then ran past
    // the aft monument and the last row was truncated, so a declared
    // installed cabin and its seated cabin disagreed by exactly one row.
    let preset = presets::get("A320-200").unwrap();
    let config = a320_declared_planning_cabin(12, 138);
    let passenger_mass_kg = config.requirements.passenger_mass_kg;
    let report = FullAnalysis::new(config)
        .run(&preset.design_vector, true)
        .unwrap();
    let layout = report.payload_layout.as_ref().expect("passenger layout");
    let LayoutSummary::Passenger(summary) = &layout.summary else {
        panic!("the A320 is a passenger aircraft");
    };

    // Declared installed seats, seated seats, and the shortfall between them.
    assert_eq!(summary.total_pax, 150, "declared installed seats");
    assert_eq!(summary.seated_pax, 150, "physically seated seats");
    assert_eq!(summary.unseated_pax, 0, "capacity shortfall");
    assert_eq!(seats(summary, "First"), 12);
    assert_eq!(seats(summary, "Economy"), 138);
    assert_eq!(seats(summary, "Business"), 0);

    // The FLOPS cabin input is that same cabin, class by class.
    let buildup = report.flops_mass_buildup.as_deref().unwrap();
    assert_eq!(buildup.inputs.first_class_passenger_count, 12);
    assert_eq!(buildup.inputs.business_class_passenger_count, 0);
    assert_eq!(buildup.inputs.tourist_class_passenger_count, 138);
    assert_eq!(buildup.inputs.passenger_count(), 150);

    // And the occupants in the zero-fuel mass are those seated occupants at
    // the single product load-case passenger mass, not a seed count.
    assert!(
        (buildup.masses.payload - layout.total_mass).abs() < 1.0e-6,
        "the ledger payload is the layout payload"
    );
    assert!(
        (layout.total_mass - 150.0 * passenger_mass_kg).abs() < 1.0,
        "seated occupants at {passenger_mass_kg} kg each, got {} kg",
        layout.total_mass
    );
}

#[test]
fn a_declared_cabin_the_shell_cannot_seat_keeps_its_shortfall_visible() {
    // The counterpart finding: a declared cabin larger than the shell can
    // seat is not quietly reduced. The installed declaration stays the FLOPS
    // furnishing and service input, the layout reports the occupants it could
    // actually seat, and the difference is carried as an explicit shortfall
    // instead of being absorbed into the mass accounting.
    let preset = presets::get("A320-200").unwrap();
    let config = a320_declared_planning_cabin(12, 240);
    let passenger_mass_kg = config.requirements.passenger_mass_kg;
    let report = FullAnalysis::new(config)
        .run(&preset.design_vector, true)
        .unwrap();
    let layout = report.payload_layout.as_ref().expect("passenger layout");
    let LayoutSummary::Passenger(summary) = &layout.summary else {
        panic!("the A320 is a passenger aircraft");
    };

    assert_eq!(summary.total_pax, 252, "declared installed seats");
    assert!(
        summary.unseated_pax > 0,
        "252 seats do not fit an A320 shell; the shortfall must stay visible"
    );
    assert_eq!(summary.seated_pax + summary.unseated_pax, 252);
    assert_eq!(
        summary.seated_pax,
        seats(summary, "First") + seats(summary, "Economy")
    );
    // The shortfall is attributable: the declared total exceeds both the
    // registered certified capacity and, before it, the floor the shell has
    // at the declared pitch, so what binds is cabin length rather than the
    // exit rating the summary reports alongside it.
    assert!(summary.total_pax > summary.max_certifiable_capacity);
    assert!(
        summary.seated_pax < summary.max_certifiable_capacity,
        "floor, not the {} seat certified ceiling, binds at this pitch",
        summary.max_certifiable_capacity
    );

    // Installed equipment is priced as declared: a shortfall is an occupancy
    // finding, not a lighter cabin.
    let buildup = report.flops_mass_buildup.as_deref().unwrap();
    assert_eq!(buildup.inputs.first_class_passenger_count, 12);
    assert_eq!(buildup.inputs.tourist_class_passenger_count, 240);
    assert_eq!(buildup.inputs.passenger_count(), 252);

    // The occupants carried are only the seated ones.
    assert!(
        (layout.total_mass - summary.seated_pax as f64 * passenger_mass_kg).abs() < 1.0,
        "payload is the seated occupants, got {} kg for {} seated",
        layout.total_mass,
        summary.seated_pax
    );
    assert!(
        (buildup.masses.payload - layout.total_mass).abs() < 1.0e-6,
        "the ledger payload is the layout payload"
    );
}
