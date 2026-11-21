// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The analyzed-takeoff rotation (nose-wheel liftoff) boundary against the
//! manufacturers' published most-forward CG, on the product basis: the
//! registered preset's full analysis and the item-ledger CG gate the
//! feasibility report uses.
//!
//! A published forward limit is the worst of several criteria (rotation,
//! trim, handling, gear loads; FAA AC 25-7D, section 42.11), so the rotation
//! boundary alone may lie forward of it but must not lie aft of it by more
//! than [`TOLERANCE_PCT_MAC`]: an inequality, not an agreement band.
//!
//! Anchor provenance. The A220-300 value is a flight envelope limit (ARP
//! p. 119). The A320-200 17 %MAC, the A340-300 20.3 %MAC and the A380-800
//! 34.65-37.8 %MAC are the ACAP most-forward CG used in the pavement-load
//! analysis at MRW (Aircraft Characteristics section 7-3), not certified
//! limits. The A380 is not an anchor at all. No published forward limit
//! was retrieved for the DC-10.
//!
//! The tail lift at rotation is derived from the tail geometry with the
//! plain-flap large-deflection correction and the tail-section lift cap, and
//! the pitch inertia is transferred to the main-gear contact. A trimmable
//! stabiliser is taken at its nose-up stop with full up-elevator, the tail at
//! its maximum downlift within the section cap (Roskam's take-off rotation
//! check); the V2 trim balance (`envelope::ths_trim`, compared with the Airbus
//! Safety First #09 A330 points) is the separate take-off trim limit.
//! The tail-section stall limit is sourced (NASA TM-4074 NACA 0012 data, the
//! Roskam VI plain-flap increment, Raymer eq. 12.21; `envelope::tail_stall`)
//! and no longer binds on any registered preset. The A320 and A220 lie far
//! forward of the published values (those limits are set by criteria other
//! than rotation).

use alas_config::AlasConfig;
use alas_pipeline::FullAnalysis;

/// Largest amount, %MAC, by which the rotation boundary may sit aft of a
/// published most-forward CG.
///
/// Unchanged at 1.0 %MAC.
const TOLERANCE_PCT_MAC: f64 = 1.0;

/// The analyzed-takeoff rotation boundary of `preset`, converted to the
/// manufacturer's MAC frame (LEMAC m aft of the nose tip, MAC m).
fn rotation_limit_in_manufacturer_frame(preset: &str, lemac_m: f64, mac_m: f64) -> f64 {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    let design = alas_config::presets::get(preset)
        .unwrap_or_else(|error| panic!("{preset}: {error}"))
        .design_vector;
    let report = FullAnalysis::new(config.clone())
        .run(&design, true)
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    let feasibility = alas_pipeline::assess_physical_feasibility(&config, &design, &report, None);
    let model_cg = feasibility
        .model_cg
        .as_ref()
        .unwrap_or_else(|| panic!("{preset}: no model CG assessment"));
    let rotation_pct_mac = model_cg
        .loading_states
        .iter()
        .find(|state| state.state == alas_opt::ModelCgLoadingState::AnalyzedTakeoff)
        .map(|state| state.physical_limits.rotation_fwd_pct_mac)
        .unwrap_or_else(|| panic!("{preset}: no analyzed takeoff state"));
    let frame = report
        .airplane
        .mac_frame()
        .unwrap_or_else(|| panic!("{preset}: no MAC frame"));
    (frame.x_at_pct(rotation_pct_mac) - lemac_m) / mac_m * 100.0
}

/// - A320-200: 17 %MAC, the ACAP most-forward CG used in the pavement-load
///   analysis at MRW, not a certified limit (Airbus A320 Aircraft
///   Characteristics, Fig. 7-3-0-991-010); LEMAC 15.26 m, MAC 4.1935 m
///   (EASA TCDS A.064).
/// - A220-300: 12.0 %MAC, the most-forward flight limit (Airbus A220-300
///   Aircraft Recovery Publication BD500-3AB48-10400-00, p. 119); LEMAC
///   16.535 m, MAC 3.781 m.
#[test]
fn the_rotation_limit_is_not_aft_of_published_forward_limits() {
    for (preset, lemac_m, mac_m, published) in [
        ("A320-200", 15.26, 4.1935, 17.0),
        ("A220-300", 16.535, 3.781, 12.0),
    ] {
        let rotation = rotation_limit_in_manufacturer_frame(preset, lemac_m, mac_m);
        assert!(
            rotation <= published + TOLERANCE_PCT_MAC,
            "{preset}: rotation limit {rotation:.2} %MAC is aft of the published {published} + {TOLERANCE_PCT_MAC}"
        );
    }
}

