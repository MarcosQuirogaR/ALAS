// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless run of the sandbox's Quick Analysis on configuration documents.
//!
//! The Quick Analysis is only reachable from the desktop sandbox, so this
//! example drives the same entry point (`run_quick_analysis`) and writes one
//! JSON line per configuration with every published metric, for checks of
//! the numbers against the baseline analysis of the same aircraft.
//!
//! Usage: `quick_check CONFIG.json [CONFIG.json ...]`; a name without a
//! `.json` suffix is a registered preset.
#![allow(clippy::print_stderr, clippy::print_stdout, missing_docs)]

use std::sync::atomic::AtomicBool;

use alas_config::AlasConfig;
use alas_pipeline::quick_analysis::{run_quick_analysis, QuickAnalysisRequest};
use serde_json::{json, Value};

fn load(arg: &str) -> Result<AlasConfig, String> {
    let document: Value = if std::path::Path::new(arg)
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        let text = std::fs::read_to_string(arg).map_err(|error| format!("{arg}: {error}"))?;
        serde_json::from_str(&text).map_err(|error| format!("{arg}: {error}"))?
    } else {
        json!({ "preset": arg })
    };
    alas_mission::preset_load::load_document(&document)
        .map(|(config, _)| config)
        .map_err(|error| format!("{arg}: {error}"))
}

fn main() {
    let mut failed = false;
    for arg in std::env::args().skip(1) {
        let config = match load(&arg) {
            Ok(config) => config,
            Err(error) => {
                println!("{}", json!({ "config": arg, "load_error": error }));
                failed = true;
                continue;
            }
        };
        let design = config.configured_nominal_design();
        let request = QuickAnalysisRequest {
            revision: 1,
            config,
            design,
        };
        let stop = AtomicBool::new(false);
        let mut events = Vec::new();
        let summary = run_quick_analysis(
            &request,
            &mut |event| {
                events.push(json!({
                    "metric": format!("{:?}", event.metric),
                    "elapsed_ms": event.elapsed_ms,
                    "outcome": event.outcome,
                }));
            },
            &stop,
        );
        println!(
            "{}",
            json!({
                "config": arg,
                "published": summary.published,
                "initial_stage_ms": summary.initial_stage_ms,
                "final_ms": summary.final_ms,
                "events": events,
            })
        );
    }
    std::process::exit(i32::from(failed));
}
