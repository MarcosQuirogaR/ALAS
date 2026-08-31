// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Technology-neutral propulsion-system contracts.
//!
//! This module is the boundary between aircraft analyses and propulsion
//! technology physics. Consumers request a system operating point and receive
//! forces, moments, resource flows, power, limits, validity and provenance;
//! they do not reconstruct thrust or fuel burn from catalogue fields.
//! Technology models retain their own internal equations. The initial
//! [`LegacyTurbofanModel`] is deliberately a lossless adapter around
//! [`crate::mission_turbofan`] so consumers can migrate before its physics is
//! replaced.

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

/// Technology-independent interface consumed by aircraft analyses.
pub trait PropulsionSystemModel: Send + Sync {
    /// Evaluate one system operating point.
    fn evaluate(&self, request: &PropulsionRequest) -> Result<PropulsionResult, PropulsionError>;
    /// Solve the technology controls needed to deliver a requested body force.
    /// Implementations that lack an inverse solver return
    /// [`PropulsionError::UnsupportedDemand`].
    fn solve_for_force(
        &self,
        mut request: PropulsionRequest,
        required_body_force_n: [f64; 3],
    ) -> Result<PropulsionResult, PropulsionError> {
        request.demand = PropulsionDemand::RequiredBodyForceN(required_body_force_n);
        self.evaluate(&request)
    }
    /// Return available minimum and maximum output at one flight condition.
    fn capability(
        &self,
        flight: FlightCondition,
        failure: FailureState,
    ) -> Result<PropulsionCapability, PropulsionError>;
    /// Return the explicit installed propulsion mass and CG inventory.
    fn mass_inventory(&self) -> &[PropulsionMassItem];
    /// Return geometry and axes of installed propulsion units.
    fn installation(&self) -> &PropulsionInstallation;
    /// Return model, dataset, calibration, and evidence traceability.
    fn provenance(&self) -> &ModelProvenance;
    /// Return model capabilities, validity, or provenance without evaluating
    /// an operating point.
    fn diagnostics(&self, query: DiagnosticsQuery) -> PropulsionDiagnostics;
}

/// Lossless system adapter around the current mission turbofan evaluator.
pub struct LegacyTurbofanModel {
    inputs: TurbofanInputs,
    params: VehicleBuilderParams,
    compressor_nondimensional_massflow: f64,
    provenance: ModelProvenance,
    mass_inventory: Vec<PropulsionMassItem>,
    installation: PropulsionInstallation,
}

const LEGACY_VALIDITY_REASON: &str = "legacy mission turbofan compatibility model is preserved for migration parity and has not been validated as a physical engine deck";

impl LegacyTurbofanModel {
    /// Construct a lossless adapter around an already sized mission turbofan.
    ///
    /// The preserved evaluator returns one scalar all-engine force and no
    /// per-unit forces. Consequently, this compatibility adapter accepts only
    /// finite, co-axial `+X` installations whose lateral and vertical offsets
    /// sum to zero. That symmetry is what makes its reported zero installed
    /// moment physically representable. New technology implementations should
    /// calculate each unit's force and moment instead of using this constraint.
    pub fn new(
        inputs: TurbofanInputs,
        params: VehicleBuilderParams,
        compressor_nondimensional_massflow: f64,
        provenance: ModelProvenance,
        mass_inventory: Vec<PropulsionMassItem>,
        installation: PropulsionInstallation,
    ) -> Result<Self, PropulsionError> {
        let engine_count = inputs.number_of_engines;
        if !engine_count.is_finite()
            || engine_count < 1.0
            || engine_count.fract().abs() > f64::EPSILON
        {
            return Err(PropulsionError::InvalidInput {
                field: "number of engines",
                value: engine_count,
            });
        }
        let engine_count = engine_count as usize;
        if installation.unit_positions_m.len() != engine_count
            || installation.thrust_axes_body.len() != engine_count
        {
            return Err(PropulsionError::InvalidInstallation(format!(
                "expected {engine_count} positions and thrust axes"
            )));
        }
        if installation
            .unit_positions_m
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
        {
            return Err(PropulsionError::InvalidInstallation(
                "unit positions must be finite".to_owned(),
            ));
        }
        let axis_tolerance = 1.0e-12;
        if installation.thrust_axes_body.iter().any(|axis| {
            axis.iter().any(|value| !value.is_finite())
                || (axis[0] - 1.0).abs() > axis_tolerance
                || axis[1].abs() > axis_tolerance
                || axis[2].abs() > axis_tolerance
        }) {
            return Err(PropulsionError::InvalidInstallation(
                "legacy scalar adapter requires finite unit thrust axes aligned with +X".to_owned(),
            ));
        }
        let summed_y_m: f64 = installation.unit_positions_m.iter().map(|p| p[1]).sum();
        let summed_z_m: f64 = installation.unit_positions_m.iter().map(|p| p[2]).sum();
        let position_scale_m = installation
            .unit_positions_m
            .iter()
            .flat_map(|position| position.iter())
            .fold(1.0_f64, |scale, value| scale.max(value.abs()));
        let symmetry_tolerance_m = position_scale_m * 1.0e-12;
        if summed_y_m.abs() > symmetry_tolerance_m || summed_z_m.abs() > symmetry_tolerance_m {
            return Err(PropulsionError::InvalidInstallation(
                "legacy scalar adapter requires symmetric offsets producing zero net moment"
                    .to_owned(),
            ));
        }

        Ok(Self {
            inputs,
            params,
            compressor_nondimensional_massflow,
            provenance,
            mass_inventory,
            installation,
        })
    }

