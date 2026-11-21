// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What `parity_analysis.rs` reads: the shape of `golden/aero/analysis.json`,
//! and the two constructions that have to match the generator's: the
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
use alas_atmo::Atmosphere;
use alas_config::analysis::AnalysisConfig;
use alas_config::geometry::GeometryConfig;
use alas_config::physics::DragModelConfig;
use alas_geom::asb::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_testkit::reference_ambient::reference_ambient;
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

/// A `null` incidence is a NaN one, see [`TrimmedInputs::trim_ih_deg`].
pub fn or_nan(value: Option<f64>) -> f64 {
    value.unwrap_or(f64::NAN)
}

/// The frozen-reference aircraft, with or without its nacelles.
///
/// The product builder intentionally owns a newer transport-planform default;
/// these fixtures were generated before that product correction.  Keep the
/// parity input explicit so a frozen translation test cannot silently compare
/// the product geometry to the historical reference geometry.
pub fn build(include_engines: bool) -> Airplane {
    let mut plane = AircraftBuilder::new_reference_compatibility(Some(GeometryConfig::default()))
        .build(None, include_engines)
        .expect("the default aircraft builds");
    // The frozen aero fixtures predate the projected XY area contract. Restore
    // their historical area scale only here; the builder's lateral/Y b_ref is
    // already the value recorded by that fixture.
    if let Some(wing) = plane.wings.first() {
        let s_ref = wing.unfolded_area();
        plane.s_ref = s_ref;
    }
    plane
}

/// The default analysis configuration at a named mesh resolution.
pub fn analysis_config(spanwise: i64, chordwise: i64) -> AnalysisConfig {
    AnalysisConfig {
        spanwise_resolution: spanwise,
        chordwise_resolution: chordwise,
        ..Default::default()
    }
}

/// The mesh every `golden/aero` fixture was generated at.
///
/// The fixtures were generated with one panel in each direction, and a parity
/// fixture is only evidence if this side meshes the same way. The product
/// default is eight chordwise panels because one panel samples the mean camber
/// line only where it is zero (see `alas_config::analysis`), so these tests
/// state the fixture mesh explicitly rather than inheriting the product
/// default.
pub fn reference_mesh() -> AnalysisConfig {
    analysis_config(1, 1)
}

/// The frozen Python drag-model defaults: viscous margin 1.10 and no
/// lift-dependent viscous term. The product default is the sourced leakage
/// and protuberance allowance of the propulsion class (`DEFAULT_VISCOUS_MARGIN`);
/// the generator's `_aero` still runs the frozen convention.
pub fn frozen_drag_model() -> DragModelConfig {
    DragModelConfig {
        viscous_margin: Some(1.10),
        lift_dependent_viscous_fraction: 0.0,
        ..DragModelConfig::default()
    }
}

/// An analysis of `plane` at the fixture's sweep, every configuration group
/// at its frozen default: the generator's `_aero`.
pub fn aero<'a>(
    plane: &'a Airplane,
    fixture: &Fixture,
    analysis: AnalysisConfig,
) -> AeroAnalysis<'a> {
    AeroAnalysis::new_reference_compatibility(
        plane,
        fixture.sweep_deg,
        Some(GeometryConfig::default()),
        Some(frozen_drag_model()),
        Some(analysis),
    )
}

/// Sorted case names, so a failure report reads the same way on every run.
pub fn names<T>(cases: &HashMap<String, T>) -> Vec<&String> {
    let mut names: Vec<&String> = cases.keys().collect();
    names.sort();
    names
}

/// The altitude at which the product's atmosphere has the static pressure
/// `pressure_pa`, by bisection on the monotone pressure of the standard
/// atmosphere (m, geopotential, between -1 km and 20 km).
fn altitude_of_pressure(pressure_pa: f64) -> f64 {
    let (mut low, mut high) = (-1000.0_f64, 20_000.0_f64);
    for _ in 0..200 {
        let middle = 0.5 * (low + high);
        if Atmosphere::new(middle).pressure() > pressure_pa {
            low = middle;
        } else {
            high = middle;
        }
    }
    0.5 * (low + high)
}

/// An atmosphere that gives the empirical buildup the Reynolds number the
/// reference's fitted atmosphere gave it at `altitude_m`.
///
/// The fixtures were generated with the reference's fitted atmosphere, which
/// differs from the U.S. Standard Atmosphere, 1976 the product evaluates by up
/// to 1.9 % in temperature and 1.7 % in density near 11 km. The buildup reads
/// the ambient state only through `Re / (M l) = rho a / mu`, so that is the
/// quantity to restore. The reference's state at the altitude is read from
/// `golden/atmo/differentiable.json`. The product's standard-day state is
/// moved through an explicit state, a pressure level and a temperature
/// deviation: the temperature is the reference's (so the viscosity is, by
/// Sutherland's law, which both evaluate with the same constants), and the
/// pressure level is the one at which `rho a = p sqrt(gamma / (R T))`, with
/// the product's gas constant, equals the reference's. The product's gas
/// constant (287.0528742 J/(kg K)) differs from the reference's (287.0530720)
/// by 6.9e-7, so the pressure of the level is lower than the reference's by
/// 3.4e-7 relative; matching the pressure instead would leave a 3.4e-7
/// difference in the Reynolds number, which is outside `Tier::Closed`.
pub fn reference_reynolds_atmosphere(altitude_m: f64) -> Atmosphere {
    let reference = reference_ambient(altitude_m);
    let level = Atmosphere::new(altitude_m);
    let gas_constant = alas_atmo::GAS_CONSTANT_AIR;
    let gamma = level.ratio_of_specific_heats();
    let density_speed = reference.density_kg_m3 * reference.speed_of_sound_m_s;
    let pressure = density_speed / (gamma / (gas_constant * reference.temperature_k)).sqrt();
    let level = Atmosphere::new(altitude_of_pressure(pressure));
    let atmosphere =
        level.with_temperature_deviation(reference.temperature_k - level.temperature());
    let got = atmosphere.density() * atmosphere.speed_of_sound() / atmosphere.dynamic_viscosity();
    let want = density_speed / reference.dynamic_viscosity_pa_s;
    assert!(
        (got / want - 1.0).abs() < 1e-12,
        "rho a / mu at {altitude_m} m: {got} against the recorded {want}"
    );
    atmosphere
}
