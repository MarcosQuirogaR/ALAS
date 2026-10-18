// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A nominal assessment is bit-identical across processes.
//!
//! `HashMap` seeds its hasher per process, so any sum or output order taken
//! over a hash map differs between runs by an ulp or a key order. The report's
//! maps are ordered (`BTreeMap`); this test runs the same preset in two fresh
//! child processes and compares the report quantities bit for bit.

// A test asserts on values it constructed, so a failed unwrap is the
// assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::process::Command;

use alas_config::{presets, AlasConfig, DesignMode};
use alas_pipeline::FullAnalysis;

const CHILD_ENV: &str = "ALAS_DETERMINISM_CHILD";
const MARKER: &str = "DETERMINISM-DIGEST";
const PRESET: &str = "A320-200";

/// Child body: runs the nominal analysis and prints the exact report
/// quantities (shortest round-trip `f64` formatting is bit-exact).
fn child_digest() -> String {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": PRESET })).unwrap();
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    let design = presets::get(PRESET).unwrap().design_vector;
    let report = FullAnalysis::new(config).run(&design, true).unwrap();
    let total_kg: f64 = report.component_masses.values().sum();
    let oew_kg: f64 = ["Fuel", "Payload"]
        .iter()
        .fold(total_kg, |rest, key| rest - report.component_masses[*key]);
    format!(
        "{MARKER} total={:016x} oew={:016x} masses={:?} coordinates={:?} geometry={:?}",
        total_kg.to_bits(),
        oew_kg.to_bits(),
        report.component_masses,
        report.mass_coordinates,
        report.geometry_summary
    )
}

fn run_child() -> String {
    let exe = std::env::current_exe().unwrap();
    let output = Command::new(exe)
        .args([
            "--exact",
            "child_prints_digest",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "child failed: {output:?}");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.find(MARKER).map(|at| &line[at..]))
        .expect("child printed no digest")
        .to_owned()
}

#[test]
fn child_prints_digest() {
    if std::env::var_os(CHILD_ENV).is_some() {
        writeln!(std::io::stdout(), "{}", child_digest()).unwrap();
    }
}

#[test]
fn report_quantities_are_bit_identical_across_processes() {
    let first = run_child();
    let second = run_child();
    assert_eq!(first, second, "report differs between processes");
}
