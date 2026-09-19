// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Public result types for the pipeline's physical-feasibility checks.

use alas_config::{CgEnvelopeEvidence, CgEnvelopeSource};
use alas_opt::ModelCgEnvelopeAssessment;

use super::{CruiseEquilibriumAssessment, FuelLoadingAssessment, MassBalanceAssessment};

/// Stable identifier for one physical failure mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingCode {
    /// The maneuver envelope does not satisfy VS < VA <= VC < VD.
    InvalidEnvelopeSpeedOrder,
    /// Cruise aerodynamics did not produce a positive finite efficiency.
    InvalidCruiseAerodynamics,
    /// The MTOW mass closure left no positive fuel.
    NonPositiveFuel,
    /// Usable capacity limits the load case below the configured MTOW.
    TankLimitedTakeoffMass,
    /// No usable-fuel capacity could be established for the analyzed design.
    FuelCapacityUnavailable,
    /// Frozen compatibility code for the former untyped envelope Boolean.
    CgEnvelopeViolation,
    /// The typed model CG assessment could not be constructed.
    ModelCgAssessmentUnavailable,
    /// A loading state lies forward of the configured model CG range.
    ModelCgForwardRangeViolation,
    /// A loading state exceeds the modeled nose-gear tire capacity.
    NoseGearStrengthViolation,
    /// A loading state exceeds the modeled main-gear tire capacity.
    MainGearStrengthViolation,
    /// A loading state carries too little nose load for steering authority.
    MinimumNoseGearLoadViolation,
    /// The analyzed point lies outside a public manufacturer planning envelope.
    PublicPlanningCgEnvelopeViolation,
    /// The cruise trim solve did not produce a finite result.
    TrimUnavailable,
    /// Static margin is below the configured physical floor.
    InsufficientStaticMargin,
    /// The built wing reference area exceeds its configured maximum.
    WingAreaLimit,
    /// The reported cruise attitude left the window the optimizer selected
    /// the candidate inside, because the two are measured on different
    /// panel meshes.
    ReportedCruiseAttitudeOutsideWindow,
    /// A requested mission produced no telemetry.
    MissionUnavailable,
    /// At least one mission segment did not converge.
    MissionNotConverged,
    /// Mission fuel burn is not a positive finite quantity.
    InvalidMissionFuelBurn,
    /// Mission fuel burn exceeds the fuel carried in the analyzed load case.
    MissionFuelShortfall,
    /// A cruise force record contains a non-finite result.
    InvalidCruiseForceBalance,
    /// The selected airport could not be resolved for field-performance checks.
    FieldPerformanceUnavailable,
    /// Take-off distance exceeds the departure field available.
    FieldTakeoffDistanceViolation,
    /// Landing distance exceeds the arrival field available.
    FieldLandingDistanceViolation,
    /// The analyzed arrival mass exceeds the configured maximum landing mass.
    LandingMassLimitViolation,
    /// Static thrust-to-weight is below the selected departure-field requirement.
    ThrustMarginViolation,
    /// A mission control point requires more than full throttle.
    MissionThrottleLimitViolation,
    /// The requested passenger count exceeds the seats the layout placed.
    PassengerCapacityShortfall,
    /// The requested net cargo exceeds the net load the ULD layout placed.
    CargoCapacityShortfall,
    /// The modeled zero-fuel mass exceeds the published maximum zero-fuel
    /// weight for an unchanged registered preset.
    MaximumZeroFuelWeightViolation,
    /// The modeled payload exceeds the configured structural payload cap.
    StructuralPayloadLimitViolation,
    /// The fuel the policy requires for the route does not fit under the
    /// takeoff-mass limit or in the usable tanks.
    ReserveFuelShortfall,
    /// The fuel-policy takeoff-mass closure did not settle within its budget.
    DispatchNotConverged,
    /// The fuel policy could not be priced on this aircraft.
    FuelPolicyUnavailable,
    /// The item-level mass ledger could not be built for this aircraft.
    MassLedgerUnavailable,
    /// The tank arrangement could not be resolved on the built geometry.
    FuelTankLayoutUnavailable,
    /// The item ledger and the lumped model disagree about the takeoff
    /// centre of gravity by more than the reporting band.
    MassModelDisagreement,
}

