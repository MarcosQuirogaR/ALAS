// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_config::{AlasConfig, MacFrame};
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::breakdown::{MassBreakdown, MassCoordinates};
use alas_mass::stations::StationError;
use alas_payload::oew::oew_and_cg;
use alas_perf::landing_gear::size_landing_gear_with_group_stations;

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

/// One independently evaluated hard model constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCgConstraint {
    /// Positive stability buffer from the neutral point, as a fraction of MAC.
    StaticStabilityFloor,
    /// Configured forward extent from the physical aft stability boundary.
    ConfiguredForwardCgRange,
    /// Rated nose-gear tire capacity.
    NoseGearStrength,
    /// Rated main-gear tire capacity.
    MainGearStrength,
    /// Nose-load fraction required for ground steering authority.
    MinimumNoseGearLoad,
}

impl ModelCgConstraint {
    /// Stable report label for this constraint.
    pub const fn label(self) -> &'static str {
        match self {
            Self::StaticStabilityFloor => "static-stability floor",
            Self::ConfiguredForwardCgRange => "configured forward CG range",
            Self::NoseGearStrength => "nose-gear strength",
            Self::MainGearStrength => "main-gear strength",
            Self::MinimumNoseGearLoad => "minimum nose-gear load",
        }
    }

    /// Unit shared by the measured value and limit.
    pub const fn unit(self) -> &'static str {
        match self {
            Self::StaticStabilityFloor => "fraction MAC",
            Self::ConfiguredForwardCgRange => "% MAC",
            Self::NoseGearStrength | Self::MainGearStrength => "kg",
            Self::MinimumNoseGearLoad => "fraction weight",
        }
    }
}

/// Result of applying one hard constraint to one loading state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelCgConstraintAssessment {
    /// Physical or configured constraint being evaluated.
    pub constraint: ModelCgConstraint,
    /// Value produced by the analyzed loading state.
    pub actual: f64,
    /// Governing hard limit.
    pub limit: f64,
    /// Whether the value lies on the infeasible side of the limit.
    pub violated: bool,
    /// Dimensionless violation used only to rank the worst constraint.
    pub normalized_exceedance: f64,
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
}

