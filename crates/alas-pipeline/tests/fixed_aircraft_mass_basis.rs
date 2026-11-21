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
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
    // The registered seed equals the 150-seat planning cabin the layout seats,
    // so a different seed is set to keep the synchronization below from
    // passing by coincidence of equal numbers.
    config.requirements.num_passengers = 120;
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
    // The seed count differs from the seated cabin, so the synchronization
    // above is load-bearing rather than a coincidence of equal numbers.
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
    // A closure below MTOW is the mission-sized path; under Hard MTOW the
    // structure of every design mode stays at the declared MTOW.
    config.optimizer.objective.mtow_sizing = alas_config::MtowSizing::SizedByMission;
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
    // The shortfall is attributable: the declared total exceeds the ceiling
    // the fixed aircraft carries, which is the 150-seat planning cabin (below
    // the 180-seat certified exit limit), and the row pass stops exactly
    // there.
    assert!(summary.total_pax > summary.max_certifiable_capacity);
    assert_eq!(summary.max_certifiable_capacity, 150);
    assert_eq!(summary.seated_pax, summary.max_certifiable_capacity);

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

/// The summary names the configured MTOW sizing mode and says whether it set
/// the analysed mass: a fixed aircraft analysed at its design point keeps the
/// configured MTOW under any mode, and only a sized report is closed by it.
#[test]
fn the_summary_says_whether_the_mtow_sizing_mode_set_the_analysed_mass() {
    let preset = presets::get("A320-200").unwrap();
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    config.optimizer.objective.mtow_sizing = alas_config::MtowSizing::PayloadAdjusted;
    let analysis = FullAnalysis::new(config.clone());
    let at_design = analysis.run(&preset.design_vector, true).unwrap();
    let at_closure = analysis
        .run_at_sized_takeoff_mass(
            &preset.design_vector,
            true,
            0.9 * config.requirements.mtow_kg,
        )
        .unwrap();
    let line = |report| {
        alas_pipeline::format_summary(report, Some(&config))
            .lines()
            .find(|line| line.trim_start().starts_with("MTOW sizing"))
            .map(str::to_owned)
            .expect("the summary names the sizing mode")
    };
    let fixed = line(&at_design);
    assert!(
        fixed.contains("payload_adjusted") && fixed.contains("not applied"),
        "{fixed}"
    );
    let sized = line(&at_closure);
    assert!(
        sized.contains("payload_adjusted") && sized.contains("closed"),
        "{sized}"
    );
}

/// The summary's line on a jump of the takeoff-mass map, if it has one.
fn jump_line(report: &alas_pipeline::AnalysisReport, config: &AlasConfig) -> Option<String> {
    alas_pipeline::format_summary(report, Some(config))
        .lines()
        .find(|line| line.trim_start().starts_with("MTOW map jump"))
        .map(str::to_owned)
}

/// A jump of the takeoff-mass map the closure met reaches the summary and the
/// design database, with whether the held mission plan settled it; a closure
/// that met none reports none.
#[test]
fn a_sizing_map_jump_is_stated_in_the_summary_and_the_database() {
    let preset = presets::get("A320-200").unwrap();
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
    let mut report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .unwrap();
    let artifacts = report
        .fuel
        .artifacts(&config, &preset.design_vector)
        .unwrap();
    assert!(artifacts.sizing_map_jump.is_none());
    let database = alas_pipeline::export::report_to_database(&report, &config);
    assert!(database.weights["sizing_map_jump"].is_null());
    assert!(jump_line(&report, &config).is_none());

    let jump = alas_opt::mdo::SizingMapJump {
        lower_kg: 75_599.9,
        lower_next_kg: 75_650.0,
        upper_kg: 75_600.0,
        upper_next_kg: 75_236.0,
        resolved_by_held_plan: true,
        survives_held_plan: false,
    };
    let mut jumped = (*artifacts).clone();
    jumped.sizing_map_jump = Some(jump);
    report.fuel =
        alas_pipeline::fuel_model::ReportFuel::sized(alas_pipeline::fuel_model::SizedFuel {
            artifacts: std::sync::Arc::new(jumped),
            takeoff_mass_kg: 75_236.0,
            takeoff_fuel_kg: f64::NAN,
            trip_fuel_kg: f64::NAN,
            design_range_m: f64::NAN,
            design_landing_mass_kg: f64::NAN,
            takeoff_loading: None,
            mid_cruise_mass_kg: f64::NAN,
        });
    let line = jump_line(&report, &config).expect("the summary states the jump");
    assert!(
        line.contains("75650 kg from 75600 kg") && line.contains("settled on the mission plan"),
        "{line}"
    );
    let database = alas_pipeline::export::report_to_database(&report, &config);
    let record = &database.weights["sizing_map_jump"];
    assert_eq!(record["upper_kg"], 75_600.0);
    assert_eq!(record["resolved_by_held_plan"], true);
    assert_eq!(record["survives_held_plan"], false);
}
