// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! TEST RUN of the registered Airbus A400M Atlas preset (`A400M`). The preset
//! carries the sourced and estimated inputs of `.agent/research/a400m-data.md`
//! (EASA TCDS A.169, E.033, P.012, Airbus brochure TMMA0026/01/2025,
//! Bundeswehr operator page), each flagged S(ourced), I(nferred) or
//! E(stimate) beside the number in `alas-config/src/presets/military.rs`.
//!
//! Nothing here is a calibration or a physical validation of the A400M.
//!
//! Ignored by default so the normal gate is unaffected. Run with
//! `cargo test --release -p alas-acceptance --test a400m_test_run -- --ignored --nocapture`.

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)
)]

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, DesignMode, MtowSizing};
use alas_pipeline::FullAnalysis;

const MTOW_KG: f64 = 141_000.0; // B1 p024, G1 (military)
const MLW_KG: f64 = 123_000.0; // B1 p024
const PUBLISHED_OEW_KG: f64 = 78_600.0; // G1 (MEW vs OEW not stated)
const FUSELAGE_LENGTH_M: f64 = 45.091; // T1 overall length

/// Range-payload points of B1 p024: (payload kg, range nmi, label).
const RANGE_PAYLOAD: [(f64, f64, &str); 3] = [
    (37_000.0, 1_780.0, "37 t / 3300 km"),
    (30_000.0, 2_400.0, "30 t / 4450 km"),
    (20_000.0, 3_400.0, "20 t / 6300 km"),
];

/// The registered A400M design vector.
fn a400m_design() -> DesignVector {
    alas_config::presets::get("A400M")
        .expect("A400M preset is registered")
        .design_vector
}

/// The A400M configuration loaded from the registered preset. `payload_kg` is
/// the cargo mass; `design_range_nmi` feeds the mission-sized closure.
fn a400m_config(payload_kg: f64, design_range_nmi: f64) -> AlasConfig {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A400M" }))
        .expect("A400M preset loads");
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    // Evaluate the aircraft as itself (declared MTOW), not as a clean-sheet
    // closure (see memory: alas-mass-sizing-basis).
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    config.optimizer.objective.mtow_sizing = MtowSizing::FixedRequirement;
    config.optimizer.objective.design_range_nmi = design_range_nmi;
    config.requirements.cargo_payload_kg = payload_kg;
    config.mass_model.flops_transport.design_range_nmi = Some(design_range_nmi);
    config
}

/// Equivalent-twin surrogate, kept ONLY as a comparison against the real
/// four-engine run. Before the N-engine generalisation of the turboprop
/// adapter (alas-prop turboprop/system.rs) the mission, fuel-sizing and
/// field-performance models accepted exactly two engines, so the A400M could
/// only be flown this way. This surrogate replaces the four TP400-D6 by two engines of twice
/// the power on an equivalent actuator disc (diameter x sqrt 2 at the same
/// tip speed, i.e. rpm / sqrt 2, same disc loading), mounted at the mean
/// thrust-weighted station of the real four (10 m). Masses (engine, propeller,
/// nacelle wetted area x density, frontal area) are preserved. NOT the A400M:
/// one-engine-inoperative cases lose 50 percent of power instead of 25
/// percent, and there is no inner/outer engine moment arm.
fn twin_surrogate(mut config: AlasConfig) -> AlasConfig {
    let root2 = std::f64::consts::SQRT_2;
    let engine = &mut config.geometry.engine;
    engine.spanwise_positions_m = vec![10.0, -10.0];
    engine.radius_scale_m *= root2;
    if let Some(spec) = engine.turboprop.as_mut() {
        spec.takeoff_shaft_power_kw *= 2.0;
        spec.maximum_reserve_shaft_power_kw *= 2.0;
        spec.maximum_continuous_shaft_power_kw *= 2.0;
        spec.maximum_climb_shaft_power_kw *= 2.0;
        spec.maximum_cruise_shaft_power_kw *= 2.0;
        spec.maximum_cruise_fuel_flow_kg_h *= 2.0; // two-engine figure, doubled power
        spec.propeller_diameter_m *= root2;
        spec.governed_propeller_speed_rpm /= root2;
    }
    let tp = &mut config.mass_model.flops_turboprop;
    tp.engine_dry_mass_kg = tp.engine_dry_mass_kg.map(|m| m * 2.0);
    tp.baseline_shaft_power_kw = tp.baseline_shaft_power_kw.map(|p| p * 2.0);
    tp.propeller_assembly_mass_kg = tp.propeller_assembly_mass_kg.map(|m| m * 2.0);
    tp.nacelle_area_density_kg_m2 *= root2;
    config.mass_model.flops_transport.wing_mounted_engine_count = Some(2);
    config
}

