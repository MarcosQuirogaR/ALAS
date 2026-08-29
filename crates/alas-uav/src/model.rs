// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Typed fixed-wing UAV selections, demands, findings, and reports.

use crate::catalog::{BatterySpec, Dimensions, EscSpec, MotorSpec, PropellerSpec, ServoSpec};

/// Severity of a physical feasibility finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// A supplied limit is exceeded or supplied values are incompatible.
    Failure,
    /// A required check cannot be completed from the supplied evidence.
    Unverified,
}

/// Stable categories for findings shown by optimizers and user interfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingKind {
    /// An input is zero, negative, non-finite, or internally inconsistent.
    InvalidInput,
    /// A physically required catalogue or analysis field is absent.
    MissingData,
    /// Battery cell count falls outside a component's supported range.
    CellCountMismatch,
    /// Battery discharge rating is exceeded.
    BatteryCurrentOverload,
    /// ESC continuous current rating is exceeded.
    EscCurrentOverload,
    /// Motor current rating is exceeded.
    MotorCurrentOverload,
    /// Motor electrical power rating is exceeded.
    MotorPowerOverload,
    /// BEC continuous or peak current rating is exceeded.
    BecCurrentOverload,
    /// Servo voltage is outside its published operating range.
    ServoVoltageMismatch,
    /// A selected receiver or electronics supply range excludes the control bus.
    ControlVoltageMismatch,
    /// Servo torque demand exceeds interpolated published stall torque.
    ServoTorqueOverload,
    /// Selected propeller is not the motor's published recommendation.
    PropellerCompatibilityUnverified,
    /// A component bounding box does not fit inside its assigned bay.
    PackagingViolation,
    /// Loaded center of gravity lies outside the declared airframe limits.
    CenterOfGravityViolation,
    /// Required lift coefficient exceeds maximum lift coefficient.
    InsufficientLift,
    /// Available thrust at speed is less than calculated drag.
    InsufficientThrust,
    /// Mission energy plus reserve exceeds allowed battery energy.
    EnergyShortfall,
    /// Useful propulsive efficiency is below the stated design objective.
    EfficiencyShortfall,
    /// Structural demand or load factor exceeds an evaluated limit.
    StructuralOverload,
    /// Selected landing gear is rated below aircraft takeoff mass.
    LandingGearOverload,
}

/// One actionable failure or evidence gap.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Whether the finding proves failure or prevents verification.
    pub severity: Severity,
    /// Machine-readable physical category.
    pub kind: FindingKind,
    /// Component, flight case, or subsystem involved.
    pub subject: String,
    /// Concise explanation suitable for a run report.
    pub message: String,
    /// Demand or observed value where meaningful.
    pub required: Option<f64>,
    /// Rating or available value where meaningful.
    pub available: Option<f64>,
    /// Units shared by `required` and `available`.
    pub units: Option<&'static str>,
}

/// Axis-aligned location of a component in aircraft body axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Component centre x coordinate, positive aft.
    pub center_x_m: f64,
    /// Component centre y coordinate, positive right.
    pub center_y_m: f64,
    /// Component centre z coordinate, positive up.
    pub center_z_m: f64,
    /// Whether the component must fit inside the airframe equipment bay.
    pub inside_equipment_bay: bool,
}

/// Axis-aligned equipment bay in aircraft body axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EquipmentBay {
    /// Forward bound.
    pub min_x_m: f64,
    /// Aft bound.
    pub max_x_m: f64,
    /// Left bound.
    pub min_y_m: f64,
    /// Right bound.
    pub max_y_m: f64,
    /// Lower bound.
    pub min_z_m: f64,
    /// Upper bound.
    pub max_z_m: f64,
}

/// Fixed airframe properties before removable components and payload are fitted.
#[derive(Debug, Clone, PartialEq)]
pub struct Airframe {
    /// Wings, fuselage, empennage, landing gear, and installed wiring mass.
    pub fixed_mass_kg: f64,
    /// Longitudinal CG of the fixed airframe.
    pub fixed_cg_x_m: f64,
    /// Wing reference area.
    pub wing_area_m2: f64,
    /// Maximum usable lift coefficient for the checked configuration.
    pub maximum_lift_coefficient: f64,
    /// Zero-lift drag coefficient for the checked configuration.
    pub zero_lift_drag_coefficient: f64,
    /// Induced-drag factor in `C_D = C_D0 + k C_L^2`.
    pub induced_drag_factor: f64,
    /// Forward loaded-CG limit.
    pub forward_cg_limit_x_m: f64,
    /// Aft loaded-CG limit.
    pub aft_cg_limit_x_m: f64,
    /// Bay used by placements marked `inside_equipment_bay`.
    pub equipment_bay: EquipmentBay,
}

