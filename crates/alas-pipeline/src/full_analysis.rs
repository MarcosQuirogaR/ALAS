// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! High-fidelity aerodynamic, mass and stability analysis of a candidate design.
//!
//! [`FullAnalysis::run`] builds the 3-D aircraft geometry, calculates the
//! two-pass mass distribution (lumped first, then detailed cabin layout),
//! sets the moment reference to the true physical centre of gravity, runs a
//! fine-mesh vortex-lattice polar sweep, fits the drag polar, finds the
//! operating design point, evaluates neutral point and static margin, checks
//! typed model-derived CG and ground-reaction constraints, and solves the
//! trimmed cruise point.

use std::collections::HashMap;
use std::f64::consts::PI;

use alas_aero::analysis::{AeroAnalysis, PolarSweep, TrimPoint};
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_config::design_variables::DesignVector;
use alas_config::{presets, AlasConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{FlopsMassBuildup, MassCoordinateModel};
use alas_math::lstsq::least_squares;
use alas_opt::envelope::{assess_model_cg_envelope, check_cg_envelope};
use alas_payload::build::{build_payload_layout, build_payload_layout_reference_compatibility};
use alas_payload::layout::PayloadLayout;
use alas_payload::oew::oew_and_cg;
use alas_stab::neutral_point::NeutralPointConditions;
use alas_stab::trim::{
    neutral_point_reference_compatibility_with_system, neutral_point_with_system,
    stability_and_trim_reference_compatibility_with_system, stability_and_trim_with_system,
};
use serde::{Deserialize, Serialize};

/// Operating conditions at the cruise design point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DesignPoint {
    /// Angle of attack, in degrees.
    pub alpha_deg: f64,
    /// Lift coefficient.
    pub cl: f64,
    /// Total drag coefficient.
    pub cd: f64,
    /// Lift-to-drag ratio.
    pub l_over_d: f64,
}

/// Genuinely trimmed cruise operating point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrimmedDesignPoint {
    /// Compressibility-corrected trim angle shown in summaries, in degrees.
    ///
    /// This is a presentation quantity. It is intentionally distinct from
    /// [`Self::geometric_body_alpha_deg`]: a 2-D section solver and an
    /// aircraft-attitude requirement must use the geometric VLM angle rather
    /// than this Prandtl-Glauert display correction.
    pub alpha_deg: f64,
    /// Geometric aircraft-body angle passed to the trimmed VLM solve, in
    /// degrees.
    ///
    /// Positive values raise the aircraft nose relative to the freestream.
    /// Root incidence, local washout, and induced angle are not included;
    /// consumers that need a local section condition must add them explicitly.
    pub geometric_body_alpha_deg: f64,
    /// Trimmed horizontal-stabilizer incidence, in degrees.
    pub trim_ih_deg: f64,
    /// Lift coefficient.
    pub cl: f64,
    /// Total drag coefficient.
    pub cd: f64,
    /// Lift-to-drag ratio.
    pub l_over_d: f64,
    /// Residual pitching moment (should be close to zero).
    pub cm_residual: f64,
}

/// Provenance of a least-squares parabolic fit of the clean polar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolarFitStatus {
    /// The configured fit window produced a successful least-squares fit.
    Fitted,
    /// The configured window was too small, so the documented fallback
    /// window produced the fit instead.
    FittedFallbackWindow,
    /// The configured and fallback windows did not contain enough points;
    /// the historical constant coefficients were retained.
    FallbackInsufficientPoints,
    /// The selected points could not be solved by least squares; the
    /// historical constant coefficients were retained.
    FallbackLeastSquaresFailure,
}

impl PolarFitStatus {
    /// Stable status text for reports and machine-readable exports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fitted => "fitted",
            Self::FittedFallbackWindow => "fitted_fallback_window",
            Self::FallbackInsufficientPoints => "fallback_insufficient_points",
            Self::FallbackLeastSquaresFailure => "fallback_least_squares_failure",
        }
    }
}

/// Least-squares parabolic fit of the clean polar: `CD = CD0 + k * CL^2`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PolarFit {
    /// Zero-lift parasitic drag coefficient.
    pub cd0: f64,
    /// Induced drag factor `k`.
    pub k: f64,
    /// Oswald efficiency factor `e = 1 / (pi * AR * k)`.
    pub oswald_e: f64,
    /// Wing aspect ratio.
    pub aspect_ratio: f64,
    /// Provenance of the fit coefficients, including any retained fallback.
    pub status: PolarFitStatus,
}

