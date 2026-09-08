// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::{error::Error, fmt};

use crate::mission_turbofan::{
    evaluate_thrust, Freestream, ThrustOutput, TurbofanInputs, VehicleBuilderParams,
};

/// Stable identifier for a propulsion technology implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelIdentity {
    /// Human-readable implementation family (for example, `legacy-turbofan`).
    pub family: String,
    /// Version of the equations or calibrated data contract.
    pub version: String,
}

/// Traceability attached to every evaluated operating point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelProvenance {
    /// Physics implementation that produced the result.
    pub model: ModelIdentity,
    /// Engine/deck/catalogue record selected by the configuration boundary.
    pub dataset: Option<String>,
    /// Bibliographic or calibration references used by that record.
    pub sources: Vec<String>,
}

/// Ambient and kinematic state supplied by the aircraft analysis.
///
/// The full thermodynamic state is explicit so propulsion does not silently
/// select a second atmosphere. SI units and body axes are used throughout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlightCondition {
    /// Geometric altitude, m.
    pub altitude_m: f64,
    /// Freestream Mach number.
    pub mach: f64,
    /// Static pressure, Pa.
    pub pressure_pa: f64,
    /// Static temperature, K.
    pub temperature_k: f64,
    /// Static density, kg/m^3.
    pub density_kg_m3: f64,
    /// Dynamic viscosity, Pa*s.
    pub dynamic_viscosity_pa_s: f64,
    /// Local gravitational acceleration, m/s^2.
    pub gravity_m_s2: f64,
    /// Ratio of specific heats.
    pub gamma: f64,
    /// Specific heat at constant pressure, J/(kg*K).
    pub cp_j_kgk: f64,
    /// Specific gas constant, J/(kg*K).
    pub gas_constant_j_kgk: f64,
    /// Local speed of sound, m/s.
    pub speed_of_sound_m_s: f64,
    /// Freestream velocity, m/s.
    pub velocity_m_s: f64,
    /// Freestream stagnation temperature, K.
    pub stagnation_temperature_k: f64,
    /// Freestream stagnation pressure, Pa.
    pub stagnation_pressure_pa: f64,
}

impl From<&Freestream> for FlightCondition {
    fn from(value: &Freestream) -> Self {
        Self {
            altitude_m: value.altitude_m,
            mach: value.mach,
            pressure_pa: value.pressure_pa,
            temperature_k: value.temperature_k,
            density_kg_m3: value.density_kg_m3,
            dynamic_viscosity_pa_s: value.dynamic_viscosity_pa_s,
            gravity_m_s2: value.gravity_m_s2,
            gamma: value.gamma,
            cp_j_kgk: value.cp_j_kgk,
            gas_constant_j_kgk: value.r_j_kgk,
            speed_of_sound_m_s: value.speed_of_sound_m_s,
            velocity_m_s: value.velocity_m_s,
            stagnation_temperature_k: value.stagnation_temperature_k,
            stagnation_pressure_pa: value.stagnation_pressure_pa,
        }
    }
}

impl From<FlightCondition> for Freestream {
    fn from(value: FlightCondition) -> Self {
        Self {
            pressure_pa: value.pressure_pa,
            temperature_k: value.temperature_k,
            density_kg_m3: value.density_kg_m3,
            dynamic_viscosity_pa_s: value.dynamic_viscosity_pa_s,
            gravity_m_s2: value.gravity_m_s2,
            gamma: value.gamma,
            cp_j_kgk: value.cp_j_kgk,
            r_j_kgk: value.gas_constant_j_kgk,
            speed_of_sound_m_s: value.speed_of_sound_m_s,
            velocity_m_s: value.velocity_m_s,
            mach: value.mach,
            stagnation_temperature_k: value.stagnation_temperature_k,
            stagnation_pressure_pa: value.stagnation_pressure_pa,
            altitude_m: value.altitude_m,
        }
    }
}

