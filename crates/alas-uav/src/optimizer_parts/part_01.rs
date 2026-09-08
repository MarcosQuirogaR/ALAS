// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

#[path = "../optimizer/generation.rs"]
mod generation;
#[path = "../optimizer/topology.rs"]
mod topology;
#[path = "../optimizer/validation.rs"]
mod validation;

use std::fmt;

use crate::catalog::{Catalog, Dimensions};
use crate::{Finding, FindingKind, UavDesign, UavReport};

pub use generation::{
    EmpennageGeometry, FuselageGeometry, GeneratedGeometry, LandingGearGeometry, WingGeometry,
};
pub use topology::{
    optimize_for_topology, optimize_for_topology_with_control, TopologyOptimizationError,
    TopologyOptimizedUav,
};
use validation::validate_problem;

/// Inclusive finite bounds for one continuous design variable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VariableBounds {
    /// Smallest permitted value.
    pub minimum: f64,
    /// Largest permitted value.
    pub maximum: f64,
}

impl VariableBounds {
    fn valid_positive(self) -> bool {
        self.minimum.is_finite()
            && self.maximum.is_finite()
            && self.minimum > 0.0
            && self.maximum >= self.minimum
    }

    fn sample(self, unit: f64) -> f64 {
        self.minimum + unit * (self.maximum - self.minimum)
    }
}

/// Mission and payload objectives that every accepted design must meet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DesignObjectives {
    /// Required airborne time at the design cruise point.
    pub endurance_s: f64,
    /// Required still-air cruise range.
    pub range_m: f64,
    /// Design true airspeed.
    pub cruise_speed_m_s: f64,
    /// Maximum acceptable one-g stall speed.
    pub maximum_stall_speed_m_s: f64,
    /// Useful payload mass.
    pub payload_mass_kg: f64,
    /// Payload bounding box.
    pub payload_dimensions: Dimensions,
    /// Minimum drag-power to electrical-power ratio at cruise.
    pub minimum_propulsive_efficiency: f64,
    /// Ranking emphasis on efficiency rather than takeoff mass.
    pub efficiency_priority: f64,
}

/// Continuous geometry search domain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeometrySearchBounds {
    /// Wing reference area bounds.
    pub wing_area_m2: VariableBounds,
    /// Wing aspect-ratio bounds.
    pub wing_aspect_ratio: VariableBounds,
    /// Overall fuselage-length bounds.
    pub fuselage_length_m: VariableBounds,
    /// Wing leading-edge location as a fraction of fuselage length.
    pub wing_leading_edge_fraction: VariableBounds,
}

/// Explicit preliminary-airframe assumptions and applicability limits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreliminaryModel {
    /// Air density used for all preliminary flight points.
    pub air_density_kg_m3: f64,
    /// Maximum usable lift coefficient.
    pub maximum_lift_coefficient: f64,
    /// Aircraft zero-lift drag coefficient.
    pub zero_lift_drag_coefficient: f64,
    /// Oswald span efficiency used in `k = 1 / (pi e AR)`.
    pub oswald_efficiency: f64,
    /// Required positive maneuver load factor.
    pub limit_load_factor: f64,
    /// Structural factor applied to the spar-cap bending capacity.
    pub structural_safety_factor: f64,
    /// Horizontal-tail volume coefficient.
    pub horizontal_tail_volume_coefficient: f64,
    /// Vertical-tail volume coefficient.
    pub vertical_tail_volume_coefficient: f64,
    /// Horizontal-tail aspect ratio.
    pub horizontal_tail_aspect_ratio: f64,
    /// Vertical-tail aspect ratio.
    pub vertical_tail_aspect_ratio: f64,
    /// Forward CG limit from wing leading edge, divided by mean chord.
    pub forward_cg_chord_fraction: f64,
    /// Aft CG limit from wing leading edge, divided by mean chord.
    pub aft_cg_chord_fraction: f64,
    /// Clearance around the largest packaged item.
    pub equipment_clearance_m: f64,
    /// Longitudinal gap between packaged items.
    pub equipment_gap_m: f64,
    /// Fraction of fuselage length ahead of the equipment bay.
    pub nose_length_fraction: f64,
    /// Fraction of fuselage length behind the equipment bay.
    pub tailcone_length_fraction: f64,
    /// Spar-cap width divided by wing chord.
    pub spar_cap_width_fraction: f64,
    /// Distance between cap centroids divided by wing chord.
    pub spar_cap_separation_fraction: f64,
    /// Aileron planform area divided by wing area, per actuator.
    pub aileron_area_fraction: f64,
    /// Aileron chord divided by wing chord.
    pub aileron_chord_fraction: f64,
    /// Elevator area divided by horizontal-tail area.
    pub elevator_area_fraction: f64,
    /// Elevator chord divided by horizontal-tail chord.
    pub elevator_chord_fraction: f64,
    /// Absolute hinge-moment coefficient at the design condition.
    pub hinge_moment_coefficient: f64,
    /// Landing-gear track divided by wing span.
    pub landing_gear_track_fraction: f64,
    /// Landing-gear wheelbase divided by fuselage length.
    pub landing_gear_wheelbase_fraction: f64,
    /// Propeller-to-ground clearance.
    pub propeller_ground_clearance_m: f64,
    /// Fixed wiring, fastener, adhesive, and finish mass.
    pub fixed_systems_mass_kg: f64,
}

