// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Normalized-force inverse of the two-engine ATR adapter.

use super::*;

/// Relative thrust tolerance of the normalized-force solve. It is a numerical
/// convergence criterion a few orders above rounding, not a physical constant.
pub(super) const SOLVER_RELATIVE_TOLERANCE: f64 = 1.0e-12;

/// Safety cap on Illinois iterations. Ending at it, or on a collapsed
/// bracket, without meeting `SOLVER_RELATIVE_TOLERANCE` on thrust is an error.
const MAXIMUM_SOLVER_ITERATIONS: usize = 100;

/// Bracket width below which the normalized-force solve switches from
/// bisection to the secant; 1/128 of the power-fraction range, well under the
/// separation of the roots on either side of the governor/fallback thrust drop.
const ROOT_SELECTION_WIDTH: f64 = 1.0 / 128.0;

impl Atr72TurbopropSystem {
    pub(super) fn solve_normalized_force_fraction(
        unit_model: Pw127m568fModel,
        condition: TurbopropCondition,
        rating: Pw127mRating,
        mode: TurbopropMode,
        requested_force_fraction: f64,
    ) -> Result<f64, PropulsionError> {
        let output_at = |power_fraction| {
            unit_model.evaluate(
                condition,
                TurbopropCommand {
                    rating,
                    power_fraction,
                    mode,
                    propeller_speed_rpm: unit_model.governed_propeller_speed_rpm,
                },
            )
        };
        let maximum = output_at(1.0).map_err(Self::map_error)?.total_thrust_n;
        let target = requested_force_fraction * maximum;
        // Lowest governed fraction on the 1/1000 grid. The governor absorbs
        // the propeller power only while the required CP lies inside the
        // blade-angle range. Shaft power rises monotonically with the
        // fraction, so the governed set is an interval that contains 1.0
        // (the maximum above evaluated) and a bisection over the grid index
        // finds its lower end in about ten evaluations.
        const GRID: u32 = 1_000;
        let (mut failing_step, mut governed_step) = (0_u32, GRID);
        let mut minimum = maximum;
        while governed_step - failing_step > 1 {
            let middle = (failing_step + governed_step) / 2;
            if let Ok(output) = output_at(f64::from(middle) / f64::from(GRID)) {
                governed_step = middle;
                minimum = output.total_thrust_n;
            } else {
                failing_step = middle;
            }
        }
        let low = f64::from(governed_step) / f64::from(GRID);
        if target < minimum {
            return Err(PropulsionError::OutsideModelDomain(
                "requested force is below the lowest governed point; no PW127M flight-idle schedule is available"
                    .to_owned(),
            ));
        }
        // Illinois-modified regula falsi (Dowell & Jarratt, BIT 11, 1971) on
        // thrust(f) - target, with the bracket [low, 1] kept at every step so
        // the solve cannot leave the governed interval.
        //
        // Thrust is not monotone in f: where the generic governor has no
        // solution the model falls back to an actuator-disk extension that
        // carries more thrust than the governed surface just above it, so
        // thrust drops (about 3.7 kN to 0.4 kN near f = 0.23 at sea level,
        // M 0.25) and a low target has a root on each side of the drop. The
        // original 60-step bisection selected one of them through its first
        // midpoints. Bisecting the same way until the bracket is narrower
        // than the separation of those roots keeps that selection, and the
        // secant then only refines a locally monotone crossing.
        let (mut a, mut residual_a) = (low, minimum - target);
        let (mut b, mut residual_b) = (1.0, maximum - target);
        if residual_a == 0.0 {
            return Ok(a);
        }
        if residual_b == 0.0 {
            return Ok(b);
        }
        let tolerance = SOLVER_RELATIVE_TOLERANCE * target.abs().max(1.0e-6 * maximum.abs());
        let mut side = 0_i8;
        let mut fraction = 0.5 * (a + b);
        let mut residual = f64::INFINITY;
        for _ in 0..MAXIMUM_SOLVER_ITERATIONS {
            // Plain bisection while the bracket is wide, Illinois once it is
            // narrower than `ROOT_SELECTION_WIDTH`.
            let bisecting = b - a > ROOT_SELECTION_WIDTH;
            let secant = (a * residual_b - b * residual_a) / (residual_b - residual_a);
            fraction = if !bisecting && secant > a && secant < b {
                secant
            } else {
                0.5 * (a + b)
            };
            residual = output_at(fraction).map_err(Self::map_error)?.total_thrust_n - target;
            if residual.abs() <= tolerance || b - a <= 4.0 * f64::EPSILON {
                break;
            }
            if residual < 0.0 {
                a = fraction;
                residual_a = residual;
                if side == -1 {
                    residual_b *= 0.5;
                }
                side = if bisecting { 0 } else { -1 };
            } else {
                b = fraction;
                residual_b = residual;
                if side == 1 {
                    residual_a *= 0.5;
                }
                side = if bisecting { 0 } else { 1 };
            }
        }
        // A collapsed bracket or the iteration cap ends the loop without force
        // convergence where thrust jumps across the target (the map is
        // discontinuous between the governed surface and its fallback) or no
        // fraction reaches it; no power fraction then delivers the request.
        if residual.is_nan() || residual.abs() > tolerance {
            return Err(PropulsionError::OutsideModelDomain(format!(
                "normalized-force solve did not converge: thrust residual {residual} N exceeds \
                 {tolerance} N at power fraction {fraction}"
            )));
        }
        Ok(fraction)
    }
}
