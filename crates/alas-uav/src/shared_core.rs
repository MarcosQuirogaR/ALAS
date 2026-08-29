// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Adapter from preliminary UAV output to the production geometry and VLM core.
//!
//! The mixed optimizer owns preliminary sizing and its parabolic-polar checks.
//! This module does not replace those checks: it reconstructs the generated
//! aircraft with [`alas_geom`] primitives, independently runs
//! [`alas_aero::vlm`], and reports the disagreement. Airfoil identity and
//! operating point are explicit inputs because neither can be inferred from a
//! retail component catalogue.

use std::fmt;

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::{self, VlmResult};
use alas_atmo::Atmosphere;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, FuselageXSecError, DEFAULT_SHAPE};
use alas_geom::aircraft::wing::{Wing, WingXSec};

use crate::optimizer::{GeneratedGeometry, OptimizedUav, TopologyOptimizedUav};
use crate::topology::{TopologyUnavailableReason, UavAnalysisPath, UavTopology};

const STANDARD_GRAVITY_M_S2: f64 = 9.806_65;

/// User-controlled inputs that preliminary geometry does not determine.
#[derive(Debug, Clone, PartialEq)]
pub struct SharedCoreInputs {
    /// Main-wing section resolved by the production airfoil library.
    pub main_airfoil_name: String,
    /// Horizontal- and vertical-tail section.
    pub tail_airfoil_name: String,
    /// Geopotential altitude passed to the production atmosphere, in metres.
    pub altitude_m: f64,
    /// True airspeed, in metres per second.
    pub speed_m_s: f64,
    /// Geometric angle of attack, in degrees.
    pub angle_of_attack_deg: f64,
    /// Spanwise panels per loft interval.
    pub spanwise_resolution: usize,
    /// Chordwise panels per loft interval.
    pub chordwise_resolution: usize,
}

/// A successfully completed independent shared-core assessment.
#[derive(Debug, Clone, PartialEq)]
pub struct SharedCoreAssessment {
    /// The exact production geometry supplied to VLM and downstream viewers.
    pub airplane: Airplane,
    /// Production VLM force, moment, and coefficient result.
    pub vlm: VlmResult,
    /// One-g lift coefficient required at the evaluated density and speed.
    pub required_lift_coefficient: f64,
    /// VLM lift minus one-g weight; negative means the evaluated angle is low.
    pub lift_margin_n: f64,
    /// Induced drag coefficient predicted by the preliminary parabolic polar
    /// at the VLM lift coefficient.
    pub preliminary_induced_drag_coefficient: f64,
    /// VLM induced drag coefficient minus the preliminary prediction.
    pub induced_drag_coefficient_delta: f64,
    /// Parasite drag retained from the explicit preliminary model input.
    pub preliminary_zero_lift_drag_coefficient: f64,
}

impl SharedCoreAssessment {
    /// Classify whether this exact VLM operating point carries one-g weight.
    pub fn lift_verdict(&self) -> SharedCoreLiftVerdict {
        if self.lift_margin_n >= 0.0 {
            SharedCoreLiftVerdict::Passed
        } else {
            SharedCoreLiftVerdict::InsufficientLift
        }
    }
}

/// Product-level lift verdict from the shared production VLM core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedCoreLiftVerdict {
    /// VLM lift equals or exceeds one-g weight at the evaluated operating point.
    Passed,
    /// VLM completed, but the selected operating point does not support weight.
    InsufficientLift,
}

/// Completed shared-core assessment plus its directly supported hard verdict.
///
/// VLM does not provide viscous drag, stall, propulsion, or structural proof,
/// so this type deliberately gates lift only. Preliminary checks for those
/// other disciplines remain separately visible and are not promoted here.
#[derive(Debug, Clone, PartialEq)]
pub struct SharedCoreVerification {
    /// Full numerical result retained for comparison and diagnosis.
    pub assessment: SharedCoreAssessment,
    /// Whether the evaluated VLM operating point carries one-g weight.
    pub lift_verdict: SharedCoreLiftVerdict,
}