/// One installed battery and its location.
#[derive(Debug, Clone, PartialEq)]
pub struct InstalledBattery {
    /// Catalogue identifier or user-defined name.
    pub id: String,
    /// Physical battery specification.
    pub spec: BatterySpec,
    /// Installed location.
    pub placement: Placement,
}

/// One installed motor and its location.
#[derive(Debug, Clone, PartialEq)]
pub struct InstalledMotor {
    /// Catalogue identifier or user-defined name.
    pub id: String,
    /// Physical motor specification.
    pub spec: MotorSpec,
    /// Installed location.
    pub placement: Placement,
}

/// One installed ESC and its location.
#[derive(Debug, Clone, PartialEq)]
pub struct InstalledEsc {
    /// Catalogue identifier or user-defined name.
    pub id: String,
    /// Physical ESC specification.
    pub spec: EscSpec,
    /// Installed location.
    pub placement: Placement,
}

/// One installed propeller and its location.
#[derive(Debug, Clone, PartialEq)]
pub struct InstalledPropeller {
    /// Catalogue identifier or user-defined name.
    pub id: String,
    /// Physical propeller specification.
    pub spec: PropellerSpec,
    /// Installed location.
    pub placement: Placement,
}

/// One additional motor, ESC, and propeller installation on the same aircraft.
///
/// [`UavDesign`] retains its primary propulsion fields for existing callers;
/// this type represents each further identical or independently selected
/// propulsor so mass, packaging, and per-unit electrical ratings remain
/// physical for distributed-electric designs.
#[derive(Debug, Clone, PartialEq)]
pub struct InstalledPropulsor {
    /// Motor installed in this propulsor set.
    pub motor: InstalledMotor,
    /// ESC installed with this motor.
    pub esc: InstalledEsc,
    /// Propeller installed on this motor.
    pub propeller: InstalledPropeller,
}

/// One installed servo and its worst-case control demand.
#[derive(Debug, Clone, PartialEq)]
pub struct InstalledServo {
    /// Catalogue identifier or user-defined name.
    pub id: String,
    /// Physical servo specification.
    pub spec: ServoSpec,
    /// Installed location.
    pub placement: Placement,
    /// Maximum required actuator torque from a hinge-moment analysis.
    pub required_torque_nm: f64,
    /// Expected continuous current for the checked mission segment.
    pub continuous_current_a: Option<f64>,
}

/// A payload or avionics item represented by its measured mass and envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct InstalledMass {
    /// Item name.
    pub id: String,
    /// Measured or manufacturer-published mass.
    pub mass_kg: Option<f64>,
    /// Physical envelope.
    pub dimensions: Option<Dimensions>,
    /// Installed location.
    pub placement: Placement,
}

/// Electrical demand at the limiting propulsion condition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropulsionElectricalDemand {
    /// Total battery current, including propulsion and auxiliary loads.
    pub battery_current_a: f64,
    /// Current through one ESC and motor.
    pub motor_current_a: f64,
    /// Electrical input power to one motor.
    pub motor_power_w: f64,
}

/// Receiver/servo power-bus demand.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlBusDemand {
    /// Selected BEC output voltage.
    pub voltage_v: f64,
    /// Continuous non-servo avionics current on the BEC.
    pub other_continuous_current_a: f64,
}

/// Explicit energy-use policy; neither reserve nor depth of discharge is guessed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MissionEnergyDemand {
    /// Integrated electrical mission energy before reserve.
    pub mission_energy_wh: f64,
    /// Maximum permitted fraction of nameplate capacity consumed.
    pub maximum_depth_of_discharge: f64,
    /// Fraction of the permitted energy retained after the planned mission.
    pub reserve_fraction: f64,
}

