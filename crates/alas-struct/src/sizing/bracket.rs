// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Root bracketing on a monotone feasibility predicate, for the station
//! gauge searches of the product strength law.
//!
//! Each step takes the secant through the last two trials (Dekker; the first
//! interpolates between the bracket ends), the midpoint where that secant is
//! undefined or leaves the bracket, and bisects whenever three steps have not
//! halved the bracket (Brent's safeguard) or while the residual is flat (an
//! end moved without its residual changing). Once
//! the estimate lies within half a tolerance of the latest trial, the next
//! trial is placed nine tenths of a tolerance across it, toward the other
//! end, so a converged estimate closes the bracket in one step instead of
//! creeping the far end in. Feasibility is decided by the caller's predicate
//! alone; the residual only orders the trials, so the bracket keeps the
//! meaning (and the feasible end) a plain bisection would give it.

/// One trial: the abscissa and the residual there, negative where the
/// predicate fails and positive where it passes, about linear near the root.
#[derive(Clone, Copy, Debug)]
pub(super) struct End {
    pub(super) x: f64,
    pub(super) residual: f64,
}

/// Steps after which a bracket that has not halved is bisected.
const HALVING_WINDOW: usize = 3;

/// Narrow `[lower, upper]`, `lower` failing the predicate and `upper` passing
/// it, until `done(lower, upper)` or `max_steps` trials; return the final
/// ends. `evaluate(x)` returns whether `x` passes and its residual;
/// `tolerance(upper)` is the bracket width `done` accepts at that upper end.
pub(super) fn narrow(
    mut lower: End,
    mut upper: End,
    tolerance: impl Fn(f64) -> f64,
    mut done: impl FnMut(&End, &End) -> bool,
    mut evaluate: impl FnMut(f64) -> (bool, f64),
    max_steps: usize,
) -> (End, End) {
    let mut previous: Option<End> = None;
    let mut latest: Option<End> = None;
    let mut widths = [f64::INFINITY; HALVING_WINDOW];
    // An end whose residual did not change when the predicate moved it: the
    // residual is flat there and says nothing about the root until a trial on
    // that side changes it.
    let (mut lower_flat, mut upper_flat) = (false, false);
    for step in 0..max_steps {
        if done(&lower, &upper) {
            break;
        }
        let width = upper.x - lower.x;
        let midpoint = 0.5 * (lower.x + upper.x);
        let inside = |x: f64| x > lower.x && x < upper.x;
        let secant = |a: &End, b: &End| {
            let slope = b.residual - a.residual;
            if a.residual.is_finite() && b.residual.is_finite() && slope != 0.0 {
                b.x - b.residual * (b.x - a.x) / slope
            } else {
                f64::NAN
            }
        };
        let stalled = widths[step % HALVING_WINDOW] < 2.0 * width;
        widths[step % HALVING_WINDOW] = width;
        // The first trials interpolate between the bracket ends; later ones
        // take the secant through the last two trials, and the midpoint when
        // it is undefined (a flat residual, as below a minimum-gauge floor
        // where the installed section does not change) or leaves the bracket.
        let estimate = match (&previous, &latest) {
            (Some(a), Some(b)) => secant(a, b),
            _ if lower.residual < 0.0 && upper.residual > 0.0 => secant(&lower, &upper),
            _ => f64::NAN,
        };
        let flat = lower_flat || upper_flat;
        let mut point = if !stalled && !flat && inside(estimate) {
            estimate
        } else {
            midpoint
        };
        if let Some(last) = &latest {
            let tolerance = tolerance(upper.x);
            // A passing trial at a zero residual is the root to within the
            // residual's own resolution: close the bracket below it.
            let on_root = last.x == upper.x && last.residual == 0.0 && !upper_flat;
            if on_root || (!stalled && !flat && (point - last.x).abs() < 0.5 * tolerance) {
                point = if last.x == upper.x {
                    upper.x - 0.9 * tolerance
                } else {
                    lower.x + 0.9 * tolerance
                };
            }
        }
        if !inside(point) {
            point = midpoint;
        }
        if !inside(point) {
            break;
        }
        let (passes, residual) = evaluate(point);
        let trial = End { x: point, residual };
        if passes {
            upper_flat = residual == upper.residual;
            upper = trial;
        } else {
            lower_flat = residual == lower.residual;
            lower = trial;
        }
        previous = latest;
        latest = Some(trial);
    }
    (lower, upper)
}

#[cfg(test)]
mod tests {
    use super::{narrow, End};

    fn solve(root: f64, curve: impl Fn(f64) -> f64) -> (f64, f64, usize) {
        let tolerance = 1.0e-9;
        let mut calls = 0;
        let (lower, upper) = narrow(
            End {
                x: 0.0,
                residual: curve(0.0) - curve(root),
            },
            End {
                x: 1.0,
                residual: curve(1.0) - curve(root),
            },
            |upper| tolerance * upper,
            |lower, upper| upper.x - lower.x <= tolerance * upper.x,
            |x| {
                calls += 1;
                (x >= root, curve(x) - curve(root))
            },
            200,
        );
        (lower.x, upper.x, calls)
    }

    #[test]
    fn a_smooth_root_is_bracketed_to_the_tolerance_in_a_few_trials() {
        for root in [1.0e-6, 0.137, 0.5, 0.999] {
            let (lower, upper, calls) = solve(root, |x| (1.0 + 3.0 * x).ln());
            assert!(lower < root && root <= upper, "{root}: [{lower}, {upper}]");
            assert!(upper - lower <= 1.0e-9 * upper);
            assert!(calls <= 12, "{root}: {calls} trials");
        }
    }

    #[test]
    fn a_step_in_the_residual_still_closes_by_bisection() {
        // The residual says nothing about where the predicate flips.
        let (lower, upper, calls) = solve(0.3, |x| if x < 0.3 { -1.0 } else { 1.0 });
        assert!(lower < 0.3 && 0.3 <= upper);
        assert!(upper - lower <= 1.0e-9 * upper);
        assert!(calls <= 40, "{calls} trials");
    }
}
