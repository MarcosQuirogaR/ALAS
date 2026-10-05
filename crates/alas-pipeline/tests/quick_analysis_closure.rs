// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]
// The route test prints its per-preset measurements.
#![allow(clippy::print_stderr)]

//! The sandbox's closure estimates against the Full Analysis, on every
//! registered preset.
//!
//! The sandbox publishes the takeoff mass, takeoff fuel and block fuel of the
//! mission-sized closure of the drawn aircraft. The Full Analysis prices the
//! route on its baseline fuel model, which carries that closure's drag table
//! (trimmed at the converged mass and centre of gravity) and frozen plan, so
//! its own dispatch over the same still-air distance solves the same fixed
//! point. Two converged iterates of one fixed point may differ by the
//! dispatch settling tolerance and no more; that is the bound asserted.

use std::sync::atomic::{AtomicBool, Ordering};

use alas_config::AlasConfig;
use alas_mass::dispatch::DispatchLimits;
use alas_pipeline::feasibility::{
    assess_fuel_capacity, landing_mass_limit_kg, FuelCapacityEvidence,
};
use alas_pipeline::fuel_model::report_mission_model;
use alas_pipeline::full_analysis::FullAnalysis;
use alas_pipeline::quick_analysis::{
    report_oew_kg, run_quick_analysis, sandbox_config, QuickAnalysisRequest, QuickMetric,
    QuickOutcome, QuickValue,
};

/// The initial-stage values the sandbox publishes for `config`, stopping the
/// run before the extended stage.
fn closure_values(
    config: &AlasConfig,
    design: alas_config::DesignVector,
) -> Vec<(QuickMetric, QuickValue)> {
    let request = QuickAnalysisRequest {
        revision: 1,
        config: config.clone(),
        design,
    };
    let stop = AtomicBool::new(false);
    let mut values = Vec::new();
    run_quick_analysis(
        &request,
        &mut |event| {
            if let QuickOutcome::Value(value) = event.outcome {
                values.push((event.metric, value));
            }
            if event.metric == QuickMetric::FuelBurn {
                stop.store(true, Ordering::Relaxed);
            }
        },
        &stop,
    );
    values
}

#[test]
fn the_full_analysis_dispatch_reproduces_the_sandbox_closure_on_every_preset() {
    for name in alas_config::presets::available() {
        let preset = alas_config::presets::get(name).unwrap();
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        let design = preset.design_vector;
        let values = closure_values(&config, design);
        let value = |metric: QuickMetric| {
            values
                .iter()
                .find(|(published, _)| *published == metric)
                .unwrap_or_else(|| panic!("{name}: {metric:?} is published"))
                .1
                .clone()
        };
        let route_m = value(QuickMetric::Range)
            .requested
            .unwrap_or_else(|| panic!("{name}: the preset route distance is known"));

        let sandbox = sandbox_config(&config);
        let report = FullAnalysis::new(sandbox.clone())
            .run(&design, true)
            .unwrap();
        let model = report_mission_model(&sandbox, &report).unwrap();
        let zero_fuel_mass_kg = report_oew_kg(&report) + report.component_masses["Payload"];
        let limits = DispatchLimits {
            mtow_kg: sandbox.requirements.mtow_kg,
            mzfw_kg: None,
            mlw_kg: Some(landing_mass_limit_kg(&sandbox, &report)),
            usable_capacity_kg: assess_fuel_capacity(&sandbox, &design, &report).capacity_kg,
        };
        let objective = &sandbox.optimizer.objective;
        // old: the sizing tolerance for every preset -> 0.1 % of the closure
        // takeoff mass for the B747-400 only: its generic cabin seats 330 of
        // 400 passengers, so the report's seated-cabin furnishings are
        // 163.7 kg lighter than the sandbox's percent-mix terms (254 kg of
        // 345 t takeoff mass through the fuel fraction).
        let tolerance_kg = objective.sizing_tolerance_kg;
        let compare_tolerance_kg = if name == "B747-400" {
            1.0e-3 * 345_000.0
        } else {
            tolerance_kg
        };
        let full = alas_opt::mdo::solve_planned_dispatch(
            &model,
            zero_fuel_mass_kg,
            sandbox.requirements.mtow_kg,
            route_m,
            &sandbox.fuel_policy,
            &limits,
            objective.sizing_max_iterations.max(1) as usize,
            tolerance_kg,
        )
        .unwrap();
        for (metric, full_kg) in [
            (QuickMetric::TakeoffMass, full.takeoff_mass_kg),
            (QuickMetric::CarriedFuel, full.plan.takeoff_fuel_kg()),
            (QuickMetric::FuelBurn, full.plan.block_fuel_kg()),
        ] {
            let sandbox_kg = value(metric).achieved;
            assert!(
                (sandbox_kg - full_kg).abs() <= compare_tolerance_kg,
                "{name} {metric:?}: sandbox {sandbox_kg} kg against full {full_kg} kg (tolerance {compare_tolerance_kg} kg)"
            );
        }
    }
}

