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

use std::collections::BTreeMap;
use std::f64::consts::PI;

use alas_aero::analysis::{AeroAnalysis, PolarSweep, TrimPoint};
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_config::design_variables::DesignVector;
#[cfg(test)]
use alas_config::presets;
use alas_config::AlasConfig;
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
    /// The configured and fallback windows did not contain enough points:
    /// the fit failed and its coefficients are not a number.
    FallbackInsufficientPoints,
    /// The selected points could not be solved by least squares (a
    /// non-finite sample or a rank-deficient system): the fit failed and its
    /// coefficients are not a number.
    FallbackLeastSquaresFailure,
    /// The least-squares solve returned a non-positive zero-lift drag or a
    /// non-positive curvature: the fit failed and its coefficients are not a
    /// number.
    NonPhysicalCoefficients,
}

impl PolarFitStatus {
    /// Stable status text for reports and machine-readable exports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fitted => "fitted",
            Self::FittedFallbackWindow => "fitted_fallback_window",
            Self::FallbackInsufficientPoints => "failed_insufficient_points",
            Self::FallbackLeastSquaresFailure => "failed_least_squares",
            Self::NonPhysicalCoefficients => "failed_non_physical_coefficients",
        }
    }

    /// Whether the coefficients are a least-squares fit of the polar. A
    /// failed fit carries no coefficients.
    #[must_use]
    pub const fn is_fitted(self) -> bool {
        matches!(self, Self::Fitted | Self::FittedFallbackWindow)
    }
}

/// Read a coefficient a failed fit wrote as JSON `null` back as not a number.
fn coefficient_or_nan<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<f64>::deserialize(deserializer)?.unwrap_or(f64::NAN))
}

/// Least-squares clean polar: `CD = CD0 + c1 CL + k CL^2`, wave drag excluded.
/// Camber and fixed tail incidence can shift the minimum away from zero lift.
/// Frozen parity fits keep the two-term fit of the total drag.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PolarFit {
    /// Zero-lift intercept of the wave-free drag, zero-lift wake included.
    /// Not a number when the fit failed ([`PolarFitStatus::is_fitted`]).
    #[serde(deserialize_with = "coefficient_or_nan")]
    pub cd0: f64,
    /// Linear lift term of the shifted polar; zero on frozen parity fits.
    /// Not a number when the fit failed.
    #[serde(default, deserialize_with = "coefficient_or_nan")]
    pub c1: f64,
    /// Quadratic curvature `k` of the wave-free drag. Not a number when the
    /// fit failed.
    #[serde(deserialize_with = "coefficient_or_nan")]
    pub k: f64,
    /// Curvature factor `e = 1 / (pi AR k)` of the wave-free polar; distinct
    /// from wake span efficiency, as it carries the lift-dependent viscous drag.
    /// Not a number when the fit failed.
    #[serde(deserialize_with = "coefficient_or_nan")]
    pub oswald_e: f64,
    /// Wing aspect ratio.
    pub aspect_ratio: f64,
    /// Provenance of the fit coefficients, including any retained fallback.
    pub status: PolarFitStatus,
}

impl PolarFit {
    /// Fitted wave-free drag at a lift coefficient, including the shifted minimum.
    pub fn cd(&self, cl: f64) -> f64 {
        self.cd0 + cl * (self.c1 + self.k * cl)
    }
}

/// Complete aerodynamic, mass, and stability report of an aircraft design.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
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
    pub geometry_summary: BTreeMap<String, f64>,
    /// Breakdown of masses by component name, in kg.
    pub component_masses: BTreeMap<String, f64>,
    /// The verified pure-FLOPS groups that produced `component_masses`, when
    /// this report used the production architecture. Keeping the grouped
    /// evaluation on the report lets the item-level ledger consume the same
    /// result instead of re-running or relabelling a lumped approximation.
    pub flops_mass_buildup: Option<Box<FlopsMassBuildup>>,
    /// Centroid positions by component name, in meters `[x, y, z]`.
    pub mass_coordinates: BTreeMap<String, [f64; 3]>,
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

    /// The takeoff mass of the fuel load the report's masses carry, in kg:
    /// zero-fuel mass plus `min(MTOW - ZFW, usable capacity)` when the
    /// analysis loaded the tanks itself, else
    /// [`Self::analysis_takeoff_mass_kg`]. Below the MTOW limit only when the
    /// usable tanks cap the fuel, which the report flags as volume-limited.
    pub fn loaded_takeoff_mass_kg(&self, fallback_mtow_kg: f64) -> f64 {
        self.geometry_summary
            .get("analysis_loaded_takeoff_mass_kg")
            .copied()
            .filter(|kg| kg.is_finite() && *kg > 0.0)
            .unwrap_or_else(|| self.analysis_takeoff_mass_kg(fallback_mtow_kg))
    }

    /// Aircraft takeoff-weight limit, kg, distinct from its analyzed load.
    /// Payload-range corners and dispatch limits use this capability bound.
    pub fn aircraft_mtow_limit_kg(&self, fallback_mtow_kg: f64) -> f64 {
        self.geometry_summary
            .get("analysis_mtow_limit_kg")
            .copied()
            .filter(|kg| kg.is_finite() && *kg > 0.0)
            .unwrap_or(fallback_mtow_kg)
    }
}