/// Why the geometry conversion or shared-core solve did not complete.
#[derive(Debug, Clone, PartialEq)]
pub enum SharedCoreFailure {
    /// The requested product path lacks the topology-specific physical model.
    UnsupportedTopology {
        /// Selected fixed-wing arrangement.
        topology: UavTopology,
        /// Engineering path that cannot make a supportable claim.
        path: UavAnalysisPath,
        /// Missing mixer or control-geometry model.
        reason: TopologyUnavailableReason,
    },
    /// An explicit input is invalid or an airfoil name cannot be resolved.
    InvalidInput(String),
    /// The fuselage primitive rejected a generated cross-section.
    Geometry(FuselageXSecError),
    /// The production VLM solver rejected or could not solve the mesh.
    Aerodynamics(vlm::VlmError),
    /// The accepted optimizer result unexpectedly lacks mass or CG evidence.
    MissingAcceptedEvidence(&'static str),
}

impl fmt::Display for SharedCoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedTopology {
                topology,
                path,
                reason,
            } => write!(
                formatter,
                "{} is unavailable for {}: {}",
                topology.label(),
                path.label(),
                reason.description()
            ),
            Self::InvalidInput(message) => formatter.write_str(message),
            Self::Geometry(error) => write!(formatter, "UAV geometry conversion failed: {error}"),
            Self::Aerodynamics(error) => write!(formatter, "shared VLM evaluation failed: {error}"),
            Self::MissingAcceptedEvidence(field) => {
                write!(formatter, "accepted UAV has no verified {field}")
            }
        }
    }
}

impl std::error::Error for SharedCoreFailure {}

impl From<FuselageXSecError> for SharedCoreFailure {
    fn from(error: FuselageXSecError) -> Self {
        Self::Geometry(error)
    }
}

impl From<vlm::VlmError> for SharedCoreFailure {
    fn from(error: vlm::VlmError) -> Self {
        Self::Aerodynamics(error)
    }
}

/// Convert generated preliminary geometry into the production airplane model.
///
/// The optimizer currently generates untapered planforms, so each surface has
/// two equal-chord sections. The tail quarter chord remains at the same 90%
/// fuselage station used by preliminary sizing. No sweep, twist, or airfoil is
/// invented.
pub fn build_airplane(
    geometry: GeneratedGeometry,
    center_of_gravity_x_m: f64,
    main_airfoil_name: &str,
    tail_airfoil_name: &str,
) -> Result<Airplane, SharedCoreFailure> {
    build_airplane_for_topology(
        UavTopology::ConventionalTail,
        geometry,
        center_of_gravity_x_m,
        main_airfoil_name,
        tail_airfoil_name,
    )
}

/// Convert generated preliminary geometry into a selected production topology.
///
/// The supplied [`GeneratedGeometry`] stores reference horizontal and vertical
/// tail areas. A V-tail derives two planar panels whose horizontal and vertical
/// projected areas recover those values. A flying wing uses only the main wing;
/// its ignored empennage values may be zero but must remain finite. This is a
/// geometry/VLM boundary, not evidence that a V-tail mixer or flying-wing
/// elevon/reflex control system has been sized.
pub fn build_airplane_for_topology(
    topology: UavTopology,
    geometry: GeneratedGeometry,
    center_of_gravity_x_m: f64,
    main_airfoil_name: &str,
    tail_airfoil_name: &str,
) -> Result<Airplane, SharedCoreFailure> {
    validate_geometry(topology, geometry, center_of_gravity_x_m)?;
    let main_airfoil = Airfoil::from_name(main_airfoil_name).ok_or_else(|| {
        SharedCoreFailure::InvalidInput(format!(
            "main airfoil '{main_airfoil_name}' is not available"
        ))
    })?;
    let tail_airfoil = Airfoil::from_name(tail_airfoil_name).ok_or_else(|| {
        SharedCoreFailure::InvalidInput(format!(
            "tail airfoil '{tail_airfoil_name}' is not available"
        ))
    })?;

    let wing = geometry.wing;
    let mut wings = vec![main_wing(wing, main_airfoil)];
    wings.extend(tail_wings(topology, geometry, tail_airfoil));

    let fuselage = fuselage(geometry)?;
    Ok(Airplane {
        name: airplane_name(topology),
        xyz_ref: [center_of_gravity_x_m, 0.0, 0.0],
        wings,
        fuselages: vec![fuselage],
        s_ref: wing.area_m2,
        c_ref: wing.mean_chord_m,
        b_ref: wing.span_m,
    })
}

