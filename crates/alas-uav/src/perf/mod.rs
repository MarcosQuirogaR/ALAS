// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Time-resolved mission segments for fixed-wing electric UAVs.
//!
//! A mission is a list of `spec::mission::Segment`s flown by a point-mass
//! model integrated with RK4 at `dt <= 0.05 s`, so the battery current is a
//! time series that can be logged at 10 Hz and block-averaged over 0.5 s
//! exactly as the ACC measurement box does (`score::quantity::Series`).
//!
//! Conventions (SI throughout): lengths m, masses kg, time s, angles rad,
//! speeds m/s, forces N, currents A, energies J. `x` is the ground-projected
//! path length, `h` the height above the field, `v` the true airspeed. A
//! positive wind is a *headwind* (it lowers ground speed on the straights it
//! opposes). Mass is constant (electric aircraft).
//!
//! The powertrain enters through the narrow [`Powertrain`] trait: thrust,
//! battery current and terminal voltage at (throttle, airspeed, density,
//! battery state). [`StubPowertrain`] is a test double; the physical model
//! plugs in behind the same trait. The aerodynamic polar enters through
//! [`DragPolar`].
//!
//! Validity: quasi-steady aerodynamics, small thrust incidence, constant wind,
//! no gusts, no structure or stability feedback, rigid point mass. The flare
//! and the rotation are simple kinematic models (see `ground` and `landing`)
//! and carry model-form uncertainty that is not calibrated here.

mod airframe;
mod flight;
mod ground;
mod integrate;
mod landing;
mod mission;
mod outputs;
mod powertrain;
mod turn;

pub use airframe::{Airframe, DragPolar, Environment, ParabolicPolar, G0};
pub use ground::{ground_roll, GroundRollResult, TakeoffSpec};
pub use integrate::{Sample, Y};
pub use landing::{land, LandingResult, LandingSpec};
pub use mission::{fly_mission, CruiseControl, MissionConfig, MissionResult, SegmentSummary};
pub use outputs::score_inputs;
pub use powertrain::{BatteryState, PowerPoint, Powertrain, StubPowertrain};
pub use turn::{steady_turn, SteadyTurn};

/// A mission or segment could not be flown.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PerfError {
    /// An input is not finite or outside its documented range.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// The required lift coefficient exceeds the maximum.
    #[error("stall: CL {cl:.3} exceeds CLmax {cl_max:.3} ({context})")]
    Stall {
        /// Required lift coefficient.
        cl: f64,
        /// Maximum lift coefficient.
        cl_max: f64,
        /// Where it happened.
        context: String,
    },
    /// The aircraft cannot sustain the commanded condition.
    #[error("cannot sustain: {0}")]
    CannotSustain(String),
    /// A segment did not reach its end condition.
    #[error("no convergence: {0}")]
    NoConvergence(String),
    /// The powertrain model refused the operating point.
    #[error("powertrain: {0}")]
    Powertrain(String),
    /// A segment kind that is not simulated in v1.
    #[error("unsupported segment: {0}")]
    Unsupported(String),
}
