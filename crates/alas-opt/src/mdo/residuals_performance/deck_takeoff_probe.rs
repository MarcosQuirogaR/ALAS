// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Diagnostic table of the take-off thrust ratio at V2 and of the take-off
//! thrust checks of every registered jet (lane fix-deck-takeoff). Ignored
//! test; it prints one `DECKTO` line per jet.
//!
//! `cargo test -p alas-opt --release deck_takeoff_table -- --ignored --nocapture`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

use super::improved_climb::{resolve_improved_climb, ImprovedClimbInputs};
use super::oei_deck::{EngineOutCondition, EngineOutModel, EngineOutRating, YawAirframe};
use crate::mdo::residuals_vmc::{evaluate_candidate, performance_with_vmc};
use crate::mdo::sizing::run_candidate;
use alas_config::{ActiveEngineModel, AlasConfig};
use alas_perf::performance::compute_v_speeds_at_masses;

const G0: f64 = 9.806_65;

/// `cargo test -p alas-opt --release deck_takeoff_table -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic table of all registered jets; slow"]
fn deck_takeoff_table() {
    for registered in alas_config::presets::registry() {
        let config =
            AlasConfig::from_value(&serde_json::json!({"preset": registered.name})).unwrap();
        let Ok(ActiveEngineModel::Turbofan(spec)) = config.geometry.engine.active_model() else {
            continue;
        };
        let design = alas_config::presets::get(registered.name)
            .unwrap()
            .design_vector;
        let outcome = run_candidate(&config, &design.to_array()).unwrap();
        let assessment = crate::assess_product_candidate(&config, &design).unwrap();
        let s_ref = outcome.plane.s_ref;
        let tow = outcome.sized.takeoff_mass_kg;
        let departure = outcome.departure.unwrap();
        // The flown V2: the production path (VMC-resolved performance, the
        // improved-climb selection that actually meets the gradient).
        let vmc = evaluate_candidate(&outcome, &config);
        let resolved = performance_with_vmc(&config, &vmc);
        let model = EngineOutModel::new(
            &resolved,
            Some(YawAirframe {
                plane: &outcome.plane,
                cg_x_m: outcome.cg_x,
            }),
            s_ref,
            Some(outcome.static_thrust_kn * 1_000.0),
        )
        .unwrap()
        .with_weight_kg(tow);
        let installed_tw =
            outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0 / (tow * G0);
        let (flown, selection) = resolve_improved_climb(
            &resolved,
            &ImprovedClimbInputs {
                model: &model,
                departure,
                mass_kg: tow,
                s_ref_m2: s_ref,
                drag: &outcome.sized.fuel_artifacts.drag,
                installed_tw_sls: installed_tw,
            },
        );
        let perf = &flown.performance;
        let speeds = compute_v_speeds_at_masses(
            tow,
            tow,
            s_ref,
            departure,
            perf.cl_max_to,
            perf.cl_max_land,
            perf,
        );
        let v2_over_vsr = speeds.v2_ms / speeds.v_stall_to_ms;
        let v2 = speeds.v2_ms;
        let field_factor = perf.improved_climb_field_factor();
        let improvement = selection.map_or_else(
            || "n/a".to_owned(),
            |sel| {
                format!(
                    "improved {} meets {} best-climb V2/VSR {:.3}",
                    sel.improved, sel.meets, sel.v2_over_vsr
                )
            },
        );
        let (_, r_v2) = model
            .thrust(
                EngineOutCondition {
                    altitude_m: departure.elevation_m,
                    isa_deviation_c: departure.isa_deviation_c,
                    tas_m_s: v2,
                },
                EngineOutRating::TakeoffGoAround,
                true,
            )
            .unwrap();
        // Indicative deck ground roll (all engines, dry concrete mu 0.02,
        // CL on the ground 0.1) to the lift-off speed 1.15 VSR: not the
        // certified field length (a static-T/W correlation).
        let atmosphere =
            alas_atmo::us1976_compute_values(departure.elevation_m, departure.isa_deviation_c);
        let drag = &outcome.sized.fuel_artifacts.drag;
        let v_lof = 1.15 * speeds.v_stall_to_ms;
        let weight_n = tow * G0;
        let n = 400;
        let mut roll_m = 0.0;
        for i in 0..n {
            let v = (i as f64 + 0.5) / n as f64 * v_lof;
            let (thrust_per_engine, _) = model
                .thrust(
                    EngineOutCondition {
                        altitude_m: departure.elevation_m,
                        isa_deviation_c: departure.isa_deviation_c,
                        tas_m_s: v,
                    },
                    EngineOutRating::TakeoffGoAround,
                    false,
                )
                .unwrap();
            let thrust = thrust_per_engine * outcome.n_engines as f64;
            let mach = v / atmosphere.speed_of_sound_m_s;
            let cl = 0.1;
            let q = 0.5 * atmosphere.density_kg_m3 * v * v;
            let cd = drag.cd_airport_base_at_atmosphere(
                cl,
                mach,
                departure.elevation_m,
                departure.isa_deviation_c,
            ) + perf.oei_climb_delta_cd
                + perf.takeoff_gear_delta_cd;
            let lift = cl * q * s_ref;
            let accel = G0 / weight_n * (thrust - cd * q * s_ref - 0.02 * (weight_n - lift));
            roll_m += v * (v_lof / n as f64) / accel.max(0.05);
        }
        let rows: Vec<String> = assessment
            .residuals
            .iter()
            .filter(|row| {
                matches!(
                    row.id,
                    "oei_second_segment"
                        | "oei_final_segment"
                        | "oei_approach_climb"
                        | "landing_climb"
                        | "takeoff_field"
                        | "oei_second_segment_within_takeoff_band"
                        | "oei_approach_climb_within_takeoff_band"
                        | "landing_climb_within_takeoff_band"
                )
            })
            .map(|row| {
                format!(
                    "{} req {:.4} inst {:.4} short {:+.1} %{}",
                    row.id,
                    row.limit,
                    row.actual,
                    100.0 * (1.0 - row.actual / row.limit),
                    if row.violated() { " VIOL" } else { "" }
                )
            })
            .collect();
        let (chart_r, bartel_young_r) =
            alas_prop::empirical_turbofan::takeoff_chart::takeoff_thrust_sources(
                spec.bypass_ratio,
                atmosphere.pressure_pa / 101_325.0,
                v2 / atmosphere.speed_of_sound_m_s,
            );
        eprintln!(
            "DECKTO {:10} BPR {:.1} V2/VSR {:.3} (field factor {field_factor:.3}; {improvement}) V2 {:.1} m/s M {:.3} r_V2 {:.4} (sources: Jenkinson {chart_r:.4}, Bartel-Young {bartel_young_r:.4}) roll(deck) {:.0} m | {} | hard {:?}",
            registered.name,
            spec.bypass_ratio,
            v2_over_vsr,
            v2,
            v2 / atmosphere.speed_of_sound_m_s,
            r_v2,
            roll_m,
            rows.join(" | "),
            assessment.violated_hard_ids(),
        );
    }
}