/// Installed avionics and electrical policies not represented by retail records.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SystemsDefinition {
    /// Additional flight-computer mass not represented by a catalogue record.
    pub avionics_mass_kg: f64,
    /// Additional flight-computer bounding box.
    pub avionics_dimensions: Dimensions,
    /// Continuous avionics electrical power drawn from the battery.
    pub avionics_power_w: f64,
    /// Continuous avionics current on the regulated control bus.
    pub control_bus_current_a: f64,
    /// Selected BEC voltage.
    pub control_bus_voltage_v: f64,
    /// Minimum receiver channels required by the control architecture.
    pub minimum_receiver_channels: u16,
    /// Continuous servo current divided by published stall current.
    pub servo_continuous_current_fraction: f64,
    /// Maximum fraction of battery nameplate energy consumed.
    pub maximum_depth_of_discharge: f64,
    /// Fraction of permitted energy retained after the mission.
    pub reserve_fraction: f64,
}

/// One independently evaluated per-propulsor motor/propeller operating point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropulsionOperatingPoint {
    /// True airspeed.
    pub speed_m_s: f64,
    /// Thrust available from one propeller at that airspeed.
    pub thrust_n: f64,
    /// Current through one motor and ESC at that operating point.
    pub motor_current_a: f64,
    /// Electrical input power to one motor at that operating point.
    pub motor_power_w: f64,
}

/// Traceable in-flight performance map for one discrete propulsion combination.
#[derive(Debug, Clone, PartialEq)]
pub struct PropulsionMap {
    /// Motor catalogue identifier.
    pub motor_id: String,
    /// Propeller catalogue identifier.
    pub propeller_id: String,
    /// Battery series-cell count used during evaluation.
    pub series_cells: u16,
    /// Number of identical motor/ESC/propeller installations on the shared pack.
    ///
    /// Stored operating points remain per propulsor, so the optimizer can
    /// check each motor and ESC against its individual rating while deriving
    /// total thrust, mass, and battery current from this count.
    pub motor_count: u16,
    /// Source or test-report identifier displayed in audits.
    pub evidence: String,
    /// Strictly increasing speed points.
    pub points: Vec<PropulsionOperatingPoint>,
}

/// One solved in-flight mission phase for the selected installed powertrain.
#[derive(Debug, Clone, PartialEq)]
pub struct MissionPhase {
    /// Stable display name, such as `climb` or `cruise`.
    pub name: String,
    /// Phase duration.
    pub duration_s: f64,
    /// True airspeed.
    pub speed_m_s: f64,
    /// Local air density used by the propulsion calculation.
    pub air_density_kg_m3: f64,
    /// Commanded ESC duty fraction used by the evaluated propulsion solver.
    pub throttle: f64,
    /// Aggregate installed thrust at this phase condition.
    pub total_thrust_n: f64,
    /// Per-motor current, used for motor and ESC rating checks.
    pub motor_current_a: f64,
    /// Per-motor electrical input power.
    pub motor_power_w: f64,
    /// Battery current summed across the installed propulsors.
    pub battery_current_a: f64,
    /// Battery electrical input power summed across the installed propulsors.
    pub battery_power_w: f64,
}