/// FullAnalysis at the declared MTOW plus the physical feasibility
/// assessment, printed in full.
fn analyze(label: &str, config: &AlasConfig, design: &DesignVector) {
    println!("=== A400M TEST RUN [{label}]: FullAnalysis at declared MTOW {MTOW_KG} kg, requested cargo {} kg ===", config.requirements.cargo_payload_kg);
    let report = match FullAnalysis::new(config.clone()).run(design, true) {
        Ok(report) => report,
        Err(error) => {
            println!("FULL ANALYSIS FAILED: {error}");
            return;
        }
    };
    let m = |key: &str| {
        report
            .component_masses
            .get(key)
            .copied()
            .unwrap_or(f64::NAN)
    };
    let payload = m("Payload");
    let fuel = m("Fuel");
    // OEW from the operating-empty groups; MTOW - payload - fuel is wrong when
    // the fuel is volume-limited (takeoff mass then below MTOW).
    let oew: f64 = [
        "Wing",
        "H-Stab",
        "V-Stab",
        "Fuselage",
        "Gear",
        "Propulsion",
        "Systems",
        "Furnishings",
    ]
    .iter()
    .map(|key| m(key))
    .sum();
    let takeoff_mass = oew + payload + fuel;
    println!("component masses [kg]:");
    let mut keys: Vec<_> = report.component_masses.iter().collect();
    keys.sort_by(|a, b| a.0.cmp(b.0));
    for (name, value) in keys {
        println!("  {name:32} {value:12.1}");
    }
    println!("declared MTOW {MTOW_KG:.0}  loaded takeoff mass {takeoff_mass:.0}  payload {payload:.0}  fuel {fuel:.0}  OEW (sum of operating-empty groups) {oew:.0}");
    println!(
        "OEW vs published {PUBLISHED_OEW_KG}: {:+.0} kg ({:+.1} %)",
        oew - PUBLISHED_OEW_KG,
        (oew / PUBLISHED_OEW_KG - 1.0) * 100.0
    );
    println!(
        "static margin {:.4}  x_NP {:.3} m  physical CG {:?}",
        report.static_margin, report.x_neutral_point, report.physical_cg
    );
    println!("design point {:?}", report.design_point);
    if let Some(frame) = report.airplane.mac_frame() {
        let lemac = frame.x_at_pct(0.0);
        let mac = frame.x_at_pct(100.0) - lemac;
        println!(
            "MAC {mac:.3} m (published 5.671), LEMAC {lemac:.3} m aft of nose; physical CG {:.1} %MAC; clean NP {:.1} %MAC; c_ref {:.3} m; s_ref {:.2} m2",
            (report.physical_cg[0] - lemac) / mac * 100.0,
            (report.x_neutral_point - lemac) / mac * 100.0,
            report.airplane.c_ref,
            report.airplane.s_ref
        );
    }
    println!("cg_envelope_ok {:?}", report.cg_envelope_ok);
    if let Some(layout) = report.payload_layout.as_ref() {
        println!("payload layout summary: {:?}", layout.summary);
        println!(
            "payload layout cg_x {:.3} m, total mass {:.0} kg, {} items",
            layout.cg_x,
            layout.total_mass,
            layout.items.len()
        );
    }
    for key in [
        "analysis_mass_basis_kg",
        "effective_structural_payload_limit_kg",
        "wing_area_m2",
        "aspect_ratio",
    ] {
        println!(
            "geometry_summary[{key}] = {:?}",
            report.geometry_summary.get(key)
        );
    }
    let feasibility = alas_pipeline::assess_physical_feasibility(config, design, &report, None);
    println!("feasible: {}", feasibility.is_feasible());
    for finding in &feasibility.findings {
        println!("FINDING {finding:?}");
    }
    if let Some(model_cg) = feasibility.model_cg.as_ref() {
        println!(
            "model CG hard constraints pass: {}",
            model_cg.hard_constraints_pass()
        );
        for state in &model_cg.loading_states {
            let l = &state.physical_limits;
            println!(
                "  state {:?}: static_margin {:.4}  fwd {:.2} ({:?}) aft {:.2} ({:?}) rotation_fwd {:.2} %MAC  usable range {:.2}",
                state.state, state.static_margin, l.fwd_limit_pct_mac, l.fwd_limit_governance,
                l.aft_limit_pct_mac, l.aft_limit_governance, l.rotation_fwd_pct_mac, l.usable_range_pct_mac
            );
        }
    } else {
        println!("no model CG assessment");
    }
    println!(
        "native_mission_error {:?}",
        feasibility.native_mission_error
    );
}

