// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Result and error types of the model-derived CG envelope assessment.

use super::{ModelCgConstraintAssessment, PhysicalCgLimits};
use alas_mass::stations::StationError;

/// Outcome of the frozen Python-compatible envelope check.
///
/// This type exists for reference parity. Product code uses
/// [`ModelCgEnvelopeAssessment`], whose typed constraints do not conflate the
/// preferred static margin with the hard stability floor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CgEnvelopeResult {
    /// True if any loading state falls outside its dynamic [fwd, aft] limits.
    pub violation: bool,
    /// Maximum fraction of MAC by which a CG limit was violated (0.0 if compliant).
    pub worst_exceedance: f64,
}

/// Named loading state evaluated by the model-derived assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCgLoadingState {
    /// Operating empty weight.
    OperatingEmpty,
    /// Zero-fuel load actually analyzed from OEW plus modeled payload.
    AnalyzedZeroFuel,
    /// Mid-mission load with half of the analyzed usable fuel remaining.
    OperationalMidMission,
    /// Reserve/arrival loading case with ten percent of analyzed fuel remaining.
    OperationalReserve,
    /// Takeoff load actually analyzed after any usable-fuel cap is applied.
    AnalyzedTakeoff,
}

impl ModelCgLoadingState {
    /// Stable report label for this loading state.
    pub const fn label(self) -> &'static str {
        match self {
            Self::OperatingEmpty => "OEW",
            Self::AnalyzedZeroFuel => "analyzed ZFW",
            Self::OperationalMidMission => "operational mid-mission",
            Self::OperationalReserve => "operational reserve",
            Self::AnalyzedTakeoff => "analyzed TOW",
        }
    }
}

/// Model quantities and constraint verdicts for one loading state.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelCgLoadingAssessment {
    /// Loading state identifier.
    pub state: ModelCgLoadingState,
    /// Total mass in this state, in kilograms.
    pub mass_kg: f64,
    /// Longitudinal center of gravity from the model aircraft nose, in meters.
    pub cg_x_m: f64,
    /// Center of gravity in the built aerodynamic model's MAC frame.
    pub cg_pct_mac: f64,
    /// Static margin from the common modeled neutral point.
    pub static_margin: f64,
    /// Nose-gear reaction divided by this loading state's total weight.
    pub nose_gear_load_fraction: f64,
    /// Whether this state's ground reactions are inside the validity domain
    /// of the two-group static split that produced them.
    ///
    /// `N_nose = m (x_mlg - x_cg) / (x_mlg - x_nlg)` and
    /// `N_main = m - N_nose` describe an aeroplane resting on both gear
    /// groups **in compression**. With the centre of gravity aft of the
    /// effective main-gear station the nose reaction comes out negative and
    /// the main reaction exceeds the whole weight: the aeroplane sits on its
    /// tail, the nose leg carries nothing, and neither number is a load the
    /// gear actually sees. When this is `false` the two strength assessments
    /// are omitted from [`Self::constraints`] rather than reported, because a
    /// negative reaction trivially satisfies a rated-capacity upper bound and
    /// would otherwise print as a passing strength margin.
    /// [`ModelCgConstraint::MinimumNoseGearLoad`] is the constraint that
    /// describes this state and is always retained.
    pub ground_reactions_admissible: bool,
    /// This state's physical aft/forward CG boundaries and which mechanism
    /// governs each: weight/`h_cg`
    /// dependent, so reported per state rather than once for the design.
    pub physical_limits: PhysicalCgLimits,
    /// Independently evaluated hard constraints.
    pub constraints: Vec<ModelCgConstraintAssessment>,
}

/// Non-governing optimizer preference evaluated at the analyzed takeoff point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StaticMarginPreferenceAssessment {
    /// Analyzed takeoff model static margin, as a fraction of MAC.
    pub actual: f64,
    /// Configured optimizer target, as a fraction of MAC.
    pub target: f64,
    /// Signed difference `actual - target`.
    pub deviation: f64,
}

impl StaticMarginPreferenceAssessment {
    /// Whether the analyzed takeoff point is at or above the preferred buffer.
    pub fn met_or_exceeded(self) -> bool {
        self.actual >= self.target
    }
}

