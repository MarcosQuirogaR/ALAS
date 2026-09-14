// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Diagnostic probe for ALAS core consistency:
//! 1. Tracing public planning CG vs model CG in A220-300 preset.
//! 2. Tracing the ~656 kg sized vs dispatch mass ledger difference.
#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_pipeline::{assess_physical_feasibility, DesignPipeline, FullAnalysis, PipelineOptions};

fn main() {
    println!("============================================================");
    println!("       ALAS CORE CONSISTENCY DIAGNOSTIC PROBE");
    println!("============================================================\n");

    probe_a220_public_planning_cg();
    println!();
    probe_sized_vs_dispatch_ledger();
}

fn probe_a220_public_planning_cg() {
    println!("--- PROBE 1: A220-300 Public Planning CG & Frame Identity ---");
    let preset = presets::get("A220-300").expect("A220-300 preset");
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A220-300" }))
        .expect("preset config");

    let full_analysis = FullAnalysis::new(config.clone());
    let full_report = full_analysis
        .run(&preset.design_vector, false)
        .expect("full analysis report");

    let pipeline = DesignPipeline::new(config.clone());
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: true,
        parallel: true,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    let pipeline_res = pipeline
        .run(&options, &alas_pipeline::RunEnvironment::default())
        .expect("pipeline execution");

    let model_cg = pipeline_res
        .feasibility
        .model_cg
        .as_ref()
        .and_then(|a| {
            a.loading_states
                .iter()
                .find(|s| s.state == alas_opt::ModelCgLoadingState::AnalyzedTakeoff)
        })
        .map(|s| s.cg_pct_mac)
        .unwrap_or(f64::NAN);

    let public_cg = pipeline_res
        .feasibility
        .cg_envelope
        .cg_pct_mac
        .unwrap_or(f64::NAN);
    let status = pipeline_res.feasibility.cg_envelope.planning_status;

    println!(
        "Full report Wing mass: {:.6} kg",
        full_report
            .component_masses
            .get("Wing")
            .copied()
            .unwrap_or(0.0)
    );
    println!(
        "Full report Fuel mass: {:.6} kg",
        full_report
            .component_masses
            .get("Fuel")
            .copied()
            .unwrap_or(0.0)
    );
    println!(
        "Full report Wing station x: {:.6} m",
        full_report
            .mass_coordinates
            .get("Wing")
            .copied()
            .unwrap_or([0.0; 3])[0]
    );
    println!(
        "Full report Fuel station x: {:.6} m",
        full_report
            .mass_coordinates
            .get("Fuel")
            .copied()
            .unwrap_or([0.0; 3])[0]
    );
    println!("Physical CG x: {:.6} m", full_report.physical_cg[0]);
    println!(
        "Model-frame analyzed takeoff CG: {:.15} % MAC (pinned: 41.388990540031074)",
        model_cg
    );
    println!(
        "Public planning CG:              {:.15} % MAC (pinned: 37.9907052058242)",
        public_cg
    );
    println!("Planning CG status:              {:?}", status);

    // Frame identity check
    let envelope = preset
        .reference
        .planning_cg_envelope
        .expect("planning envelope");
    let reference = envelope.mac_reference;
    let x_public_m = reference.lemac_from_aircraft_nose_m
        + (public_cg / 100.0) * reference.mean_aerodynamic_chord_m;
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("airplane");
    let wing = airplane.wings.first().expect("main wing");
    let x_mac_le_m = wing.aerodynamic_center(0.25)[0] - 0.25 * airplane.c_ref;
    let x_model_m = x_mac_le_m + (model_cg / 100.0) * airplane.c_ref;

    println!("x_public_m: {:.6} m", x_public_m);
    println!("x_model_m:  {:.6} m", x_model_m);
    println!(
        "|x_public_m - x_model_m|: {:.3e} m (identity tolerance: 1e-6)",
        (x_public_m - x_model_m).abs()
    );
}