/// Source-resolved mission phases replacing a single cruise-energy surrogate.
#[derive(Debug, Clone, PartialEq)]
pub struct MissionProfile {
    /// Source identifier retained for design review and CPACS/export adapters.
    pub evidence: String,
    /// Ordered mission phases integrated into energy and peak-current demand.
    pub phases: Vec<MissionPhase>,
}

impl PropulsionMap {
    fn at_speed(&self, speed_m_s: f64) -> Option<PropulsionOperatingPoint> {
        if let Some(point) = self
            .points
            .iter()
            .find(|point| point.speed_m_s == speed_m_s)
        {
            return Some(*point);
        }
        self.points.windows(2).find_map(|window| {
            let lower = window[0];
            let upper = window[1];
            (speed_m_s > lower.speed_m_s && speed_m_s < upper.speed_m_s).then(|| {
                let fraction = (speed_m_s - lower.speed_m_s) / (upper.speed_m_s - lower.speed_m_s);
                PropulsionOperatingPoint {
                    speed_m_s,
                    thrust_n: interpolate(lower.thrust_n, upper.thrust_n, fraction),
                    motor_current_a: interpolate(
                        lower.motor_current_a,
                        upper.motor_current_a,
                        fraction,
                    ),
                    motor_power_w: interpolate(lower.motor_power_w, upper.motor_power_w, fraction),
                }
            })
        })
    }
}

/// Complete input to the deterministic mixed design search.
#[derive(Debug)]
pub struct OptimizationProblem<'a> {
    /// Reviewed component catalogue.
    pub catalog: &'a Catalog,
    /// Independently evaluated propulsion maps.
    pub propulsion_maps: &'a [PropulsionMap],
    /// Optional multi-phase electrical mission resolved before the search.
    ///
    /// Without this profile, the legacy endurance/range cruise surrogate is
    /// retained for compatibility with existing reviewed fixtures.
    pub mission_profile: Option<&'a MissionProfile>,
    /// Hard design objectives.
    pub objectives: DesignObjectives,
    /// Continuous geometry domain.
    pub geometry_bounds: GeometrySearchBounds,
    /// Preliminary physics assumptions.
    pub model: PreliminaryModel,
    /// Avionics and energy-use assumptions.
    pub systems: SystemsDefinition,
    /// Electronics role required by the mission, such as `gps_sensor`.
    pub required_electronics_role: &'a str,
    /// Seed controlling discrete choices and continuous samples.
    pub seed: u64,
    /// Number of candidates to evaluate.
    pub evaluations: usize,
}

/// Selected catalogue identifiers for an accepted aircraft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedComponents {
    /// Battery identifier.
    pub battery_id: String,
    /// Motor identifier.
    pub motor_id: String,
    /// Number of identical installed motor, ESC, and propeller sets.
    pub motor_count: u16,
    /// ESC identifier.
    pub esc_id: String,
    /// Propeller identifier.
    pub propeller_id: String,
    /// Servo identifier shared by the three primary actuators.
    pub servo_id: String,
    /// Number of installed instances of the selected servo.
    pub servo_count: u16,
    /// Structural material-stock identifier.
    pub material_id: String,
    /// Receiver identifier.
    pub receiver_id: String,
    /// Additional electronics identifier.
    pub electronics_id: String,
    /// Landing-gear hardware identifier.
    pub landing_gear_id: String,
    /// Propulsion-map evidence identifier.
    pub propulsion_evidence: String,
}

/// Objective values derived for one accepted design.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectiveMetrics {
    /// Still-air mission duration imposed by endurance and range.
    pub mission_duration_s: f64,
    /// Integrated electrical energy.
    pub mission_energy_wh: f64,
    /// Drag power divided by electrical input power at cruise.
    pub propulsive_efficiency: f64,
    /// Predicted loaded takeoff mass.
    pub takeoff_mass_kg: f64,
    /// Lower-is-better ranking value.
    pub score: f64,
}