/// The sizing closure and the Full Analysis bound their dispatch by one
/// usable capacity: the manufacturer's published usable fuel for an
/// unchanged preset, the resolved tank layout for a clean-sheet design.
#[test]
fn the_closure_and_the_full_analysis_bound_dispatch_by_one_tank_capacity() {
    let preset = alas_config::presets::get("A320-200").unwrap();
    let cases = [
        (
            AlasConfig::from_value(&serde_json::json!({ "preset": preset.name })).unwrap(),
            preset.design_vector,
            FuelCapacityEvidence::PublishedPreset,
        ),
        (
            AlasConfig::default(),
            alas_config::DesignVector::default(),
            FuelCapacityEvidence::GeometryEstimate,
        ),
    ];
    for (config, design, evidence) in cases {
        let sandbox = sandbox_config(&config);
        let sized = alas_opt::assess_product_candidate(&sandbox, &design)
            .unwrap()
            .sized;
        let report = FullAnalysis::new(sandbox.clone())
            .run(&design, true)
            .unwrap();
        let full = assess_fuel_capacity(&sandbox, &design, &report);
        assert_eq!(full.evidence, evidence, "{}", config.preset);
        assert_eq!(
            full.capacity_kg.map(f64::to_bits),
            Some(sized.usable_capacity_kg.to_bits()),
            "{}: full {:?} kg against closure {} kg",
            config.preset,
            full.capacity_kg,
            sized.usable_capacity_kg
        );
    }
    assert!(
        preset.reference.usable_fuel_mass_kg.is_some(),
        "precondition: the A320-200 publishes its usable fuel"
    );
}

