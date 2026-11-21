// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Which B747-400 planform features carry its wing-alone aerodynamic centre
//! and tail term. Builds the preset airplane (no sizing loop)
//! with one feature removed at a time and prints the
//! `support/np_terms.rs` decomposition in each variant's own published-frame
//! MAC, plus the built planform's geometric MAC and quarter-MAC station.
//!
//! `cargo run --release -p alas-pipeline --example np_gap_wing_variants`
#![allow(clippy::print_stdout, missing_docs)]
// Diagnostic over a curated preset: a failure aborts the run.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_pipeline::full_analysis::FullAnalysis;
use serde_json::json;

#[path = "support/np_terms.rs"]
mod np_terms;

/// One planform variant: a label and the edit it applies.
type Variant = (
    &'static str,
    Box<dyn Fn(&mut AlasConfig, &mut alas_config::DesignVector)>,
);

fn main() {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "B747-400".to_owned());
    let preset = presets::get(&name).unwrap();
    let base = AlasConfig::from_value(&json!({ "preset": name })).unwrap();
    let mut fine = base.analysis.clone();
    fine.spanwise_resolution = fine.fine_spanwise_resolution;
    fine.chordwise_resolution = fine.fine_chordwise_resolution;
    let variants: Vec<Variant> = vec![
        ("baseline", Box::new(|_, _| {})),
        (
            "no winglet",
            Box::new(|c, _| c.geometry.wing.wingtip_device = None),
        ),
        (
            "no Yehudi (root 12.02, sob 0.935)",
            Box::new(|c, d| {
                d.root_chord_m = 12.02;
                c.geometry.wing.side_of_body_chord_ratio = Some(0.935);
            }),
        ),
        (
            "no winglet, no Yehudi",
            Box::new(|c, d| {
                c.geometry.wing.wingtip_device = None;
                d.root_chord_m = 12.02;
                c.geometry.wing.side_of_body_chord_ratio = Some(0.935);
            }),
        ),
        (
            "no winglet, no Yehudi, LE sweep 37.6",
            Box::new(|c, d| {
                c.geometry.wing.wingtip_device = None;
                d.root_chord_m = 12.02;
                c.geometry.wing.side_of_body_chord_ratio = Some(0.935);
                d.sweep_deg = 37.6;
            }),
        ),
    ];
    for (label, apply) in variants {
        let mut config = base.clone();
        let mut design = preset.design_vector;
        apply(&mut config, &mut design);
        let plane = FullAnalysis::new(config)
            .build_airplane(&design, true)
            .unwrap();
        let frame = plane.mac_frame().unwrap();
        let wing = plane.wings.iter().find(|w| w.name == "Main Wing").unwrap();
        let terms = np_terms::decompose(&plane, &fine, 0.2, frame.x_lemac_m, frame.chord_m);
        println!(
            "{label}: frame MAC {:.3} LEMAC {:.3} | built S {:.1} MAC {:.3} c/4 x {:.2} ({:+.3} frame c)",
            frame.chord_m,
            frame.x_lemac_m,
            wing.projected_area(),
            wing.mean_aerodynamic_chord(),
            wing.aerodynamic_center(0.25)[0],
            (wing.aerodynamic_center(0.25)[0] - frame.x_lemac_m) / frame.chord_m,
        );
        println!("{}", np_terms::line("   ", &terms));
    }
}
