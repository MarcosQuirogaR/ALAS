// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/analysis/airfoil_screening.py
// Reference: alas @ rust-port-baseline.

//! Data structures and constants for airfoil database screening.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Cruise Mach at/above which 2-D NeuralFoil and 3-D VLM stop capturing wave drag.
pub const TRANSONIC_MACH_CAVEAT: f64 = 0.75;

/// Real, wind-tunnel-validated transonic and supercritical reference sections.
pub const REFERENCE_AIRFOILS: &[&str] = &[
    "whitcomb", "rae2822", "sc20406", "sc20410", "sc20412", "sc20414", "sc20606", "sc20610",
    "sc20612", "sc20614", "sc20706", "sc20710", "sc20712", "sc20714", "sc21006", "sc21010",
];

/// A candidate's real trimmed CL must land within this fraction of `cl_target`.
pub const CL_FEASIBILITY_TOL: f64 = 0.02;

/// Minimum NeuralFoil confidence admitted to the ranking stage.
///
/// NeuralFoil exposes this value as an out-of-distribution signal rather than
/// as a universal probability.  ALAS therefore uses a deliberately
/// conservative, model-facing floor: values below five percent are diagnostic
/// extrapolations and cannot enter an aerodynamic ranking.  The raw minimum
/// is retained on every successful result so a later campaign can audit the
/// sensitivity without rerunning the model.
pub const MIN_NEURALFOIL_ANALYSIS_CONFIDENCE: f64 = 0.05;

/// Maximum absolute pitching-moment residual admitted by a 3-D screening trim.
///
/// Screening is a gate, not a final flight-dynamics solve.  A two-count
/// residual accommodates the deliberately coarse 3-D probe/interpolation,
/// while rejecting singular/fallback trims that merely happen to return
/// finite numbers.  The value is explicit so a higher-fidelity campaign can
/// tighten it without changing the closure logic.
pub const TRIM_CM_RESIDUAL_TOL: f64 = 2.0e-3;

/// Maximum allowed trim-alpha extrapolation outside the stability probe window.
pub const TRIM_ALPHA_SLACK_DEG: f64 = 12.0;

/// User-selected ranking priority for an otherwise fixed flight condition.
///
/// The objective changes ranking only. Mach, Reynolds number, and the target
/// lift coefficient remain explicit analysis inputs, so a ranking preference
/// cannot silently create a different aerodynamic operating point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreeningObjective {
    /// Retain the established efficiency-and-fuel-capacity blend.
    Balanced,
    /// Rank by 2-D or 3-D aerodynamic efficiency alone.
    Efficiency,
    /// Rank by wing fuel capacity alone.
    FuelCapacity,
    /// Rank by drag-bucket retention around the target lift coefficient.
    Robustness,
}

impl ScreeningObjective {
    /// Stable label for the UI and persisted run summaries.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Balanced => "balanced",
            Self::Efficiency => "efficiency",
            Self::FuelCapacity => "fuel_capacity",
            Self::Robustness => "robustness",
        }
    }

    /// Ranking weights implied by this objective.
    pub const fn weights(self, balanced: (f64, f64, f64)) -> (f64, f64, f64) {
        match self {
            Self::Balanced => balanced,
            Self::Efficiency => (1.0, 0.0, 0.0),
            Self::FuelCapacity => (0.0, 1.0, 0.0),
            Self::Robustness => (0.0, 0.0, 1.0),
        }
    }
}

/// Flow regime determined from the supplied section Mach number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ScreeningFlowRegime {
    /// The effective section Mach is below the transonic caveat threshold.
    #[default]
    Subcritical,
    /// The effective section Mach reaches the transonic caveat threshold.
    Transonic,
}

impl ScreeningFlowRegime {
    /// Classify the flow regime without modifying the operating condition.
    pub fn from_section_mach(section_mach: f64) -> Self {
        if section_mach >= TRANSONIC_MACH_CAVEAT {
            Self::Transonic
        } else {
            Self::Subcritical
        }
    }

    /// Stable label for user-facing evidence.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Subcritical => "subcritical",
            Self::Transonic => "transonic",
        }
    }
}

