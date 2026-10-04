// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! TEST RUN of the Airbus A400M Atlas on the ALAS model. The A400M is not a
//! registered preset; this builds a configuration from the ATR72-600 preset
//! (closest registered high-wing T-tail turboprop with sponson main gear) and
//! overrides geometry, masses, engine, fuel and requirements with the sourced
//! A400M values of `.agent/research/a400m-data.md` (EASA TCDS A.169, E.033,
//! P.012, Airbus brochure TMMA0026/01/2025, Bundeswehr operator page).
//!
//! Every input that is NOT sourced is marked ESTIMATE in a comment. Nothing
//! here is a calibration or a physical validation of the A400M.
//!
//! Ignored by default so the normal gate is unaffected. Run with
//! `cargo test --release -p alas-acceptance --test a400m_test_run -- --ignored --nocapture`.

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)
)]

use alas_config::design_variables::DesignVector;
use alas_config::{
    AlasConfig, CenterTankConfig, DesignMode, FeedTankConfig, MtowSizing, WingTankConfig,
};
use alas_pipeline::FullAnalysis;

const MTOW_KG: f64 = 141_000.0; // B1 p024, G1 (military)
const MLW_KG: f64 = 123_000.0; // B1 p024
const PUBLISHED_OEW_KG: f64 = 78_600.0; // G1 (MEW vs OEW not stated)
const FUEL_DENSITY_KG_M3: f64 = 785.0; // T1 III.9 (EASA TCDS A.169)
const FUSELAGE_LENGTH_M: f64 = 45.091; // T1 overall length

/// Range-payload points of B1 p024: (payload kg, range nmi, label).
const RANGE_PAYLOAD: [(f64, f64, &str); 3] = [
    (37_000.0, 1_780.0, "37 t / 3300 km"),
    (30_000.0, 2_400.0, "30 t / 4450 km"),
    (20_000.0, 3_400.0, "20 t / 6300 km"),
];

/// The A400M design vector. Wing: single trapezoid (no kink) with the
/// inferred taper 0.33 reproducing the TCDS MAC 5.671 m at S = 221.5 m2
/// (research note Item 1). All wing chords and sweep are INFERENCE/ESTIMATE.
fn a400m_design() -> DesignVector {
    let span_m = 42.357; // T1 III.4 (certified)
    let root_chord_m = 7.86; // INFERRED from S, b and MAC (not published)
    let tip_chord_m = 2.60; // INFERRED
    let break_fraction = 0.32; // ESTIMATE: ALAS needs a break station; the planform has none
    let break_chord_m = root_chord_m + (tip_chord_m - root_chord_m) * break_fraction;
    DesignVector {
        span_m,
        root_chord_m,
        break_chord_m,
        tip_chord_m,
        // Leading-edge sweep INFERRED from 15 deg quarter-chord sweep
        // (secondary source S1) and taper 0.33: 18.3 deg.
        sweep_deg: 18.3,
        tip_twist_deg: -2.0, // ESTIMATE, unpublished
        wing_x_shift_m: 0.0,
        tail_scale: 1.0,
        fuselage_length_m: FUSELAGE_LENGTH_M,
        tail_x_shift_m: 0.0,
        airfoil_thickness_scale: 1.0,
        airfoil_camber_scale: 1.0,
        ..DesignVector::default()
    }
}

fn wing_cell(start: f64, end: f64, burn_priority: i64, published_l: f64) -> WingTankConfig {
    WingTankConfig {
        enabled: true,
        span_start_fraction: start,
        span_end_fraction: end,
        usable_fraction: 0.92,
        burn_priority,
        published_usable_volume_l: Some(published_l),
        feed: FeedTankConfig::default(),
    }
}

