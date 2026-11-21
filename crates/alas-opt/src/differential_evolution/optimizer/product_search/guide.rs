// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Surrogate pre-selection of the exploration kernel's trials.
//!
//! The exploration analyses several hundred candidates on the cheap model.
//! Every analysed candidate leaves its mission objective and the signed
//! margin of each hard residual; a radial-basis surrogate of those
//! (`search::surrogate`) is refitted once per generation, and for every
//! target the kernel's alternative trials are ordered by the surrogate:
//! predicted violation first (Deb's rule on the predicted margins), then
//! predicted objective. The best-ranked alternative is the one analysed.
//!
//! Measured on this problem (A320-200, 120 recorded candidates of a finished
//! search re-scored with the in-loop model, 72 to fit and 48 to predict, 11
//! free variables): Spearman 1.000 between predicted and analysed objective,
//! and 45 of 48 feasibility verdicts predicted from the per-row margins
//! (the summed violation alone predicted 70-80 %).
//!
//! The guide never scores a candidate. A wrong prediction costs one
//! analysis of a poorer trial; it cannot make an infeasible design feasible
//! or change a reported number.

use std::collections::BTreeSet;

use crate::search::surrogate::Surrogate;

/// Alternative trials the kernel builds per target for the guide to order
/// (engineering choice: enough for the ordering to matter, few enough that
/// the chosen trial is still a typical draw of the operator).
pub(super) const ALTERNATIVES: usize = 8;

/// A hard residual is modelled once some analysed candidate came within this
/// normalized margin of its limit; rows that are never near their limit
/// need no model.
const NEAR_LIMIT: f64 = 0.05;

/// Analysed candidates the surrogate is fitted to at most: the most recent
/// ones, which are the ones nearest the current population. Bounds the
/// dense solve (order `n^3`) to a few tens of milliseconds per generation.
const MAX_NODES: usize = 600;

/// One analysed candidate: unit coordinates, objective, hard margins.
struct Observation {
    point: Vec<f64>,
    objective: f64,
    margins: Vec<(&'static str, f64)>,
}

struct Model {
    surrogate: Surrogate,
    /// Number of modelled hard rows; quantity 0 is the objective.
    rows: usize,
}

/// The surrogate guide of one exploration stage.
pub(super) struct SurrogateGuide {
    /// `(index, lower, width)` of every free coordinate.
    free: Vec<(usize, f64, f64)>,
    observations: Vec<Observation>,
    seen: BTreeSet<Vec<u64>>,
    model: Option<Model>,
    fitted_on: usize,
}

impl SurrogateGuide {
    pub(super) fn new(bounds: &[(f64, f64)]) -> Self {
        Self {
            free: bounds
                .iter()
                .enumerate()
                .filter(|(_, (lower, upper))| upper > lower)
                .map(|(index, &(lower, upper))| (index, lower, upper - lower))
                .collect(),
            observations: Vec::new(),
            seen: BTreeSet::new(),
            model: None,
            fitted_on: 0,
        }
    }

    fn unit(&self, values: &[f64]) -> Vec<f64> {
        self.free
            .iter()
            .map(|&(index, lower, width)| (values[index] - lower) / width)
            .collect()
    }

    /// Record one analysed candidate. Repeats and non-finite values are
    /// ignored.
    pub(super) fn observe(
        &mut self,
        values: &[f64],
        objective: f64,
        margins: &[(&'static str, f64)],
    ) {
        if !objective.is_finite() || values.len() < self.free.len() {
            return;
        }
        let key: Vec<u64> = values.iter().map(|value| value.to_bits()).collect();
        if !self.seen.insert(key) {
            return;
        }
        self.observations.push(Observation {
            point: self.unit(values),
            objective,
            margins: margins.to_vec(),
        });
    }

    fn refit(&mut self) {
        self.fitted_on = self.observations.len();
        let start = self.observations.len().saturating_sub(MAX_NODES);
        let data = &self.observations[start..];
        let Some(first) = data.first() else {
            self.model = None;
            return;
        };
        // Rows every candidate carries with a finite margin and that came
        // near their limit, in the first candidate's order.
        let mut outputs: Vec<Vec<f64>> = vec![data.iter().map(|o| o.objective).collect()];
        for &(id, _) in &first.margins {
            let column: Option<Vec<f64>> = data
                .iter()
                .map(|observation| {
                    observation
                        .margins
                        .iter()
                        .find(|(other, _)| *other == id)
                        .map(|&(_, margin)| margin)
                        .filter(|margin| margin.is_finite())
                })
                .collect();
            let Some(column) = column else {
                continue;
            };
            let (low, high) = column
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), &value| {
                    (low.min(value), high.max(value))
                });
            if high > -NEAR_LIMIT && high > low {
                outputs.push(column);
            }
        }
        let points: Vec<Vec<f64>> = data.iter().map(|o| o.point.clone()).collect();
        self.model = Surrogate::fit(&points, &outputs).map(|surrogate| Model {
            surrogate,
            rows: outputs.len() - 1,
        });
    }

    /// The index of the alternative to analyse: the least predicted
    /// violation, then the least predicted objective; the first alternative
    /// when no model can be fitted yet.
    pub(super) fn choose(&mut self, alternatives: &[Vec<f64>]) -> usize {
        if self.fitted_on != self.observations.len() {
            self.refit();
        }
        let Some(model) = &self.model else {
            return 0;
        };
        let mut best = (0usize, f64::INFINITY, f64::INFINITY);
        for (index, values) in alternatives.iter().enumerate() {
            let predicted = model.surrogate.predict(&self.unit(values));
            let violation: f64 = predicted[1..=model.rows]
                .iter()
                .map(|margin| margin.max(0.0))
                .sum();
            let objective = predicted[0];
            if !(violation.is_finite() && objective.is_finite()) {
                continue;
            }
            if (violation, objective) < (best.1, best.2) {
                best = (index, violation, objective);
            }
        }
        best.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guide_orders_alternatives_by_predicted_violation_then_objective() {
        // Objective falls with x; the one hard row is violated above x = 0.6.
        let bounds = [(0.0, 10.0), (5.0, 5.0), (0.0, 1.0)];
        let mut guide = SurrogateGuide::new(&bounds);
        assert_eq!(guide.choose(&[vec![9.0, 5.0, 0.5], vec![1.0, 5.0, 0.5]]), 0);
        for i in 0..40 {
            let x = 10.0 * f64::from(i) / 39.0;
            let y = f64::from(i * 7 % 13) / 13.0;
            guide.observe(
                &[x, 5.0, y],
                100.0 - x + 0.1 * y,
                &[("row", x / 10.0 - 0.6), ("far", -0.5)],
            );
            // A repeat is one observation.
            guide.observe(&[x, 5.0, y], 0.0, &[]);
        }
        assert_eq!(guide.observations.len(), 40);
        let alternatives = [
            vec![9.0, 5.0, 0.5], // best objective, violated
            vec![5.5, 5.0, 0.5], // feasible, good objective
            vec![2.0, 5.0, 0.5], // feasible, poor objective
        ];
        assert_eq!(guide.choose(&alternatives), 1);
        assert_eq!(guide.model.as_ref().map(|model| model.rows), Some(1));
        // With every alternative violated the least violated leads.
        assert_eq!(guide.choose(&[vec![9.5, 5.0, 0.2], vec![7.0, 5.0, 0.9]]), 1);
    }
}