#[test]
#[ignore = "A400M test run: not part of the normal gate"]
fn a400m_full_analysis_and_feasibility() {
    let design = a400m_design();
    let config = a400m_config(20_000.0, 3_400.0);
    analyze("4 x TP400-D6 (as built)", &config, &design);
    let twin = twin_surrogate(config);
    analyze("equivalent twin surrogate", &twin, &design);
    // Sensitivity: main gear 1.0 m further aft (stations are estimates).
    let aft = shift_main_gear(a400m_config(20_000.0, 3_400.0), 1.0);
    analyze(
        "4 x TP400-D6, main gear +1.0 m aft (sensitivity)",
        &aft,
        &design,
    );
}

/// Move the main-gear stations (and the wheelbase) aft by `dx_m`.
fn shift_main_gear(mut config: AlasConfig, dx_m: f64) -> AlasConfig {
    let gear = &mut config.landing_gear;
    if let Some(fractions) = gear.reference_mlg_x_fractions.as_mut() {
        for fraction in fractions.iter_mut() {
            *fraction += dx_m / FUSELAGE_LENGTH_M;
        }
    }
    gear.reference_wheelbase_m = gear.reference_wheelbase_m.map(|w| w + dx_m);
    config
}

/// Reference: how the same model treats registered presets of the same class
/// (OEW from the operating-empty groups vs the preset's own reference OEW,
/// gear mass fraction), to separate A400M-specific error from model bias.
#[test]
#[ignore = "A400M test run: not part of the normal gate"]
fn a400m_reference_presets_bias() {
    for name in ["ATR72-600", "A320-200", "A340-300"] {
        let preset = alas_config::presets::get(name).expect("preset");
        let mut config =
            AlasConfig::from_value(&serde_json::json!({ "preset": name })).expect("config");
        config.structures.run_nastran = false;
        config.structures.run_patran_export = false;
        config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
        let report = FullAnalysis::new(config.clone())
            .run(&preset.design_vector, true)
            .expect("full analysis");
        let m = |key: &str| {
            report
                .component_masses
                .get(key)
                .copied()
                .unwrap_or(f64::NAN)
        };
        let oew: f64 = [
            "Wing",
            "H-Stab",
            "V-Stab",
            "Fuselage",
            "Gear",
            "Propulsion",
            "Systems",
            "Furnishings",
        ]
        .iter()
        .map(|key| m(key))
        .sum();
        println!(
            "REFERENCE {name}: model OEW {oew:.0} vs reference {:?}; gear {:.0} kg = {:.2} % of MTOW; wing {:.0}; fuselage {:.0}; furnishings {:.0}; propulsion {:.0}; systems {:.0}",
            preset.reference.oew_kg,
            m("Gear"),
            100.0 * m("Gear") / config.requirements.mtow_kg,
            m("Wing"), m("Fuselage"), m("Furnishings"), m("Propulsion"), m("Systems")
        );
    }
}