/// One evaluated aerodynamic and propeller operating point.
#[derive(Debug, Clone, PartialEq)]
pub struct FlightCondition {
    /// Case name, such as `cruise` or `turn`.
    pub name: String,
    /// Air density at the condition.
    pub density_kg_m3: f64,
    /// True airspeed.
    pub speed_m_s: f64,
    /// Normal load factor, positive upward.
    pub load_factor: f64,
    /// Thrust available at this speed from a propeller/motor operating map.
    pub available_thrust_n: Option<f64>,
}

/// Structural demands produced by an analytical or finite-element model.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralCase {
    /// Load-case name.
    pub name: String,
    /// Applied positive load factor.
    pub load_factor: f64,
    /// Evaluated allowable positive load factor.
    pub allowable_load_factor: Option<f64>,
    /// Evaluated wing-root bending-moment demand.
    pub wing_root_bending_moment_nm: Option<f64>,
    /// Evaluated wing-root bending-moment capacity including safety factors.
    pub allowable_wing_root_bending_moment_nm: Option<f64>,
}

/// Complete selected-component and analysis input to the feasibility pass.
#[derive(Debug, Clone, PartialEq)]
pub struct UavDesign {
    /// Generated airframe.
    pub airframe: Airframe,
    /// Selected propulsion battery.
    pub battery: InstalledBattery,
    /// Selected motor.
    pub motor: InstalledMotor,
    /// Selected ESC.
    pub esc: InstalledEsc,
    /// Selected propeller.
    pub propeller: InstalledPropeller,
    /// Additional installed motor/ESC/propeller sets beyond the primary set.
    pub additional_propulsors: Vec<InstalledPropulsor>,
    /// Installed control actuators.
    pub servos: Vec<InstalledServo>,
    /// Payload, receiver, autopilot, sensors, and other mass items.
    pub other_items: Vec<InstalledMass>,
    /// Limiting electrical demand.
    pub propulsion_demand: PropulsionElectricalDemand,
    /// Control-bus selection and continuous avionics demand.
    pub control_bus: ControlBusDemand,
    /// Mission energy integration and reserve policy.
    pub mission_energy: MissionEnergyDemand,
    /// Aerodynamic/propulsion operating points.
    pub flight_conditions: Vec<FlightCondition>,
    /// Structural cases from the selected structural model.
    pub structural_cases: Vec<StructuralCase>,
}

impl UavDesign {
    /// Iterate every installed propulsor as motor, ESC, and propeller triples.
    pub fn propulsors(
        &self,
    ) -> impl Iterator<Item = (&InstalledMotor, &InstalledEsc, &InstalledPropeller)> {
        std::iter::once((&self.motor, &self.esc, &self.propeller)).chain(
            self.additional_propulsors
                .iter()
                .map(|propulsor| (&propulsor.motor, &propulsor.esc, &propulsor.propeller)),
        )
    }

    /// Count all installed motor/ESC/propeller sets.
    pub fn propulsor_count(&self) -> usize {
        1 + self.additional_propulsors.len()
    }
}

/// Derived result for one flight condition.
#[derive(Debug, Clone, PartialEq)]
pub struct FlightConditionResult {
    /// Case name.
    pub name: String,
    /// Lift coefficient required at the declared load factor.
    pub required_lift_coefficient: f64,
    /// Drag from the declared parabolic polar when the point is pre-stall.
    pub required_thrust_n: Option<f64>,
}

/// Coupled UAV feasibility result.
#[derive(Debug, Clone, PartialEq)]
pub struct UavReport {
    /// Loaded takeoff mass when every installed mass is known.
    pub takeoff_mass_kg: Option<f64>,
    /// Loaded longitudinal centre of gravity when every mass is known.
    pub center_of_gravity_x_m: Option<f64>,
    /// Nominal nameplate battery energy.
    pub nominal_battery_energy_wh: Option<f64>,
    /// Energy available to the planned mission after explicit policy limits.
    pub mission_energy_available_wh: Option<f64>,
    /// Derived aerodynamic point results.
    pub flight_conditions: Vec<FlightConditionResult>,
    /// Proven failures and unverified constraints.
    pub findings: Vec<Finding>,
}

impl UavReport {
    /// True only when every requested constraint was checked and passed.
    pub fn verified_feasible(&self) -> bool {
        self.findings.is_empty()
    }

    /// Whether at least one supplied constraint is demonstrably violated.
    pub fn has_failure(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity == Severity::Failure)
    }

    /// Whether missing evidence prevents a complete verdict.
    pub fn has_unverified_constraint(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity == Severity::Unverified)
    }
}