/// A generated aircraft that passed the complete typed feasibility report.
#[derive(Debug, Clone, PartialEq)]
pub struct OptimizedUav {
    /// Generated airframe and placements consumed by the feasibility pass.
    pub design: UavDesign,
    /// Empty-finding acceptance report.
    pub report: UavReport,
    /// Wing, fuselage, empennage, and landing-gear geometry.
    pub geometry: GeneratedGeometry,
    /// Discrete catalogue selections.
    pub components: SelectedComponents,
    /// Objective metrics and ranking score.
    pub metrics: ObjectiveMetrics,
    /// Zero-based search evaluation that produced this design.
    pub evaluation_index: usize,
    /// Search seed retained for reproduction.
    pub seed: u64,
}

/// Count of candidate rejections in one typed physical category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectionCount {
    /// Physical failure or evidence-gap category.
    pub kind: FindingKind,
    /// Number of candidates carrying at least one such finding.
    pub candidates: usize,
}

/// Evidence returned when the search cannot verify any generated aircraft.
#[derive(Debug, Clone, PartialEq)]
pub struct NoFeasibleDesign {
    /// Number of candidates attempted.
    pub evaluated_candidates: usize,
    /// Per-category counts; one candidate can contribute to several rows.
    pub rejections: Vec<RejectionCount>,
    /// First representative cause retained for each rejection category.
    ///
    /// Counts alone cannot tell a designer whether the search stopped on a
    /// missing catalogue field, a physical overload, or an invalid result.
    /// The examples preserve the optimizer's evidence boundary without
    /// promoting any rejected candidate to a verified design.
    pub examples: Vec<RejectionExample>,
    /// Best generated aircraft that was evaluated but did not pass all checks.
    ///
    /// This is present only when candidate generation had enough evidence to
    /// build a geometry. It is a review artifact, never a feasible result.
    pub best_evaluated: Option<RejectedUav>,
}

/// One representative typed cause from a rejected candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectionExample {
    /// Physical failure or evidence-gap category.
    pub kind: FindingKind,
    /// Catalogue record or model quantity associated with the cause.
    pub subject: String,
    /// Evidence-bound explanation shown to the caller.
    pub message: String,
}

/// A generated aircraft retained as a non-verified review artifact.
#[derive(Debug, Clone, PartialEq)]
pub struct RejectedUav {
    /// Generated airframe and placements consumed by the feasibility pass.
    pub design: UavDesign,
    /// Complete findings that prevented verification.
    pub report: UavReport,
    /// Geometry available for a review figure.
    pub geometry: GeneratedGeometry,
    /// Discrete catalogue selections used for the candidate.
    pub components: SelectedComponents,
    /// Objective metrics when the candidate had enough data to compute them.
    pub metrics: Option<ObjectiveMetrics>,
    /// Zero-based search evaluation that produced this candidate.
    pub evaluation_index: usize,
    /// Search seed retained for reproduction.
    pub seed: u64,
}

/// Monotonic progress emitted after each completely evaluated candidate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OptimizationProgress {
    /// Candidates whose generation and feasibility checks have completed.
    pub evaluated_candidates: usize,
    /// Fixed candidate budget from the problem definition.
    pub total_candidates: usize,
    /// Candidates that passed every requested constraint so far.
    pub verified_candidates: usize,
    /// Best lower-is-better score seen so far.
    pub best_score: Option<f64>,
}

/// Input or physical search failure.
#[derive(Debug, Clone, PartialEq)]
pub enum OptimizationError {
    /// The requested domain or objective is mathematically invalid.
    InvalidProblem(String),
    /// Every generated design failed or lacked required evidence.
    NoFeasibleDesign(Box<NoFeasibleDesign>),
    /// A caller requested cancellation between deterministic candidates.
    Cancelled {
        /// Number of candidates fully evaluated before cancellation.
        evaluated_candidates: usize,
    },
}

impl fmt::Display for OptimizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProblem(message) => write!(formatter, "invalid UAV problem: {message}"),
            Self::NoFeasibleDesign(summary) => write!(
                formatter,
                "no verified UAV design after {} candidates",
                summary.evaluated_candidates
            ),
            Self::Cancelled {
                evaluated_candidates,
            } => write!(
                formatter,
                "UAV optimization cancelled after {evaluated_candidates} candidates"
            ),
        }
    }
}
