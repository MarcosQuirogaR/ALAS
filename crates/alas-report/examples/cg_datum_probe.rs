// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CG datum probe: prints the MAC datum each consumer would use and the
//! DOW CG and limits per preset.
#![allow(clippy::print_stdout, clippy::print_stderr)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::optimizer::DesignMode;
use alas_config::AlasConfig;
use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};
use alas_report::families::mass_balance::load_trim::data::load_trim_data_from_pipeline;

fn main() {
    let presets: Vec<String> = std::env::args().skip(1).collect();
    for name in presets {
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        config.structures.run_nastran = false;
        config.structures.run_patran_export = false;
        config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
        let options = PipelineOptions {
            optimize: false,
            compare_baseline: false,
            parallel: true,
            aerodynamic_solver: Default::default(),
            optimization_solver: Default::default(),
            output_dir: None,
            save_plots: false,
            seed: Some(42),
            quiet: true,
        };
        let result = match DesignPipeline::new(config).run(&options, &RunEnvironment::default()) {
            Ok(r) => r,
            Err(e) => {
                println!("{name}: ERR {e}");
                continue;
            }
        };
        let report = result
            .optimized_report
            .as_ref()
            .or(result.baseline_analysis.as_ref())
            .unwrap();
        let p = &report.airplane;
        let f = p.mac_frame().unwrap();
        let w0 = &p.wings[0];
        let mw = p.main_wing().unwrap();
        println!(
            "{name}: c_ref {:.3} frame(lemac {:.3}, chord {:.3}) wing0 '{}' ref={:?} lemac0 {:.3} mac0 {:.3} | main '{}' lemac {:.3} mac_station(x {:.3}, c {:.3}) | cg_phys {:.3} ({:.1}%) np {:.3} ({:.1}%)",
            p.c_ref, f.x_lemac_m, f.chord_m, w0.name, w0.reference.map(|r| (r.mac_le_x_m, r.mac_m)),
            w0.reference_mac_station_le_x(), w0.mac_station().chord_m,
            mw.name, mw.reference_mac_station_le_x(), mw.mac_station().x_le_m, mw.mac_station().chord_m,
            report.physical_cg[0], f.pct_mac(report.physical_cg[0]),
            report.x_neutral_point, f.pct_mac(report.x_neutral_point),
        );
        if let Some(m) = result.feasibility.model_cg.as_ref() {
            println!("   MLG {:.1}% np_aft {:.1} ground_aft {:.1} cfg_fwd {:.1} maxnose_fwd {:.1} scissor {:.1}", m.main_gear_station_pct_mac, m.aerodynamic_aft_limit_pct_mac, m.ground_aft_limit_pct_mac, m.configured_forward_limit_pct_mac, m.max_nose_load_fwd_limit_pct_mac, m.scissor_plot_fwd_limit_pct_mac);
            for s in &m.loading_states {
                let l = &s.physical_limits;
                println!("     {:?} {:.0} kg cg {:.1}% | aft {:.1} {:?} (np {:.1} ground {:.1} tipback {:.1}) fwd {:.1} {:?} (maxnose {:.1} rot {:.1} landtrim {:.1})", s.state, s.mass_kg, s.cg_pct_mac, l.aft_limit_pct_mac, l.aft_limit_governance, l.aerodynamic_aft_pct_mac, l.ground_aft_pct_mac, l.tip_back_aft_pct_mac, l.fwd_limit_pct_mac, l.fwd_limit_governance, l.max_nose_load_fwd_pct_mac, l.rotation_fwd_pct_mac, l.landing_trim_fwd_pct_mac);
            }
        }
        if let Some(d) = load_trim_data_from_pipeline(&result) {
            let first = |v: &Vec<alas_report::families::mass_balance::load_trim::LimitVertex>| {
                v.first().map(|x| {
                    (
                        x.mass_kg.round(),
                        (x.fwd_pct_mac * 10.0).round() / 10.0,
                        (x.aft_pct_mac * 10.0).round() / 10.0,
                    )
                })
            };
            println!(
                "   sheet lemac {:.3} mac {:.3} DOW {:.1}% @ {:.0} kg | ground {:?} takeoff {:?} flight {:?} landing {:?}",
                d.x_lemac_m, d.mac_m, d.steps[0].pct_mac, d.steps[0].mass_kg,
                first(&d.ground_limits), first(&d.takeoff_limits), first(&d.flight_limits), first(&d.landing_limits)
            );
            for s in &d.steps {
                println!(
                    "     step {} / {}: {:.0} kg {:.1}%",
                    s.item, s.state, s.mass_kg, s.pct_mac
                );
            }
            for (k, v) in &report.component_masses {
                let x = report
                    .mass_coordinates
                    .get(k)
                    .map(|c| c[0])
                    .unwrap_or(f64::NAN);
                println!(
                    "     comp {k}: {:.0} kg x {:.2} m = {:.1}%",
                    v,
                    x,
                    f.pct_mac(x)
                );
            }
        }
    }
}
