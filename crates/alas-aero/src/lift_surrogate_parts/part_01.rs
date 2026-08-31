// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::collections::BTreeMap;

use alas_math::{BicubicSpline, BicubicSplineError};

use crate::vorlax::{self, VlmError, VlmGeometry, VlmSettings};

/// The angles of attack `Vortex_Lattice.__defaults__` trains at, in degrees.
///
/// The last two are far past any attached-flow regime a panel method can
/// speak about. They are upstream's, and they are load-bearing: they are what
/// stop the spline's outermost interval from being extrapolated into by a
/// mission that reaches a high angle of attack on rotation.
const TRAINING_ANGLES_DEG: [f64; 10] = [-5.0, -2.0, 0.0, 2.0, 5.0, 8.0, 10.0, 12.0, 45.0, 75.0];

/// The Mach numbers `Fidelity_Zero.__defaults__` overrides
/// `Vortex_Lattice`'s sixteen with. Subsonic throughout, which is what makes
/// the supersonic and transonic branches unreachable.
const TRAINING_MACH: [f64; 8] = [0.0, 0.1, 0.2, 0.3, 0.5, 0.75, 0.85, 0.9];

/// `Fidelity_Zero.__defaults__`'s allowance for the lift a fuselage carries
/// that a wings-only panel method cannot see.
///
/// Source: adg.stanford.edu, the Stanford AA241 course notes, by way of
/// upstream's own citation.
pub const FUSELAGE_LIFT_CORRECTION: f64 = 1.14;

/// The grid the surrogate is fitted through.
#[derive(Debug, Clone, PartialEq)]
pub struct TrainingGrid {
    /// Angles of attack, in radians and increasing.
    pub angle_of_attack_rad: Vec<f64>,
    /// Mach numbers, increasing and all below one.
    pub mach: Vec<f64>,
}

impl Default for TrainingGrid {
    fn default() -> Self {
        Self {
            angle_of_attack_rad: TRAINING_ANGLES_DEG
                .iter()
                .map(|degrees| degrees.to_radians())
                .collect(),
            mach: TRAINING_MACH.to_vec(),
        }
    }
}

/// What the surrogate answers at one flight condition.
#[derive(Debug, Clone, PartialEq)]
pub struct LiftSolution {
    /// The whole aircraft's inviscid lift coefficient, before the fuselage
    /// allowance.
    pub inviscid_lift_coefficient: f64,
    /// The whole aircraft's inviscid induced drag coefficient.
    pub inviscid_induced_drag_coefficient: f64,
    /// Each wing's lift coefficient, on that wing's own reference area, in
    /// the order the geometry holds its wings.
    pub wing_lift_coefficient: Vec<f64>,
    /// Each wing's inviscid induced drag coefficient, likewise.
    pub wing_induced_drag_coefficient: Vec<f64>,
    /// Whether either query coordinate was clamped to the training boundary.
    ///
    /// The legacy [`LiftSurrogate::evaluate`] operation still returns the
    /// reference-compatible edge value, but it no longer leaves the caller
    /// guessing whether that happened. Product callers that require an
    /// in-domain model can use [`LiftSurrogate::evaluate_checked`].
    pub domain: SurrogateDomainStatus,
}

/// Domain status attached to every surrogate evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurrogateDomainStatus {
    /// Whether angle of attack was below the first or above the last knot.
    pub alpha_clamped: bool,
    /// Whether Mach was below the first or above the last knot.
    pub mach_clamped: bool,
    /// Distance below/above the alpha training interval, in radians; zero
    /// means the query is inside the interval.
    pub alpha_distance_rad: f64,
    /// Distance below/above the Mach training interval; zero means the query
    /// is inside the interval.
    pub mach_distance: f64,
}

impl SurrogateDomainStatus {
    /// True when both coordinates lie inside the trained rectangle.
    pub const fn in_domain(self) -> bool {
        !self.alpha_clamped && !self.mach_clamped
    }
}

impl Default for SurrogateDomainStatus {
    fn default() -> Self {
        Self {
            alpha_clamped: false,
            mach_clamped: false,
            alpha_distance_rad: 0.0,
            mach_distance: 0.0,
        }
    }
}

/// Why a checked surrogate evaluation was rejected.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum SurrogateDomainError {
    /// A query coordinate was not finite.
    #[error("surrogate query must be finite (alpha={alpha_rad:?}, Mach={mach:?})")]
    NonFinite {
        /// Requested angle of attack in radians.
        alpha_rad: f64,
        /// Requested Mach number.
        mach: f64,
    },
    /// A finite query lies outside the trained rectangle.
    #[error("surrogate query is outside the trained domain: alpha={alpha_rad:.6} rad, Mach={mach:.6}; domain alpha=[{alpha_min:.6}, {alpha_max:.6}], Mach=[{mach_min:.6}, {mach_max:.6}]")]
    OutOfDomain {
        /// Requested angle of attack in radians.
        alpha_rad: f64,
        /// Requested Mach number.
        mach: f64,
        /// Lowest trained angle.
        alpha_min: f64,
        /// Highest trained angle.
        alpha_max: f64,
        /// Lowest trained Mach.
        mach_min: f64,
        /// Highest trained Mach.
        mach_max: f64,
    },
}

/// Why a surrogate could not be trained.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SurrogateError {
    /// The vortex lattice could not solve the training grid.
    #[error("the training grid could not be solved: {0}")]
    Solve(#[from] VlmError),
    /// A training table could not be fitted.
    #[error("the {quantity} surface could not be fitted: {source}")]
    Fit {
        /// Which table failed.
        quantity: String,
        /// What the fit reported.
        #[source]
        source: BicubicSplineError,
    },
    /// The grid reaches Mach 1 or beyond, where the supersonic and transonic
    /// branches this row does not translate would be selected.
    #[error("the training grid reaches Mach {mach}; only the subsonic branch is translated")]
    Supersonic {
        /// The offending Mach number.
        mach: f64,
    },
    /// [`LiftSurrogate::from_training`] was given a wing tag its tables do not
    /// carry. Unreachable from [`LiftSurrogate::train`], which builds both
    /// from the same geometry.
    #[error("no {quantity} table for wing {tag}")]
    MissingWingTable {
        /// The wing whose table was absent.
        tag: String,
        /// Which of the two tables was missing.
        quantity: String,
    },
}

/// The trained surrogate: two aircraft surfaces and two per wing.
#[derive(Debug, Clone)]
pub struct LiftSurrogate {
    grid: TrainingGrid,
    wing_tags: Vec<String>,
    training: TrainingTables,
    lift: BicubicSpline,
    drag: BicubicSpline,
    wing_lift: Vec<BicubicSpline>,
    wing_drag: Vec<BicubicSpline>,
}

/// The tables the surfaces were fitted through, kept so a caller can compare
/// the fit against the samples it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct TrainingTables {
    /// Aircraft lift coefficient, `[angle of attack][Mach]`.
    pub lift_coefficient: Vec<Vec<f64>>,
    /// Aircraft inviscid induced drag coefficient, likewise.
    pub drag_coefficient: Vec<Vec<f64>>,
    /// Per-wing lift coefficient, by wing tag.
    pub wing_lift_coefficient: BTreeMap<String, Vec<Vec<f64>>>,
    /// Per-wing inviscid induced drag coefficient, by wing tag.
    pub wing_drag_coefficient: BTreeMap<String, Vec<Vec<f64>>>,
}