/// Evaluates high-fidelity multidisciplinary analyses for candidate aircraft designs.
#[derive(Debug, Clone)]
pub struct FullAnalysis {
    /// Configuration governing geometry, requirements, analysis fidelity, and mass models.
    pub config: AlasConfig,
    /// The frozen reference replay: its geometry builder, unfolded reference
    /// area, tail pressure ratio, aerodynamics and trim. Only
    /// [`Self::new_reference_compatibility`] sets it.
    reference_compatibility: bool,
    /// The retained comparison mass methods, on whatever aircraft the run
    /// analyses: a mass model never changes the wing the aerodynamics sees.
    reference_masses: bool,
    /// Whether the configuration is a sized closure's, whose takeoff fuel is
    /// the closure's own rather than the maximum the declared MTOW admits.
    closure_bound: bool,
    /// Whether and when a load plan replaces the nominal payload placement.
    load_plan: LoadPlanPolicy,
    /// The operator's take-off CG target, % MAC of the model frame; the plan
    /// aims for the envelope midpoint when `None`.
    operator_cg_target_pct_mac: Option<f64>,
    /// The sized candidate's closure fuel every report of this analysis
    /// carries from the start ([`Self::run_sized_candidate`]), so the load
    /// plan judges the CG envelope on the closure the final feasibility
    /// judges instead of closing the same aircraft again per report.
    sized_fuel: Option<crate::fuel_model::SizedFuel>,
}

mod load_plan;
pub use load_plan::LoadPlanPolicy;

impl FullAnalysis {
    /// This analysis with the nominal payload placement only (no load plan).
    #[must_use]
    pub fn without_load_plan(mut self) -> Self {
        self.load_plan = LoadPlanPolicy::Off;
        self
    }

    /// This analysis with `policy` deciding when the load plan applies.
    #[must_use]
    pub fn with_load_plan(mut self, policy: LoadPlanPolicy) -> Self {
        self.load_plan = policy;
        self
    }

    /// This analysis with a declared operator take-off CG target, % MAC of
    /// the model frame, in place of the envelope midpoint.
    #[must_use]
    pub fn with_operator_cg_target_pct_mac(mut self, pct_mac: Option<f64>) -> Self {
        self.operator_cg_target_pct_mac = pct_mac;
        self
    }
}

mod maps;
mod mass_pass;
mod np_conditions;
mod station_coordinates;
use maps::{breakdown_to_map, coordinates_to_map};
pub(crate) use station_coordinates::station_coordinates_for;
pub use station_coordinates::StationPlacementFailure;

mod polar_axis;
pub(crate) use polar_axis::run_polar;
mod polar_point;
use polar_point::{design_point_at_cl, design_point_nearest};

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

mod cruise_condition;
mod cruise_point;
pub(crate) use cruise_point::summary_lines as cruise_point_summary_lines;
pub use cruise_point::{CruisePointBasis, CruisePointMission};
mod route_point;
pub use route_point::{route_cruise_point, RouteCruisePoint};
mod design_point;
mod flown_polar;
use alas_opt::mdo::payload_limit;
mod payload_pass;
use alas_opt::mdo::route_payload_cap;
mod run;
mod takeoff_loading;
#[cfg(test)]
// Failed expectations and unwraps here are failed test assertions.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

pub(crate) use payload_limit::effective_structural_payload_limit_kg;
pub(crate) use route_payload_cap::ROUTE_PAYLOAD_OFFLOADED_KEY;

/// `geometry_summary` key present (as 1.0) only when the reported cruise
/// point is not the polar at the cruise lift coefficient: the lift lies
/// outside the analysed sweep, the trimmed point was clamped, or the sweep
/// could not be interpolated and its nearest sample was reported instead.
pub(crate) const CRUISE_CL_OUTSIDE_POLAR_KEY: &str = "reported_cruise_cl_outside_polar";
/// `geometry_summary` key: the cruise lift coefficient the report asked the
/// polar for, written with [`CRUISE_CL_OUTSIDE_POLAR_KEY`].
pub(crate) const CRUISE_CL_REQUESTED_KEY: &str = "reported_cruise_cl_requested";
/// `geometry_summary` keys: the lift-coefficient range of the analysed
/// polar, written with [`CRUISE_CL_OUTSIDE_POLAR_KEY`].
pub(crate) const POLAR_CL_MIN_KEY: &str = "analysed_polar_cl_min";
pub(crate) const POLAR_CL_MAX_KEY: &str = "analysed_polar_cl_max";

/// The warning text for a reported cruise point that is not the polar at the
/// cruise lift coefficient, or `None` when the report carries no such mark.
pub fn cruise_point_off_polar_note(report: &AnalysisReport) -> Option<String> {
    let summary = &report.geometry_summary;
    summary.get(CRUISE_CL_OUTSIDE_POLAR_KEY)?;
    let value = |key: &str| summary.get(key).copied().unwrap_or(f64::NAN);
    Some(format!(
        "the cruise lift coefficient {:.3} is not a point of the analysed polar (CL {:.3} to \
         {:.3}): the reported cruise drag and L/D are the polar's value at CL {:.3}, not at the \
         cruise lift",
        value(CRUISE_CL_REQUESTED_KEY),
        value(POLAR_CL_MIN_KEY),
        value(POLAR_CL_MAX_KEY),
        report.design_point.cl,
    ))
}
