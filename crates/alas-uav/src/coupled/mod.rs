// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! M2 integration: `UavSpec` -> aerodynamics + powertrain + mission -> score.
//!
//! | module | content |
//! |---|---|
//! | [`geometry`] | spec wing -> lattice airplane with tip-clustered stations and NeuralFoil sections |
//! | [`hull`] | non-wing parasite-drag components (tails, boom, fuselage, wheels) |
//! | [`polar`] | `perf::DragPolar` from the viscous VLM, the drag build-up and the 1 g Reynolds line |
//! | [`powertrain`] | `perf::Powertrain` from `power::Powertrain::solve` (tabulated BEMT propeller) |
//! | [`trim_drag`] | induced drag of the tail lift that trims the pitching moment |
//! | [`slipstream_drag`] | drag increment of the parts in the propeller wash (installed-thrust loss) |
//! | [`assess`] | the whole chain: models, mission flight, score |
//! | [`summary`] | quasi-steady numbers teams publish (static thrust, Vstall, Vmax, climb rate, endurance) |
//!
//! Ground effect on the take-off roll uses Raymer Eq. 12.60 (the default of
//! `perf::Airframe`).  Mass and CG come from the tree overrides until the
//! airframe lane lands; the fuselage envelope and wheels come from [`hull::Hull`].
//!
//! # Status
//!
//! Implemented and numerically verified by the tests of this module.  Physical
//! validation against published aircraft is the blind-prediction test
//! (`tests/uav_blind.rs`) and its report: errors are reported, not tuned away.

pub mod assess;
pub mod geometry;
pub mod hull;
pub mod polar;
pub mod powertrain;
pub mod slipstream_drag;
pub mod summary;
pub mod trim_drag;

pub use assess::{tree_mass, AeroSettings, Assessment, GroundSetup, Models, Vehicle};
pub use geometry::{build_wing, resolve_airfoil, WingBuild};
pub use hull::{wheel, Fuselage, Hull};
pub use polar::{PolarInfo, PolarSettings, WingPolar};
pub use powertrain::{ElectricPowertrain, ElectricSettings};
pub use slipstream_drag::{MotorStation, SlipstreamDrag};
pub use summary::{performance_summary, PerformanceSummary};
pub use trim_drag::{wing_cm0, TrimDrag, TrimSettings};

/// A failure of the integrated chain.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CoupledError {
    /// The spec is incomplete or out of range for the integrated models.
    #[error("spec: {0}")]
    Spec(String),
    /// Aerodynamic model.
    #[error(transparent)]
    Aero(#[from] crate::aero::AeroError),
    /// Powertrain model.
    #[error(transparent)]
    Power(#[from] crate::power::PowerError),
    /// Mission model.
    #[error(transparent)]
    Perf(#[from] crate::perf::PerfError),
    /// Score engine.
    #[error(transparent)]
    Score(#[from] crate::score::ScoreError),
}