fn probe_sized_vs_dispatch_ledger() {
    println!("--- PROBE 2: Sized vs Dispatch Mass Ledger Reconciliation ---");
    let config = AlasConfig::default();
    let design = alas_config::design_variables::DesignVector {
        span_m: 64.25,
        root_chord_m: 12.125,
        break_chord_m: 7.3,
        tip_chord_m: 1.0,
        sweep_deg: 34.0,
        tip_twist_deg: 1.0,
        wing_x_shift_m: -1.6250000000000004,
        tail_scale: 1.0,
        fuselage_length_m: 76.25000000029104,
        tail_x_shift_m: -2.0,
        airfoil_thickness_scale: 0.8,
        airfoil_camber_scale: 1.2625,
        bump_upper_front: -0.002375,
        bump_upper_rear: -0.0006249999999999997,
        bump_lower_mid: 0.002,
        bump_lower_rear: -0.004,
    };

    let assessment =
        alas_opt::assess_product_candidate(&config, &design).expect("candidate assessment");
    let report = FullAnalysis::new(config.clone())
        .run_at_sized_takeoff_mass(&design, true, assessment.sized.takeoff_mass_kg)
        .expect("sized report");

    let payload_kg = report
        .component_masses
        .get("Payload")
        .copied()
        .unwrap_or(0.0);
    let gross_fuel_kg = report.component_masses.get("Fuel").copied().unwrap_or(0.0);
    let oew_keys = [
        "Wing",
        "H-Stab",
        "V-Stab",
        "Fuselage",
        "Gear",
        "Propulsion",
        "Systems",
        "Furnishings",
    ];
    let oew_kg: f64 = oew_keys
        .iter()
        .map(|k| report.component_masses.get(*k).copied().unwrap_or(0.0))
        .sum();

    println!("Basis A (Sized structural component model):");
    println!("  OEW:                     {:12.3} kg", oew_kg);
    println!("  Payload:                 {:12.3} kg", payload_kg);
    println!("  OEW + Payload:           {:12.3} kg", oew_kg + payload_kg);
    println!("  Gross Fuel (MTOW closure): {:10.3} kg", gross_fuel_kg);
    println!(
        "  Sum (takeoff mass basis): {:11.3} kg",
        oew_kg + payload_kg + gross_fuel_kg
    );

    // Physical feasibility and fuel loading assessment via public pipeline API
    let feasibility = assess_physical_feasibility(&config, &design, &report, None);
    let fuel_loading = feasibility.fuel_loading;
    let zfw_kg = fuel_loading.zero_fuel_mass_kg;
    let unusable_fuel_kg = fuel_loading.unusable_fuel_kg.unwrap_or(0.0);
    let usable_closure_fuel_kg = fuel_loading.mtow_closure_fuel_kg;

    println!("\nBasis B (Certified dispatch model):");
    println!("  ZFW:                     {:12.3} kg", zfw_kg);
    println!("  Unusable Fuel:           {:12.3} kg", unusable_fuel_kg);
    println!(
        "  Usable Closure Fuel:     {:12.3} kg",
        usable_closure_fuel_kg
    );
    println!(
        "  Sum (zero-fuel + usable):{:11.3} kg",
        zfw_kg + usable_closure_fuel_kg
    );

    println!("\nLedger Reconciliation:");
    println!(
        "  Difference ZFW - (OEW + Payload): {:10.3} kg",
        zfw_kg - (oew_kg + payload_kg)
    );
    println!(
        "  Unusable Fuel (from layout):     {:10.3} kg",
        unusable_fuel_kg
    );
    println!(
        "  Residual gap:                    {:10.3e} kg",
        (zfw_kg - (oew_kg + payload_kg)) - unusable_fuel_kg
    );

    if let Some(mb) = &feasibility.mass_balance {
        println!("\nObservable Tank Breakdown (from mass balance):");
        for tank in &mb.tanks {
            println!(
                "  Tank {:<18} ({:<8}): usable = {:10.3} kg, unusable = {:8.3} kg",
                tank.id, tank.kind, tank.usable_capacity_kg, tank.unusable_kg
            );
        }
        println!(
            "  Total layout unusable fuel:       {:10.3} kg",
            mb.unusable_fuel_kg
        );
        println!(
            "  Total layout usable capacity:    {:10.3} kg",
            mb.usable_capacity_kg
        );

        println!("\nObservable Loading States (from mass balance):");
        for state in &mb.states {
            println!(
                "  State {:<20}: mass = {:10.3} kg, cg_x = {:7.3} m ({:6.2} %MAC)",
                state.label, state.mass_kg, state.cg_m[0], state.cg_pct_mac
            );
        }
    }
}
