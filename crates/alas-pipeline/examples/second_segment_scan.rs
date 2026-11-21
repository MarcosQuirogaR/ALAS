// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Second-segment requirement versus `V2 / VSR` for a preset at MTOW (diagnostic).
//!
//! `cargo run --release -p alas-pipeline --example second_segment_scan [preset]`
//! prints, for several `V2 / VSR`, the lift coefficient, the take-off drag
//! terms, the thrust ratio at V2 and the required SLS T/W of the improved-climb
//! selection against the installed one.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the run.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::airports::Airport;
use alas_config::{presets, AlasConfig};
use alas_opt::mdo::{second_segment_evidence, EngineOutModel, YawAirframe};
use alas_perf::performance::{
    assess_oei_climb, compute_v_speeds_at_masses, far25_oei_gradient, oei_cl_at_v2,
};
use alas_pipeline::full_analysis::FullAnalysis;
use serde_json::json;

fn main() {
    let names: Vec<String> = std::env::args().skip(1).collect();
    for name in names {
        run(&name);
    }
}

fn run(name: &str) {
    let preset = presets::get(name).unwrap();
    let config = AlasConfig::from_value(&json!({"preset": name})).unwrap();
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .unwrap();
    let mtow = config.requirements.mtow_kg;
    let s_ref = report.airplane.s_ref;
    let perf = &config.performance;
    let airport = Airport::custom("ISA sea level", 0.0, 10_000.0, 10_000.0, 0.0, 0.0, 0.0);
    let schedule = compute_v_speeds_at_masses(
        mtow,
        config.landing_mass_limit_kg(mtow),
        s_ref,
        &airport,
        perf.cl_max_to,
        perf.cl_max_land,
        perf,
    );
    let n = config.geometry.engine.spanwise_positions_m.len() as i64;
    let static_n = n as f64 * config.geometry.engine.thrust_kn() * 1000.0;
    let installed = static_n / (mtow * 9.80665);
    let model = EngineOutModel::new(
        &config,
        Some(YawAirframe {
            plane: &report.airplane,
            cg_x_m: report.physical_cg[0],
        }),
        s_ref,
        Some(static_n / n as f64),
    )
    .unwrap()
    .with_weight_kg(mtow);
    let artifacts = report.fuel.artifacts(&config, &report.design).unwrap();
    let a0 = alas_atmo::us1976_compute_values(0.0, 0.0).speed_of_sound_m_s;
    let shift = alas_opt::mdo::drag_table::high_lift_lift_shift(perf.cl_max_to, perf.cl_max_clean);
    println!(
        "{name}: mtow {mtow:.0} S {s_ref:.1} installed T/W {installed:.4} CLmaxTO {:.3} clean {:.3} shift {shift:.3} dCD_flap {:.4} VSR {:.2} schedule {:.3}",
        perf.cl_max_to,
        perf.cl_max_clean,
        perf.oei_climb_delta_cd,
        schedule.v_stall_to_ms,
        schedule.v2_ms / schedule.v_stall_to_ms
    );
    println!(
        "span {:.2} AR {:.2}",
        report.airplane.b_ref,
        report.airplane.b_ref.powi(2) / s_ref
    );
    for x in [1.13, 1.2, 1.34] {
        let mut speeds = schedule;
        speeds.v2_ms = x * schedule.v_stall_to_ms;
        let ev = second_segment_evidence(Some(&model), perf, &airport, &speeds, &|m, h, d| {
            artifacts.drag.cd_at_atmosphere(0.0, m, h, d)
        });
        let cl = oei_cl_at_v2(perf.cl_max_to, x).unwrap();
        if x < 1.14 {
            trim_check(&model, &ev, speeds.v2_ms, s_ref);
        }
        let mach = speeds.v2_ms / a0;
        let cd = artifacts
            .drag
            .cd_airport_base_at_atmosphere(cl, mach, 0.0, 0.0, shift);
        let a = assess_oei_climb(
            cd,
            0.0,
            n,
            far25_oei_gradient(n).unwrap(),
            cl,
            perf.oei_climb_delta_cd,
            perf.cl_max_to,
            Some(ev.condition),
        );
        println!(
            "x {x:.2} V2 {:.1} CL {cl:.3} CDbase {cd:.4} wind {:?} trim {:?} r {:?} L/D_base {:.2} req_inflight {:?} req_sls {:?}",
            speeds.v2_ms,
            ev.condition.windmilling_cd,
            ev.condition.asymmetric_trim_cd,
            ev.condition.condition_to_sls_thrust_ratio,
            cl / cd,
            a.required_inflight_tw,
            a.required_sls_tw
        );
        if let Some(t) = artifacts.drag.table() {
            components(t, cl, mach, shift);
            if x < 1.14 {
                induced_sweep(t, report.airplane.b_ref.powi(2) / s_ref);
            }
        }
    }
}