/// One screened airfoil's evaluation outcome across 2-D, 3-D, and MSES stages.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AirfoilCandidateResult {
    /// Identifier name of the airfoil in the library.
    pub name: String,
    /// Evaluation status ("ok" or "error").
    pub status: String,
    /// Error message if evaluation failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Stage 1 (2-D) lift-to-drag ratio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l_over_d: Option<f64>,
    /// Design cruise lift coefficient.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cl: Option<f64>,
    /// Stage 1 (2-D) drag coefficient at design CL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cd: Option<f64>,
    /// Stage 1 angle of attack at design CL, degrees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha_deg: Option<f64>,
    /// Minimum NeuralFoil analysis confidence over the swept alpha range.
    ///
    /// This is retained even though the candidate is admitted only when it
    /// clears [`MIN_NEURALFOIL_ANALYSIS_CONFIDENCE`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_confidence: Option<f64>,
    /// Maximum thickness-to-chord fraction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_thickness_frac: Option<f64>,
    /// Wing fuel tank volume with this root section, m^3.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tank_volume_m3: Option<f64>,
    /// Wing fuel tank capacity in kg.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tank_capacity_kg: Option<f64>,
    /// Stage 1 blended ranking score.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// Off-design robustness: mean L/D across cl_target +/- cl_band divided by at-target L/D.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub robustness: Option<f64>,
    /// Whether Stage 2 (3-D wing) re-simulation was performed and succeeded.
    #[serde(default)]
    pub refined: bool,
    /// Stage 2 (3-D wing) trimmed lift-to-drag ratio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l_over_d_3d: Option<f64>,
    /// Stage 2 trimmed drag coefficient.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cd_3d: Option<f64>,
    /// Stage 2 trimmed angle of attack, degrees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha_3d_deg: Option<f64>,
    /// Stage 2 pitching-moment residual at the trim point.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cm_residual_3d: Option<f64>,
    /// Stage 2 static margin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub static_margin_3d: Option<f64>,
    /// Stage 2 blended ranking score.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_3d: Option<f64>,
    /// Error message if Stage 2 refinement failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refine_error: Option<String>,
    /// Whether Stage 3 (MSES) viscous/wave solve was performed and succeeded.
    #[serde(default)]
    pub mses_verified: bool,
    /// Stage 3 (MSES) lift-to-drag ratio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l_over_d_mses: Option<f64>,
    /// Stage 3 (MSES) total drag coefficient.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cd_mses: Option<f64>,
    /// Stage 3 (MSES) wave drag coefficient.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cdw_mses: Option<f64>,
    /// Stage 3 (MSES) status string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mses_status: Option<String>,
    /// Stage 3 (MSES) error message if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mses_error: Option<String>,
    /// Whether this candidate is a curated reference airfoil.
    #[serde(default)]
    pub is_reference: bool,
}

/// JSON-safe summary of a full screening run across all stages.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AirfoilScreeningResult {
    /// Baseline airfoil name before screening.
    pub baseline_airfoil: String,
    /// Evaluated cruise Mach number.
    pub cruise_mach: f64,
    /// Evaluated cruise Reynolds number.
    pub cruise_reynolds: f64,
    /// Evaluated cruise altitude in meters.
    pub cruise_altitude_m: f64,
    /// Target level-flight cruise lift coefficient.
    pub cl_target: f64,
    /// True when the user replaced the live level-flight CL with `target_cl`.
    pub uses_explicit_target_cl: bool,
    /// Section Mach after the live wing sweep correction.
    pub section_mach: f64,
    /// Flow regime derived from `section_mach`; it is not a ranking setting.
    pub flow_regime: ScreeningFlowRegime,
    /// True when Mach >= TRANSONIC_MACH_CAVEAT.
    pub transonic_caveat: bool,
    /// Whether Stage 2 3-D re-ranking ran.
    pub refined_3d: bool,
    /// Total number of candidates examined.
    pub n_total: usize,
    /// Number of candidates that passed Stage 1 successfully.
    pub n_ok: usize,
    /// Number of candidates that errored out in Stage 1.
    pub n_error: usize,
    /// Number of candidates refined in Stage 2.
    pub n_refined: usize,
    /// Number of candidates verified with MSES in Stage 3.
    pub n_mses_verified: usize,
    /// Whether the screening was cancelled early.
    pub cancelled: bool,
    /// Ranked list of top candidates.
    pub candidates: Vec<AirfoilCandidateResult>,
    /// Diagnostic sample of failed candidates.
    pub errors: Vec<HashMap<String, String>>,
}

