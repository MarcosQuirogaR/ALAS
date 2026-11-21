// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Public result types for the pipeline's physical-feasibility checks.

use alas_opt::ModelCgEnvelopeAssessment;

pub use super::cg_envelope_types::{
    CgEnvelopeAssessment, PlanningCgStatus, PlanningCurveComparison,
};
use super::{CruiseEquilibriumAssessment, FuelLoadingAssessment, MassBalanceAssessment};

mod finding_code;

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
    ///
    /// When this fires it is often a *symptom* rather than the defect: see
    /// [`alas_opt::AftCgLimitGovernance`], carried on
    /// [`FeasibilityReport::model_cg`], for whether the envelope's aft
    /// boundary is the one that actually governs the layout.
    MinimumNoseGearLoadViolation,
    /// The longitudinal tip-back angle at a loading state's CG height is
    /// below the required minimum, so the aircraft can sit on its tail.
    TipBackViolation,
    /// The aft fuselage lower contour scrapes the ground at a pitch angle
    /// below the required rotation attitude.
    TailScrapeViolation,
    /// The analyzed point lies outside a public manufacturer planning envelope.
    PublicPlanningCgEnvelopeViolation,
    /// The cruise trim solve did not produce a finite result.
    TrimUnavailable,
    /// Static margin is below the configured physical floor.
    InsufficientStaticMargin,
    /// The closed take-off mass lies outside the band around the mass the
    /// fixed engine rating was matched to.
    EngineMassMismatch,
    /// A held clean-sheet engine is more than the engine rating window above
    /// the thrust its own closed design requires (warning only: the engine is
    /// the user's choice and the design stays feasible).
    EngineOversize,
    /// A declared or design cruise level the priced route cannot fly (the aircraft cannot
    /// climb to it at the dispatched mass) is flown at the highest level it can; the
    /// declared level is reported against the flown one (warning only).
    DeclaredCruiseLevelLowered,
    /// The check of a declared cruise level against the dispatched trip could not be made
    /// (the trip could not be flown on its plan); warning only.
    DeclaredCruiseLevelUnchecked,
    /// A turboprop of a layout outside its clearance sample (low wing, nacelle
    /// gear) declares no belly clearance, so the full documented sample of the
    /// jets and sponson turboprops stands in at both ends (warning only: an
    /// assumption to replace by a declared `fuselage_ground_clearance_m`).
    GroundClearanceSampleAssumed,
    /// The take-off safety speed was raised above the CS 25.107 schedule to meet
    /// the second-segment gradient (improved climb); the field length is charged.
    ImprovedClimbV2,
    /// The departure aerodrome's elevation or temperature limits the take-off
    /// weight below the mission mass (climb and obstacle limit; an operating
    /// limitation, warning only).
    TakeoffWeightLimited,
    /// The computed airborne minimum control speed exceeds the CS 25.149(c)
    /// ceiling of 1.13 VSR, or the fin cannot control the engine-out moment.
    MinimumControlSpeedViolation,
    /// The minimum control speed could not be evaluated (no fin or fuselage
    /// data, or an unavailable drag table); the declared ratio stays.
    MinimumControlSpeedUnavailable,
    /// The rudder deflection that holds the design crosswind exceeds its limit.
    CrosswindControlViolation,
    /// The weathercock stability of the fuselage and fin is below its floor.
    DirectionalStabilityViolation,
    /// The built wing reference area exceeds its configured maximum.
    WingAreaLimit,
    /// The mandatory structural response could not be evaluated.
    StructuralResponseUnavailable,
    /// Structural strength or rib spacing does not meet the sized loads.
    StructuralStrengthViolation,
    /// The wing response leaves the configured linear model's validity domain.
    StructuralLinearModelDomain,
    /// The compressible polar is a linear Prandtl-Glauert solve above the Mach
    /// range DATCOM substantiates.
    AeroLinearModelDomain,
    /// The selected drag-divergence law does not use a declared Korn factor
    /// override, or is applied to a section class its source does not cover.
    DragDivergenceLawSelection,
    /// The cruise TSFC the fuel results rest on is not a published datum of
    /// the engine: a family proxy or an unsourced estimate.
    PropulsionCruiseTsfcNotPublished,
    /// The V2 climb-out drag locus is read with an input its source does not
    /// cover: an undeclared slat state, a Krueger-only leading edge or a
    /// critical engine position without a stated engine-failure reduction.
    TakeoffClimbOutPolarDomain,
    /// A solved finite-element displacement proves excessive linear-model rotation.
    StructuralFemModelDomain,
    /// An attempted structural solver failed or returned incomplete static data.
    StructuralSolverFailed,
    /// The reported cruise attitude left the window the optimizer selected
    /// the candidate inside, because the two are measured on different
    /// panel meshes.
    ReportedCruiseAttitudeOutsideWindow,
    /// A requested mission produced no telemetry.
    MissionUnavailable,
    /// At least one mission segment did not converge.
    MissionNotConverged,
    /// The native flight flew a planned climb or descent path change level,
    /// because no constant rate between the thrust and flight-idle limits
    /// flies it.
    MissionPathFlownLevel,
    /// The first leg of a flight, a climb, cannot be flown from where it starts
    /// (the field) at the mass loaded, so the flight stops on it rather than
    /// fly the rest level from there.
    MissionFirstClimbRefused,
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
    /// A stated design take-off field length longer than the route's was
    /// ignored: a stated value can only shorten the requirement.
    TakeoffOverrideIgnored,
    /// A stated design landing field length longer than the route's was
    /// ignored: a stated value can only shorten the requirement.
    LandingOverrideIgnored,
    /// A stated design field length belongs to the configured aerodrome, which
    /// the dispatched plan does not use, so it played no part in judging the
    /// dispatched aerodromes. It covers that judgement only: the design sizing
    /// still reads the configured aerodrome, and the message says when a
    /// shorter stated length still applies there.
    StatedFieldLengthNotApplied,
    /// Landing distance exceeds the arrival field available.
    FieldLandingDistanceViolation,
    /// The analyzed arrival mass exceeds the configured maximum landing mass.
    LandingMassLimitViolation,
    /// The weight hierarchy `MZFW <= MLW <= MTOW` is broken: the maximum
    /// landing mass exceeds the takeoff mass of the design, or the zero-fuel
    /// mass exceeds the maximum landing mass.
    MassLimitHierarchyViolation,
    /// Static thrust-to-weight is below the selected departure-field requirement.
    ThrustMarginViolation,
    /// A cruise or top-of-climb thrust shortfall inside the documented
    /// scatter of the Jenkinson chart correlation (an engine without a
    /// cruise or climb thrust datum): flagged, not a failure.
    ThrustWithinChartScatter,
    /// A mission control point requires more than full throttle.
    MissionThrottleLimitViolation,
    /// The requested passenger count exceeds the seats the layout placed.
    PassengerCapacityShortfall,
    /// The baggage mass exceeds every modelled hold compartment; the excess
    /// is reported as overload rather than dropped from the payload.
    BaggageOverload,
    /// A galley or lavatory the cabin provisions could not be placed: the
    /// section is too narrow at every bay, so the layout carries fewer than
    /// the provisioned count.
    MonumentDropped,
    /// The requested net cargo exceeds the net load the ULD layout placed.
    CargoCapacityShortfall,
    /// The modeled zero-fuel mass exceeds the published maximum zero-fuel
    /// weight for an unchanged registered preset.
    MaximumZeroFuelWeightViolation,
    /// No maximum zero-fuel weight applies to a registered aircraft's run (a
    /// variant of another aircraft type, more structural payload than
    /// registered, changed geometry, or no declared value), so the zero-fuel
    /// check could not run. Set `requirements.mzfw_kg` to state the limit.
    MaximumZeroFuelWeightUndefined,
    /// A variant of another aircraft type than its registered preset states
    /// no `requirements.mlw_kg`: the landing mass is checked against the
    /// preset's own limit or the mass-model fraction, not a limit of the
    /// variant flown.
    MaximumLandingWeightUndefined,
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
    /// Independent structural and empirical wing mass estimates differ;
    /// this is a diagnostic comparison, not a physical mass limit.
    StructuralMassModelDifference,
    /// The candidate primary box leaves no positive secondary inventory
    /// within the authoritative current complete-wing mass.
    StructuralInventoryUnverified,
    /// A mass event or tank operation of the mission plan (an offload, a
    /// refuelling, a transfer, a trim move, a burn) could not be carried out
    /// in full by the tanks.
    MissionPlanOperationFailed,
    /// The payload auto-balance could not reach its zero-fuel centre of gravity
    /// target: the movable items reached the end of their ranges or the passes
    /// ran out.
    PayloadAutoBalanceIncomplete,
    /// The aircraft mass at some instant of the mission plan exceeds the
    /// maximum take-off mass (fuel taken on in flight).
    MissionPlanMassLimitViolation,
    /// The trim drag of a mission plan's centre of gravity had not settled.
    MissionPlanTrimUnsettled,
    /// An imported CPACS document was read in a way the results depend on: its ALAS
    /// main-wing record did not fit it, or it declares no reference area.
    CpacsImportReading,
    /// The reported cruise drag and lift-to-drag ratio are not the polar at
    /// the cruise lift coefficient: that lift lies outside the analysed
    /// sweep, or the sweep's nearest sample was reported in its place.
    CruisePointOutsidePolar,
    /// The design mission the reported cruise point belongs to could not be
    /// closed: the point is the mean of the take-off and zero-fuel masses.
    CruisePointUnpriced,
    /// The design mission of the reported cruise point was priced, but no
    /// flown state of its mid-cruise mass was found (no plan, a plan on the
    /// climb floor, a loading whose mission could not be flown): the point
    /// stands at the design cruise altitude and Mach, which need not be a
    /// level the mission flies.
    CruisePointLevelUnplanned,
    /// The propulsion mass was placed at the no-nacelle wing-centroid
    /// station because the built geometry has no nacelle: the centre of
    /// gravity and every figure derived from it describe a different
    /// aircraft.
    PropulsionStationFallback,
    /// The station the propulsion mass was placed at could not be resolved,
    /// so whether it is a measured nacelle station is not known.
    PropulsionStationUnknown,
    /// The take-off thrust at the minimum control speed did not close on the
    /// speed within its iteration limit: the reported value is the last
    /// iterate.
    MinimumControlSpeedNotConverged,
    /// A reference quantity of the aircraft is non-finite or below the floor
    /// the stability correlations hold it at, so the stability results were
    /// evaluated with the floor instead of the aircraft's own geometry.
    StabilityReferenceBelowFloor,
    /// The propulsion deck answered a flown mission point outside its
    /// evidence domain: thrust and fuel flow there are an extrapolation.
    PropulsionDeckExtrapolated,
    /// Whether the propulsion deck answered the flown mission points inside
    /// its evidence domain could not be established.
    PropulsionDeckValidityUnknown,
}

