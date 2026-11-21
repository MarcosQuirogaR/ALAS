// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The report polar on the compressible lattice against the incompressible
//! relabel basis, at the design lift (A1 evidence).
//!
//! `cargo run --release -p alas-pipeline --example compressible_polar_probe <preset> [...]`
//! runs the full analysis twice (`compressible_trim` on and off) and prints the
//! design point of each polar and the trimmed design point.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the run.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_pipeline::full_analysis::FullAnalysis;
use serde_json::json;

fn main() {
    for name in std::env::args().skip(1) {
        let design = presets::get(&name).unwrap().design_vector;
        for compressible in [false, true] {
            let mut value = json!({"preset": name});
            value["analysis"] = json!({"compressible_trim": compressible});
            let config = AlasConfig::from_value(&value).unwrap();
            let report = FullAnalysis::new(config).run(&design, true).unwrap();
            let p = &report.polar;
            // Nearest sweep point to the design lift, for the polar columns.
            let k =
                p.cl.iter()
                    .enumerate()
                    .min_by(|a, b| {
                        (a.1 - report.design_point.cl)
                            .abs()
                            .total_cmp(&(b.1 - report.design_point.cl).abs())
                    })
                    .map(|(k, _)| k)
                    .unwrap();
            let trim = report.trimmed_design_point;
            println!(
                "{name} {}: polar pt alpha {:.3} CL {:.4} CD {:.5} CDi {:.5} Cm {:+.4} L/D {:.2} | trim alpha {:?} CL {:?} CD {:?} L/D {:?} | static margin {:.4}",
                if compressible { "compressible" } else { "relabel     " },
                p.alpha_deg[k], p.cl[k], p.cd[k], p.cd_induced[k], p.cm[k], p.l_over_d[k],
                trim.map(|t| (t.alpha_deg * 1000.0).round() / 1000.0),
                trim.map(|t| (t.cl * 1e4).round() / 1e4),
                trim.map(|t| (t.cd * 1e5).round() / 1e5),
                trim.map(|t| (t.l_over_d * 100.0).round() / 100.0),
                report.static_margin,
            );
        }
    }
}