/// The take-off band at work: the AVE with the failed-engine windmilling drag
/// coefficient overridden upward (`performance.oei_windmilling_cd`, a
/// configuration knob; the derived value is 0.0039), the second-segment rows
/// and the verdict. Ignored; prints.
///
/// `cargo test -p alas-opt --release deck_takeoff_band_sweep -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic sweep; prints"]
fn deck_takeoff_band_sweep() {
    let name = std::env::var("ALAS_PROBE_PRESETS").unwrap_or_else(|_| "AVE".to_owned());
    let design = alas_config::presets::get(&name).unwrap().design_vector;
    for windmilling_cd in [0.0039, 0.012, 0.016, 0.020, 0.024, 0.028, 0.032, 0.036] {
        let mut config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
        config.performance.oei_windmilling_cd = Some(windmilling_cd);
        let assessment = match crate::assess_product_candidate(&config, &design) {
            Ok(assessment) => assessment,
            Err(error) => {
                eprintln!("BAND {name} windmilling {windmilling_cd:.4}: {error}");
                continue;
            }
        };
        let rows: Vec<String> = assessment
            .residuals
            .iter()
            .filter(|row| {
                row.id.starts_with("oei_second_segment") || row.id.contains("takeoff_band")
            })
            .map(|row| {
                format!(
                    "{} req {:.4} inst {:.4} viol {:.4}",
                    row.id, row.limit, row.actual, row.normalized_violation
                )
            })
            .collect();
        eprintln!(
            "BAND {name} windmilling cd {windmilling_cd:.4}: {} | hard {:?}",
            rows.join(" | "),
            assessment.violated_hard_ids()
        );
    }
}