/// The A400M configuration, started from the ATR72-600 preset. `payload_kg`
/// is the cargo mass; `design_range_nmi` feeds the mission-sized closure.
fn a400m_config(payload_kg: f64, design_range_nmi: f64) -> AlasConfig {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "ATR72-600" }))
        .expect("ATR72-600 preset loads");
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    // The ATR seed leaks through the preset NAME: with `preset == "ATR72-600"`
    // the FLOPS landing mass defaulted to the ATR's declared MLW of 22 350 kg
    // (gear 1.16 t, 0.8 % of MTOW; 5.5 t once corrected). Clear the name once
    // everything wanted from the seed is loaded, and declare the design
    // masses explicitly (MTOW/MLW from B1 p024, military).
    config.preset = String::new();
    config.mass_model.flops_structure.design_gross_mass_kg = Some(MTOW_KG);
    config.mass_model.flops_structure.design_landing_mass_kg = Some(MLW_KG);
    // Evaluate the aircraft as itself (declared MTOW), not as a clean-sheet
    // closure (see memory: alas-mass-sizing-basis).
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    config.optimizer.objective.mtow_sizing = MtowSizing::FixedRequirement;
    config.optimizer.objective.design_range_nmi = design_range_nmi;

    // ---- requirements --------------------------------------------------
    let req = &mut config.requirements;
    req.cruise_mach = 0.72; // B1 p012
    req.cruise_altitude_m = 11_278.0; // B1 p012: M0.72 at 37 000 ft
    req.mtow_kg = MTOW_KG;
    req.aircraft_type = "cargo".to_owned(); // military cargo hold, not a passenger cabin
    req.cabin_preset = "Custom".to_owned();
    req.num_passengers = 0;
    req.optimize_passenger_capacity = false;
    req.cargo_payload_kg = payload_kg;
    req.max_structural_payload_kg = 37_000.0; // B1 p024
    req.max_wing_area_m2 = 230.0; // ESTIMATE: above 221.5 m2
    req.min_wing_loading_kg_m2 = 300.0; // ESTIMATE: loose floor (MTOW/S = 637)
    req.dive_speed_m_s = 170.0; // ESTIMATE: VMO 154.3 m/s IAS (T1), VD not published

    // ---- cargo hold ------------------------------------------------------
    // The ATR seed carries a bulk-only lower hold with the ATR's two baggage
    // compartments; the A400M has neither. Reset to the generic cargo deck
    // (main-deck PMC pallets, LD3 lower holds). ESTIMATE: the real hold is
    // 17.70 m x 4.00 m x 3.85 m (B1), loaded with 463L pallets, vehicles or
    // paratroops, none of which the pallet loader represents.
    config.cabin = alas_config::CabinConfig::default();

    // ---- mission profile (ESTIMATES) -------------------------------------
    // The ATR seed profile is a 140 m/s cruise at 6096 m. Replaced by an
    // A400M-like schedule: cruise M0.72 at 11 278 m (TAS 212.4 m/s, ISA);
    // climb 155 kt CAS then 220 kt CAS and 2000/800 ft/min (EUROCONTROL
    // BADA-style ATC values, secondary S4, low confidence); takeoff speed
    // chosen above the 1.0 g stall at the model CL limit of 2.44 at 141 t
    // (64.6 m/s); landing speed Vat 130 kt (S4).
    {
        let p = &mut config.mission.profile;
        p.takeoff_air_speed_m_s = 74.0;
        p.takeoff_climb_rate_m_s = 8.0;
        p.initial_climb_air_speed_m_s = 95.0; // 79.7 m/s (155 kt, S4) breaches the model climb CL limit 1.5 at 134 t
        p.initial_climb_rate_m_s = 10.2;
        p.step_climb_1_air_speed_m_s = 113.0;
        p.step_climb_1_rate_m_s = 6.0;
        p.step_climb_2_air_speed_m_s = 113.0;
        p.step_climb_2_rate_m_s = 4.1;
        p.cruise_1_air_speed_m_s = 212.4;
        p.cruise_2_air_speed_m_s = 212.4;
        p.cruise_3_air_speed_m_s = 212.4;
        p.descent_1_air_speed_m_s = 125.0;
        p.descent_1_rate_m_s = 10.0;
        p.descent_2_air_speed_m_s = 110.0;
        p.descent_2_rate_m_s = 8.0;
        p.descent_3_air_speed_m_s = 95.0;
        p.descent_3_rate_m_s = 6.0;
        p.descent_4_air_speed_m_s = 80.0;
        p.descent_4_rate_m_s = 4.5;
        p.landing_air_speed_m_s = 72.0;
    }

    // ---- engine: 4 x TP400-D6 -------------------------------------------
    let engine = &mut config.geometry.engine;
    engine.engine_name = "TP400-D6".to_owned();
    engine.turboprop = None;
    engine.apply_engine_spec();
    // ESTIMATE: nacelle silhouette is the ATR one stretched to 5.5 m, radius
    // 1.1 m; TCDS E.033 gives engine length 4.180 m and radius 1.218 m only.
    engine.nacelle_profile = vec![
        (0.0, 0.35),
        (0.64, 0.9),
        (1.47, 1.0),
        (4.4, 0.8),
        (5.5, 0.35),
    ];
    engine.radius_scale_m = 1.1;
    // ESTIMATE: engine stations are not published. Inner/outer at 0.285 and
    // 0.66 semispan (21.18 m). Inner prop disc edge at 6.0 - 2.667 = 3.33 m
    // clears the 2.8 m fuselage radius.
    engine.spanwise_positions_m = vec![14.0, 6.0, -6.0, -14.0];
    engine.z_m = -1.0; // ESTIMATE
    engine.inlet_x_offset_m = 4.5; // ESTIMATE: spinner/inlet ahead of LE

    // ---- wing geometry ---------------------------------------------------
    let wing = &mut config.geometry.wing;
    // ESTIMATE: LEMAC (hence wing placement) is unpublished (WBM not public).
    // Chosen so the main gear (19.5 m) sits about 1.1 m behind a 30 %MAC CG:
    // LEMAC 17.4 m; MAC-LE is 2.9 m aft of the root LE (trapezoid y_mac*tan(18.3)).
    wing.root_datum_x_m = 14.5;
    // ESTIMATE: high wing; root above the 2.8 m fuselage crown. Flat wing
    // (dihedral not published, 0 deg used).
    wing.root_z_m = 3.3;
    wing.break_z_m = 3.3;
    wing.tip_z_m = 3.3;
    wing.root_twist_deg = 2.0; // ESTIMATE
    wing.break_twist_deg = 0.6; // ESTIMATE (linear root to tip)
    wing.break_span_fraction = 0.32;
    wing.side_of_body_chord_ratio = None;
    wing.outboard_sweep_decrement_deg = 0.0;
    // ESTIMATE: airfoils are Airbus proprietary. Supercritical sections
    // (SC(2)-0714 root, SC(2)-0410 tip) as a generic M0.72 stand-in.
    wing.root_airfoil = "SC2-0714".to_owned();
    wing.tip_airfoil = "sc20410".to_owned();
    wing.airfoil_class = alas_config::AirfoilClass::Supercritical;

    // ---- empennage (T-tail) ---------------------------------------------
    // Sourced: HT span 19.03 m, sweep 32.5 deg, VT height 8.02 m (secondary
    // S1, design-stage 2004). Everything else ESTIMATE (areas unpublished).
    let tail = &mut config.geometry.empennage;
    tail.tail_airfoil = "naca0012".to_owned();
    tail.hstab_offset_from_tail_m = FUSELAGE_LENGTH_M - 36.5; // ESTIMATE
    tail.hstab_z_m = 2.0 + 8.02; // fin root 2.0 m (ESTIMATE) + 8.02 m fin height (S1)
    tail.hstab_root_chord_m = 4.4; // ESTIMATE
    tail.hstab_tip_chord_m = 1.9; // ESTIMATE; HT area 59.9 m2
    tail.hstab_root_twist_deg = 0.0;
    tail.hstab_tip_twist_deg = 0.0;
    // half-span 9.515 m (S1); LE sweep ESTIMATE 34 deg (32.5 deg reference
    // line not stated).
    tail.hstab_tip_le_m = (6.42, 9.515, 0.0);
    tail.vstab_offset_from_tail_m = FUSELAGE_LENGTH_M - 30.9; // ESTIMATE
    tail.vstab_z_m = 2.0; // ESTIMATE
    tail.vstab_root_chord_m = 8.0; // ESTIMATE
    tail.vstab_tip_chord_m = 4.4; // ESTIMATE, equals the HT root chord (T-tail join)
    tail.vstab_tip_le_m = (5.6, 0.0, 8.02); // height from S1; sweep 35 deg ESTIMATE

    // ---- fuselage --------------------------------------------------------
    let fuselage = &mut config.geometry.fuselage;
    fuselage.diameter_m = 5.6; // T1 "Width", read as fuselage max width
    fuselage.height_m = None; // not published: circular section assumed (ESTIMATE)
    fuselage.cabin_start_x_m = 8.0; // ESTIMATE: hold starts behind the flight deck
    fuselage.tailcone_length_m = 14.0; // ESTIMATE: hold + 5.4 m ramp (23.1 m) ends at 31.1 m
    fuselage.tail_z_m = 3.0; // ESTIMATE: strong aft upsweep for the ramp
                             // belly upsweep length: no tail-strike angle is published; left unset.
    fuselage.belly_upsweep_length_m = None;

    // ---- landing gear ----------------------------------------------------
    // T1 III.21: nose 2 wheels; main 12 wheels; secondary source: 3 twin-wheel
    // legs per side in sponsons. Stations/track are ESTIMATES: wheelbase 13.4 m
    // is a very-low-confidence secondary figure, nose gear 6.1 m aft of the
    // nose is a guess, track 6.0 m is a guess (secondary 7.9 m is probably
    // the outer tyre width).
    let gear = &mut config.landing_gear;
    gear.n_nlg_wheels = 2;
    gear.n_mlg_struts = 6;
    gear.wheels_per_mlg_strut = 2;
    gear.track_diameter_factor = 6.0 / 5.6;
    gear.reference_wheelbase_m = Some(13.4);
    gear.reference_track_m = Some(6.0);
    gear.reference_station_frame = Some("nose_tip_drawing_reference".to_owned());
    gear.reference_station_fuselage_length_m = Some(FUSELAGE_LENGTH_M);
    gear.reference_nlg_x_fraction = Some(6.1 / FUSELAGE_LENGTH_M);
    let (x_mid, x_spread) = (6.1 + 13.4, 1.2); // ESTIMATE: leg spacing 1.2 m
    gear.reference_mlg_x_fractions = Some(
        [x_mid - x_spread, x_mid, x_mid + x_spread]
            .iter()
            .cycle()
            .take(6)
            .map(|x| x / FUSELAGE_LENGTH_M)
            .collect(),
    );
    // Trimmable horizontal stabiliser: class takeoff setting (4.3 deg).
    gear.takeoff_stabilizer_nose_up_deg = Some(4.3);
    gear.elevator_up_travel_deg = None; // not published

    // ---- fuel tanks (T1 III.9, normal fill, litres) ------------------------
    // centre 14 566; inner L+R 17 143 + 17 050 = 34 193; feed tanks 1-4 sum
    // 7 726 + 5 782 = 13 508 (modelled inside the inner cell). Total 62 267 L.
    // Semispan stations are ESTIMATES.
    let tanks = &mut config.fuel_tanks;
    tanks.inner_wing = wing_cell(0.10, 0.85, 2, 34_193.0 + 13_508.0);
    tanks.mid_wing.enabled = false;
    tanks.outer_wing.enabled = false;
    tanks.center = CenterTankConfig {
        enabled: true,
        usable_fraction: 0.80,
        burn_priority: 1,
        published_usable_volume_l: Some(14_566.0),
    };
    tanks.trim.enabled = false;
    tanks.auxiliary.enabled = false;

    // ---- mass model ------------------------------------------------------
    let mass = &mut config.mass_model;
    mass.fuel_density_kg_m3 = FUEL_DENSITY_KG_M3;
    mass.mlw_fraction_mtow = MLW_KG / MTOW_KG;

    let turboprop = &mut mass.flops_turboprop;
    turboprop.engine_dry_mass_kg = Some(1_952.0); // T2: CW 1938.1 / CCW 1965.1 kg mean
    turboprop.baseline_shaft_power_kw = Some(7_971.0); // T2
    turboprop.gearbox_inside_engine_mass = true; // ESTIMATE: PGB listed in the engine TCDS
    turboprop.propeller_blade_count = 8; // T3
    turboprop.propeller_assembly_mass_kg = Some(683.0); // T3 max
    turboprop.propeller_assembly_accessories_included = None;
    turboprop.propeller_accessory_mass_kg = 0.0;
    // ESTIMATES: nacelle mass/area anchor (ATR 19.7 kg/m2 area density, kept),
    // installation 0.32 x engine dry mass (ATR ratio), oil 150 kg for 4 engines.
    turboprop.nacelle_reference_mass_kg = None;
    turboprop.nacelle_reference_area_m2 = None;
    turboprop.nacelle_area_density_kg_m2 = 19.7;
    turboprop.engine_installation_mass_kg = 0.32 * 4.0 * 1_952.0;
    turboprop.engine_oil_mass_kg = 150.0;

    let structure = &mut mass.flops_structure;
    structure.military_cargo_floor = 1.0; // military cargo floor (FLOPS CARGF)
    structure.composite_utilization = 0.3; // ESTIMATE: CFRP wing skins (S1)
    structure.maximum_operating_altitude_m = Some(10_668.0); // T1 civil 35 000 ft
    structure.design_zero_fuel_mass_kg = Some(109_600.0); // T1 civil MZFW
    structure.baseline_engine_mass_kg = None;

    let transport = &mut mass.flops_transport;
    transport.maximum_mach = Some(0.72); // T1 MMO
    transport.design_range_nmi = Some(design_range_nmi);
    transport.flight_crew_count = Some(3); // ESTIMATE: 2 pilots + loadmaster
    transport.flight_attendant_count = Some(0);
    transport.galley_crew_count = Some(0);
    transport.first_class_passenger_count = Some(0);
    transport.business_class_passenger_count = Some(0);
    transport.tourist_class_passenger_count = Some(0);
    transport.wing_mounted_engine_count = Some(4);
    transport.fuselage_mounted_engine_count = Some(0);
    transport.fuel_tank_count = Some(7); // T1: centre, 2 inner, 4 feed
    transport.maximum_fuel_capacity_kg = Some(48_879.0); // T1 normal fill
    transport.apu_installed = true; // ESTIMATE
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
