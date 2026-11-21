// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Audit of the load-and-trim states of registered presets.
//!
//! `cargo run --release -p alas-viz --example cg_audit -- PRESET...` runs each
//! preset as itself, builds the sheet data once from the pipeline result and
//! once from the analysis report alone (the desktop's "Analyze reference"
//! path), and prints each state's CG, limits and verdict for both, so a
//! disagreement between the two paths or a state far outside its limits shows.

#![allow(clippy::print_stdout, clippy::print_stderr)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_report::families::mass_balance::load_trim::data::{
    load_trim_data_from_pipeline, load_trim_data_from_report,
};

fn run(presets: Vec<String>) {
    for name in presets {
        let mut config =
            alas_config::AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        config.structures.run_nastran = false;
        config.structures.run_patran_export = false;
        config.optimizer.design_space.mode = alas_config::optimizer::DesignMode::BaselineSandbox;
        let options = alas_pipeline::PipelineOptions {
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
        let result = match alas_pipeline::DesignPipeline::new(config)
            .run(&options, &alas_pipeline::RunEnvironment::default())
        {
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
        let frame = report.airplane.mac_frame().unwrap();
        println!(
            "== {name}: MAC {:.3} m LEMAC {:.3} m, physical_cg {:.1} %MAC",
            frame.chord_m,
            frame.x_lemac_m,
            frame.pct_mac(report.physical_cg[0])
        );
        for (group, mass) in &report.component_masses {
            let x = report
                .mass_coordinates
                .get(group)
                .map_or(f64::NAN, |c| c[0]);
            println!(
                "   lumped {group}: {mass:.0} kg at x {x:.2} m = {:.1} %MAC",
                frame.pct_mac(x)
            );
        }
        let pipeline = load_trim_data_from_pipeline(&result);
        let from_report = load_trim_data_from_report(report, &result.config);
        // The desktop's pre-run draft: a quick mass report with the preview
        // neutral point, no aerodynamic analysis.
        let started = std::time::Instant::now();
        let design = alas_config::presets::get(&name).unwrap().design_vector;
        let draft = alas_geom::builder::AircraftBuilder::new(Some(result.config.geometry.clone()))
            .build(Some(&design), true)
            .map_err(|e| e.to_string())
            .and_then(|plane| {
                alas_report::families::mass_balance::quick_preview_report(
                    plane,
                    &result.config,
                    design,
                )
            })
            .and_then(|mut draft| {
                alas_report::families::mass_balance::attach_preview_neutral_point(
                    &mut draft,
                    &result.config,
                )?;
                Ok(draft)
            });
        let from_draft = match draft {
            Ok(draft) => {
                println!(
                    "   draft design equals the report's: {}; draft physical_cg {:.1} %MAC, payload layout {}",
                    format!("{:?}", draft.design) == format!("{:?}", report.design),
                    frame.pct_mac(draft.physical_cg[0]),
                    draft.payload_layout.is_some()
                );
                for (group, mass) in &draft.component_masses {
                    let x = draft.mass_coordinates.get(group).map_or(f64::NAN, |c| c[0]);
                    let full = report
                        .component_masses
                        .get(group)
                        .copied()
                        .unwrap_or(f64::NAN);
                    println!("   draft lumped {group}: {mass:.0} kg (run {full:.0}) at x {x:.2} m");
                }
                for (label, r) in [("run  ", report), ("draft", &draft)] {
                    let feas = alas_pipeline::assess_physical_feasibility(
                        &result.config,
                        &r.design,
                        r,
                        None,
                    );
                    if let Some(mb) = feas.mass_balance.as_ref() {
                        for st in &mb.states {
                            println!(
                                "   ledger {label} {}: {:.0} kg cg x {:.3} m pct {:.1}",
                                st.label, st.mass_kg, st.cg_m[0], st.cg_pct_mac
                            );
                        }
                    } else {
                        println!("   ledger {label}: none");
                    }
                    if let Some(m) = feas.model_cg.as_ref() {
                        for st in &m.loading_states {
                            println!(
                                "   gate {label} {:?}: {:.0} kg x {:.3} m pct {:.1}",
                                st.state, st.mass_kg, st.cg_x_m, st.cg_pct_mac
                            );
                        }
                    }
                    println!(
                        "   mac_frame {label}: {:?}",
                        r.airplane.mac_frame().map(|f| (f.x_lemac_m, f.chord_m))
                    );
                }
                load_trim_data_from_report(&draft, &result.config)
            }
            Err(error) => {
                println!("   draft: {error}");
                None
            }
        };
        println!(
            "   draft sheet took {:.2} s",
            started.elapsed().as_secs_f64()
        );
        for (label, data) in [
            ("pipeline", pipeline),
            ("report  ", from_report),
            ("draft   ", from_draft),
        ] {
            let Some(data) = data else {
                println!("   {label}: no sheet");
                continue;
            };
            let mut line = format!("   {label}:");
            for s in &data.steps {
                match &s.gate {
                    Some(g) => line.push_str(&format!(
                        " {} {:.0}kg {:.1} [{:.1},{:.1}]{}",
                        s.state,
                        s.mass_kg,
                        s.pct_mac,
                        g.fwd_pct_mac,
                        g.aft_pct_mac,
                        if g.violated { "!" } else { "" }
                    )),
                    None => {
                        line.push_str(&format!(" {} {:.0}kg {:.1}", s.state, s.mass_kg, s.pct_mac))
                    }
                }
            }
            println!("{line}");
            for e in data.envelopes() {
                let (a, b) = (e.vertices.first().unwrap(), e.vertices.last().unwrap());
                println!(
                    "      {:?} {:.0}..{:.0} kg: fwd {:.1}->{:.1} aft {:.1}->{:.1}",
                    e.kind,
                    e.bottom_kg,
                    e.top_kg,
                    a.fwd_pct_mac,
                    b.fwd_pct_mac,
                    a.aft_pct_mac,
                    b.aft_pct_mac
                );
            }
        }
    }
}

fn main() {
    let presets: Vec<String> = std::env::args().skip(1).collect();
    let handle = std::thread::Builder::new()
        .stack_size(512 * 1024 * 1024)
        .spawn(move || run(presets))
        .expect("spawn");
    handle.join().expect("audit thread");
}
