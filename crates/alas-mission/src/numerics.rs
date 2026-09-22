// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Analyses/Mission/Segments/Conditions/Numerics.py and
// mission analysis model/Methods/Missions/Segments/Common/Numerics.py.
// Upstream: mission analysis model 2.5.2, LGPL-2.1 (relicensed under GPL-2.0-or-later per
// LGPL-2.1 section 3; compatible with this program's AGPL-3.0-or-later).
// Reference: alas @ rust-port-baseline.

//! The pseudospectral scaffolding a mission segment is solved on.
//!
//! A mission segment is a boundary-value problem in time. mission analysis model turns it into
//! an algebraic system by sampling the unknown at a fixed set of Chebyshev
//! nodes and replacing `d/dt` and the running integral with dense matrices
//! that act on those samples. [`Numerics`] is the container mission analysis model holds that
//! scaffolding in, and the two methods here are the two steps that fill it:
//! [`Numerics::initialize_differentials_dimensionless`] builds the operators on
//! the dimensionless `[0, 1]` node grid once, and
//! [`Numerics::update_differentials_time`] rescales them onto the segment's
//! real time span every time the solver revises how long the segment takes.
//!
//! The operators themselves are [`alas_math::chebyshev_data`], which is checked
//! against the reference on its own; this module is the container's defaults
//! and the reshaping and time-scaling mission analysis model wraps around it.
//!
//! Scope. Upstream stores the discretization as a swappable function and a
//! single noise segment substitutes a linear operator for the Chebyshev one;
//! nothing in this program's transport mission does, so the Chebyshev kernel is
//! called directly rather than through a function slot. The `converged` flag,
//! `step_size`, `solver_jacobian` and `max_evaluations` are the solver's own
//! bookkeeping, carried as fields at their defaults so the container round-trips
//! against the reference, and driven by the segment solver that lands later.

use alas_math::{chebyshev_data, ChebyshevData, ChebyshevError};

/// One set of pseudospectral operators over a node grid.
///
/// `control_points` is the grid, `differentiate` approximates `d/dx` on it and
/// `integrate` the running integral; both act on values sampled at the nodes.
/// All three are empty until [`Numerics::initialize_differentials_dimensionless`]
/// (for the dimensionless grid) or [`Numerics::update_differentials_time`] (for
/// the time grid) fills them, matching the empty arrays mission analysis model's `__defaults__`
/// leaves them at.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Differentials {
    /// The node grid: the sampled points the operators act between.
    pub control_points: Vec<f64>,
    /// The `N x N` differentiation operator.
    pub differentiate: Vec<Vec<f64>>,
    /// The `N x N` integration operator.
    pub integrate: Vec<Vec<f64>>,
}

/// mission analysis model's `Numerics` conditions container for one mission segment.
///
/// The defaults reproduce `Numerics.__defaults__`: sixteen control points, a
/// solution tolerance of `1e-8`, no analytic Jacobian, and empty operator grids
/// until they are initialized.
#[derive(Debug, Clone, PartialEq)]
pub struct Numerics {
    /// Number of Chebyshev control points the segment is discretized on.
    pub number_control_points: i64,
    /// How the solver forms its Jacobian. `"none"` upstream by default.
    pub solver_jacobian: String,
    /// Convergence tolerance on the residual.
    pub tolerance_solution: f64,
    /// Evaluation budget, `0` for unlimited, as upstream leaves it.
    pub max_evaluations: f64,
    /// Whether the segment solve converged; `None` until it has run.
    pub converged: Option<bool>,
    /// Solver step size; `None` until the solver sets one.
    pub step_size: Option<f64>,
    /// The operators on the dimensionless `[0, 1]` node grid.
    pub dimensionless: Differentials,
    /// The operators rescaled onto the segment's real time span.
    pub time: Differentials,
}

impl Default for Numerics {
    fn default() -> Self {
        Self {
            number_control_points: 16,
            solver_jacobian: "none".to_owned(),
            tolerance_solution: 1e-8,
            max_evaluations: 0.0,
            converged: None,
            step_size: None,
            dimensionless: Differentials::default(),
            time: Differentials::default(),
        }
    }
}

impl Numerics {
    /// The container's tag, constant across every segment upstream.
    pub const TAG: &'static str = "numerics";

    /// Build the dimensionless operators on the `[0, 1]` Chebyshev grid.
    ///
    /// Populates [`Self::dimensionless`] from
    /// [`alas_math::chebyshev_data`] at [`Self::number_control_points`].
    /// Upstream reshapes the node vector into a column here; the values are the
    /// same either way, so the grid is stored as the flat vector the operators
    /// are indexed against.
    ///
    /// # Errors
    ///
    /// [`ChebyshevError::NonPositiveN`] when the control-point count is not
    /// positive, and [`ChebyshevError::SingularIntegrationOperator`] if the
    /// integration operator could not be built: the same two failures the
    /// kernel reports.
    pub fn initialize_differentials_dimensionless(&mut self) -> Result<(), ChebyshevError> {
        let ChebyshevData {
            x,
            differentiation,
            integration,
        } = chebyshev_data(self.number_control_points, true)?;

        self.dimensionless.control_points = x;
        self.dimensionless.differentiate = differentiation;
        // `chebyshev_data` returns `Some` whenever integration is requested,
        // which it always is here; an empty operator rather than a panic is the
        // library-safe answer to a `None` that cannot occur on this call.
        self.dimensionless.integrate = integration.unwrap_or_default();
        Ok(())
    }