impl AftCgLimitGovernance {
    /// How far the reported aerodynamic aft limit overhangs the ground one,
    /// % MAC; zero when the aerodynamic boundary governs or none was placed.
    #[must_use]
    pub const fn overhang_pct_mac(self) -> f64 {
        match self {
            Self::GroundMinimumNoseLoad { margin_pct_mac } => margin_pct_mac,
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
    /// Aft aerodynamic boundary set by the hard physical stability floor.
    pub aerodynamic_aft_limit_pct_mac: f64,
    /// Forward model boundary obtained from the configured CG range.
    ///
    /// Deliberately still measured from
    /// [`Self::aerodynamic_aft_limit_pct_mac`] and **not** from
    /// [`Self::governing_aft_limit_pct_mac`]: re-anchoring it on the tighter
    /// of the two boundaries would move the forward limit forward and turn a
    /// reported forward-CG violation into a silent pass.
    pub configured_forward_limit_pct_mac: f64,
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
    /// Which of the two aft boundaries actually governs.
    pub aft_limit_governance: AftCgLimitGovernance,
    /// Soft optimizer preference, reported without governing feasibility.
    pub target_static_margin: StaticMarginPreferenceAssessment,
}

impl ModelCgEnvelopeAssessment {
    /// Whether every hard constraint passes in every loading state.
    pub fn hard_constraints_pass(&self) -> bool {
        self.loading_states
            .iter()
            .flat_map(|state| &state.constraints)
            .all(|constraint| !constraint.violated)
    }

    /// The more forward of the aerodynamic and ground aft boundaries, % MAC.
    ///
    /// Reported only. No constraint in this assessment is evaluated against
    /// it; see [`Self::configured_forward_limit_pct_mac`] for why.
    pub fn governing_aft_limit_pct_mac(&self) -> f64 {
        if self.ground_aft_limit_pct_mac.is_finite() {
            self.aerodynamic_aft_limit_pct_mac
                .min(self.ground_aft_limit_pct_mac)
        } else {
            self.aerodynamic_aft_limit_pct_mac
        }
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
    pub fn worst_hard_exceedance(&self) -> f64 {
        self.loading_states
            .iter()
            .flat_map(|state| &state.constraints)
            .filter(|constraint| constraint.violated)
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
    /// Every quantity this assessment reports about the ground — both gear
    /// strength limits, the minimum nose-gear load, and the wheelbase that
    /// normalizes them — is a moment about the main-gear station. When
    /// [`alas_mass::stations`] refuses to supply one, the wing-mounted
    /// fallback this module rebuilds is outside its stated domain, and the
    /// reactions computed from it would be reported as if they were measured.
    /// The assessment is therefore refused whole rather than returned with
    /// the gear constraints evaluated at an invented station; the carried
    /// [`StationError`] keeps the two heights that decided it.
    #[error("model CG assessment has no measured main-gear station: {0}")]
    MainGearStationNotMeasured(#[source] StationError),
}

#[derive(Debug, Clone, Copy)]
struct LoadingConstraintInputs {
    state: ModelCgLoadingState,
    mass_kg: f64,
    cg_x_m: f64,
    cg_pct_mac: f64,
    static_margin: f64,
    static_margin_floor: f64,
    configured_forward_limit_pct_mac: f64,
    nose_gear_load_kg: f64,
    nose_gear_capacity_kg: f64,
    main_gear_load_kg: f64,
    main_gear_capacity_kg: f64,
    minimum_nose_gear_load_fraction: f64,
}

fn lower_bound_constraint(
    constraint: ModelCgConstraint,
    actual: f64,
    limit: f64,
    normalization: f64,
) -> ModelCgConstraintAssessment {
    let deficit = (limit - actual).max(0.0);
    ModelCgConstraintAssessment {
        constraint,
        actual,
        limit,
        violated: actual < limit,
        normalized_exceedance: deficit / normalization.max(f64::EPSILON),
    }
}

fn upper_bound_constraint(
    constraint: ModelCgConstraint,
    actual: f64,
    limit: f64,
    normalization: f64,
) -> ModelCgConstraintAssessment {
    let excess = (actual - limit).max(0.0);
    ModelCgConstraintAssessment {
        constraint,
        actual,
        limit,
        violated: actual > limit,
        normalized_exceedance: excess / normalization.max(f64::EPSILON),
    }
}

fn assess_loading_constraints(inputs: LoadingConstraintInputs) -> ModelCgLoadingAssessment {
    let nose_gear_load_fraction = inputs.nose_gear_load_kg / inputs.mass_kg;
    // Both groups in compression is the stated domain of the split that
    // produced these two reactions; see
    // `ModelCgLoadingAssessment::ground_reactions_admissible`. The test is on
    // the reactions themselves, with no margin and no tolerance: zero nose
    // load is the tipping point itself and is still a reaction the model can
    // state.
    let ground_reactions_admissible =
        inputs.nose_gear_load_kg >= 0.0 && inputs.main_gear_load_kg >= 0.0;
    let constraints = vec![
        lower_bound_constraint(
            ModelCgConstraint::StaticStabilityFloor,
            inputs.static_margin,
            inputs.static_margin_floor,
            1.0,
        ),
        lower_bound_constraint(
            ModelCgConstraint::ConfiguredForwardCgRange,
            inputs.cg_pct_mac,
            inputs.configured_forward_limit_pct_mac,
            100.0,
        ),
        upper_bound_constraint(
            ModelCgConstraint::NoseGearStrength,
            inputs.nose_gear_load_kg,
            inputs.nose_gear_capacity_kg,
            inputs.mass_kg,
        ),
        upper_bound_constraint(
            ModelCgConstraint::MainGearStrength,
            inputs.main_gear_load_kg,
            inputs.main_gear_capacity_kg,
            inputs.mass_kg,
        ),
        lower_bound_constraint(
            ModelCgConstraint::MinimumNoseGearLoad,
            nose_gear_load_fraction,
            inputs.minimum_nose_gear_load_fraction,
            1.0,
        ),
    ];
    let mut constraints = constraints;
    if !ground_reactions_admissible {
        // Drop only the two rated-capacity comparisons. This cannot admit a
        // candidate that would otherwise be rejected: both are upper bounds
        // that a negative reaction already satisfies, so what is removed is a
        // passing verdict on a number the model did not measure, and
        // `MinimumNoseGearLoad` keeps rejecting the state.
        constraints.retain(|constraint| {
            !matches!(
                constraint.constraint,
                ModelCgConstraint::NoseGearStrength | ModelCgConstraint::MainGearStrength
            )
        });
    }
    ModelCgLoadingAssessment {
        state: inputs.state,
        mass_kg: inputs.mass_kg,
        cg_x_m: inputs.cg_x_m,
        cg_pct_mac: inputs.cg_pct_mac,
        static_margin: inputs.static_margin,
        nose_gear_load_fraction,
        ground_reactions_admissible,
        constraints,
    }
}

fn loading_states(
    oew_mass: f64,
    oew_cg_x: f64,
    payload_mass: f64,
    payload_cg_x: f64,
    mtow_cg_x: f64,
    fuel_mass: f64,
) -> [(f64, f64); 3] {
    let mzfw_mass = oew_mass + payload_mass;
    let mzfw_cg_x = (oew_mass * oew_cg_x + payload_mass * payload_cg_x) / mzfw_mass.max(1.0);
    let mtow_mass = oew_mass + payload_mass + fuel_mass.max(0.0);

    [
        (oew_cg_x, oew_mass),
        (mzfw_cg_x, mzfw_mass),
        (mtow_cg_x, mtow_mass),
    ]
}

/// Product load cases extending the reference three-point envelope with
/// explicit mid-mission and reserve fuel states.
fn operational_loading_states(
    oew_mass: f64,
    oew_cg_x: f64,
    payload_mass: f64,
    payload_cg_x: f64,
    fuel_mass: f64,
    fuel_cg_x: f64,
    mtow_cg_x: f64,
) -> Vec<(ModelCgLoadingState, f64, f64)> {
    let mzfw_mass = oew_mass + payload_mass;
    let mzfw_cg_x = (oew_mass * oew_cg_x + payload_mass * payload_cg_x) / mzfw_mass.max(1.0);
    let with_fuel = |fraction: f64, state: ModelCgLoadingState| {
        let fuel = fuel_mass.max(0.0) * fraction;
        let mass = mzfw_mass + fuel;
        let cg = if fuel > 0.0 {
            (mzfw_mass * mzfw_cg_x + fuel * fuel_cg_x) / mass.max(1.0)
        } else {
            mzfw_cg_x
        };
        (state, cg, mass)
    };
    vec![
        (ModelCgLoadingState::OperatingEmpty, oew_cg_x, oew_mass),
        (ModelCgLoadingState::AnalyzedZeroFuel, mzfw_cg_x, mzfw_mass),
        with_fuel(0.50, ModelCgLoadingState::OperationalMidMission),
        with_fuel(0.10, ModelCgLoadingState::OperationalReserve),
        (
            ModelCgLoadingState::AnalyzedTakeoff,
            mtow_cg_x,
            mzfw_mass + fuel_mass.max(0.0),
        ),
    ]
}