/// Which physical mechanism actually bounds the aft end of the CG envelope.
///
/// A transport's aft centre-of-gravity limit is the **more forward** of two
/// independent boundaries: the aerodynamic one, where the static margin falls
/// to its floor, and the ground one, where the nose-gear reaction falls to the
/// steering-load minimum and the aeroplane approaches tipping back onto its
/// tail. [`ModelCgEnvelopeAssessment`] has only ever derived the first.
///
/// When the aerodynamic boundary lies **aft** of the ground boundary, the
/// envelope admits loading states that tip the aeroplane, and
/// [`ModelCgConstraint::MinimumNoseGearLoad`] fires as a symptom at whichever
/// state happens to land there, with nothing naming the missing boundary.
/// This enum is that name.
///
/// It is a diagnostic, not a new threshold: neither
/// [`ModelCgEnvelopeAssessment::aerodynamic_aft_limit_pct_mac`] nor
/// [`ModelCgEnvelopeAssessment::configured_forward_limit_pct_mac`] is
/// re-derived from it, so no constraint is loosened and no candidate that the
/// existing limits reject becomes feasible.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AftCgLimitGovernance {
    /// No main-gear station or wheelbase was usable, so the ground boundary
    /// could not be placed and only the aerodynamic one exists.
    NotEvaluated,
    /// The aerodynamic boundary is at or forward of the ground boundary: the
    /// envelope's aft end is the one the assessment already reports.
    Aerodynamic,
    /// The ground boundary is forward of the aerodynamic one by
    /// `margin_pct_mac`, so the reported aft limit is **not** the governing
    /// one and the gap is the band of CG positions the envelope admits and
    /// the gear cannot carry.
    GroundMinimumNoseLoad {
        /// `aerodynamic_aft_limit_pct_mac - ground_aft_limit_pct_mac`, % MAC,
        /// strictly positive in this variant.
        margin_pct_mac: f64,
    },
    /// The longitudinal tip-back boundary is forward of both
    /// the aerodynamic and ground boundaries at the state this was formed
    /// from, by `margin_pct_mac` against the more forward of the other two.
    TipBack {
        /// Positive overhang, % MAC, against the more forward of the
        /// aerodynamic and ground boundaries.
        margin_pct_mac: f64,
    },
}

impl AftCgLimitGovernance {
    /// How far the reported aerodynamic aft limit overhangs the ground one,
    /// % MAC; zero when the aerodynamic boundary governs or none was placed.
    #[must_use]
    pub const fn overhang_pct_mac(self) -> f64 {
        match self {
            Self::GroundMinimumNoseLoad { margin_pct_mac } | Self::TipBack { margin_pct_mac } => {
                margin_pct_mac
            }
            Self::NotEvaluated | Self::Aerodynamic => 0.0,
        }
    }
}

/// Typed preliminary assessment of model stability and ground reactions.
///
/// This is not public planning, AFM, or WBM evidence. Manufacturer-source
/// comparisons remain a separate pipeline contract.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelCgEnvelopeAssessment {
    /// Loading states evaluated by the model.
    pub loading_states: Vec<ModelCgLoadingAssessment>,
    /// Hard longitudinal-stability floor, as a fraction of MAC.
    pub minimum_physical_static_margin: f64,
    /// Aft aerodynamic boundary: the critical (most-forward) neutral point
    /// minus [`Self::minimum_physical_static_margin`]. Weight/state
    /// independent.
    pub aerodynamic_aft_limit_pct_mac: f64,
    /// Diagnostic only: the clean, single-condition neutral point,
    /// `%MAC`; never used as a limit.
    pub clean_np_pct_mac: f64,
    /// The physical forward CG boundary: the more aft of
    /// [`Self::max_nose_load_fwd_limit_pct_mac`] and
    /// [`Self::scissor_plot_fwd_limit_pct_mac`]. Weight/state independent.
    /// (See [`ModelCgConstraint::PhysicalForwardCgLimit`].)
    pub configured_forward_limit_pct_mac: f64,
    /// Diagnostic only: the maximum-nose-load-handling forward boundary.
    pub max_nose_load_fwd_limit_pct_mac: f64,
    /// Diagnostic only: the scissor-plot forward boundary estimate.
    pub scissor_plot_fwd_limit_pct_mac: f64,
    /// Effective main-gear longitudinal station in the built model's own MAC
    /// frame, % MAC; `NaN` when no station could be placed.
    pub main_gear_station_pct_mac: f64,
    /// Aft boundary at which the nose-gear reaction falls to
    /// `mass_model.pct_load_nlg_min`, % MAC; `NaN` when no station could be
    /// placed.
    ///
    /// Derived from the same two-point static split the loading states use:
    /// `x_cg = x_mlg - pct_load_nlg_min * wheelbase`.
    pub ground_aft_limit_pct_mac: f64,
    /// Which of the three aft boundaries governs at the tightest (most
    /// forward) loading state; per-state detail (tip-back is `h_cg`
    /// dependent) lives on [`ModelCgLoadingAssessment::physical_limits`].
    pub aft_limit_governance: AftCgLimitGovernance,
    /// Most forward (worst-case) physical aft limit across every state, % MAC.
    pub worst_aft_limit_pct_mac: f64,
    /// True when this design carries real gear-tire ratings declared by
    /// configuration (forced wheel counts), so [`ModelCgConstraint::NoseGearStrength`]/
    /// [`ModelCgConstraint::MainGearStrength`] are non-tautological.
    /// When false, tire capacity was auto-sized to exactly this envelope and
    /// those two constraints are omitted from every loading state.
    pub capacity_basis_declared: bool,
    /// True when the sized nose/main tire is under-rated,
    /// declared basis or not.
    pub tire_overloaded: bool,
    /// Required tip-back/rotation angle, degrees: `max(min_tip_back_deg,
    /// tail_scrape_angle_deg)`.
    pub required_tip_back_deg: f64,
    /// Fuselage lower-contour scrape angle at the main gear, degrees, when
    /// the built geometry could place one.
    pub scrape_angle_deg: Option<f64>,
    /// Soft optimizer preference, reported without governing feasibility.
    pub target_static_margin: StaticMarginPreferenceAssessment,
}