/// Run production VLM without replacing the optimizer's feasibility report.
pub fn assess_with_shared_core(
    optimized: &OptimizedUav,
    inputs: &SharedCoreInputs,
) -> Result<SharedCoreAssessment, SharedCoreFailure> {
    validate_inputs(inputs)?;
    let mass_kg = optimized
        .report
        .takeoff_mass_kg
        .ok_or(SharedCoreFailure::MissingAcceptedEvidence("takeoff mass"))?;
    let cg_x_m = optimized.report.center_of_gravity_x_m.ok_or(
        SharedCoreFailure::MissingAcceptedEvidence("loaded center of gravity"),
    )?;
    assess_generated_geometry_with_shared_core(
        optimized.geometry,
        mass_kg,
        cg_x_m,
        optimized.design.airframe.induced_drag_factor,
        optimized.design.airframe.zero_lift_drag_coefficient,
        inputs,
    )
}

/// Run the production VLM assessment for a topology-preserving result.
pub fn assess_with_shared_core_for_topology(
    optimized: &TopologyOptimizedUav,
    inputs: &SharedCoreInputs,
) -> Result<SharedCoreAssessment, SharedCoreFailure> {
    let preliminary = optimized.preliminary();
    let mass_kg = preliminary
        .report
        .takeoff_mass_kg
        .ok_or(SharedCoreFailure::MissingAcceptedEvidence("takeoff mass"))?;
    let cg_x_m = preliminary.report.center_of_gravity_x_m.ok_or(
        SharedCoreFailure::MissingAcceptedEvidence("loaded center of gravity"),
    )?;
    assess_generated_geometry_with_topology(
        optimized.topology,
        preliminary.geometry,
        mass_kg,
        cg_x_m,
        preliminary.design.airframe.induced_drag_factor,
        preliminary.design.airframe.zero_lift_drag_coefficient,
        inputs,
    )
}

/// Run the shared-core assessment and classify only the lift condition that
/// the production VLM directly establishes.
pub fn verify_with_shared_core(
    optimized: &OptimizedUav,
    inputs: &SharedCoreInputs,
) -> Result<SharedCoreVerification, SharedCoreFailure> {
    let assessment = assess_with_shared_core(optimized, inputs)?;
    let lift_verdict = assessment.lift_verdict();
    Ok(SharedCoreVerification {
        assessment,
        lift_verdict,
    })
}

/// Classify the lift condition for a topology-preserving VLM assessment.
pub fn verify_with_shared_core_for_topology(
    optimized: &TopologyOptimizedUav,
    inputs: &SharedCoreInputs,
) -> Result<SharedCoreVerification, SharedCoreFailure> {
    let assessment = assess_with_shared_core_for_topology(optimized, inputs)?;
    let lift_verdict = assessment.lift_verdict();
    Ok(SharedCoreVerification {
        assessment,
        lift_verdict,
    })
}

/// Evaluate explicit generated geometry and preliminary polar inputs in VLM.
///
/// This lower-level entry point supports independent characterization and
/// import workflows without requiring a fabricated component selection. It
/// applies the same production path as [`assess_with_shared_core`].
pub fn assess_generated_geometry_with_shared_core(
    geometry: GeneratedGeometry,
    takeoff_mass_kg: f64,
    center_of_gravity_x_m: f64,
    preliminary_induced_drag_factor: f64,
    preliminary_zero_lift_drag_coefficient: f64,
    inputs: &SharedCoreInputs,
) -> Result<SharedCoreAssessment, SharedCoreFailure> {
    assess_generated_geometry_with_topology(
        UavTopology::ConventionalTail,
        geometry,
        takeoff_mass_kg,
        center_of_gravity_x_m,
        preliminary_induced_drag_factor,
        preliminary_zero_lift_drag_coefficient,
        inputs,
    )
}