    fn demand_fraction(&self, demand: PropulsionDemand) -> Result<f64, PropulsionError> {
        match demand {
            PropulsionDemand::NormalizedForce(value)
                if value.is_finite() && (0.0..=1.0).contains(&value) =>
            {
                Ok(value)
            }
            PropulsionDemand::NormalizedForce(value) => Err(PropulsionError::InvalidInput {
                field: "normalized force demand",
                value,
            }),
            PropulsionDemand::RequiredBodyForceN(_) => Err(PropulsionError::UnsupportedDemand(
                "legacy turbofan adapter has no inverse force solver",
            )),
            PropulsionDemand::Rating(_) => Err(PropulsionError::UnsupportedDemand(
                "legacy turbofan adapter has no named rating schedules",
            )),
            PropulsionDemand::RatedFraction { .. } => Err(PropulsionError::UnsupportedDemand(
                "legacy turbofan adapter has no named rating schedules",
            )),
        }
    }

    fn raw_evaluate(
        &self,
        flight: FlightCondition,
        demand: f64,
    ) -> Result<ThrustOutput, PropulsionError> {
        let freestream = Freestream::from(flight);
        let raw = evaluate_thrust(
            &freestream,
            &self.inputs,
            &self.params,
            self.compressor_nondimensional_massflow,
            demand,
        );
        for (name, value) in [
            ("thrust", raw.thrust_n),
            ("fuel flow", raw.fuel_flow_rate_kg_s),
            ("power", raw.power_w),
        ] {
            if !value.is_finite() {
                return Err(PropulsionError::NonFiniteOutput(name));
            }
        }
        Ok(raw)
    }
}

impl PropulsionSystemModel for LegacyTurbofanModel {
    fn evaluate(&self, request: &PropulsionRequest) -> Result<PropulsionResult, PropulsionError> {
        if request.mode != OperatingMode::Normal {
            return Err(PropulsionError::UnsupportedMode(request.mode));
        }
        if request.failure != FailureState::None {
            return Err(PropulsionError::UnsupportedFailureState);
        }
        if request.loads != PropulsionLoads::default() {
            return Err(PropulsionError::UnsupportedDemand(
                "legacy turbofan adapter has no accessory, bleed, or electrical load model",
            ));
        }
        let demand = self.demand_fraction(request.demand)?;
        let raw = self.raw_evaluate(request.flight, demand)?;
        Ok(PropulsionResult {
            body_force_n: [raw.thrust_n, 0.0, 0.0],
            body_moment_nm: [0.0; 3],
            resource_flows: vec![ResourceFlow {
                resource: ResourceKind::JetA,
                mass_flow_kg_s: Some(raw.fuel_flow_rate_kg_s),
                power_w: None,
            }],
            state_derivatives: vec![StateDerivative {
                state: "jet_a_mass_kg".to_owned(),
                rate_per_s: -raw.fuel_flow_rate_kg_s,
                unit: "kg/s".to_owned(),
            }],
            shaft_power_w: None,
            electrical_power_w: None,
            heat_rejection_w: None,
            torque_nm: None,
            rotational_speed_rpm: None,
            achieved_demand: PropulsionDemand::NormalizedForce(demand),
            active_limits: Vec::new(),
            residuals: Vec::new(),
            validity: ValidityStatus::Extrapolated {
                reason: LEGACY_VALIDITY_REASON.to_owned(),
            },
            provenance: self.provenance.clone(),
            trace: Some(TechnologyTrace::LegacyTurbofan(raw)),
        })
    }