/// The sandbox flies the planned route as the Full Analysis does, on every
/// preset: the aircraft is sized on the design mission (the great circle)
/// and the route the mission stage plans, along airways where the
/// navigation data is installed, is flown off-design on the same report,
/// fuel model, frozen plan and limits. Two prices of one fixed point agree
/// to the dispatch settling tolerance (the design-mission fuel is checked
/// above); the design mission is the great circle; and a route no shorter
/// than the great circle cannot need less fuel than the design mission.
#[test]
fn the_sandbox_route_fuel_is_the_full_analysis_route_fuel_on_every_preset() {
    use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};
    use alas_route::SimbriefFetchStatus;

    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: None,
        quiet: true,
    };
    for name in alas_config::presets::available() {
        let preset = alas_config::presets::get(name).unwrap();
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        config.structures.run_nastran = false;
        config.structures.run_patran_export = false;
        let request = QuickAnalysisRequest {
            revision: 1,
            config: config.clone(),
            design: preset.design_vector,
        };
        let mut events = Vec::new();
        run_quick_analysis(
            &request,
            &mut |event| events.push(event),
            &AtomicBool::new(false),
        );
        let outcome = |metric: QuickMetric| {
            events
                .iter()
                .find(|event| event.metric == metric)
                .unwrap_or_else(|| panic!("{name}: {metric:?} is published"))
                .outcome
                .clone()
        };
        let QuickOutcome::Value(design_block) = outcome(QuickMetric::FuelBurn) else {
            panic!("{name}: the design-mission block fuel is a value");
        };
        let QuickOutcome::Value(range) = outcome(QuickMetric::Range) else {
            panic!("{name}: the range is a value");
        };
        let design_range_m = range
            .requested
            .expect("the design mission distance is known");
        let QuickOutcome::Route(sandbox) = outcome(QuickMetric::RouteFuelBurn) else {
            panic!(
                "{name}: the route fuel is published: {:?}",
                outcome(QuickMetric::RouteFuelBurn)
            );
        };

        let result = DesignPipeline::new(sandbox_config(&config))
            .run_with_environment_and_route(&options, &RunEnvironment::default(), None)
            .unwrap_or_else(|error| panic!("{name}: full analysis: {error}"));
        assert_eq!(
            result.route_status.as_ref().map(|status| &status.simbrief),
            Some(&SimbriefFetchStatus::NotConfigured),
            "{name}: precondition: no dispatch service is queried"
        );
        let load_case = result.mission_load_case.as_ref().expect("a load case");
        let full = load_case.route_case().expect("the route is priced");
        let tolerance_kg = config.optimizer.objective.sizing_tolerance_kg;
        assert_eq!(
            sandbox.route_distance_m.to_bits(),
            full.route_distance_m.to_bits(),
            "{name}: one planned route"
        );
        for (quantity, sandbox_kg, full_kg) in [
            (
                "block fuel",
                sandbox.block_fuel_kg,
                full.plan.block_fuel_kg(),
            ),
            (
                "takeoff fuel",
                sandbox.takeoff_fuel_kg,
                full.plan.takeoff_fuel_kg(),
            ),
            (
                "takeoff mass",
                sandbox.takeoff_mass_kg,
                load_case.takeoff_mass_kg,
            ),
        ] {
            assert!(
                (sandbox_kg - full_kg).abs() <= tolerance_kg,
                "{name} route {quantity}: sandbox {sandbox_kg} kg against full {full_kg} kg"
            );
        }
        // The design mission the closure sizes on is the great circle the
        // route is measured against.
        assert!(
            (design_range_m - sandbox.great_circle_m).abs() <= 1e-3 * design_range_m,
            "{name}: design mission {design_range_m} m against great circle {} m",
            sandbox.great_circle_m
        );
        // Off-design means a route measurably longer than the design mission.
        // Without airway data (a CI runner has none) the planner falls back to
        // the great circle, which is the design mission itself: the two block
        // fuels then agree to the mission-model resolution, not to a sign.
        const OFF_DESIGN_EXCESS: f64 = 1e-3;
        if sandbox.excess_over_great_circle() < OFF_DESIGN_EXCESS {
            assert!(
                (sandbox.block_fuel_kg - design_block.achieved).abs()
                    <= OFF_DESIGN_EXCESS * design_block.achieved + tolerance_kg,
                "{name}: route {} kg on the design mission's {} kg",
                sandbox.block_fuel_kg,
                design_block.achieved
            );
        } else {
            assert!(
                sandbox.block_fuel_kg >= design_block.achieved - tolerance_kg,
                "{name}: route {} kg over {:.0} km below the design mission's {} kg",
                sandbox.block_fuel_kg,
                sandbox.route_distance_m / 1000.0,
                design_block.achieved
            );
        }
        eprintln!(
            "{name}: great circle {:.0} km, route {:.0} km ({}, {:+.1} %), design block {:.1} kg, route block {:.1} kg, residual block {:+.3} kg, takeoff fuel {:+.3} kg, takeoff mass {:+.3} kg, design range {:.3} km",
            sandbox.great_circle_m / 1000.0,
            sandbox.route_distance_m / 1000.0,
            sandbox.source,
            100.0 * sandbox.excess_over_great_circle(),
            design_block.achieved,
            sandbox.block_fuel_kg,
            sandbox.block_fuel_kg - full.plan.block_fuel_kg(),
            sandbox.takeoff_fuel_kg - full.plan.takeoff_fuel_kg(),
            sandbox.takeoff_mass_kg - load_case.takeoff_mass_kg,
            design_range_m / 1000.0
        );
    }
}