/// Evaluate explicit topology-specific geometry with the production VLM core.
pub fn assess_generated_geometry_with_topology(
    topology: UavTopology,
    geometry: GeneratedGeometry,
    takeoff_mass_kg: f64,
    center_of_gravity_x_m: f64,
    preliminary_induced_drag_factor: f64,
    preliminary_zero_lift_drag_coefficient: f64,
    inputs: &SharedCoreInputs,
) -> Result<SharedCoreAssessment, SharedCoreFailure> {
    ensure_topology_support(topology, UavAnalysisPath::SharedCoreLift)?;
    validate_inputs(inputs)?;
    if !takeoff_mass_kg.is_finite()
        || takeoff_mass_kg <= 0.0
        || !preliminary_induced_drag_factor.is_finite()
        || preliminary_induced_drag_factor <= 0.0
        || !preliminary_zero_lift_drag_coefficient.is_finite()
        || preliminary_zero_lift_drag_coefficient < 0.0
    {
        return Err(SharedCoreFailure::InvalidInput(
            "shared-core mass and preliminary drag inputs must be finite and physically admissible"
                .to_owned(),
        ));
    }
    let airplane = build_airplane_for_topology(
        topology,
        geometry,
        center_of_gravity_x_m,
        &inputs.main_airfoil_name,
        &inputs.tail_airfoil_name,
    )?;
    let atmosphere = Atmosphere::new(inputs.altitude_m);
    let operating_point = OperatingPoint::new(
        atmosphere,
        inputs.speed_m_s,
        inputs.angle_of_attack_deg,
        0.0,
        0.0,
        0.0,
        0.0,
    );
    let vlm = vlm::run(
        &airplane,
        &operating_point,
        inputs.spanwise_resolution,
        inputs.chordwise_resolution,
    )?;
    let dynamic_pressure_pa = operating_point.dynamic_pressure();
    let required_lift_coefficient =
        takeoff_mass_kg * STANDARD_GRAVITY_M_S2 / (dynamic_pressure_pa * geometry.wing.area_m2);
    let preliminary_induced_drag_coefficient =
        preliminary_induced_drag_factor * vlm.cl_lift.powi(2);
    Ok(SharedCoreAssessment {
        lift_margin_n: vlm.lift - takeoff_mass_kg * STANDARD_GRAVITY_M_S2,
        induced_drag_coefficient_delta: vlm.cd_drag - preliminary_induced_drag_coefficient,
        preliminary_zero_lift_drag_coefficient,
        airplane,
        vlm,
        required_lift_coefficient,
        preliminary_induced_drag_coefficient,
    })
}

fn ensure_topology_support(
    topology: UavTopology,
    path: UavAnalysisPath,
) -> Result<(), SharedCoreFailure> {
    match topology.availability(path) {
        crate::topology::TopologyAvailability::Available => Ok(()),
        crate::topology::TopologyAvailability::Unavailable(reason) => {
            Err(SharedCoreFailure::UnsupportedTopology {
                topology,
                path,
                reason,
            })
        }
    }
}

fn main_wing(wing: crate::optimizer::WingGeometry, airfoil: Airfoil) -> Wing {
    let half_span_m = wing.span_m / 2.0;
    Wing::new(
        "UAV Main Wing",
        vec![
            WingXSec::new(
                [wing.leading_edge_x_m, 0.0, 0.0],
                wing.mean_chord_m,
                0.0,
                airfoil.clone(),
            ),
            WingXSec::new(
                [wing.leading_edge_x_m, half_span_m, 0.0],
                wing.mean_chord_m,
                0.0,
                airfoil,
            ),
        ],
        true,
    )
}

fn tail_wings(topology: UavTopology, geometry: GeneratedGeometry, airfoil: Airfoil) -> Vec<Wing> {
    if !topology.uses_generated_empennage() {
        return Vec::new();
    }
    let tail = geometry.empennage;
    let tail_x_m =
        geometry.wing.leading_edge_x_m + 0.25 * geometry.wing.mean_chord_m + tail.tail_arm_m;
    let horizontal_chord_m = tail.horizontal_area_m2 / tail.horizontal_span_m;
    let vertical_chord_m = tail.vertical_area_m2 / tail.vertical_span_m;
    match topology {
        UavTopology::ConventionalTail | UavTopology::TTail => {
            let horizontal_z_m = if topology == UavTopology::TTail {
                tail.vertical_span_m
            } else {
                0.0
            };
            vec![
                Wing::new(
                    "UAV Horizontal Tail",
                    vec![
                        WingXSec::new(
                            [tail_x_m, 0.0, horizontal_z_m],
                            horizontal_chord_m,
                            0.0,
                            airfoil.clone(),
                        ),
                        WingXSec::new(
                            [tail_x_m, tail.horizontal_span_m / 2.0, horizontal_z_m],
                            horizontal_chord_m,
                            0.0,
                            airfoil.clone(),
                        ),
                    ],
                    true,
                ),
                Wing::new(
                    "UAV Vertical Tail",
                    vec![
                        WingXSec::new([tail_x_m, 0.0, 0.0], vertical_chord_m, 0.0, airfoil.clone()),
                        WingXSec::new(
                            [tail_x_m, 0.0, tail.vertical_span_m],
                            vertical_chord_m,
                            0.0,
                            airfoil,
                        ),
                    ],
                    false,
                ),
            ]
        }
        UavTopology::VTail => {
            let horizontal_projection_m = tail.horizontal_span_m / 2.0;
            let inclination_rad = tail
                .vertical_area_m2
                .sqrt()
                .atan2(tail.horizontal_area_m2.sqrt());
            let cosine = inclination_rad.cos();
            let span_m = horizontal_projection_m / cosine;
            let surface_area_m2 = 0.5 * (tail.horizontal_area_m2 + tail.vertical_area_m2);
            let chord_m = surface_area_m2 / span_m;
            let tip_z_m = span_m * inclination_rad.sin();
            vec![
                inclined_tail(
                    "UAV V-Tail Port",
                    tail_x_m,
                    -horizontal_projection_m,
                    tip_z_m,
                    chord_m,
                    airfoil.clone(),
                ),
                inclined_tail(
                    "UAV V-Tail Starboard",
                    tail_x_m,
                    horizontal_projection_m,
                    tip_z_m,
                    chord_m,
                    airfoil,
                ),
            ]
        }
        UavTopology::FlyingWing => Vec::new(),
    }
}

