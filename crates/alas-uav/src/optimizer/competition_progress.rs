// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Progress reporting and the best-draw tracker of the competition search.

use super::competition::{CompetitionEvaluation, Draw, Fitness, Outcome};

/// Progress of a running competition search, reported after each evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompetitionProgress {
    /// Objective evaluations completed.
    pub evaluated: usize,
    /// Total evaluation budget.
    pub budget: usize,
    /// Evaluations that were feasible so far.
    pub feasible: usize,
    /// Round score of the best feasible candidate so far, points.
    pub best_score: Option<f64>,
}

#[derive(Clone)]
pub(super) struct BestDraw {
    pub(super) draw: Draw,
    pub(super) fitness: Fitness,
    pub(super) evaluation: CompetitionEvaluation,
}

#[derive(Default)]
pub(super) struct Tracker<'a> {
    pub(super) evaluations: usize,
    pub(super) feasible: usize,
    pub(super) best: Option<BestDraw>,
    pub(super) budget: usize,
    pub(super) observer: Option<&'a mut dyn FnMut(CompetitionProgress) -> bool>,
    pub(super) cancelled: bool,
}

impl Tracker<'_> {
    pub(super) fn observe(&mut self, draw: Draw, outcome: Outcome) -> Fitness {
        self.evaluations += 1;
        if outcome.evaluation.feasible {
            self.feasible += 1;
        }
        let fitness = outcome.fitness;
        let improves = self
            .best
            .as_ref()
            .is_none_or(|best| fitness.better_than(best.fitness));
        if improves {
            self.best = Some(BestDraw {
                draw,
                fitness,
                evaluation: outcome.evaluation,
            });
        }
        if let Some(observer) = self.observer.as_mut() {
            let progress = CompetitionProgress {
                evaluated: self.evaluations,
                budget: self.budget,
                feasible: self.feasible,
                best_score: self
                    .best
                    .as_ref()
                    .filter(|best| best.evaluation.feasible)
                    .and_then(|best| best.evaluation.score),
            };
            if !observer(progress) {
                self.cancelled = true;
            }
        }
        fitness
    }
}
