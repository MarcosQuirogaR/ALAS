// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fuel tank probe: prints each resolved tank centroid against the wing
//! leading edge and the fuel centroid at the lumped closure fuel mass.
#![allow(clippy::print_stdout, clippy::print_stderr)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::optimizer::DesignMode;
use alas_config::AlasConfig;
use alas_mass::tanks::resolve_product_layout;
use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};

fn main() {
    for name in std::env::args().skip(1) {
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        config.structures.run_nastran = false;
        config.structures.run_patran_export = false;
        config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
        let cfg = config.clone();
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
        let result = DesignPipeline::new(config)
            .run(&options, &RunEnvironment::default())
            .unwrap();
        let report = result
            .optimized_report
            .as_ref()
            .or(result.baseline_analysis.as_ref())
            .unwrap();
        let f = report.airplane.mac_frame().unwrap();
        let w = report.airplane.main_wing().unwrap();
        let root = &w.xsecs[0];
        let tip = w.xsecs.last().unwrap();
        println!(
            "{name}: lemac {:.3} mac {:.3} | wing root LE x {:.3} chord {:.3} | tip LE x {:.3} chord {:.3} y {:.3}",
            f.x_lemac_m, f.chord_m, root.xyz_le[0], root.chord, tip.xyz_le[0], tip.chord, tip.xyz_le[1]
        );
        let layout = resolve_product_layout(&cfg, &report.design, &report.airplane).unwrap();
        for t in layout.tanks() {
            println!(
                "   tank {:<14} {:?} {:?} cap {:.0} kg vol {:.2} m3 centroid x {:.3} ({:.1}% MAC) y {:.2} z {:.2} extent {:?} prio {}",
                t.id, t.kind, t.side, t.usable_capacity_kg, t.geometric_volume_m3,
                t.centroid_m[0], f.pct_mac(t.centroid_m[0]), t.centroid_m[1], t.centroid_m[2], t.extent_m, t.burn_priority
            );
        }
        for p in layout.fuel_cg_curve(5) {
            println!(
                "   curve {:.0} kg x {:.3} ({:.1}%)",
                p.fuel_kg,
                p.cg_m[0],
                f.pct_mac(p.cg_m[0])
            );
        }
    }
}