/// The same inequality for the A340-300: 20.3 %MAC, the ACAP most-forward CG
/// used in the pavement-load analysis at the WV025/WV029 ramp weight (Airbus
/// A340-200/-300 Aircraft Characteristics, Dec 2025, Fig. 7-3-0-991-007), not
/// a certified limit; LEMAC 28.083 m from that section's two-point statics,
/// MAC 7.270 m (EASA TCDS).
///
/// The boundary rests on the tail-section stall cap and on the pitch radius
/// of gyration of the item ledger (the rotation offset solves
/// `kappa d^2 + d = r` with `r = available - kappa (k_y^2 + h_cg^2)`); 1 % of
/// the cap moves it by 0.19 to 0.38 %MAC. The same tolerance applies as for
/// the other aircraft: no allowance is added for this one.
#[test]
fn the_a340_300_rotation_limit_is_not_aft_of_its_published_forward_limit() {
    let rotation = rotation_limit_in_manufacturer_frame("A340-300", 28.083, 7.270);
    assert!(
        rotation <= 20.3 + TOLERANCE_PCT_MAC,
        "A340-300: rotation limit {rotation:.2} %MAC is aft of the published 20.3 + {TOLERANCE_PCT_MAC}"
    );
}

/// The registered base preset and the settings the probe analyses for
/// `name`. The acceptance matrix's three freighters are not registered
/// presets: each is its passenger preset converted by the cargo requirements
/// (no passengers, a containerised payload at the freighter's structural
/// payload, its MTOW, MLW share, design range and flight crew), so the probe
/// builds the same settings. The freighter's MLW is also stated as the
/// requirement (`requirements.mlw_kg`), which governs in every design mode;
/// the fraction alone would give way to the passenger preset's declared MLW
/// in a fixed-aircraft mode (`AlasConfig::landing_mass_limit_kg`). Every
/// other name is a registered preset as shipped.
fn probe_configuration(name: &str) -> (&str, serde_json::Value) {
    let (base, mtow_kg, payload_kg, mlw_fraction, range_nmi, crew) = match name {
        "A320P2F" => ("A320-200", 78_000.0, 21_400.0, 0.84615, 2_052.0, 2),
        "B747-400F" => ("B747-400", 396_893.0, 111_605.0, 0.74514, 4_444.0, 2),
        "DC-10-30F" => ("DC-10", 259_450.0, 73_935.0, 0.73602, 3_300.0, 3),
        registered => return (registered, serde_json::json!({ "preset": registered })),
    };
    let settings = serde_json::json!({
        "preset": base,
        "requirements": {
            "mtow_kg": mtow_kg,
            "mlw_kg": mlw_fraction * mtow_kg,
            "aircraft_type": "cargo",
            "cabin_preset": "Custom",
            "num_passengers": 0,
            "cargo_payload_kg": payload_kg,
            "max_structural_payload_kg": payload_kg,
        },
        "mass_model": {
            "mlw_fraction_mtow": mlw_fraction,
            "flops_transport": {
                "design_range_nmi": range_nmi,
                "flight_crew_count": crew,
                "flight_attendant_count": 0,
                "first_class_passenger_count": 0,
                "business_class_passenger_count": 0,
                "tourist_class_passenger_count": 0,
                "containerized_cargo_kg": payload_kg,
                "cargo_loading": "containerized",
                "cabin_equipment_method": "flops_transport_v1",
            },
        },
        "cabin": { "cargo": { "lower_deck_uld": "AUTO" } },
    });
    (base, settings)
}

