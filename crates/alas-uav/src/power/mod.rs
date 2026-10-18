// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Powertrain v2: motor, battery, ESC, propeller and the coupled operating point.
//!
//! This module is the M2 physics core for the UAV propulsion chain
//! (UAV-PLAN section 3.2).  It is independent of the legacy
//! [`crate::propulsion_electric`] solver, which stays as the catalogue-bound
//! consumer, and has no dependency on the component database for its core
//! equations: every model is a plain SI parameter struct, and [`from_spec`]
//! builds them from `alas-uav-db` rows.
//!
//! # Conventions
//!
//! * SI throughout: m, kg, s, N, N*m, W, A, V, Ohm, K, rad/s, coulomb.  RPM
//!   appears only in output helpers.
//! * Shaft speed `omega` is positive in the thrust-producing sense.  Thrust is
//!   positive forward along the shaft axis; airspeed `v` is the true airspeed
//!   component along that axis (positive forward).
//! * Battery current is positive when discharging.  A motor current below the
//!   no-load current is rejected: regeneration and windmilling are outside the
//!   validity domain and reported through [`LimitFlag::Windmilling`].
//! * The motor resistance is the terminal (phase-to-phase equivalent DC)
//!   value, the same convention as Drela's model and the database.
//! * Temperatures are kelvin.  Winding temperature drives `Rm`, the magnet
//!   flux (hence `Kv`, `Kt`) and the no-load current.
//!
//! # Validity
//!
//! Steady state below magnetic saturation; the propeller coefficient map or
//! the BEMT low-Re polar range; no ESC commutation limits; no battery
//! polarisation transient (the optional 1-RC branch of the plan is not
//! modelled); no Peukert effect beyond the resistive sag.  Defaults that are
//! not from the database are flagged in the item documentation and must be
//! replaced by data before a number is claimed as validated.

pub mod battery;
pub mod bemt;
pub mod esc;
pub mod from_spec;
mod interp;
mod limits;
pub mod motor;
pub mod operating;
pub mod polar;
pub mod prop;
pub mod prop_map;
pub mod tabulate;

pub use battery::{BatteryModel, BatteryState, BatteryStep};
pub use bemt::{BemtGeometry, BemtSolution};
pub use esc::EscModel;
pub use limits::{Demand, LimitFlag, Limiter, RatingBasis};
pub use motor::{MotorLimits, MotorModel, MotorState, MotorThermal};
pub use operating::{Dynamics, Flight, OperatingPoint, Powertrain};
pub use polar::{AnalyticPolar, SectionPolar, TabulatedPolar};
pub use prop::{PropLoads, PropModel, PropSource};
pub use prop_map::PropMapTable;
pub use tabulate::{tabulate_bemt, TabulatedProp};

/// A failure of a powertrain model or of the operating-point solve.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PowerError {
    /// A parameter or request is not finite or violates a documented bound.
    #[error("{0}")]
    InvalidInput(String),
    /// A model needs a database field that is absent (never defaulted silently).
    #[error("missing {field} for {what}")]
    Missing {
        /// The model or component that needs the field.
        what: String,
        /// The absent field.
        field: &'static str,
    },
    /// The propeller map or polar has no data at the requested condition.
    #[error("propeller model has no data at {0}")]
    OutOfRange(String),
    /// The motor, ESC, battery and propeller have no equilibrium at the request.
    #[error("no operating point: {0}")]
    NoSolution(String),
    /// A thrust demand above what the powertrain can deliver.
    #[error("thrust demand {demand_n:.3} N exceeds the available {available_n:.3} N")]
    DemandUnreachable {
        /// Requested total thrust, N.
        demand_n: f64,
        /// Largest thrust at the duty limit, N.
        available_n: f64,
    },
}

pub(crate) fn finite_positive(value: f64, what: &str) -> Result<f64, PowerError> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(PowerError::InvalidInput(format!(
            "{what} must be finite and > 0, got {value}"
        )))
    }
}

pub(crate) fn finite_nonneg(value: f64, what: &str) -> Result<f64, PowerError> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(PowerError::InvalidInput(format!(
            "{what} must be finite and >= 0, got {value}"
        )))
    }
}