/// Where the mass model placed the propulsion group, as far as this run can
/// tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PropulsionStationEvidence {
    /// The design carries no propulsion mass, so there is nothing to place.
    #[default]
    NoPropulsionMass,
    /// A nacelle station measured on the built geometry.
    MeasuredNacelle,
    /// The no-nacelle fallback: the wing centroid, offset by a fixed drop.
    WingCentroidFallback,
    /// The component stations could not be resolved, or the propulsion mass
    /// is not finite: the station is not known.
    Unknown,
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

/// Physical status attached to every completed pipeline result.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FeasibilityReport {
    /// Every failed check; an empty list means all implemented checks passed.
    pub findings: Vec<PhysicalFinding>,
    /// Public CG evidence and planning-only comparison for the selected preset.
    pub cg_envelope: CgEnvelopeAssessment,
    /// Typed hard model constraints, separate from public planning evidence.
    pub model_cg: Option<ModelCgEnvelopeAssessment>,
    /// Design-envelope CG at capped maximum usable-fuel loading. Dispatch
    /// telemetry and landing checks remain in [`Self::model_cg`].
    pub design_model_cg: Option<ModelCgEnvelopeAssessment>,
    /// Fuel budget, capacity, actual load, takeoff mass, and mission burn.
    pub fuel_loading: FuelLoadingAssessment,
    /// Explicit cruise force-balance evidence from the flown mission.
    pub cruise_equilibrium: Option<CruiseEquilibriumAssessment>,
    /// The item-level mass statement: tanks, stations, and the mass, centre
    /// of gravity and inertia of each named loading state.
    pub mass_balance: Option<MassBalanceAssessment>,
    /// Where the product mass model placed the propulsion group.
    ///
    /// [`PropulsionStationEvidence::WingCentroidFallback`] means the design
    /// carries nonzero propulsion mass but no measured nacelle station
    /// exists on the built geometry
    /// ([`alas_mass::stations::ComponentStations::propulsion_station_fallback`]).
    /// Then [`Self::model_cg`] and every %MAC/CG figure this run publishes
    /// are not physical evidence for this design: they describe an aircraft
    /// with its engines placed at the wing, not this one. This is a
    /// modelling-fidelity property of *this run's geometry* (no nacelle
    /// built), not a physical infeasibility of the aircraft, so it is a
    /// warning finding ([`FindingCode::PropulsionStationFallback`]) rather
    /// than part of [`Self::is_feasible`]. A station that could not be
    /// resolved is [`PropulsionStationEvidence::Unknown`] and raises
    /// [`FindingCode::PropulsionStationUnknown`]: it is never read as "no
    /// fallback".
    pub propulsion_station: PropulsionStationEvidence,
    /// Loading potato, fuel vector and ZFW operational limits (items 6/8),
    /// `None` when the payload layout or fuel-tank arrangement could not be
    /// resolved (already reported by [`Self::mass_balance`]'s own findings).
    pub operational_envelope: Option<super::operational_envelope::OperationalEnvelopeAssessment>,
    /// Why the native pseudospectral mission could not be flown, when it
    /// failed. Telemetry only: the route is judged on the segment mission
    /// model ([`FuelLoadingAssessment::mission`]).
    pub native_mission_error: Option<String>,
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
