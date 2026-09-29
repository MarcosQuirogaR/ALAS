// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Candidate assembly and coupling to the hard feasibility gate.

mod geometry;
mod mission;
mod selection;
use super::{
    ObjectiveMetrics, OptimizationError, OptimizationProblem, PreliminaryModel,
    PropulsionOperatingPoint, SelectedComponents, SystemsDefinition,
};
use crate::catalog::ServoSpec;
use crate::feasibility::{
    Airframe, ControlBusDemand, InstalledBattery, InstalledEsc, InstalledMass, InstalledMotor,
    InstalledPropeller, InstalledPropulsor, InstalledServo, MissionEnergyDemand, Placement,
    PropulsionElectricalDemand, StructuralCase, STANDARD_GRAVITY_M_S2,
};
use crate::{Finding, FindingKind, Severity, UavDesign, UavReport};
use geometry::{
    airframe_masses, empennage_geometry, fuselage_geometry, place_equipment, wing_geometry,
};
pub use geometry::{
    EmpennageGeometry, FuselageGeometry, GeneratedGeometry, LandingGearGeometry, WingGeometry,
};
use mission::{flight_conditions, mission_duration, mission_summary};
use selection::{select_and_require, Selection};
use std::f64::consts::PI;

mod sampling;
pub(super) use sampling::*;
pub(in crate::optimizer) use selection::Choices;
mod metrics;
pub(super) use metrics::*;