/// Probe: the physical forward and aft boundaries of the analysed take-off
/// state of the registered presets, %MAC in the model frame, next to the
/// governing mechanism. The published forward limits above are the only anchors
/// found; this prints the rest so the type dependence of the usable range is
/// visible.
#[test]
#[ignore = "diagnostic; prints"]
// A probe prints its numbers; it asserts nothing.
#[allow(clippy::print_stderr)]
fn physical_boundaries_probe() {
    let only = std::env::var("ALAS_PROBE_PRESETS").ok();
    for preset in [
        "A220-300",
        "A320-200",
        "A320P2F",
        "A340-300",
        "A350-900",
        "A380-800",
        "A400M",
        "ATR72-600",
        "AVE",
        "B747-400",
        "B747-400F",
        "B787-9",
        "C295",
        "C919",
        "DC-10",
        "DC-10-30F",
        "E195-E2",
        "LJ60",
    ] {
        if only
            .as_deref()
            .is_some_and(|list| !list.split(',').any(|p| p == preset))
        {
            continue;
        }
        let (base, settings) = probe_configuration(preset);
        let config = match AlasConfig::from_value(&settings) {
            Ok(config) => config,
            Err(error) => {
                eprintln!("BOUNDARY-SKIP {preset}: configuration rejected: {error}");
                continue;
            }
        };
        let design = match alas_config::presets::get(base) {
            Ok(design) => design,
            Err(error) => {
                eprintln!("BOUNDARY-SKIP {preset}: no registered preset {base}: {error}");
                continue;
            }
        };
        let report = match FullAnalysis::new(config.clone()).run(&design.design_vector, true) {
            Ok(report) => report,
            Err(error) => {
                eprintln!("BOUNDARY-SKIP {preset}: {error}");
                continue;
            }
        };
        let feasibility = alas_pipeline::assess_physical_feasibility(
            &config,
            &design.design_vector,
            &report,
            None,
        );
        let Some(model_cg) = feasibility.model_cg.as_ref() else {
            let causes: Vec<String> = feasibility
                .findings
                .iter()
                .filter(|finding| format!("{:?}", finding.code).contains("Cg"))
                .take(3)
                .map(|finding| format!("{:?}: {}", finding.code, finding.message))
                .collect();
            eprintln!("BOUNDARY-NOCG {preset}: no model CG assessment; {causes:?}");
            continue;
        };
        for state in &model_cg.loading_states {
            let limits = &state.physical_limits;
            let f_max = config.mass_model.pct_load_nlg_max_handling;
            let wheelbase_pct =
                (model_cg.main_gear_station_pct_mac - limits.max_nose_load_fwd_pct_mac) / f_max;
            eprintln!(
                "CAND {preset} {:?} m={:.0}: rotation {:.1} v2trim {:.1} clean {:.1} landing {:.1} [{}] (approach {:.1} flare {:.1}) nose15 {:.1} nose{:.0} {:.1}; stop {:?}; iced-tail {:.1} design-rule {:.1}",
                state.state,
                state.mass_kg,
                limits.rotation_fwd_pct_mac,
                limits.takeoff_trim_fwd_pct_mac,
                limits.clean_trim_fwd_pct_mac,
                limits.landing_trim_fwd_pct_mac,
                limits.landing_trim_condition,
                limits.landing_approach_fwd_pct_mac,
                limits.landing_flare_fwd_pct_mac,
                model_cg.main_gear_station_pct_mac - 0.15 * wheelbase_pct,
                100.0 * f_max,
                limits.max_nose_load_fwd_pct_mac,
                config.landing_gear.stabilizer_nose_up_travel_deg,
                limits.iced_tail_fwd_pct_mac,
                limits.scissor_plot_fwd_pct_mac,
            );
            eprintln!(
                "BOUNDARY {preset} {:?} m={:.0} cg={:.1} nlg={:.4} envfwd={:.1} gndaft={:.1} aero_aft={:.1} worst_aft={:.1}: fwd {:.1} ({:?}) rotation {:.1} (stabiliser {:?} deg nose-up) max-nose-load {:.1} landing-trim {:.1}; aft {:.1} ({:?}); usable {:.1} %MAC",
                state.state,
                state.mass_kg,
                state.cg_pct_mac,
                state.nose_gear_load_fraction,
                model_cg.configured_forward_limit_pct_mac,
                model_cg.ground_aft_limit_pct_mac,
                model_cg.aerodynamic_aft_limit_pct_mac,
                model_cg.worst_aft_limit_pct_mac,
                limits.fwd_limit_pct_mac,
                limits.fwd_limit_governance,
                limits.rotation_fwd_pct_mac,
                limits.rotation_stabilizer_nose_up_deg,
                limits.max_nose_load_fwd_pct_mac,
                limits.landing_trim_fwd_pct_mac,
                limits.aft_limit_pct_mac,
                limits.aft_limit_governance,
                limits.usable_range_pct_mac,
            );
        }
    }
}