/// Technology-neutral demand sent by a mission or performance solver.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PropulsionDemand {
    /// Fraction of the currently available forward-propulsive rating.
    NormalizedForce(f64),
    /// Required body-axis force, N. Models may reject unsupported inverse solves.
    RequiredBodyForceN([f64; 3]),
    /// A named, certification- or operation-relevant propulsion rating.
    Rating(PropulsionRating),
    /// Fraction of a named rating's available force, in `[0, 1]`.
    ///
    /// This is distinct from [`Self::NormalizedForce`], whose reference
    /// rating is technology/model dependent.
    RatedFraction {
        /// Certification- or operation-relevant rating schedule.
        rating: PropulsionRating,
        /// Requested fraction of force available at that rating.
        fraction: f64,
    },
}

/// Named ratings shared by gas-turbine, propeller and future technologies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropulsionRating {
    /// Takeoff/go-around rating, normally time limited.
    TakeoffGoAround,
    /// Highest indefinitely sustainable rating.
    MaximumContinuous,
    /// Maximum climb schedule.
    MaximumClimb,
    /// Normal cruise schedule.
    Cruise,
    /// Minimum airborne running rating.
    FlightIdle,
}

/// Explicit discrete operating state, separate from a zero demand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatingMode {
    /// Normal forward-propulsive operation.
    Normal,
    /// Engine running at its airborne idle schedule.
    FlightIdle,
    /// Engine running at its ground idle schedule.
    GroundIdle,
    /// Engine and propulsor stopped where physically possible.
    Shutdown,
    /// Commanded reverse thrust or propeller beta operation.
    Reverse,
    /// Propeller blades commanded to the feather angle.
    Feathered,
    /// Unpowered rotating machinery driven by the freestream.
    Windmilling,
}

/// Failure selection for capability and operating-point calculations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureState {
    /// All installed propulsion units available.
    None,
    /// Zero-based installed-unit indices that are unavailable.
    UnitsUnavailable(Vec<usize>),
}

/// Aircraft services extracted from the propulsion system at this point.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PropulsionLoads {
    /// Mechanical accessory power extracted from shafts, W.
    pub accessory_power_w: f64,
    /// Compressor bleed mass flow extracted for aircraft services, kg/s.
    pub bleed_mass_flow_kg_s: f64,
    /// Electrical power demanded from the propulsion system, W.
    pub electrical_power_w: f64,
}

/// Named persistent states owned by the propulsion and energy system.
///
/// Examples are battery state of charge, spool speed, tank pressure and
/// component temperature. Values carry their unit explicitly; technology
/// models define and validate the names they consume.
#[derive(Debug, Clone, PartialEq)]
pub struct PropulsionStateValue {
    /// Technology-defined stable state name.
    pub name: String,
    /// Current state value.
    pub value: f64,
    /// SI unit or dimensionless marker.
    pub unit: String,
}

/// Persistent state presented to one otherwise deterministic evaluation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropulsionState {
    /// Named state values supplied to the technology model.
    pub values: Vec<PropulsionStateValue>,
}

/// A complete system-level operating-point request.
#[derive(Debug, Clone, PartialEq)]
pub struct PropulsionRequest {
    /// Ambient and kinematic operating condition.
    pub flight: FlightCondition,
    /// Requested force, normalized command, or named rating.
    pub demand: PropulsionDemand,
    /// Discrete propulsion operating mode.
    pub mode: OperatingMode,
    /// Installed-unit availability selection.
    pub failure: FailureState,
    /// Aircraft accessory, bleed and electrical extraction requests.
    pub loads: PropulsionLoads,
    /// Energy, thermal, and dynamic component state at the start of the step.
    pub state: PropulsionState,
    /// Time step used only by stateful technology implementations, s.
    pub time_step_s: Option<f64>,
}

/// Energy or consumable carrier crossing the system boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceKind {
    /// Conventional kerosene or sustainable drop-in aviation fuel.
    JetA,
    /// Gaseous or liquid hydrogen.
    Hydrogen,
    /// Stored electrical energy.
    ElectricalEnergy,
    /// Technology-defined carrier not yet represented by a core variant.
    Custom(String),
}

