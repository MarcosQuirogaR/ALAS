// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! ISA sea-level take-off field length breakdown per jet preset (diagnostic).
//!
//! `cargo run --release -p alas-pipeline --example tofl_probe [preset ...]`
//! prints one line per preset: masses, speeds, `VR`, `V1`, all-engine distance
//! (and its CS 25.113(a)(2) factor), accelerate-stop, continued take-off,
//! balanced field and the field length, plus the Vref of the same run.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the line.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_pipeline::field_reference::report_isa_sea_level_field_reference;
use alas_pipeline::full_analysis::FullAnalysis;
use serde_json::json;

fn main() {
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    for name in presets::available() {
        if !wanted.is_empty() && !wanted.iter().any(|w| w == name) {
            continue;
        }
        match line(name) {
            Ok(text) => println!("{text}"),
            Err(error) => println!("{name}: ERROR {error}"),
        }
    }
}

fn line(name: &str) -> Result<String, String> {
    let preset = presets::get(name).map_err(|e| e.to_string())?;
    // TOFL_PERF='{"takeoff_gear_delta_cd":0.0}' overrides performance fields
    // (sensitivity runs); TOFL_CLMAX_SCALE multiplies the preset CLmax_TO.
    let mut value = json!({"preset": name});
    if let Ok(text) = std::env::var("TOFL_PERF") {
        value["performance"] = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    }
    let mut config = AlasConfig::from_value(&value).map_err(|e| e.to_string())?;
    if let Some(scale) = std::env::var("TOFL_CLMAX_SCALE")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
    {
        config.performance.cl_max_to *= scale;
    }
    // TOFL_THRUST_KN sets the rated take-off thrust per engine (deck rating sensitivity).
    if let Some(thrust) = std::env::var("TOFL_THRUST_KN")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
    {
        config
            .geometry
            .engine
            .set_thrust_kn(thrust)
            .map_err(|e| format!("{e:?}"))?;
    }
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .map_err(|e| e.to_string())?;
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .map_err(|e| e.to_string())?;
    let mtow = config.requirements.mtow_kg;
    let mlw = config.landing_mass_limit_kg(mtow);
    let r = report_isa_sea_level_field_reference(&config, &report, airplane.s_ref, mtow, mlw)?;
    // TOFL_MASS_SWEEP="60000,64000" re-flies the ISA sea-level field at those take-off
    // masses on the same analysed airframe (weight-trend diagnosis).
    let mut sweep = String::new();
    if let Ok(text) = std::env::var("TOFL_MASS_SWEEP") {
        for mass in text.split(',').filter_map(|s| s.trim().parse::<f64>().ok()) {
            match report_isa_sea_level_field_reference(&config, &report, airplane.s_ref, mass, mlw)
            {
                Ok(s) => {
                    let f = s.takeoff_field.as_ref();
                    sweep += &format!(
                        "\n    sweep {mass:.0} kg: TOFL {:.0} | AEO x1.15 {:.0} GO {:.0} ASD {:.0} | VSR {:.1} VR {:.1} V2 {:.1} ({:.3} VSR) V1 {:.1} | OEI grad {:?}",
                        s.takeoff_field_length_m,
                        f.map_or(f64::NAN, |f| 1.15 * f.all_engine_m),
                        f.map_or(f64::NAN, |f| f.continued_m),
                        f.map_or(f64::NAN, |f| f.accelerate_stop_m),
                        s.v_stall_takeoff_m_s,
                        f.map_or(f64::NAN, |f| f.vr_m_s),
                        s.v2_m_s,
                        s.v2_m_s / s.v_stall_takeoff_m_s,
                        f.map_or(f64::NAN, |f| f.v1_m_s),
                        s.oei.available_gradient,
                    );
                }
                Err(error) => sweep += &format!("\n    sweep {mass:.0} kg: ERROR {error}"),
            }
        }
    }
    let Some(f) = r.takeoff_field else {
        return Ok(format!(
            "{name}: TOFL {:.0} m (propeller/replay method) | BFL {:.0} | Vref {:.1} | LDR {:.0} LFL {:.0}",
            r.takeoff_field_length_m, r.model_balanced_field_length_m, r.vref_m_s, r.landing_distance_m, r.landing_field_length_m
        ));
    };
    let legs = breakdown(&config, &report, &r).unwrap_or_else(|e| format!("breakdown: {e}"));
    let liftoff = alas_opt::envelope::liftoff_lift(&report.airplane, &config).map_or_else(
        || "liftoff: none".to_owned(),
        |l| {
            let th = l.tail_strike_deg;
            let cl_ts = l.cl_ground(th);
            let clmax = config.performance.cl_max_to;
            let (cl_mu, limited) = l.unstick_lift(clmax);
            format!(
                "liftoff: dCL0flap {:.3} CL0 {:.3} CLa_wf {:.3}/rad h_ac {:.2} ext {:.3} th_static {:?} th_ext {:.2} CL(0) {:.3} CL(th) {:.3} CLmax_TO {:.3} geo_limited {} VMU/VSR {:.3} VSR {:.2}",
                l.flap_zero_alpha_lift, l.cl_zero_alpha, l.cl_alpha_wf_per_rad, l.quarter_chord_height_m, l.oleo_extension_m,
                l.static_tail_strike_deg.map(|d| (d * 100.0).round() / 100.0), th,
                l.cl_ground(0.0), cl_ts, clmax, limited, (clmax / cl_mu).sqrt(), r.v_stall_takeoff_m_s,
            )
        },
    );
    Ok(format!(
        "{name}: mtow {mtow:.0} T/W {:.4} VSR {:.1} V2 {:.1} ({:.3} VSR) Vref {:.1} | VR {:.1} V1 {:.1} VEF {:.1} | AEO {:.0} x1.15 {:.0} | ASD {:.0} GO {:.0} BFL {:.0} | TOFL {:.0} CLTO {:.3} | LDR {:.0} LFL {:.0} VS1gL {:.1}",
        r.static_tw, r.v_stall_takeoff_m_s, r.v2_m_s, r.v2_m_s / r.v_stall_takeoff_m_s, r.vref_m_s,
        f.vr_m_s, f.v1_m_s, f.vef_m_s, f.all_engine_m, 1.15 * f.all_engine_m,
        f.accelerate_stop_m, f.continued_m, f.balanced_m, f.tofl_m, config.performance.cl_max_to, r.landing_distance_m, r.landing_field_length_m, r.vs1g_landing_m_s,
    ) + "
    " + &legs + "
    " + &liftoff + "
    flap: " + &format!("{:?} deg; {}", r.takeoff_flap_deg, r.takeoff_flap_selection.as_deref().unwrap_or("single setting")) + &sweep)
}

/// The legs of the jet field the reference solved, re-evaluated with the same inputs:
/// one rotation per ground plane of the lift-off lift, the longer field kept, as the
/// pipeline does.
fn breakdown(
    config: &AlasConfig,
    report: &alas_pipeline::full_analysis::AnalysisReport,
    r: &alas_pipeline::field_reference::IsaSeaLevelFieldReference,
) -> Result<String, String> {
    use alas_config::airports::Airport;
    use alas_opt::mdo::{
        jet_takeoff_field_breakdown, longest_on_ground_planes, EngineOutModel, JetFieldInputs,
        V2ClimbOutPolar, YawAirframe,
    };
    let airport = Airport::custom("ISA sea level", 0.0, 10_000.0, 10_000.0, 0.0, 0.0, 0.0);
    let a0 = alas_atmo::us1976_compute_values(0.0, 0.0).speed_of_sound_m_s;
    let polar = alas_pipeline::field_performance::report_field_polar(
        config,
        report,
        r.v2_m_s / a0,
        &airport,
    )?;
    let plane = &report.airplane;
    let airframe = YawAirframe {
        plane,
        cg_x_m: report.physical_cg[0],
    };
    let model = EngineOutModel::new(config, Some(airframe), plane.s_ref, None)?
        .with_weight_kg(r.takeoff_mass_kg);
    let fly = |liftoff| {
        jet_takeoff_field_breakdown(&JetFieldInputs {
            perf: &config.performance,
            engine_out: &model,
            airport: &airport,
            wing_area_m2: plane.s_ref,
            mass_kg: r.takeoff_mass_kg,
            static_thrust_n: r.static_thrust_n,
            polar: &polar,
            speeds: r.speeds,
            climb_out: Some(V2ClimbOutPolar::of(
                config,
                alas_opt::mdo::climb_out_aspect_ratio(config, plane.b_ref, plane.s_ref),
            )),
            liftoff,
        })
    };
    let (f, b) = longest_on_ground_planes(Some(plane), config, fly, |field| field.0.tofl_m)?;
    Ok(format!(
        "legs (TOFL {:.0}, VR {:.1}): AEO roll {:.0} rot {:.0} air {:.0} (VLOF {:.1}, grad {:.4}) | GO roll {:.0}+{:.0} rot {:.0} air {:.0} (VLOF {:.1}, grad {:.4}, free {:.4}) | STOP roll {:.0}+{:.0} brake {:.0} allow {:.0} | polar cd0 {:.4} k {:.4} | tail margin {:?} deg",
        f.tofl_m, f.vr_m_s,
        b.all_engine_roll_m, b.all_engine_rotation_m, b.all_engine_air_m, b.all_engine_vlof_m_s, b.all_engine_gradient_v2,
        b.continued_roll_all_engine_m, b.continued_roll_one_engine_m, b.continued_rotation_m, b.continued_air_m,
        b.continued_vlof_m_s, b.engine_out_gradient_v2, b.engine_out_gradient_v2_free_air,
        b.stop_roll_all_engine_m, b.stop_roll_one_engine_m, b.stop_braking_m, b.stop_allowance_m,
        polar.zero_lift(), polar.k, b.tail_margin_deg.map(|m| (m * 100.0).round() / 100.0),
    ))
}