/// The improved-climb selection of a preset at an unreachable installed T/W
/// (the schedule and best-climb requirements are then both reported).
///
/// `ALAS_PROBE_PRESETS=B787-9 cargo test -p alas-opt --release deck_improved_climb_terms -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic; prints"]
fn deck_improved_climb_terms() {
    let name = std::env::var("ALAS_PROBE_PRESETS").unwrap_or_else(|_| "B787-9".to_owned());
    let config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
    let design = alas_config::presets::get(&name).unwrap().design_vector;
    let outcome = run_candidate(&config, &design.to_array()).unwrap();
    let vmc = evaluate_candidate(&outcome, &config);
    let resolved = performance_with_vmc(&config, &vmc);
    let s_ref = outcome.plane.s_ref;
    let mass = outcome.sized.takeoff_mass_kg;
    let model = EngineOutModel::new(
        &resolved,
        Some(YawAirframe {
            plane: &outcome.plane,
            cg_x_m: outcome.cg_x,
        }),
        s_ref,
        Some(outcome.static_thrust_kn * 1_000.0),
    )
    .unwrap()
    .with_weight_kg(mass);
    let (_, selection) = resolve_improved_climb(
        &resolved,
        &ImprovedClimbInputs {
            model: &model,
            departure: outcome.departure.unwrap(),
            mass_kg: mass,
            s_ref_m2: s_ref,
            drag: &outcome.sized.fuel_artifacts.drag,
            installed_tw_sls: 0.20,
        },
    );
    eprintln!(
        "IMPROVED {name} mass {mass:.0} kg installed {:.4}: {:?}",
        outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0 / (mass * G0),
        selection
    );
}

/// Turboprop approach-climb breakdown (lane blockers B). Ignored; prints.
///
/// `cargo test -p alas-opt --release turboprop_climb_breakdown -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic; prints"]
fn turboprop_climb_breakdown() {
    use super::certification::{evaluate_climb_checks, CertificationInputs, ClimbOutcome};
    let names = std::env::var("ALAS_PROBE_PRESETS").unwrap_or_else(|_| "ATR72-600".to_owned());
    for name in names.split(',') {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let outcome = run_candidate(&config, &design.to_array()).unwrap();
        let s_ref = outcome.plane.s_ref;
        let tow = outcome.sized.takeoff_mass_kg;
        let model = EngineOutModel::new(
            &config,
            Some(YawAirframe {
                plane: &outcome.plane,
                cg_x_m: outcome.cg_x,
            }),
            s_ref,
            Some(outcome.static_thrust_kn * 1_000.0),
        )
        .unwrap()
        .with_weight_kg(tow);
        let inputs = CertificationInputs {
            config: &config,
            drag: &outcome.sized.fuel_artifacts.drag,
            engine_out: &model,
            s_ref_m2: s_ref,
            takeoff_mass_kg: tow,
            landing_mass_kg: outcome.sized.design_landing_mass_kg,
            departure: outcome.departure,
            arrival: outcome.arrival,
        };
        let installed_tw =
            outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0 / (tow * G0);
        eprintln!(
            "TPCLIMB {name} tow {tow:.0} mla {:.0} S {s_ref:.2} static_kN {:.2} installed_tw {installed_tw:.4} clmax_land {} flap_dcd {} gear_dcd {}",
            outcome.sized.design_landing_mass_kg,
            outcome.static_thrust_kn,
            config.performance.cl_max_land,
            config.performance.landing_flap_delta_cd,
            config.performance.landing_gear_delta_cd,
        );
        eprintln!(
            "TPCLIMB   clmax clean {} to {} land {} oei_climb_dcd {} to_gear {}",
            config.performance.cl_max_clean,
            config.performance.cl_max_to,
            config.performance.cl_max_land,
            config.performance.oei_climb_delta_cd,
            config.performance.takeoff_gear_delta_cd,
        );
        for (segment, result) in evaluate_climb_checks(&inputs) {
            match result {
                ClimbOutcome::Evaluated(e) => eprintln!(
                    "TPCLIMB   {} V {:.1} CL {:.3} CD {:.4} (wind {:.4} trim {:.4}) grad {:.3} r {:.4} req_tw {:.4} short {:+.1} %",
                    segment.id(), e.speed_m_s, e.cl, e.cd, e.windmilling_cd, e.asymmetric_trim_cd,
                    e.gradient, e.thrust_ratio, e.required_sls_tw,
                    100.0 * (1.0 - installed_tw / e.required_sls_tw),
                ),
                ClimbOutcome::EvidenceGap(why) => eprintln!("TPCLIMB   {} gap {why}", segment.id()),
            }
        }
    }
}