/// Positive consumption rate from an aircraft resource store.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceFlow {
    /// Carrier consumed or generated.
    pub resource: ResourceKind,
    /// Mass consumption, kg/s; absent for non-mass resources.
    pub mass_flow_kg_s: Option<f64>,
    /// Power consumption, W; negative values represent charging/generation.
    pub power_w: Option<f64>,
}

/// Derivative of persistent propulsion/resource state for mission integration.
#[derive(Debug, Clone, PartialEq)]
pub struct StateDerivative {
    /// Stable name of the persistent state being integrated.
    pub state: String,
    /// Signed state change per second.
    pub rate_per_s: f64,
    /// Unit of the derivative.
    pub unit: String,
}

/// One active physical, control, thermal, electrical, or certification limit.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveLimit {
    /// Stable limit name.
    pub name: String,
    /// Utilization divided by the limit; one is exactly active.
    pub utilization: f64,
}

/// Whether the result is inside the model's supported evidence domain.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidityStatus {
    /// Evaluation lies inside the declared model and evidence domain.
    Valid,
    /// Evaluation completed outside the declared validation domain.
    Extrapolated {
        /// Explanation of the exceeded evidence or model boundary.
        reason: String,
    },
}

/// Scalar residual exposing numerical or conservation closure.
#[derive(Debug, Clone, PartialEq)]
pub struct Residual {
    /// Stable residual or conservation-balance name.
    pub name: String,
    /// Signed residual value.
    pub value: f64,
    /// Residual unit.
    pub unit: String,
}

/// Optional technology-specific trace retained without leaking into consumers.
#[derive(Debug, Clone, PartialEq)]
pub enum TechnologyTrace {
    /// Complete output of the preserved mission turbofan calculation.
    LegacyTurbofan(ThrustOutput),
}

/// Authoritative output of a propulsion-system evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct PropulsionResult {
    /// Installed net force in aircraft body axes `[x, y, z]`, N.
    pub body_force_n: [f64; 3],
    /// Installed moment about the aircraft reference point `[L, M, N]`, N*m.
    pub body_moment_nm: [f64; 3],
    /// Resource consumption or generation rates.
    pub resource_flows: Vec<ResourceFlow>,
    /// Persistent-state derivatives for mission integration.
    pub state_derivatives: Vec<StateDerivative>,
    /// Net useful shaft power delivered by the system, W.
    pub shaft_power_w: Option<f64>,
    /// Net useful electrical power delivered by the system, W.
    pub electrical_power_w: Option<f64>,
    /// Heat that must be rejected by aircraft thermal management, W.
    pub heat_rejection_w: Option<f64>,
    /// Delivered shaft torque, N*m, when meaningful for the technology.
    pub torque_nm: Option<f64>,
    /// Delivered shaft or propulsor rotational speed, rev/min.
    pub rotational_speed_rpm: Option<f64>,
    /// Demand actually achieved after limits and control saturation.
    pub achieved_demand: PropulsionDemand,
    /// Physical or control limits active at the solved point.
    pub active_limits: Vec<ActiveLimit>,
    /// Numerical and conservation closure residuals.
    pub residuals: Vec<Residual>,
    /// Declared validity status of this operating point.
    pub validity: ValidityStatus,
    /// Model, dataset, calibration, and evidence traceability.
    pub provenance: ModelProvenance,
    /// Optional technology detail for diagnostics and migration parity.
    pub trace: Option<TechnologyTrace>,
}

/// Available system envelope at one flight condition and failure state.
#[derive(Debug, Clone, PartialEq)]
pub struct PropulsionCapability {
    /// Maximum installed body force `[x, y, z]`, N.
    pub maximum_body_force_n: [f64; 3],
    /// Minimum installed body force `[x, y, z]`, N.
    pub minimum_body_force_n: [f64; 3],
    /// Maximum useful shaft power, W, when applicable.
    pub maximum_shaft_power_w: Option<f64>,
    /// Limits governing the returned capability.
    pub active_limits: Vec<ActiveLimit>,
    /// Validity of the capability calculation.
    pub validity: ValidityStatus,
    /// Model and evidence provenance.
    pub provenance: ModelProvenance,
}