    /// Rescale the dimensionless operators onto a segment's real time span.
    ///
    /// With `T = time[last] - time[first]`, the nodes scale by `T`, the
    /// differentiation operator by `1/T` and the integration operator by `T`,
    /// exactly as `update_differentials_time` does. `time` is the segment's
    /// inertial-time samples; only its two endpoints are read, so it is taken
    /// as the slice upstream reads rather than the whole segment state it hangs
    /// off. An empty `time` leaves the time operators empty rather than
    /// indexing past the end.
    pub fn update_differentials_time(&mut self, time: &[f64]) {
        let Some((&last, &first)) = time.last().zip(time.first()) else {
            return;
        };
        let span = last - first;

        self.time.control_points = self
            .dimensionless
            .control_points
            .iter()
            .map(|&x| x * span)
            .collect();
        self.time.differentiate = scaled(&self.dimensionless.differentiate, 1.0 / span);
        self.time.integrate = scaled(&self.dimensionless.integrate, span);
    }
}

/// Every entry of `matrix` multiplied by `factor`.
fn scaled(matrix: &[Vec<f64>], factor: f64) -> Vec<Vec<f64>> {
    matrix
        .iter()
        .map(|row| row.iter().map(|&value| value * factor).collect())
        .collect()
}

// A test asserts on values it constructed here, so a failed expect is the
// assertion failing rather than a library invariant being broken.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_reference_container() {
        let numerics = Numerics::default();
        assert_eq!(numerics.number_control_points, 16);
        assert_eq!(numerics.solver_jacobian, "none");
        assert_eq!(numerics.tolerance_solution, 1e-8);
        assert_eq!(numerics.max_evaluations, 0.0);
        assert_eq!(numerics.converged, None);
        assert_eq!(numerics.step_size, None);
        assert!(numerics.dimensionless.control_points.is_empty());
        assert!(numerics.time.differentiate.is_empty());
        assert_eq!(Numerics::TAG, "numerics");
    }

    // What parity checks at N=16; this checks the shape holds at another N so a
    // future caller that changes the control-point count is covered.
    #[test]
    fn initialization_fills_square_operators_of_the_control_point_count() {
        let mut numerics = Numerics {
            number_control_points: 8,
            ..Numerics::default()
        };
        numerics
            .initialize_differentials_dimensionless()
            .expect("positive control-point count");
        assert_eq!(numerics.dimensionless.control_points.len(), 8);
        assert_eq!(numerics.dimensionless.differentiate.len(), 8);
        assert!(numerics
            .dimensionless
            .differentiate
            .iter()
            .all(|row| row.len() == 8));
        assert_eq!(numerics.dimensionless.integrate.len(), 8);
    }

    // The time rescale is the identity when the span is exactly one, so a unit
    // span is the cleanest way to state that the dimensionless grid is what the
    // time grid is a scaling of.
    #[test]
    fn a_unit_time_span_leaves_the_operators_unchanged() {
        let mut numerics = Numerics::default();
        numerics
            .initialize_differentials_dimensionless()
            .expect("positive control-point count");
        // time endpoints 0 and 1 -> span 1.
        numerics.update_differentials_time(&[0.0, 1.0]);
        assert_eq!(
            numerics.time.control_points,
            numerics.dimensionless.control_points
        );
        assert_eq!(
            numerics.time.differentiate,
            numerics.dimensionless.differentiate
        );
        assert_eq!(numerics.time.integrate, numerics.dimensionless.integrate);
    }

    #[test]
    fn a_span_scales_nodes_by_t_and_the_two_operators_inversely() {
        let mut numerics = Numerics::default();
        numerics
            .initialize_differentials_dimensionless()
            .expect("positive control-point count");
        let span = 600.0;
        numerics.update_differentials_time(&[10.0, 10.0 + span]);
        for (scaled_x, &x) in numerics
            .time
            .control_points
            .iter()
            .zip(&numerics.dimensionless.control_points)
        {
            assert_eq!(*scaled_x, x * span);
        }
        assert_eq!(
            numerics.time.differentiate[1][2],
            numerics.dimensionless.differentiate[1][2] / span
        );
        assert_eq!(
            numerics.time.integrate[3][4],
            numerics.dimensionless.integrate[3][4] * span
        );
    }

    #[test]
    fn nonpositive_control_points_is_an_error_not_a_panic() {
        let mut numerics = Numerics {
            number_control_points: 0,
            ..Numerics::default()
        };
        assert_eq!(
            numerics.initialize_differentials_dimensionless(),
            Err(ChebyshevError::NonPositiveN(0))
        );
    }

    #[test]
    fn an_empty_time_leaves_the_time_operators_empty() {
        let mut numerics = Numerics::default();
        numerics
            .initialize_differentials_dimensionless()
            .expect("positive control-point count");
        numerics.update_differentials_time(&[]);
        assert!(numerics.time.control_points.is_empty());
    }
}