#[allow(dead_code)]
fn components(table: &alas_opt::mdo::drag_table::TrimmedDragTable, cl: f64, mach: f64, shift: f64) {
    let atm = alas_atmo::us1976_compute_values(0.0, 0.0);
    let re = atm.density_kg_m3 * mach * atm.speed_of_sound_m_s / atm.dynamic_viscosity_pa_s;
    println!(
        "   CL {cl:.2}: cd0 {:.4} upsweep {:.4} induced {:.4} wave {:.4} e_eff {:.3}",
        table.cd0_at_reynolds_per_m(mach, re),
        table.upsweep_attitude_cd_high_lift(cl, mach, shift),
        table.induced_cd_at(cl, mach),
        table.wave_cd(cl, mach),
        table.effective_oswald(cl)
    );
}

#[allow(dead_code)]
fn induced_sweep(table: &alas_opt::mdo::drag_table::TrimmedDragTable, ar: f64) {
    for cl in [0.3, 0.5, 0.8, 1.1, 1.4, 1.6] {
        let low = table.induced_cd_at(cl, 0.24);
        let cruise = table.induced_cd(cl);
        println!(
            "   induced CL {cl:.1}: low-M {low:.4} (e {:.3}) cruise-M {cruise:.4} (e {:.3})",
            cl * cl / (std::f64::consts::PI * ar * low),
            cl * cl / (std::f64::consts::PI * ar * cruise)
        );
    }
}

/// Torenbeek App. G-8.3 (Eqs. G-84 to G-86) evaluated by hand from the declared fin,
/// rudder and engine arm: the inputs and the zero-sideslip drag next to the model's
/// yaw-trim value.
fn trim_check(
    model: &EngineOutModel,
    ev: &alas_opt::mdo::SecondSegmentEvidence,
    v2: f64,
    s_ref: f64,
) {
    let (Some(control), Some(point)) = (model.yaw_control(), ev.point) else {
        println!("   trim check: no yaw geometry");
        return;
    };
    let g = control.balance;
    let q = 0.5 * 1.225 * v2 * v2;
    let force = point.thrust_per_engine_n + point.windmilling_cd * q * s_ref;
    let fin_force = force * g.engine_arm_m / g.fin_arm_m;
    let cy = fin_force / (q * g.fin_area_m2);
    let induced = cy * cy * g.fin_area_m2 / (std::f64::consts::PI * g.fin_effective_aspect_ratio);
    let rudder = (2.3 / std::f64::consts::PI)
        * (g.rudder_area_m2 * g.fin_area_m2).sqrt()
        * g.fin_effective_aspect_ratio.powf(-4.0 / 3.0)
        * g.fin_sweep_rad.cos().powf(1.0 / 3.0)
        * cy
        * cy;
    println!(
        "   trim inputs: engine arm {:.2} m, fin arm {:.2} m, fin area {:.1} m2, A_v,eff {:.2}, rudder area {:.1} m2, sweep {:.1} deg, thrust/eng {:.0} N, wm cd {:.4}, q {:.0} Pa",
        g.engine_arm_m, g.fin_arm_m, g.fin_area_m2, g.fin_effective_aspect_ratio, g.rudder_area_m2,
        g.fin_sweep_rad.to_degrees(), point.thrust_per_engine_n, point.windmilling_cd, q
    );
    println!(
        "   G-84..86 zero-sideslip: fin force {:.0} N, CYv {:.3}, induced {:.3} m2 + rudder {:.3} m2 = CD {:.4}; model (yaw trim) CD {:?}",
        fin_force, cy, induced, rudder, (induced + rudder) / s_ref, ev.condition.asymmetric_trim_cd
    );
}
