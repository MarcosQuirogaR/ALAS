// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Source-bounded electric propeller operating-point calculations.
//!
//! The preliminary UAV optimizer needs thrust, current, and electrical power
//! at its actual flight speeds.  A catalogue title or a static-thrust claim
//! cannot supply those quantities.  This module resolves a reviewed component
//! selection into a coupled motor, battery, ESC, and propeller calculation,
//! retaining the performance-table and electrical-model boundaries with the
//! result.  It is deliberately separate from [`crate::optimizer`]: a map is
//! produced here before an optimizer consumes it, so the optimizer never has
//! to invent a propulsion number.
//!
//! The user-supplied legacy data directory contains 436 APC `PER3` tables.
//! Each is retained as a bounded Ct/Cp map.  The legacy equations are
//! represented by the fixed-voltage, zero-loss model; callers can provide
//! measured pack and ESC resistance through the public model when that
//! evidence becomes available.

mod acc2026;
mod cpacs35;
mod fixture;
mod mission;
mod solver;

use std::fmt;

pub use acc2026::{
    assess_acc2026_electrical, Acc2026ElectricalAssessment, Acc2026ElectricalFinding,
};
pub use cpacs35::{
    enrich_cpacs35_with_uav_electrical, render_uav_cpacs35_toolspecific, CpacsUavExtensionError,
    UavCpacs35ElectricalData, UAV_ELECTRICAL_EXTENSION_VERSION, UAV_ELECTRICAL_NAMESPACE,
};
pub use fixture::{
    apc_12x6e_performance_map, apc_performance_map, apc_performance_maps, PropellerPerformanceMap,
    PropellerSample,
};
pub use mission::{
    simulate_catalogue_mission, ElectricMissionPhase, ElectricMissionPhaseResult,
    ElectricMissionPlan, ElectricMissionResult,
};
pub use solver::{
    native_propulsion_map, solve_catalogue_powertrain, solve_powertrain,
    CataloguePowertrainSelection, ElectricFlightCondition, ElectricMotorModel,
    ElectricPowertrainModel, ElectricPropulsionResult, ElectricalCheck, ElectricalOperatingPoint,
    EscElectricalModel, FixedVoltageBattery,
};

/// A failure to resolve or calculate a source-bounded electric operating point.
#[derive(Debug, Clone, PartialEq)]
pub enum ElectricPropulsionError {
    /// A numerical request is not finite or violates a documented bound.
    InvalidInput(String),
    /// A selection names no catalogue record.
    MissingComponent {
        /// Requested catalogue identifier.
        id: String,
    },
    /// A selection names a record from the wrong catalogue family.
    WrongComponentFamily {
        /// Selected component identifier.
        id: String,
        /// Required catalogue family.
        expected: &'static str,
    },
    /// The selected catalogue record has no required published value.
    MissingEvidence {
        /// Selected component identifier.
        id: String,
        /// Source-backed field that is absent.
        field: &'static str,
    },
    /// The selected battery is outside a motor or ESC cell-count range.
    CellCountMismatch {
        /// Component whose range is violated.
        id: String,
        /// Selected battery cell count.
        cells: u16,
    },
    /// No propeller performance table is reviewed for the selection.
    UnsupportedPropeller {
        /// Selected propeller identifier.
        id: String,
    },
    /// The requested speed or RPM falls outside a reviewed table.
    PerformanceTableOutOfRange {
        /// True airspeed requested by the caller.
        speed_m_s: f64,
    },
    /// The reviewed RPM table does not bracket a motor/propeller equilibrium.
    NoEquilibrium {
        /// True airspeed requested by the caller.
        speed_m_s: f64,
        /// Lower RPM limit supplied by the reviewed table.
        minimum_rpm: f64,
        /// Upper RPM limit supplied by the reviewed table.
        maximum_rpm: f64,
    },
}

impl fmt::Display for ElectricPropulsionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => formatter.write_str(message),
            Self::MissingComponent { id } => write!(formatter, "catalogue has no component '{id}'"),
            Self::WrongComponentFamily { id, expected } => {
                write!(formatter, "component '{id}' is not a {expected}")
            }
            Self::MissingEvidence { id, field } => {
                write!(formatter, "component '{id}' has no published {field}")
            }
            Self::CellCountMismatch { id, cells } => {
                write!(formatter, "component '{id}' does not support the selected {cells}S battery")
            }
            Self::UnsupportedPropeller { id } => {
                write!(formatter, "no reviewed performance table is available for '{id}'")
            }
            Self::PerformanceTableOutOfRange { speed_m_s } => write!(
                formatter,
                "the reviewed propeller table does not cover {speed_m_s:.3} m/s"
            ),
            Self::NoEquilibrium {
                speed_m_s,
                minimum_rpm,
                maximum_rpm,
            } => write!(
                formatter,
                "no motor/propeller equilibrium at {speed_m_s:.3} m/s within the reviewed {minimum_rpm:.0}-{maximum_rpm:.0} RPM range"
            ),
        }
    }
}

impl std::error::Error for ElectricPropulsionError {}