    fn capability(
        &self,
        flight: FlightCondition,
        failure: FailureState,
    ) -> Result<PropulsionCapability, PropulsionError> {
        if failure != FailureState::None {
            return Err(PropulsionError::UnsupportedFailureState);
        }
        let maximum = self.raw_evaluate(flight, 1.0)?;
        let minimum = self.raw_evaluate(flight, 0.0)?;
        Ok(PropulsionCapability {
            maximum_body_force_n: [maximum.thrust_n, 0.0, 0.0],
            minimum_body_force_n: [minimum.thrust_n, 0.0, 0.0],
            maximum_shaft_power_w: None,
            active_limits: Vec::new(),
            validity: ValidityStatus::Extrapolated {
                reason: LEGACY_VALIDITY_REASON.to_owned(),
            },
            provenance: self.provenance.clone(),
        })
    }

    fn mass_inventory(&self) -> &[PropulsionMassItem] {
        &self.mass_inventory
    }

    fn installation(&self) -> &PropulsionInstallation {
        &self.installation
    }

    fn provenance(&self) -> &ModelProvenance {
        &self.provenance
    }

    fn diagnostics(&self, query: DiagnosticsQuery) -> PropulsionDiagnostics {
        let message = match query {
            DiagnosticsQuery::Provenance => "Legacy mission turbofan adapter; outputs retain their original equations and selected dataset.",
            DiagnosticsQuery::SupportedSemantics => "Supports normal mode, all units available, normalized force demand, and zero aircraft-service extraction only.",
            DiagnosticsQuery::ValidityDomain => LEGACY_VALIDITY_REASON,
        };
        PropulsionDiagnostics {
            query,
            items: vec![DiagnosticItem {
                code: "legacy-turbofan-adapter".to_owned(),
                message: message.to_owned(),
            }],
            provenance: self.provenance.clone(),
        }
    }
}

/// Initial orchestrator facade. It owns the selected system implementation and
/// is the stable injection point for mission, performance, GUI and reporting.
pub struct PropulsionOrchestrator {
    model: Box<dyn PropulsionSystemModel>,
}

impl PropulsionOrchestrator {
    /// Select the system-level technology implementation used by consumers.
    pub fn new(model: impl PropulsionSystemModel + 'static) -> Self {
        Self {
            model: Box::new(model),
        }
    }

    /// Evaluate one propulsion-system operating point.
    pub fn evaluate(
        &self,
        request: &PropulsionRequest,
    ) -> Result<PropulsionResult, PropulsionError> {
        self.model.evaluate(request)
    }

    /// Query available system output at a flight condition and failure state.
    pub fn capability(
        &self,
        flight: FlightCondition,
        failure: FailureState,
    ) -> Result<PropulsionCapability, PropulsionError> {
        self.model.capability(flight, failure)
    }

    /// Delegate an inverse force request to the selected technology model.
    pub fn solve_for_force(
        &self,
        request: PropulsionRequest,
        required_body_force_n: [f64; 3],
    ) -> Result<PropulsionResult, PropulsionError> {
        self.model.solve_for_force(request, required_body_force_n)
    }

    /// Return the selected model's installed mass inventory.
    pub fn mass_inventory(&self) -> &[PropulsionMassItem] {
        self.model.mass_inventory()
    }

    /// Return the selected model's installation geometry and thrust axes.
    pub fn installation(&self) -> &PropulsionInstallation {
        self.model.installation()
    }

    /// Return model and evidence provenance.
    pub fn provenance(&self) -> &ModelProvenance {
        self.model.provenance()
    }

