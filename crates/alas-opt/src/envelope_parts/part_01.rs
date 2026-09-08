// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::breakdown::{MassBreakdown, MassCoordinates};
use alas_payload::oew::oew_and_cg;
use alas_perf::landing_gear::size_landing_gear;

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
    pub configured_forward_limit_pct_mac: f64,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
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
    ModelCgLoadingAssessment {
        state: inputs.state,
        mass_kg: inputs.mass_kg,
        cg_x_m: inputs.cg_x_m,
        cg_pct_mac: inputs.cg_pct_mac,
        static_margin: inputs.static_margin,
        nose_gear_load_fraction,
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
