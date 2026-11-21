// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! How far a search's best design was still moving when its candidate plan
//! ended.
//!
//! A staged search stops on a plan of candidates, so "converged" is not a
//! yes-or-no stop reason any more. What a reader needs is whether the
//! history had settled: the gain of the best feasible design over the last
//! generations, and how wide the final population still was. The indicator
//! reports both for the exploration (many generations on the cheap model,
//! where a trend is visible) and for the refinement.

use serde::{Deserialize, Serialize};

/// Generations the trailing gain is measured over (engineering choice: long
/// enough that one lucky trial does not hide a stall, short against the
/// twenty to forty generations an exploration runs).
pub const INDICATOR_WINDOW_GENERATIONS: usize = 5;

/// Trailing relative gain below which the history is reported as settled:
/// 0.1 %, what a conceptual block-fuel comparison can resolve
/// (`search_methods::lshade_de`, `IMPROVEMENT_TOLERANCE`, is the per-step
/// counterpart).
pub const SETTLED_WINDOW_GAIN: f64 = 1.0e-3;

/// The convergence indicator of one search.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConvergenceIndicator {
    /// Generations the trailing gains below are measured over.
    pub window_generations: usize,
    /// Relative gain of the exploration's best feasible cost over its last
    /// `window_generations` generations (dimensionless, non-negative);
    /// `None` when the exploration ran fewer generations or found nothing
    /// feasible before the window.
    pub exploration_window_gain: Option<f64>,
    /// Relative gain of the exploration's best feasible cost over the whole
    /// stage, from its first feasible design.
    pub exploration_gain: Option<f64>,
    /// Relative gain of the refinement's best feasible cost from its
    /// promoted initial population to its end.
    pub refinement_gain: Option<f64>,
    /// Normalized design-space spread of the final refinement population:
    /// `0` collapsed, `1` the whole search box.
    pub population_spread: f64,
    /// Whether the history had settled: the trailing exploration gain and
    /// the refinement gain are both below [`SETTLED_WINDOW_GAIN`]. Reported,
    /// never a condition of delivery.
    pub settled: bool,
    /// Best feasible cost after the exploration's initial population and
    /// after each of its generations (screening model).
    pub exploration_best: Vec<Option<f64>>,
    /// The same for the refinement (refinement model).
    pub refinement_best: Vec<Option<f64>>,
}

/// `(first - last) / |first|` between two feasible costs.
fn gain(first: Option<f64>, last: Option<f64>) -> Option<f64> {
    let (first, last) = (first?, last?);
    (first.is_finite() && last.is_finite()).then(|| (first - last) / first.abs().max(1.0e-12))
}

impl ConvergenceIndicator {
    /// The indicator of the two best-feasible histories and the final
    /// refinement spread.
    #[must_use]
    pub fn of(exploration: &[Option<f64>], refinement: &[Option<f64>], spread: f64) -> Self {
        let window = INDICATOR_WINDOW_GENERATIONS;
        let last = |history: &[Option<f64>]| history.last().copied().flatten();
        let first_feasible = |history: &[Option<f64>]| history.iter().copied().flatten().next();
        let exploration_window_gain = (exploration.len() > window)
            .then(|| {
                gain(
                    exploration[exploration.len() - 1 - window],
                    last(exploration),
                )
            })
            .flatten();
        let exploration_gain = gain(first_feasible(exploration), last(exploration));
        let refinement_gain = gain(first_feasible(refinement), last(refinement));
        let below = |value: Option<f64>| value.is_some_and(|value| value < SETTLED_WINDOW_GAIN);
        Self {
            window_generations: window,
            exploration_window_gain,
            exploration_gain,
            refinement_gain,
            population_spread: spread,
            // Without an exploration the refinement's own trailing gain is
            // the only evidence.
            settled: if exploration.is_empty() {
                refinement.len() > window
                    && below(gain(
                        refinement[refinement.len() - 1 - window],
                        last(refinement),
                    ))
            } else {
                below(exploration_window_gain) && below(refinement_gain)
            },
            exploration_best: exploration.to_vec(),
            refinement_best: refinement.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flat_tail_is_settled_and_a_moving_one_is_not() {
        let mut exploration: Vec<Option<f64>> = vec![None, Some(100.0), Some(98.0), Some(97.0)];
        exploration.extend(std::iter::repeat_n(Some(96.99), 6));
        let refinement = vec![Some(97.2), Some(97.19), Some(97.19)];
        let indicator = ConvergenceIndicator::of(&exploration, &refinement, 0.08);
        assert!(indicator.settled);
        assert_eq!(indicator.exploration_window_gain, Some(0.0));
        assert!((indicator.exploration_gain.unwrap() - 0.0301).abs() < 1e-12);
        assert!(indicator.refinement_gain.unwrap() < SETTLED_WINDOW_GAIN);
        assert_eq!(indicator.population_spread, 0.08);
        // Still gaining 1 % in the window: not settled.
        let moving: Vec<Option<f64>> = (0..12).map(|i| Some(100.0 - 0.2 * f64::from(i))).collect();
        let indicator = ConvergenceIndicator::of(&moving, &refinement, 0.3);
        assert!(!indicator.settled);
        assert!(indicator.exploration_window_gain.unwrap() > SETTLED_WINDOW_GAIN);
        // Nothing feasible: no gains, not settled.
        let indicator = ConvergenceIndicator::of(&[None; 8], &[None; 3], 1.0);
        assert_eq!(indicator.exploration_gain, None);
        assert!(!indicator.settled);
        // A scan-only screening: the refinement's own tail decides.
        let flat = vec![Some(5.0); 9];
        assert!(ConvergenceIndicator::of(&[], &flat, 0.0).settled);
        assert!(!ConvergenceIndicator::of(&[], &refinement, 0.0).settled);
    }
}