    /// Query the selected technology model's capabilities and provenance.
    pub fn diagnostics(&self, query: DiagnosticsQuery) -> PropulsionDiagnostics {
        self.model.diagnostics(query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission_turbofan::{size_turbofan, PartPowerModel};

    fn fixture() -> Result<(LegacyTurbofanModel, Freestream, ThrustOutput), PropulsionError> {
        let inputs = TurbofanInputs {
            number_of_engines: 2.0,
            bypass_ratio: 10.0,
            overall_pressure_ratio: 60.0,
            fan_pressure_ratio: 1.45,
            turbine_inlet_temperature_k: 1670.0,
            cruise_mach: 0.84,
            cruise_altitude_m: 11887.2,
            design_thrust_total_n: 197136.97010079,
        };
        let params = VehicleBuilderParams {
            part_power_model: PartPowerModel::LegacyLinear,
            ..VehicleBuilderParams::default()
        };
        let sized = size_turbofan(&inputs, &params);
        let flight = sized.sea_level_static.freestream;
        let raw = evaluate_thrust(
            &flight,
            &inputs,
            &params,
            sized.compressor_nondimensional_massflow,
            0.63,
        );
        let model = LegacyTurbofanModel::new(
            inputs,
            params,
            sized.compressor_nondimensional_massflow,
            ModelProvenance {
                model: ModelIdentity {
                    family: "legacy-turbofan".to_owned(),
                    version: "1".to_owned(),
                },
                dataset: Some("regression fixture".to_owned()),
                sources: Vec::new(),
            },
            Vec::new(),
            PropulsionInstallation {
                unit_positions_m: vec![[0.0, -5.0, 0.0], [0.0, 5.0, 0.0]],
                thrust_axes_body: vec![[1.0, 0.0, 0.0]; 2],
                nacelle_wetted_area_m2: None,
                frontal_area_m2: None,
            },
        )?;
        Ok((model, flight, raw))
    }

    #[test]
    fn legacy_adapter_is_bit_exact_for_force_and_fuel() -> Result<(), PropulsionError> {
        let (model, flight, raw) = fixture()?;
        let result = model.evaluate(&PropulsionRequest {
            flight: (&flight).into(),
            demand: PropulsionDemand::NormalizedForce(0.63),
            mode: OperatingMode::Normal,
            failure: FailureState::None,
            loads: PropulsionLoads::default(),
            state: PropulsionState::default(),
            time_step_s: None,
        })?;
        assert_eq!(result.body_force_n, [raw.thrust_n, 0.0, 0.0]);
        assert_eq!(
            result.resource_flows[0].mass_flow_kg_s,
            Some(raw.fuel_flow_rate_kg_s)
        );
        assert_eq!(
            result.state_derivatives[0].rate_per_s,
            -raw.fuel_flow_rate_kg_s
        );
        assert_eq!(result.trace, Some(TechnologyTrace::LegacyTurbofan(raw)));
        assert_eq!(
            result.achieved_demand,
            PropulsionDemand::NormalizedForce(0.63)
        );
        assert_eq!(result.torque_nm, None);
        assert_eq!(result.rotational_speed_rpm, None);
        assert_eq!(
            result.validity,
            ValidityStatus::Extrapolated {
                reason: LEGACY_VALIDITY_REASON.to_owned()
            }
        );
        Ok(())
    }

    #[test]
    fn orchestrator_preserves_capability_and_metadata() -> Result<(), PropulsionError> {
        let (model, flight, _) = fixture()?;
        let orchestrator = PropulsionOrchestrator::new(model);
        let capability = orchestrator.capability((&flight).into(), FailureState::None)?;
        assert!(capability.maximum_body_force_n[0] > capability.minimum_body_force_n[0]);
        assert_eq!(orchestrator.installation().unit_positions_m.len(), 2);
        assert_eq!(orchestrator.provenance().model.family, "legacy-turbofan");
        assert_eq!(
            capability.validity,
            ValidityStatus::Extrapolated {
                reason: LEGACY_VALIDITY_REASON.to_owned()
            }
        );
        Ok(())
    }

    #[test]
    fn unsupported_semantics_are_typed_errors() -> Result<(), PropulsionError> {
        let (model, flight, _) = fixture()?;
        let result = model.evaluate(&PropulsionRequest {
            flight: (&flight).into(),
            demand: PropulsionDemand::RequiredBodyForceN([10.0, 0.0, 0.0]),
            mode: OperatingMode::Normal,
            failure: FailureState::None,
            loads: PropulsionLoads::default(),
            state: PropulsionState::default(),
            time_step_s: None,
        });
        assert!(matches!(result, Err(PropulsionError::UnsupportedDemand(_))));

        let rating = model.evaluate(&PropulsionRequest {
            flight: (&flight).into(),
            demand: PropulsionDemand::Rating(PropulsionRating::MaximumContinuous),
            mode: OperatingMode::Normal,
            failure: FailureState::None,
            loads: PropulsionLoads::default(),
            state: PropulsionState::default(),
            time_step_s: None,
        });
        assert!(matches!(rating, Err(PropulsionError::UnsupportedDemand(_))));
        Ok(())
    }

    #[test]
    fn legacy_adapter_does_not_silently_ignore_loads_or_propeller_modes(
    ) -> Result<(), PropulsionError> {
        let (model, flight, _) = fixture()?;
        let loaded = model.evaluate(&PropulsionRequest {
            flight: (&flight).into(),
            demand: PropulsionDemand::NormalizedForce(0.5),
            mode: OperatingMode::Normal,
            failure: FailureState::None,
            loads: PropulsionLoads {
                electrical_power_w: 10_000.0,
                ..PropulsionLoads::default()
            },
            state: PropulsionState::default(),
            time_step_s: None,
        });
        assert!(matches!(loaded, Err(PropulsionError::UnsupportedDemand(_))));

        let feathered = model.evaluate(&PropulsionRequest {
            flight: (&flight).into(),
            demand: PropulsionDemand::NormalizedForce(0.0),
            mode: OperatingMode::Feathered,
            failure: FailureState::None,
            loads: PropulsionLoads::default(),
            state: PropulsionState::default(),
            time_step_s: None,
        });
        assert_eq!(
            feathered,
            Err(PropulsionError::UnsupportedMode(OperatingMode::Feathered))
        );
        Ok(())
    }

    #[test]
    fn diagnostics_make_legacy_limitations_visible() -> Result<(), PropulsionError> {
        let (model, _, _) = fixture()?;
        let diagnostics = model.diagnostics(DiagnosticsQuery::SupportedSemantics);
        assert_eq!(diagnostics.items[0].code, "legacy-turbofan-adapter");
        assert!(diagnostics.items[0]
            .message
            .contains("zero aircraft-service"));
        let validity = model.diagnostics(DiagnosticsQuery::ValidityDomain);
        assert_eq!(validity.items[0].message, LEGACY_VALIDITY_REASON);
        Ok(())
    }

    #[test]
    fn normalized_force_is_strictly_bounded() -> Result<(), PropulsionError> {
        let (model, flight, _) = fixture()?;
        for demand in [-0.01, 1.01, f64::NAN] {
            let result = model.evaluate(&PropulsionRequest {
                flight: (&flight).into(),
                demand: PropulsionDemand::NormalizedForce(demand),
                mode: OperatingMode::Normal,
                failure: FailureState::None,
                loads: PropulsionLoads::default(),
                state: PropulsionState::default(),
                time_step_s: None,
            });
            assert!(matches!(result, Err(PropulsionError::InvalidInput { .. })));
        }
        Ok(())
    }

    #[test]
    fn legacy_constructor_rejects_nonrepresentable_installations() -> Result<(), PropulsionError> {
        let inputs = TurbofanInputs {
            number_of_engines: 2.0,
            bypass_ratio: 10.0,
            overall_pressure_ratio: 60.0,
            fan_pressure_ratio: 1.45,
            turbine_inlet_temperature_k: 1670.0,
            cruise_mach: 0.84,
            cruise_altitude_m: 11887.2,
            design_thrust_total_n: 197136.97010079,
        };
        let provenance = ModelProvenance {
            model: ModelIdentity {
                family: "legacy-turbofan".to_owned(),
                version: "1".to_owned(),
            },
            dataset: None,
            sources: Vec::new(),
        };
        let asymmetric = LegacyTurbofanModel::new(
            inputs,
            VehicleBuilderParams::reference_compatibility(),
            1.0,
            provenance.clone(),
            Vec::new(),
            PropulsionInstallation {
                unit_positions_m: vec![[0.0, -4.0, 0.0], [0.0, 5.0, 0.0]],
                thrust_axes_body: vec![[1.0, 0.0, 0.0]; 2],
                nacelle_wetted_area_m2: None,
                frontal_area_m2: None,
            },
        );
        assert!(matches!(
            asymmetric,
            Err(PropulsionError::InvalidInstallation(_))
        ));

        let tilted = LegacyTurbofanModel::new(
            inputs,
            VehicleBuilderParams::reference_compatibility(),
            1.0,
            provenance,
            Vec::new(),
            PropulsionInstallation {
                unit_positions_m: vec![[0.0, -5.0, 0.0], [0.0, 5.0, 0.0]],
                thrust_axes_body: vec![[1.0, 0.0, 0.0], [0.99, 0.01, 0.0]],
                nacelle_wetted_area_m2: None,
                frontal_area_m2: None,
            },
        );
        assert!(matches!(
            tilted,
            Err(PropulsionError::InvalidInstallation(_))
        ));
        Ok(())
    }
}