/// Read-only diagnostic information requested independently of evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticsQuery {
    /// Describe model identity, dataset, and evidence provenance.
    Provenance,
    /// Describe supported demands, modes, loads, and failure semantics.
    SupportedSemantics,
    /// Describe the technology model's declared validity domain.
    ValidityDomain,
}

/// One structured diagnostic item suitable for GUI and report presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticItem {
    /// Stable machine-readable diagnostic code.
    pub code: String,
    /// Concise user-facing explanation.
    pub message: String,
}

/// Result of a read-only model diagnostic query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropulsionDiagnostics {
    /// Requested diagnostic category.
    pub query: DiagnosticsQuery,
    /// Structured findings returned by the technology model.
    pub items: Vec<DiagnosticItem>,
    /// Provenance of the model answering the query.
    pub provenance: ModelProvenance,
}

/// One physical item in the propulsion mass/CG inventory.
#[derive(Debug, Clone, PartialEq)]
pub struct PropulsionMassItem {
    /// Component or assembly name.
    pub name: String,
    /// Installed mass, kg.
    pub mass_kg: f64,
    /// Component centre-of-mass position in body axes, m.
    pub body_position_m: [f64; 3],
}

/// Installed propulsion geometry needed by aero, loads, stability, and export.
#[derive(Debug, Clone, PartialEq)]
pub struct PropulsionInstallation {
    /// Reference positions of installed propulsion units in body axes, m.
    pub unit_positions_m: Vec<[f64; 3]>,
    /// Unit thrust axes expressed as body-axis unit vectors.
    pub thrust_axes_body: Vec<[f64; 3]>,
    /// Total propulsion nacelle wetted area, m^2.
    pub nacelle_wetted_area_m2: Option<f64>,
    /// Total propulsion frontal area, m^2.
    pub frontal_area_m2: Option<f64>,
}

/// Typed failures; physics implementations must not encode failure as zeros or NaN.
#[derive(Debug, Clone, PartialEq)]
pub enum PropulsionError {
    /// A scalar input was non-finite or outside its elementary valid range.
    InvalidInput {
        /// Stable name of the invalid input.
        field: &'static str,
        /// Rejected numeric value.
        value: f64,
    },
    /// The technology cannot interpret the requested control semantics.
    UnsupportedDemand(&'static str),
    /// The technology cannot represent the selected discrete mode.
    UnsupportedMode(OperatingMode),
    /// The technology cannot evaluate the selected installed-unit failures.
    UnsupportedFailureState,
    /// The request lies outside the physical or calibrated model domain.
    OutsideModelDomain(String),
    /// A technology implementation produced a non-finite output.
    NonFiniteOutput(&'static str),
    /// No propulsion model was configured.
    EmptySystem,
    /// Installation geometry cannot be represented by the selected model.
    InvalidInstallation(String),
}

impl fmt::Display for PropulsionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { field, value } => write!(f, "invalid {field}: {value}"),
            Self::UnsupportedDemand(reason) => write!(f, "unsupported propulsion demand: {reason}"),
            Self::UnsupportedMode(mode) => write!(f, "unsupported operating mode: {mode:?}"),
            Self::UnsupportedFailureState => write!(f, "unsupported propulsion failure state"),
            Self::OutsideModelDomain(reason) => {
                write!(f, "outside propulsion model domain: {reason}")
            }
            Self::NonFiniteOutput(field) => write!(f, "non-finite propulsion output: {field}"),
            Self::EmptySystem => write!(f, "propulsion system contains no technology models"),
            Self::InvalidInstallation(reason) => {
                write!(f, "invalid propulsion installation: {reason}")
            }
        }
    }
}

impl Error for PropulsionError {}