fn inclined_tail(
    name: &str,
    x_m: f64,
    tip_y_m: f64,
    tip_z_m: f64,
    chord_m: f64,
    airfoil: Airfoil,
) -> Wing {
    Wing::new(
        name,
        vec![
            WingXSec::new([x_m, 0.0, 0.0], chord_m, 0.0, airfoil.clone()),
            WingXSec::new([x_m, tip_y_m, tip_z_m], chord_m, 0.0, airfoil),
        ],
        false,
    )
}

fn airplane_name(topology: UavTopology) -> String {
    match topology {
        UavTopology::ConventionalTail => "Generated Fixed-Wing UAV".to_owned(),
        _ => format!("UAV {}", topology.label()),
    }
}

fn fuselage(geometry: GeneratedGeometry) -> Result<Fuselage, SharedCoreFailure> {
    let length_m = geometry.fuselage.length_m;
    let radius_m = 0.5 * geometry.fuselage.diameter_m;
    let sections = [
        (0.0, 0.0),
        (0.15 * length_m, radius_m),
        (0.85 * length_m, radius_m),
        (length_m, 0.0),
    ]
    .into_iter()
    .map(|(x_m, radius_m)| {
        FuselageXSec::new([x_m, 0.0, 0.0], Some(radius_m), None, None, DEFAULT_SHAPE)
    })
    .collect::<Result<Vec<_>, _>>()?;
    Ok(Fuselage::new("UAV Fuselage", sections))
}

fn validate_inputs(inputs: &SharedCoreInputs) -> Result<(), SharedCoreFailure> {
    if !inputs.altitude_m.is_finite()
        || !inputs.speed_m_s.is_finite()
        || inputs.speed_m_s <= 0.0
        || !inputs.angle_of_attack_deg.is_finite()
        || inputs.spanwise_resolution == 0
        || inputs.chordwise_resolution == 0
    {
        return Err(SharedCoreFailure::InvalidInput(
            "shared-core altitude, speed, angle, and mesh must be finite and positive where applicable"
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_geometry(
    topology: UavTopology,
    geometry: GeneratedGeometry,
    center_of_gravity_x_m: f64,
) -> Result<(), SharedCoreFailure> {
    let values = [
        geometry.wing.area_m2,
        geometry.wing.span_m,
        geometry.wing.mean_chord_m,
        geometry.fuselage.length_m,
        geometry.fuselage.diameter_m,
    ];
    let invalid_base = values
        .into_iter()
        .any(|value| !value.is_finite() || value <= 0.0);
    let invalid_empennage = topology.uses_generated_empennage()
        && [
            geometry.empennage.horizontal_area_m2,
            geometry.empennage.horizontal_span_m,
            geometry.empennage.vertical_area_m2,
            geometry.empennage.vertical_span_m,
            geometry.empennage.tail_arm_m,
        ]
        .into_iter()
        .any(|value| !value.is_finite() || value <= 0.0);
    if invalid_base
        || invalid_empennage
        || !geometry.wing.leading_edge_x_m.is_finite()
        || !center_of_gravity_x_m.is_finite()
    {
        return Err(SharedCoreFailure::InvalidInput(
            "generated geometry and loaded CG must be finite and positive where applicable"
                .to_owned(),
        ));
    }
    Ok(())
}