/// Options configuring the multi-stage airfoil screening sweep.
#[derive(Debug, Clone, PartialEq)]
pub struct AirfoilScreeningOptions {
    /// Ranking objective selected by the user.
    pub objective: ScreeningObjective,
    /// Optional explicit section target CL; `None` uses the live level-flight value.
    pub target_cl: Option<f64>,
    /// Weight on lift-to-drag ratio in the blended score (default: 0.7).
    pub ld_weight: f64,
    /// Weight on fuel tank volume in the blended score (default: 0.3).
    pub fuel_weight: f64,
    /// Weight on drag bucket flatness robustness (default: 0.0).
    pub robustness_weight: f64,
    /// CL band for off-design robustness evaluation (default: 0.05).
    pub cl_band: f64,
    /// Number of top candidates to return (default: 50).
    pub top_n: usize,
    /// NeuralFoil model size (default: "large").
    pub model_size: String,
    /// Sweep minimum angle of attack, degrees (default: -4.0).
    pub alpha_min_deg: f64,
    /// Sweep maximum angle of attack, degrees (default: 14.0).
    pub alpha_max_deg: f64,
    /// Sweep angle of attack step, degrees (default: 0.5).
    pub alpha_step_deg: f64,
    /// Minimum allowed thickness-to-chord fraction (default: 0.005).
    pub min_tc: f64,
    /// Maximum allowed thickness-to-chord fraction (default: 0.25).
    pub max_tc: f64,
    /// Filter pattern for candidate names (globs / comma-separated substrings).
    pub name_filter: String,
    /// Whether to run Stage 2 3-D refinement (default: true).
    pub refine_3d: bool,
    /// Number of top candidates to refine in Stage 2 (default: 20).
    pub refine_top_n: usize,
    /// Optional minimum static margin required in Stage 2.
    pub min_static_margin: Option<f64>,
    /// Whether to run Stage 3 MSES verification (default: true).
    pub verify_mses: bool,
    /// Number of Stage 2 survivors to verify with MSES (default: 5).
    pub mses_top_n: usize,
}

impl Default for AirfoilScreeningOptions {
    fn default() -> Self {
        Self {
            objective: ScreeningObjective::Balanced,
            target_cl: None,
            ld_weight: 0.7,
            fuel_weight: 0.3,
            robustness_weight: 0.0,
            cl_band: 0.05,
            top_n: 50,
            model_size: "large".to_string(),
            alpha_min_deg: -4.0,
            alpha_max_deg: 14.0,
            alpha_step_deg: 0.5,
            min_tc: 0.005,
            max_tc: 0.25,
            name_filter: String::new(),
            refine_3d: true,
            refine_top_n: 20,
            min_static_margin: None,
            verify_mses: true,
            mses_top_n: 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ScreeningFlowRegime, ScreeningObjective, TRANSONIC_MACH_CAVEAT};

    #[test]
    fn objective_selection_changes_ranking_weights_without_changing_the_condition() {
        assert_eq!(
            ScreeningObjective::Balanced.weights((0.7, 0.3, 0.1)),
            (0.7, 0.3, 0.1)
        );
        assert_eq!(
            ScreeningObjective::Efficiency.weights((0.7, 0.3, 0.1)),
            (1.0, 0.0, 0.0)
        );
        assert_eq!(
            ScreeningObjective::FuelCapacity.weights((0.7, 0.3, 0.1)),
            (0.0, 1.0, 0.0)
        );
        assert_eq!(
            ScreeningObjective::Robustness.weights((0.7, 0.3, 0.1)),
            (0.0, 0.0, 1.0)
        );
    }

    #[test]
    fn flow_regime_is_an_observed_section_condition_not_a_ranker_choice() {
        assert_eq!(
            ScreeningFlowRegime::from_section_mach(TRANSONIC_MACH_CAVEAT - 1e-6),
            ScreeningFlowRegime::Subcritical
        );
        assert_eq!(
            ScreeningFlowRegime::from_section_mach(TRANSONIC_MACH_CAVEAT),
            ScreeningFlowRegime::Transonic
        );
    }
}