impl FindingCode {
    /// Machine-stable spelling, for manifests, exports and the acceptance
    /// record that has to name which check rejected a delivered design.
    ///
    /// These strings are an interface: a reader matching on one is entitled
    /// to expect it not to change under them, so a rename of a variant must
    /// keep its spelling here or be treated as a breaking change.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidEnvelopeSpeedOrder => "invalid_envelope_speed_order",
            Self::InvalidCruiseAerodynamics => "invalid_cruise_aerodynamics",
            Self::NonPositiveFuel => "non_positive_fuel",
            Self::TankLimitedTakeoffMass => "tank_limited_takeoff_mass",
            Self::FuelCapacityUnavailable => "fuel_capacity_unavailable",
            Self::CgEnvelopeViolation => "cg_envelope_violation",
            Self::ModelCgAssessmentUnavailable => "model_cg_assessment_unavailable",
            Self::ModelCgForwardRangeViolation => "model_cg_forward_range_violation",
            Self::NoseGearStrengthViolation => "nose_gear_strength_violation",
            Self::MainGearStrengthViolation => "main_gear_strength_violation",
            Self::MinimumNoseGearLoadViolation => "minimum_nose_gear_load_violation",
            Self::PublicPlanningCgEnvelopeViolation => "public_planning_cg_envelope_violation",
            Self::TrimUnavailable => "trim_unavailable",
            Self::InsufficientStaticMargin => "insufficient_static_margin",
            Self::WingAreaLimit => "wing_area_limit",
            Self::ReportedCruiseAttitudeOutsideWindow => "reported_cruise_attitude_outside_window",
            Self::MissionUnavailable => "mission_unavailable",
            Self::MissionNotConverged => "mission_not_converged",
            Self::InvalidMissionFuelBurn => "invalid_mission_fuel_burn",
            Self::MissionFuelShortfall => "mission_fuel_shortfall",
            Self::InvalidCruiseForceBalance => "invalid_cruise_force_balance",
            Self::FieldPerformanceUnavailable => "field_performance_unavailable",
            Self::FieldTakeoffDistanceViolation => "field_takeoff_distance_violation",
            Self::FieldLandingDistanceViolation => "field_landing_distance_violation",
            Self::LandingMassLimitViolation => "landing_mass_limit_violation",
            Self::ThrustMarginViolation => "thrust_margin_violation",
            Self::MissionThrottleLimitViolation => "mission_throttle_limit_violation",
            Self::PassengerCapacityShortfall => "passenger_capacity_shortfall",
            Self::CargoCapacityShortfall => "cargo_capacity_shortfall",
            Self::MaximumZeroFuelWeightViolation => "maximum_zero_fuel_weight_violation",
            Self::StructuralPayloadLimitViolation => "structural_payload_limit_violation",
            Self::ReserveFuelShortfall => "reserve_fuel_shortfall",
            Self::DispatchNotConverged => "dispatch_not_converged",
            Self::FuelPolicyUnavailable => "fuel_policy_unavailable",
            Self::MassLedgerUnavailable => "mass_ledger_unavailable",
            Self::FuelTankLayoutUnavailable => "fuel_tank_layout_unavailable",
            Self::MassModelDisagreement => "mass_model_disagreement",
        }
    }
}

/// Severity of a physical finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingSeverity {
    /// The configured aircraft or load case is not physically feasible.
    Error,
    /// The result is usable but requires engineering attention.
    Warning,
}