/// Station-placement diagnostics: why does the mission-sized closure report
/// `mass_coordinates`? Re-runs the same calls the closure makes and prints
/// the typed error text.
#[test]
#[ignore = "A400M test run: not part of the normal gate"]
fn a400m_station_diagnostics() {
    use alas_mass::breakdown::MassCoordinateModel;
    let design = a400m_design();
    let config = a400m_config(20_000.0, 3_400.0);
    let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .expect("A400M geometry builds");
    let model = config.analysis_mass_model(config.requirements.mtow_kg);
    let (masses, coords, _) =
        alas_mass::breakdown::run_mass_analysis_with_model_checked_product_with_gear(
            &plane,
            &config.requirements,
            &config.geometry,
            &config.cabin,
            &config.control_surfaces,
            Some(&model),
            None,
            MassCoordinateModel::StructuralWingbox(&config.structures),
            &config.landing_gear,
        )
        .unwrap_or_else(|error| panic!("mass analysis: {error}"));
    println!("mass analysis ok: {:?}", masses.as_pairs());
    {
        let (mut m2, mut c2) = (masses, coords);
        let loaded =
            alas_mass::loading::apply_mtow_fuel_loading(&config, &design, &plane, &mut m2, &mut c2);
        println!("apply_mtow_fuel_loading: {:?}", loaded);
    }
    let sized = alas_mass::wing_reconciliation::size_design_wing_box(&config, &design, &plane);
    println!("design wing box: {:?}", sized.as_ref().map(|_| "ok"));
    if let Ok(shared) = sized {
        let placed = alas_mass::product_stations::product_mass_coordinates_with_box(
            &config, &design, &plane, &masses, coords, &shared,
        );
        println!(
            "product_mass_coordinates_with_box: {:?}",
            placed.map(|(_, cg)| cg)
        );
    }
    let deck = alas_opt::mdo::propulsion::PropulsionDeck::from_engine(
        &config.geometry.engine,
        config.requirements.cruise_mach,
        config.requirements.cruise_altitude_m,
        None,
    );
    println!(
        "PropulsionDeck::from_engine: {:?}",
        deck.as_ref().map(|d| d.identity().to_owned())
    );
    let stations = alas_mass::stations::component_stations_with_gear(
        &plane,
        &config.geometry,
        &config.requirements,
        &config.mass_model,
        &config.structures,
        &config.landing_gear,
    );
    println!("component_stations_with_gear: {:?}", stations.map(|_| "ok"));
}

