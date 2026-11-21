// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Materialise the OpenVSP projects of a preset for panel-method runs.
//!
//! `cargo run --release -p alas-pipeline --example sm1_vsp_export <out dir> <preset> [...]`
//! writes `<out>/<preset>/openvsp/optimized_aircraft.{vspscript,vsp3,cad_preview.vsp3}`
//! with the installed `vspscript.exe`, and prints the reference quantities
//! (S, MAC, b, moment origin) a panel run needs.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the run.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use alas_config::{presets, AlasConfig};
use alas_pipeline::full_analysis::FullAnalysis;
use alas_pipeline::{export_openvsp_script, materialize_openvsp_project};
use serde_json::json;

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().expect("output dir"));
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let exe = std::env::var_os("ALAS_OPENVSP_EXE").map_or_else(
        || workspace.join("external tools/OpenVSP-3.51.2-win64/vspscript.exe"),
        PathBuf::from,
    );
    for name in args {
        let config = AlasConfig::from_value(&json!({"preset": name})).unwrap();
        let design = presets::get(&name).unwrap().design_vector;
        let report = FullAnalysis::new(config.clone())
            .run(&design, true)
            .unwrap();
        let script = out
            .join(&name)
            .join("openvsp")
            .join("optimized_aircraft.vspscript");
        let export = export_openvsp_script(&report, &config, &script).unwrap();
        let export = materialize_openvsp_project(export, &exe, 300.0);
        let plane = &report.airplane;
        let frame = plane.mac_frame().unwrap();
        println!(
            "{name}: status {} vsp3 {} S {:.4} MAC {:.4} b {:.4} xref {:.4} {:.4} {:.4} LEMAC {:.4} cad_preview_geometry {}",
            export.status.as_str(),
            export.vsp3_path.display(),
            plane.s_ref,
            plane.c_ref,
            plane.b_ref,
            plane.xyz_ref[0],
            plane.xyz_ref[1],
            plane.xyz_ref[2],
            frame.x_lemac_m,
            export.cad_preview_geometry_available
        );
    }
}
