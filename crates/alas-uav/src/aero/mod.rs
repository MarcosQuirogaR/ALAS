// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Low-Reynolds aerodynamics for fixed-wing UAVs (M2 lane A, UAV-PLAN 3.1).
//!
//! | module | content |
//! |---|---|
//! | [`viscous_vlm`] | VLM circulation with NeuralFoil strip coupling (modified-incidence fixed point), wing `CL`, `CD`, `CLmax`, stall location |
//! | [`section`] | 2-D section model: NeuralFoil or imported table, with the validity guard |
//! | [`polar_csv`] | Polar CSV / XFLR5-XFOIL text import (decision D7; no XFOIL/MSES integration) |
//! | [`drag_lowre`] | Parasite drag of fuselage, tails, booms, gear with transitional skin friction |
//! | [`ground_effect`] | Ground-effect correlations and lift-slope increment |
//! | [`slipstream`] | Actuator-disk propeller slipstream on the wing strips |
//! | [`validity`] | Re, alpha, Mach and network-confidence domain guard |
//!
//! # Conventions
//!
//! SI; lengths m, speeds m/s, forces N, angles radians (degrees only at the
//! NeuralFoil and CSV boundaries).  Frame: geometry axes of `alas-geom`
//! (x aft, y right, z up).  Section angles are relative to the local chord
//! line, positive nose-up; `cm` is about the quarter chord, nose-up positive.
//! Reynolds numbers are on the local chord (wing strips) or plate length
//! (components).
//!
//! # Status
//!
//! Implemented and numerically verified (closed forms, limits, Prandtl
//! lifting-line, image-system VLM).  The accuracy statements of plan 3.1 are
//! ESTIMATES until M6.  See each module's header for its validity domain and
//! what is not modelled.

pub mod air;
pub mod drag_lowre;
pub mod ground_effect;
pub mod polar_csv;
pub mod section;
pub mod slipstream;
pub mod validity;
pub mod viscous_vlm;

pub use air::AirState;
pub use validity::{Flag, Limits, Severity, Validity};

/// A failure of an aerodynamic model.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AeroError {
    /// A parameter is not finite or violates a documented bound.
    #[error("{0}")]
    InvalidInput(String),
    /// A polar text could not be parsed.
    #[error("polar import, line {line}: {message}")]
    Csv {
        /// 1-based line number.
        line: usize,
        /// What is wrong.
        message: String,
    },
    /// A section model failed (fit or network).
    #[error("section model: {0}")]
    Section(String),
    /// The lattice solve failed.
    #[error("vortex-lattice solve: {0}")]
    Lattice(String),
    /// A query left the validity domain and the configuration refuses it.
    #[error("outside the validity domain: {0}")]
    OutsideDomain(String),
}
