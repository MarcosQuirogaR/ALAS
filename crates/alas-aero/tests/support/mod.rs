// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What `parity_analysis.rs` reads: the shape of `golden/aero/analysis.json`,
//! and the two constructions that have to match the generator's -- the
//! nominal aircraft and the `AeroAnalysis` wrapped around it.
//!
//! This lives beside the test rather than inside it because
//! `alas/physics/aerodynamics.py` has eight entry points with eight different
//! result shapes, and the test itself should read as the comparisons it makes
//! rather than as two hundred lines of field declarations.

// A test binary's failed unwrap or expect is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]
// Two test binaries compile this module, and each reads the part of `golden/aero/`
// its own row is about; an item the other one uses is not dead.
#![allow(dead_code)]

pub mod drag_buildup;
pub mod lift_surrogate;
pub mod vorlax;

use std::collections::HashMap;

use alas_aero::analysis::AeroAnalysis;
use alas_config::analysis::AnalysisConfig;
use alas_config::geometry::GeometryConfig;
use alas_config::physics::DragModelConfig;
use alas_geom::asb::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AirplaneFixture {
    pub s_ref: f64,
    pub c_ref: f64,
    pub b_ref: f64,
    pub xyz_ref: [f64; 3],
    pub wing_names: Vec<String>,
    pub fuselage_names: Vec<String>,
    pub section_thickness: f64,
}

#[derive(Debug, Deserialize)]
pub struct PgBetaInputs {
    pub mach: f64,
    pub sweep_deg: f64,
}

#[derive(Debug, Deserialize)]
pub struct PgBetaCase {
    pub inputs: PgBetaInputs,
    pub beta: f64,
}

#[derive(Debug, Deserialize)]
pub struct ReportAlphaInputs {
    pub alpha_inc_deg: f64,
    #[serde(rename = "alpha_0L_deg")]
    pub alpha_zero_lift_deg: f64,
    pub mach: f64,
    pub sweep_deg: f64,
}

#[derive(Debug, Deserialize)]
pub struct ReportAlphaCase {
    pub inputs: ReportAlphaInputs,
    pub alpha_deg: f64,
}

#[derive(Debug, Deserialize)]
pub struct ParasiteInputs {
    pub mach: f64,
    pub altitude: f64,
    pub cl: f64,
    pub include_engines: bool,
}

#[derive(Debug, Deserialize)]
pub struct ParasiteCase {
    pub inputs: ParasiteInputs,
    pub cd_parasite: f64,
}

#[derive(Debug, Deserialize)]
pub struct WaveInputs {
    pub mach: f64,
    pub cl: f64,
}

#[derive(Debug, Deserialize)]
pub struct WaveCase {
    pub inputs: WaveInputs,
    pub cd_wave: f64,
}

/// The one case run on an aircraft with no wing at all, which is the only way
/// to reach `_section_thickness`' 0.12 fallback.
#[derive(Debug, Deserialize)]
pub struct NoWings {
    pub section_thickness: f64,
    pub cd_wave: f64,
    pub cd_parasite: f64,
}

#[derive(Debug, Deserialize)]
pub struct ComponentInputs {
    pub mach: f64,
    pub altitude: f64,
    pub cl: f64,
    pub cd_induced: f64,
}

#[derive(Debug, Deserialize)]
pub struct ComponentCase {
    pub inputs: ComponentInputs,
    pub cd_parasite: f64,
    pub cd_induced: f64,
    pub cd_wave: f64,
    pub cd_total: f64,
}

#[derive(Debug, Deserialize)]
pub struct QuickInputs {
    pub cl_target: f64,
    pub mach: f64,
    pub altitude: f64,
    pub spanwise: i64,
    pub chordwise: i64,
}

#[derive(Debug, Deserialize)]
pub struct QuickCase {
    pub inputs: QuickInputs,
    pub l_over_d: f64,
    pub alpha: f64,
    pub cd: f64,
    pub cl: f64,
}

#[derive(Debug, Deserialize)]
pub struct TrimmedInputs {
    pub trim_alpha_deg: f64,
    /// `null` where the trim solve left the incidence undefined: NaN has no
    /// JSON spelling, and `gen_prop_cycle.py` set this convention.
    pub trim_ih_deg: Option<f64>,
    pub cl_alpha: f64,
    pub mach: f64,
    pub altitude: f64,
}

#[derive(Debug, Deserialize)]
pub struct TrimmedCase {
    pub inputs: TrimmedInputs,
    pub l_over_d: f64,
    pub alpha: f64,
    pub i_h: Option<f64>,
    pub cd: f64,
    pub cl: f64,
    pub cm_residual: f64,
}

#[derive(Debug, Deserialize)]
pub struct SweepInputs {
    pub mach: f64,
    pub altitude: f64,
    pub n_points: i64,
    pub alpha_min: f64,
    pub alpha_max: f64,
}

#[derive(Debug, Deserialize)]
pub struct SweepCase {
    pub inputs: SweepInputs,
    pub alpha: Vec<f64>,
    pub cl: Vec<f64>,
    pub cd: Vec<f64>,
    pub cd_induced: Vec<f64>,
    pub cd_wave: Vec<f64>,
    pub cd_parasite: Vec<f64>,
    pub cm: Vec<f64>,
    pub l_over_d: Vec<f64>,
}

#[derive(Debug, Deserialize)]
pub struct Fixture {
    pub sweep_deg: f64,
    pub airplane: AirplaneFixture,
    pub pg_beta: HashMap<String, PgBetaCase>,
    pub report_alpha: HashMap<String, ReportAlphaCase>,
    pub parasite: HashMap<String, ParasiteCase>,
    pub wave: HashMap<String, WaveCase>,
    pub no_wings: NoWings,
    pub components: HashMap<String, ComponentCase>,
    pub quick: HashMap<String, QuickCase>,
    pub trimmed: HashMap<String, TrimmedCase>,
    pub sweep: HashMap<String, SweepCase>,
}

/// A `null` incidence is a NaN one -- see [`TrimmedInputs::trim_ih_deg`].
pub fn or_nan(value: Option<f64>) -> f64 {
    value.unwrap_or(f64::NAN)
}

/// The nominal aircraft, with or without its nacelles -- the generator's
/// `AircraftBuilder(GeometryConfig()).build(include_engines=...)`.
pub fn build(include_engines: bool) -> Airplane {
    AircraftBuilder::new(Some(GeometryConfig::default()))
        .build(None, include_engines)
        .expect("the default aircraft builds")
}

/// The default analysis configuration at a named mesh resolution.
pub fn analysis_config(spanwise: i64, chordwise: i64) -> AnalysisConfig {
    AnalysisConfig {
        spanwise_resolution: spanwise,
        chordwise_resolution: chordwise,
        ..Default::default()
    }
}

/// An analysis of `plane` at the fixture's sweep, every configuration group
/// at its default -- the generator's `_aero`.
pub fn aero<'a>(
    plane: &'a Airplane,
    fixture: &Fixture,
    analysis: AnalysisConfig,
) -> AeroAnalysis<'a> {
    AeroAnalysis::new(
        plane,
        fixture.sweep_deg,
        Some(GeometryConfig::default()),
        Some(DragModelConfig::default()),
        Some(analysis),
    )
}

/// Sorted case names, so a failure report reads the same way on every run.
pub fn names<T>(cases: &HashMap<String, T>) -> Vec<&String> {
    let mut names: Vec<&String> = cases.keys().collect();
    names.sort();
    names
}