/// Turboprop cruise-point propeller breakdown (lane blockers B). Ignored.
///
/// `cargo test -p alas-opt --release turboprop_cruise_breakdown -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic; prints"]
fn turboprop_cruise_breakdown() {
    use alas_prop::turboprop::{Pw127mRating, TurbopropCommand, TurbopropCondition, TurbopropMode};
    let names = std::env::var("ALAS_PROBE_PRESETS").unwrap_or_else(|_| "ATR72-600".to_owned());
    for name in names.split(',') {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
        let Ok(ActiveEngineModel::Turboprop(spec)) = config.geometry.engine.active_model() else {
            continue;
        };
        let atmosphere = alas_atmo::Atmosphere::new(config.requirements.cruise_altitude_m);
        let tas = config.requirements.cruise_mach * atmosphere.speed_of_sound();
        let model = crate::mdo::propulsion::turboprop_unit_model(spec);
        let out = model
            .evaluate_at_temperature(
                TurbopropCondition {
                    density_kg_m3: atmosphere.density(),
                    true_airspeed_m_s: tas,
                },
                atmosphere.temperature(),
                TurbopropCommand {
                    rating: Pw127mRating::MaximumCruise,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();
        eprintln!(
            "TPCRUISE {name} engines {} alt {:.0} m M {} TAS {tas:.1} rho {:.4} spec: Pcruise {:.0} kW Pclimb {:.0} Pto {:.0} rpm {} D {} | model: shaft {:.0} kW prop {:.0} kW thrust {:.0} N eta {:.3} J {:.3} tipM {:.3} ff {:.4} kg/s",
            config.geometry.engine.spanwise_positions_m.len(),
            config.requirements.cruise_altitude_m,
            config.requirements.cruise_mach,
            atmosphere.density(),
            spec.maximum_cruise_shaft_power_kw,
            spec.maximum_climb_shaft_power_kw,
            spec.takeoff_shaft_power_kw,
            spec.governed_propeller_speed_rpm,
            spec.propeller_diameter_m,
            out.engine_shaft_power_w / 1e3,
            out.propeller_power_w / 1e3,
            out.total_thrust_n,
            out.propulsive_efficiency,
            out.advance_ratio,
            model.helical_tip_mach(tas, model.governed_propeller_speed_rpm, atmosphere.temperature()),
            out.fuel_flow_kg_s,
        );
    }
}

/// Turboprop nominal-design rows (lane blockers B). Ignored; prints.
///
/// `cargo test -p alas-opt --release turboprop_nominal_rows -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic; prints"]
fn turboprop_nominal_rows() {
    let names = std::env::var("ALAS_PROBE_PRESETS").unwrap_or_else(|_| "ATR72-600".to_owned());
    for name in names.split(',') {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let assessment = crate::assess_product_candidate(&config, &design).unwrap();
        for row in assessment.residuals.iter().filter(|row| {
            matches!(
                row.id,
                "cruise_thrust" | "toc_rate_of_climb" | "oei_approach_climb" | "oei_second_segment"
            )
        }) {
            eprintln!(
                "TPROW {name} {} req {:.4} act {:.4} viol {:.4} {:?} {}",
                row.id,
                row.limit,
                row.actual,
                row.normalized_violation,
                row.role,
                row.violated()
            );
        }
        eprintln!("TPROW {name} hard {:?}", assessment.violated_hard_ids());
    }
}

/// Mission-closed nominal rows of a turboprop (lane blockers B). Ignored.
///
/// `cargo test -p alas-opt --release turboprop_mission_closed_rows -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic; prints"]
fn turboprop_mission_closed_rows() {
    let names = std::env::var("ALAS_PROBE_PRESETS").unwrap_or_else(|_| "ATR72-600".to_owned());
    for name in names.split(',') {
        for mode in ["mtow_band", "payload_adjusted"] {
            let config = AlasConfig::from_value(&serde_json::json!({
                "preset": name,
                "optimizer": {"design_space": {"mode": "reference_adaptation"},
                              "objective": {"mtow_sizing": mode}}
            }))
            .unwrap();
            let design = alas_config::presets::get(name).unwrap().design_vector;
            let outcome = run_candidate(&config, &design.to_array()).unwrap();
            eprintln!(
                "TPMC {name} {mode} tow {:.0} static_kN {:.2} installed_tw {:.4}",
                outcome.sized.takeoff_mass_kg,
                outcome.static_thrust_kn,
                outcome.n_engines as f64 * outcome.static_thrust_kn * 1000.0
                    / (outcome.sized.takeoff_mass_kg * G0),
            );
            let assessment = crate::assess_product_candidate(&config, &design).unwrap();
            for row in assessment.residuals.iter().filter(|row| row.violated()) {
                eprintln!(
                    "TPMC   {} req {:.4} act {:.4} viol {:.4} {:?}",
                    row.id, row.limit, row.actual, row.normalized_violation, row.role
                );
            }
        }
    }
}

/// Clean-sheet turboprop start: climb breakdown and hard rows at the
/// derived nominal design of the brief file named by `ALAS_PROBE_BRIEF`
/// (lane cleansheet-turboprop). Ignored; prints.
///
/// `cargo test -p alas-opt --release clean_sheet_turboprop_start -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic; prints"]
fn clean_sheet_turboprop_start() {
    use super::certification::{evaluate_climb_checks, CertificationInputs, ClimbOutcome};
    // `ALAS_PROBE_PRESET=<name>` evaluates a registered preset instead.
    let (config, design) = if let Ok(name) = std::env::var("ALAS_PROBE_PRESET") {
        (
            AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap(),
            alas_config::presets::get(&name).unwrap().design_vector,
        )
    } else {
        let path = std::env::var("ALAS_PROBE_BRIEF").expect("ALAS_PROBE_BRIEF");
        let text = std::fs::read_to_string(path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let config = AlasConfig::from_value(&value).unwrap();
        let design = config.configured_nominal_design();
        (config, design)
    };
    eprintln!("CSTP design {design:?}");
    let outcome = run_candidate(&config, &design.to_array()).unwrap();
    let s_ref = outcome.plane.s_ref;
    let tow = outcome.sized.takeoff_mass_kg;
    let model = EngineOutModel::new(
        &config,
        Some(YawAirframe {
            plane: &outcome.plane,
            cg_x_m: outcome.cg_x,
        }),
        s_ref,
        Some(outcome.static_thrust_kn * 1_000.0),
    )
    .unwrap()
    .with_weight_kg(tow);
    let inputs = CertificationInputs {
        config: &config,
        drag: &outcome.sized.fuel_artifacts.drag,
        engine_out: &model,
        s_ref_m2: s_ref,
        takeoff_mass_kg: tow,
        landing_mass_kg: outcome.sized.design_landing_mass_kg,
        departure: outcome.departure,
        arrival: outcome.arrival,
    };
    let installed_tw = outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0 / (tow * G0);
    eprintln!(
        "CSTP tow {tow:.0} S {s_ref:.2} WS {:.1} static_kN {:.2} installed_tw {installed_tw:.4}",
        tow / s_ref,
        outcome.static_thrust_kn
    );
    if let Some(departure) = outcome.departure {
        // CS 25.121(b) second segment, term by term (same inputs as the residual).
        let perf = &config.performance;
        let drag = &outcome.sized.fuel_artifacts.drag;
        let speeds = compute_v_speeds_at_masses(
            tow,
            tow,
            s_ref,
            departure,
            perf.cl_max_to,
            perf.cl_max_land,
            perf,
        );
        let evidence = super::oei_deck::second_segment_evidence(
            Some(&model),
            perf,
            departure,
            &speeds,
            &|mach, altitude_m, isa| drag.cd_at_atmosphere(0.0, mach, altitude_m, isa),
        );
        let cond = evidence.condition;
        if let Some(cl) = alas_perf::performance::oei_cl_at_v2(perf.cl_max_to, cond.v2_over_vstall)
        {
            let atmosphere =
                alas_atmo::us1976_compute_values(departure.elevation_m, departure.isa_deviation_c);
            let mach = speeds.v2_ms / atmosphere.speed_of_sound_m_s;
            let clean = drag.cd_airport_base_at_atmosphere(
                cl,
                mach,
                departure.elevation_m,
                departure.isa_deviation_c,
            );
            let cd0 =
                drag.cd_at_atmosphere(0.0, mach, departure.elevation_m, departure.isa_deviation_c);
            let wind = cond.windmilling_cd.unwrap_or(f64::NAN);
            let trim = cond.asymmetric_trim_cd.unwrap_or(f64::NAN);
            let cd = clean + perf.oei_climb_delta_cd + wind + trim;
            let r = cond.condition_to_sls_thrust_ratio.unwrap_or(f64::NAN);
            let n = outcome.n_engines as f64;
            eprintln!(
                "CSTP seg2 {} elev {:.0} m V2 {:.1} m/s V2/VS {:.3} CL {:.3} r {:.4} | CD0 {:.4} induced {:.4} flap {:.4} feathered {:.4} trim {:.4} total {:.4} CD/CL {:.4} L/D {:.2} | req_sls_tw {:.4} gamma {:.4}",
                departure.icao,
                departure.elevation_m,
                speeds.v2_ms,
                cond.v2_over_vstall,
                cl,
                r,
                cd0,
                clean - cd0,
                perf.oei_climb_delta_cd,
                wind,
                trim,
                cd,
                cd / cl,
                cl / cd,
                n / (n - 1.0) * (perf.oei_gradient + cd / cl) / r,
                perf.oei_gradient,
            );
        }
    }
    let sized = &outcome.sized;
    eprintln!(
        "CSTP mass oew {:.0} payload {:.0} zfw {:.0} takeoff_fuel {:.0} design_mission_fuel {:.0} block {:.0} dg {:.0} mlw {:.0} usable_cap {:.0} range_nmi {:.1} min_profile_nmi {:.1}",
        sized.operating_empty_mass_kg,
        sized.payload_kg,
        sized.zero_fuel_mass_kg,
        sized.takeoff_fuel_kg,
        sized.design_mission_fuel_kg,
        sized.block_fuel_kg,
        sized.design_gross_mass_kg,
        sized.design_landing_mass_kg,
        sized.usable_capacity_kg,
        sized.design_range_m / 1852.0,
        sized.minimum_profile_range_m / 1852.0,
    );
    if let Some(flight) = &sized.mtow.offdesign {
        eprintln!(
            "CSTP offdesign range_nmi {:.1} payload {:.0} limit {:.0} required_tow {:.0} mtow {:.0} takeoff_fuel {:.0}",
            flight.range_m / 1852.0,
            flight.payload_kg,
            flight.payload_limit_kg,
            flight.required_takeoff_mass_kg,
            flight.mtow_kg,
            flight.takeoff_fuel_kg,
        );
    }
    for (segment, result) in evaluate_climb_checks(&inputs) {
        match result {
            ClimbOutcome::Evaluated(e) => eprintln!(
                "CSTP   {} V {:.1} CL {:.3} CD {:.4} (wind {:.4} trim {:.4}) grad {:.3} r {:.4} req_tw {:.4} short {:+.1} %",
                segment.id(), e.speed_m_s, e.cl, e.cd, e.windmilling_cd, e.asymmetric_trim_cd,
                e.gradient, e.thrust_ratio, e.required_sls_tw,
                100.0 * (1.0 - installed_tw / e.required_sls_tw),
            ),
            ClimbOutcome::EvidenceGap(why) => eprintln!("CSTP   {} gap {why}", segment.id()),
        }
    }
    let assessment = crate::assess_product_candidate(&config, &design).unwrap();
    let all = std::env::var("ALAS_PROBE_ALL").is_ok();
    for row in assessment
        .residuals
        .iter()
        .filter(|row| all || row.violated())
    {
        eprintln!(
            "CSTP row {} req {:.4} act {:.4} viol {:.4} {:?}",
            row.id, row.limit, row.actual, row.normalized_violation, row.role
        );
    }
}