impl ModelCgEnvelopeAssessment {
    /// Whether every hard constraint passes in every loading state.
    ///
    /// Diagnostic constraints ([`ModelCgConstraint::is_diagnostic`]) are
    /// excluded: they are reported as `Warning` findings
    /// (`alas-pipeline/src/feasibility/model_cg.rs`) but never reject.
    pub fn hard_constraints_pass(&self) -> bool {
        self.loading_states
            .iter()
            .flat_map(|state| &state.constraints)
            .filter(|constraint| !constraint.constraint.is_diagnostic())
            .all(|constraint| !constraint.violated)
    }

    /// The most forward (worst-case) physical aft boundary across every
    /// loading state, % MAC: [`Self::worst_aft_limit_pct_mac`].
    pub fn governing_aft_limit_pct_mac(&self) -> f64 {
        self.worst_aft_limit_pct_mac
    }

    /// Whether any loading state left the compression domain of the two-point
    /// ground split, that is, sat back on its tail.
    ///
    /// This separates a layout whose aft boundary is merely mis-attributed
    /// from one where the mis-attribution has already produced a reaction the
    /// gear cannot see.
    pub fn any_ground_reaction_inadmissible(&self) -> bool {
        self.loading_states
            .iter()
            .any(|state| !state.ground_reactions_admissible)
    }

    /// Largest normalized hard-constraint exceedance across all load states.
    ///
    /// Excludes diagnostic constraints, as [`Self::hard_constraints_pass`]
    /// does.
    pub fn worst_hard_exceedance(&self) -> f64 {
        self.loading_states
            .iter()
            .flat_map(|state| &state.constraints)
            .filter(|constraint| constraint.violated && !constraint.constraint.is_diagnostic())
            .map(|constraint| constraint.normalized_exceedance)
            .fold(0.0, f64::max)
    }
}

/// Why the model-derived assessment could not be formed.
///
/// `Eq` is deliberately not derived: [`Self::MainGearStationNotMeasured`]
/// carries a [`StationError`] whose evidence is floating point, and an
/// exact-equality trait on it would invite comparisons that are not
/// meaningful. Every variant remains `PartialEq`.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum ModelCgEnvelopeError {
    /// The aircraft has no main aerodynamic surface.
    #[error("model CG assessment requires a main wing")]
    MissingMainWing,
    /// The aircraft has no fuselage from which gear stations can be derived.
    #[error("model CG assessment requires a fuselage")]
    MissingFuselage,
    /// An input required by the static-equilibrium calculation is invalid.
    #[error("model CG assessment received a non-finite or non-positive input")]
    InvalidInput,
    /// No main-gear longitudinal station is available for this aircraft, so
    /// no ground reaction in this assessment has a point to act about.
    ///
    /// Every quantity this assessment reports about the ground (both gear
    /// strength limits, the minimum nose-gear load, and the wheelbase that
    /// normalizes them) is a moment about the main-gear station. When
    /// [`alas_mass::stations`] refuses to supply one, the wing-mounted
    /// fallback this module rebuilds is outside its stated domain, and the
    /// reactions computed from it would be reported as if they were measured.
    /// The assessment is therefore refused whole rather than returned with
    /// the gear constraints evaluated at an invented station; the carried
    /// [`StationError`] keeps the two heights that decided it.
    #[error("model CG assessment has no measured main-gear station: {0}")]
    MainGearStationNotMeasured(#[source] StationError),
}