/// Complete aerodynamic, mass, and stability report of an aircraft design.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisReport {
    /// The input design vector.
    pub design: DesignVector,
    /// The built 3-D aircraft geometry.
    pub airplane: Airplane,
    /// Fine-resolution polar sweep.
    pub polar: PolarSweep,
    /// Un-trimmed operating point closest to cruise `CL_required`.
    pub design_point: DesignPoint,
    /// Parabolic polar fit.
    pub polar_fit: PolarFit,
    /// Aerodynamic static margin `(x_np - x_cg) / c_ref`.
    pub static_margin: f64,
    /// Neutral point longitudinal position, in meters.
    pub x_neutral_point: f64,
    /// Scalar geometry summary metrics.
    pub geometry_summary: HashMap<String, f64>,
    /// Breakdown of masses by component name, in kg.
    pub component_masses: HashMap<String, f64>,
    /// The verified pure-FLOPS groups that produced `component_masses`, when
    /// this report used the production architecture. Keeping the grouped
    /// evaluation on the report lets the item-level ledger consume the same
    /// result instead of re-running or relabelling a lumped approximation.
    pub flops_mass_buildup: Option<Box<FlopsMassBuildup>>,
    /// Centroid positions by component name, in meters `[x, y, z]`.
    pub mass_coordinates: HashMap<String, [f64; 3]>,
    /// Global mass-weighted center of gravity `[x, y, z]`, in meters.
    pub physical_cg: [f64; 3],
    /// Detailed cabin and cargo payload layout, if successfully constructed.
    pub payload_layout: Option<PayloadLayout>,
    /// Jointly trimmed cruise operating point, if trim solve converged.
    pub trimmed_design_point: Option<TrimmedDesignPoint>,
    /// True if the active model CG and gear constraints are compliant.
    ///
    /// Reference-compatibility analyses preserve the frozen Python Boolean;
    /// product analyses use the hard physical floor and typed gear checks.
    pub cg_envelope_ok: Option<bool>,
    /// Neutral-point conditions set beside `x_neutral_point` (`ac.md`).
    pub neutral_point_conditions: Option<NeutralPointConditions>,
    /// The fuel model this report prices its fuel with: the sized
    /// candidate's carried artifacts, or the baseline aircraft's built on
    /// first use (`crate::fuel_model`).
    pub fuel: crate::fuel_model::ReportFuel,
}

impl AnalysisReport {
    /// The mission-sized takeoff mass this report was bound to, in kg, when it
    /// came from a sized run (`FullAnalysis::run_at_sized_takeoff_mass`);
    /// `None` for an unsized report, whose only mass basis is the declared
    /// `requirements.mtow_kg`. Every downstream consumer reads its takeoff
    /// mass here so the declared limit is never mistaken for the flown mass.
    pub fn sized_takeoff_mass_kg(&self) -> Option<f64> {
        let is_sized = self
            .geometry_summary
            .get("analysis_mass_basis_is_sized")
            .is_some_and(|value| value.is_finite() && *value > 0.5);
        if !is_sized {
            return None;
        }
        self.geometry_summary
            .get("analysis_mass_basis_kg")
            .copied()
            .filter(|value| value.is_finite() && *value > 0.0)
    }

    /// The design landing mass `WLDG`, kg, the sized run designed its gear and
    /// structure for (including the `ZFW + reserves` floor of the MTOW band
    /// and payload-adjusted modes); `None` for an unsized report or a sizing
    /// plan that couples the landing mass to the closure by ratio alone.
    pub fn design_landing_mass_kg(&self) -> Option<f64> {
        self.sized_takeoff_mass_kg()?;
        self.geometry_summary
            .get("analysis_design_landing_mass_kg")
            .copied()
            .filter(|value| value.is_finite() && *value > 0.0)
    }

    /// The takeoff mass a downstream figure or export analyses, in kg: the
    /// sized mass when the report carries one, else the declared
    /// `fallback_mtow_kg` (the unsized, pre-optimization basis).
    pub fn analysis_takeoff_mass_kg(&self, fallback_mtow_kg: f64) -> f64 {
        self.sized_takeoff_mass_kg().unwrap_or(fallback_mtow_kg)
    }
}

/// Evaluates high-fidelity multidisciplinary analyses for candidate aircraft designs.
#[derive(Debug, Clone)]
pub struct FullAnalysis {
    /// Configuration governing geometry, requirements, analysis fidelity, and mass models.
    pub config: AlasConfig,
    reference_compatibility: bool,
}

mod maps;
mod np_conditions;
mod station_coordinates;
use maps::{breakdown_to_map, coordinates_to_map};
pub(crate) use station_coordinates::station_coordinates_for;
pub use station_coordinates::StationPlacementFailure;

mod polar_point;
use polar_point::design_point_nearest;

/// The one-cabin-per-case rule, shared rather than mirrored.
///
/// `alas-report`'s quick preview runs the same two mass passes this module
/// does and has to apply the same rule, or its operating empty mass prices a
/// different cabin from the payload it draws beside it (measured on the
/// A320-200 as a 30-seat difference). Duplicating the rule in `alas-report`
/// is what produced that class of divergence in the first place, so the
/// module is public and there is one implementation. It cannot live in
/// `alas-mass` instead: `cabin_synchronized` reads an
/// `alas_payload::layout::PayloadLayout` and `alas-payload` already depends
/// on `alas-mass`, so that direction is a cycle.
pub mod cabin_sync;

mod design_point;
mod payload_pass;
mod run;
#[cfg(test)]
// Failed expectations and unwraps here are failed test assertions.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

pub(crate) use run::effective_structural_payload_limit_kg;