/// Mission-sized closure (design range at design payload, fuel-policy
/// reserves) for the three published range-payload points, BaselineSandbox so
/// the declared MTOW/MLW stay the structural design weights.
#[test]
#[ignore = "A400M test run: not part of the normal gate"]
fn a400m_range_payload_points() {
    let design = a400m_design();
    // The four real TP400-D6 are flown for both sizing modes; the old
    // equivalent-twin surrogate is kept as a comparison for SizedByMission.
    for (payload_kg, range_nmi, label) in RANGE_PAYLOAD {
        for (sizing, surrogate) in [
            (MtowSizing::SizedByMission, false),
            (MtowSizing::Unconstrained, false),
            (MtowSizing::SizedByMission, true),
        ] {
            let mut config = a400m_config(payload_kg, range_nmi);
            if surrogate {
                config = twin_surrogate(config);
            }
            config.optimizer.objective.mtow_sizing = sizing;
            config.optimizer.design_space.mode = DesignMode::CleanSheet;
            let engines = if surrogate {
                "twin surrogate"
            } else {
                "4 x TP400-D6"
            };
            println!("=== range-payload point {label} [{engines}]: payload {payload_kg} kg, range {range_nmi} nmi, {sizing:?} ===");
            match alas_opt::assess_product_candidate(&config, &design) {
                Err(reason) => println!("  ASSESSMENT ERROR: {reason}"),
                Ok(assessment) => {
                    let sized = &assessment.sized;
                    let plan = &sized.dispatch.plan;
                    println!(
                        "  closed {}  status {:?}  hard_feasible {}  violated {:?}",
                        sized.sizing_closed,
                        sized.dispatch.status,
                        assessment.hard_feasible,
                        assessment.violated_hard_ids()
                    );
                    println!(
                        "  MTOW(closed) {:.0}  OEW {:.0}  payload {:.0}  ZFW {:.0}  block fuel {:.0}  takeoff fuel {:.0}  trip {:.0}  contingency {:.0}  alternate {:.0}  final reserve {:.0}  taxi {:.0}",
                        sized.takeoff_mass_kg,
                        sized.operating_empty_mass_kg,
                        sized.payload_kg,
                        sized.zero_fuel_mass_kg,
                        sized.block_fuel_kg,
                        sized.takeoff_fuel_kg,
                        plan.trip.kg,
                        plan.contingency.kg,
                        plan.alternate.kg,
                        plan.final_reserve.kg,
                        plan.taxi.kg
                    );
                    println!(
                        "  design range {:.0} m  L/D {:.2}  usable capacity {:.0} kg  landing mass limit {:.0}",
                        sized.design_range_m,
                        sized.lift_to_drag,
                        sized.usable_capacity_kg,
                        sized.design_landing_mass_kg
                    );
                    for residual in &assessment.residuals {
                        if residual.normalized_violation > 0.0 {
                            println!(
                                "  residual {}: actual {:.4} limit {:.4} {} ({})",
                                residual.id,
                                residual.actual,
                                residual.limit,
                                residual.unit,
                                residual.detail.clone().unwrap_or_default()
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Field performance and one-engine-inoperative evidence for the four real
/// TP400-D6 (OEI removes one of four engines at the reserve rating) against
/// the old equivalent-twin surrogate (OEI removes half the power). The ISA
/// sea-level reference and two airports; MTOW/MLW as declared (B1). The
/// all-engine and OEI thrust come from the shared propeller deck, not from
/// any A400M calibration; published A400M field lengths are not used here.
#[test]
#[ignore = "A400M test run: not part of the normal gate"]
fn a400m_field_performance_and_oei() {
    let design = a400m_design();
    for (label, surrogate) in [("4 x TP400-D6", false), ("twin surrogate", true)] {
        let mut config = a400m_config(20_000.0, 3_400.0);
        if surrogate {
            config = twin_surrogate(config);
        }
        let report = FullAnalysis::new(config.clone())
            .run(&design, true)
            .expect("full analysis");
        let s_ref = report.airplane.s_ref;
        println!(
            "=== field/OEI [{label}] S {s_ref:.2} m2, engines {} ===",
            config.geometry.engine.spanwise_positions_m.len()
        );
        match alas_pipeline::field_reference::report_isa_sea_level_field_reference(
            &config, &report, s_ref, MTOW_KG, MLW_KG,
        ) {
            Ok(r) => println!(
                "ISA SL reference: static thrust {:.0} N ({}), T/W {:.4}, takeoff field {:.0} m, model BFL {:.0} m, landing field {:.0} m, landing distance {:.0} m, VS1g land {:.1} m/s, VREF {:.1} m/s; OEI {:?}",
                r.static_thrust_n, r.static_thrust_basis, r.static_tw, r.takeoff_field_length_m,
                r.model_balanced_field_length_m, r.landing_field_length_m, r.landing_distance_m,
                r.vs1g_landing_m_s, r.vref_m_s, r.oei
            ),
            Err(error) => println!("ISA SL reference ERROR: {error}"),
        }
        for icao in ["LEMD", "KDEN"] {
            let airport = alas_config::airports::get(icao).expect("airport");
            let polar = alas_pipeline::field_performance::report_field_polar(
                &config, &report, 0.2, airport,
            );
            let Ok((cd0, k)) = polar else {
                println!("{icao}: polar error {polar:?}");
                continue;
            };
            match alas_pipeline::field_performance::calculate(
                &config, airport, s_ref, MTOW_KG, MLW_KG, 0.0, cd0, k,
            ) {
                Ok(f) => println!(
                    "{icao} (elev {:.0} m, ISA{:+.0}): polar CD0 {cd0:.4} k {k:.4}; TODR {:.0} m, BFL {:.0} m, ASD {:.0} m, LDR {:.0} m; V1/VR/V2 {:?}",
                    airport.elevation_m, airport.isa_deviation_c, f.todr_m, f.bfl_m, f.asd_m, f.ldr_m, f.v_speeds
                ),
                Err(error) => println!("{icao}: field ERROR {error}"),
            }
        }
    }
}