/// One failed physical check with its measured and limiting values.
#[derive(Debug, Clone, PartialEq)]
pub struct PhysicalFinding {
    /// Machine-stable failure identifier.
    pub code: FindingCode,
    /// Whether the finding invalidates the load case.
    pub severity: FindingSeverity,
    /// Human-readable explanation.
    pub message: String,
    /// Measured value, when the check is quantitative.
    pub actual: Option<f64>,
    /// Governing limit, when the check is quantitative.
    pub limit: Option<f64>,
    /// Unit shared by `actual` and `limit`.
    pub unit: &'static str,
}

/// Result of comparing one analyzed point with public CG reference evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlanningCgStatus {
    /// No public planning curve was evaluated.
    #[default]
    NotEvaluated,
    /// The point lies between both published planning limits.
    WithinPublishedLimits,
    /// The point is forward of the published planning limit.
    ForwardLimitViolation,
    /// The point is aft of the published planning limit.
    AftLimitViolation,
    /// A forward limit exists at this mass, but the source omits an aft limit.
    AftLimitNotPublished,
}

/// Provenance and result of the public planning-envelope comparison.
///
/// This assessment is intentionally separate from the model-derived CG and
/// landing-gear check. A public planning curve is preliminary design evidence;
/// the actual aircraft WBM remains the operational authority.
#[derive(Debug, Clone, PartialEq)]
pub struct CgEnvelopeAssessment {
    /// Kind of source evidence registered for the selected preset.
    pub evidence: CgEnvelopeEvidence,
    /// Outcome of the planning-curve comparison, if one was possible.
    pub planning_status: PlanningCgStatus,
    /// Analyzed aircraft mass, in kilograms.
    pub mass_kg: Option<f64>,
    /// Analyzed longitudinal CG in the manufacturer planning frame, in percent MAC.
    pub cg_pct_mac: Option<f64>,
    /// Interpolated public planning forward limit, in percent MAC.
    pub forward_limit_pct_mac: Option<f64>,
    /// Interpolated public planning aft limit, in percent MAC.
    pub aft_limit_pct_mac: Option<f64>,
    /// Exact source location of the planning curve, when one is registered.
    pub source: Option<CgEnvelopeSource>,
    /// Document that controls actual-aircraft dispatch and loading.
    pub controlling_document: Option<&'static str>,
}

impl Default for CgEnvelopeAssessment {
    fn default() -> Self {
        Self {
            evidence: CgEnvelopeEvidence::Unknown,
            planning_status: PlanningCgStatus::NotEvaluated,
            mass_kg: None,
            cg_pct_mac: None,
            forward_limit_pct_mac: None,
            aft_limit_pct_mac: None,
            source: None,
            controlling_document: None,
        }
    }
}

/// Physical status attached to every completed pipeline result.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FeasibilityReport {
    /// Every failed check; an empty list means all implemented checks passed.
    pub findings: Vec<PhysicalFinding>,
    /// Public CG evidence and planning-only comparison for the selected preset.
    pub cg_envelope: CgEnvelopeAssessment,
    /// Typed hard model constraints, separate from public planning evidence.
    pub model_cg: Option<ModelCgEnvelopeAssessment>,
    /// Fuel budget, capacity, actual load, takeoff mass, and mission burn.
    pub fuel_loading: FuelLoadingAssessment,
    /// Explicit cruise force-balance evidence from the flown mission.
    pub cruise_equilibrium: Option<CruiseEquilibriumAssessment>,
    /// The item-level mass statement: tanks, stations, and the mass, centre
    /// of gravity and inertia of each named loading state.
    pub mass_balance: Option<MassBalanceAssessment>,
}

impl FeasibilityReport {
    /// Whether no error-severity physical finding was recorded.
    pub fn is_feasible(&self) -> bool {
        self.findings
            .iter()
            .all(|finding| finding.severity != FindingSeverity::Error)
    }

    /// Whether this report contains a particular failure mode.
    pub fn contains(&self, code: FindingCode) -> bool {
        self.findings.iter().any(|finding| finding.code == code)
    }
}
