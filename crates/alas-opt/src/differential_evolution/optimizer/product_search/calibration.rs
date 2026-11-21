// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Calibration of the exploration model's constraint margins against the
//! refinement model.
//!
//! The exploration model is a coarser lattice and a shorter mission
//! integration. It ranks the objective like the refinement model (measured
//! Spearman 0.997 and above on recorded candidates), but a hard residual
//! that depends on the lattice (static margin, usable CG range) is offset
//! between the two: a candidate just inside a limit on one model is just
//! outside it on the other. Measured on the LJ60 without a correction: of 31
//! candidates the exploration promoted as feasible, 19 to 25 failed the
//! static-margin floor on the refinement model.
//!
//! So the exploration does not guess those rows. After its scan, the
//! baseline and the best [`CALIBRATION_POINTS`]` - 1` scanned candidates are
//! analysed with the refinement model as well, and for every hard residual
//! the offset of its signed margin between the two models is measured on
//! those pairs. The exploration then judges each candidate on its own
//! margins moved by the mean offset plus one standard deviation of it
//! (toward the violated side). This is the additive correction of a
//! low-fidelity model by a high-fidelity sample (N. M. Alexandrov, R. M.
//! Lewis, C. R. Gumbert, L. L. Green and P. A. Newman, "Approximation and
//! Model Management in Aerodynamic Optimization with Variable-Fidelity
//! Models," Journal of Aircraft 38(6), 2001, DOI 10.2514/2.2877; M. C.
//! Kennedy and A. O'Hagan, "Predicting the Output from a Complex Computer
//! Code When Fast Approximations Are Available," Biometrika 87(1), 2000, DOI
//! 10.1093/biomet/87.1.1, for the general form), with a constant per row
//! instead of a model of the difference: fifteen pairs do not support more.
//!
//! The correction changes which candidates the exploration prefers. It
//! never changes a verdict that is reported: every promoted candidate is
//! analysed by the refinement model, and every finalist by the reporting
//! analysis.

use crate::differential_evolution::SearchObjective;
use crate::search_methods::{ScoredPoint, Tier};

use super::super::batch::BatchEvaluator;

/// Signed normalized margins of one candidate's hard residuals.
pub(super) type Margins = Vec<(&'static str, f64)>;

/// Analyses `points` with the refinement model and returns their margins,
/// `None` where a candidate produced no residual table.
pub(super) type Calibrate<'a> = dyn FnMut(&[Vec<f64>]) -> Vec<Option<Margins>> + 'a;

/// Candidates analysed by both models, the baseline included (one wave of
/// the refinement model, `product_de::NATIVE_LANE_QUANTUM`).
pub(super) const CALIBRATION_POINTS: usize = 15;

/// Pairs a row needs before its offset is used.
const MINIMUM_PAIRS: usize = 4;

/// Corrected violation below which a candidate is feasible; the residual
/// table's own numerical slack (`mdo::types`, `NUMERICAL_SLACK`).
const SLACK: f64 = 1.0e-5;

/// The measured margin offsets, refinement minus exploration.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Correction {
    /// `(row, mean offset + one standard deviation)`, rows with an offset.
    shifts: Vec<(&'static str, f64)>,
    /// Pairs the offsets were measured on.
    pub(super) pairs: usize,
}

impl Correction {
    /// The correction measured on `(exploration, refinement)` margin pairs.
    pub(super) fn from_pairs(pairs: &[(Margins, Margins)]) -> Self {
        let mut shifts = Vec::new();
        let Some((first, _)) = pairs.first() else {
            return Self::default();
        };
        for &(id, _) in first {
            let offsets: Vec<f64> = pairs
                .iter()
                .filter_map(|(coarse, fine)| {
                    let value = |margins: &Margins| {
                        margins
                            .iter()
                            .find(|(other, _)| *other == id)
                            .map(|&(_, margin)| margin)
                            .filter(|margin| margin.is_finite())
                    };
                    Some(value(fine)? - value(coarse)?)
                })
                .collect();
            if offsets.len() < MINIMUM_PAIRS {
                continue;
            }
            let count = offsets.len() as f64;
            let mean = offsets.iter().sum::<f64>() / count;
            let deviation = (offsets
                .iter()
                .map(|offset| (offset - mean).powi(2))
                .sum::<f64>()
                / count)
                .sqrt();
            let shift = mean + deviation;
            if shift.abs() > SLACK {
                shifts.push((id, shift));
            }
        }
        Self {
            shifts,
            pairs: pairs.len(),
        }
    }

    /// `margins` as the refinement model is expected to see them.
    pub(super) fn corrected(&self, margins: &[(&'static str, f64)]) -> Margins {
        margins
            .iter()
            .map(|&(id, margin)| (id, margin + self.shift(id)))
            .collect()
    }

    fn shift(&self, id: &str) -> f64 {
        self.shifts
            .iter()
            .find(|(other, _)| *other == id)
            .map_or(0.0, |&(_, shift)| shift)
    }

    /// Re-judge an analysed exploration `point` on its corrected margins:
    /// the violation gains each shifted row's corrected excess less its own,
    /// and the tier follows. Points with no residual table are unchanged.
    pub(super) fn rescore(&self, point: &mut ScoredPoint, margins: Option<&[(&'static str, f64)]>) {
        let Some(margins) = margins.filter(|_| !self.shifts.is_empty()) else {
            return;
        };
        if !matches!(point.tier, Tier::Feasible | Tier::ClosedInfeasible) {
            return;
        }
        let change: f64 = margins
            .iter()
            .map(|&(id, margin)| {
                let shift = self.shift(id);
                (margin + shift).max(0.0) - margin.max(0.0)
            })
            .sum();
        let own = if point.tier == Tier::Feasible {
            0.0
        } else {
            point.constraint_violation
        };
        let violation = own + change;
        if violation > SLACK {
            point.tier = Tier::ClosedInfeasible;
            point.constraint_violation = violation;
        } else {
            point.tier = Tier::Feasible;
            point.constraint_violation = 0.0;
        }
    }

    /// One line for the run log: pairs, shifted rows and the largest shift.
    pub(super) fn describe(&self) -> String {
        let largest = self
            .shifts
            .iter()
            .max_by(|left, right| left.1.abs().total_cmp(&right.1.abs()));
        match largest {
            Some((id, shift)) => format!(
                "calibration | pairs {} | rows shifted {} | largest {id} {shift:+.4}",
                self.pairs,
                self.shifts.len()
            ),
            None => format!("calibration | pairs {} | rows shifted 0", self.pairs),
        }
    }
}

/// Calibration analyses whose required engine the later sized analyses
/// start from (`engine_start`): three at least, so one outlier cannot set
/// the start.
const ENGINE_START_SAMPLE: usize = 3;

/// The factor on the selected engine a search model's later sized analyses
/// start their engine sizing from: the median of the `factors` the first
/// calibration's analyses required, raised by half the match tolerance
/// (the point the sizing's own update aims at), `None` below
/// [`ENGINE_START_SAMPLE`].
///
/// A clean-sheet engine sizing closes the candidate at a factor, finds the
/// factor its thrust rows require and closes again, until the installed
/// rating lies within the match tolerance above the requirement
/// (`mdo::engine::sizing`). From the selected engine that is two closures;
/// the candidates a calibration has reached differ in their requirement by
/// less than that tolerance (A320-200 clean sheet: 20 of the 21
/// best designs required 113.6 to 114.5 kN; from this start 19 of the 20
/// redesigned closed in one closure, against none from the selected
/// engine). The median of fixed analyses is the same at any worker count, so
/// a seeded run reproduces it; the replays of the finalists start from it
/// too, and the delivery guard replays the delivered design from the
/// selected engine.
pub(super) fn engine_start(mut factors: Vec<f64>) -> Option<f64> {
    factors.retain(|factor| factor.is_finite() && *factor > 0.0);
    if factors.len() < ENGINE_START_SAMPLE {
        return None;
    }
    factors.sort_by(f64::total_cmp);
    let middle = factors.len() / 2;
    let median = if factors.len() % 2 == 1 {
        factors[middle]
    } else {
        0.5 * (factors[middle - 1] + factors[middle])
    };
    Some(median * (1.0 + 0.5 * crate::mdo::ENGINE_MATCH_TOLERANCE))
}

/// The first-update slope of the later sized analyses: the median of the
/// slopes the first calibration's analyses measured inside the contraction
/// regime, from [`ENGINE_START_SAMPLE`] of them at least. Fixed analyses
/// give the same median at any worker count.
pub(super) fn engine_slope(mut slopes: Vec<f64>) -> Option<f64> {
    slopes.retain(|slope| slope.is_finite() && slope.abs() < 0.9);
    if slopes.len() < ENGINE_START_SAMPLE {
        return None;
    }
    slopes.sort_by(f64::total_cmp);
    let middle = slopes.len() / 2;
    Some(if slopes.len() % 2 == 1 {
        slopes[middle]
    } else {
        0.5 * (slopes[middle - 1] + slopes[middle])
    })
}

/// Start the later sized analyses of `evaluator`'s model from the engine
/// the first calibration's analyses of `points` required
/// ([`engine_start`]) and the slope they measured ([`engine_slope`]), and
/// return both.
pub(super) fn start_engine<E: SearchObjective + ?Sized>(
    evaluator: &mut BatchEvaluator<'_, E>,
    points: &[Vec<f64>],
) -> (Option<f64>, Option<f64>) {
    let rows: Vec<usize> = points
        .iter()
        .filter_map(|values| evaluator.cache.row(values))
        .collect();
    let factors = rows
        .iter()
        .filter_map(|&row| evaluator.objective.engine_factor(row))
        .collect();
    let slopes = rows
        .iter()
        .filter_map(|&row| evaluator.objective.engine_slope(row))
        .collect();
    let start = engine_start(factors);
    let slope = start.and(engine_slope(slopes));
    evaluator.objective.set_engine_start(start);
    evaluator.objective.set_engine_slope(slope);
    (start, slope)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(tier: Tier, violation: f64) -> ScoredPoint {
        ScoredPoint {
            values: vec![0.0],
            cost: 1.0,
            tier,
            constraint_violation: violation,
            objectives: [1.0; 3],
        }
    }

    #[test]
    fn the_offset_is_the_mean_plus_one_deviation_and_moves_the_verdict() {
        // Row `sm` is 0.02 +- 0.01 more violated on the refinement model;
        // row `fuel` is identical on both and gets no shift.
        let pairs: Vec<(Margins, Margins)> = (0..8)
            .map(|i| {
                let offset = if i % 2 == 0 { 0.01 } else { 0.03 };
                (
                    vec![("sm", -0.05), ("fuel", -0.2)],
                    vec![("sm", -0.05 + offset), ("fuel", -0.2)],
                )
            })
            .collect();
        let correction = Correction::from_pairs(&pairs);
        assert_eq!(correction.pairs, 8);
        assert_eq!(correction.shifts.len(), 1);
        assert!((correction.shift("sm") - 0.03).abs() < 1e-12);
        assert_eq!(correction.shift("fuel"), 0.0);
        // A candidate 0.02 inside the limit on the exploration model is
        // 0.01 outside it once corrected.
        let mut inside = point(Tier::Feasible, 0.0);
        correction.rescore(&mut inside, Some(&[("sm", -0.02), ("fuel", -0.1)]));
        assert_eq!(inside.tier, Tier::ClosedInfeasible);
        assert!((inside.constraint_violation - 0.01).abs() < 1e-12);
        // One with room to spare stays feasible; a failed analysis is kept.
        let mut clear = point(Tier::Feasible, 0.0);
        correction.rescore(&mut clear, Some(&[("sm", -0.2)]));
        assert_eq!(clear, point(Tier::Feasible, 0.0));
        let mut failed = point(Tier::NotClosed, 5.0);
        correction.rescore(&mut failed, Some(&[("sm", 0.5)]));
        assert_eq!(failed, point(Tier::NotClosed, 5.0));
        // An already violated candidate gains the shift on that row only.
        let mut violated = point(Tier::ClosedInfeasible, 0.04);
        correction.rescore(&mut violated, Some(&[("sm", 0.04)]));
        assert!((violated.constraint_violation - 0.07).abs() < 1e-12);
        assert_eq!(
            correction.corrected(&[("sm", 0.0), ("fuel", 0.1)]),
            vec![("sm", 0.03), ("fuel", 0.1)]
        );
    }

    #[test]
    fn too_few_pairs_or_none_change_nothing() {
        let pairs: Vec<(Margins, Margins)> =
            vec![(vec![("sm", 0.0)], vec![("sm", 0.5)]); MINIMUM_PAIRS - 1];
        let correction = Correction::from_pairs(&pairs);
        assert!(correction.shifts.is_empty());
        let mut feasible = point(Tier::Feasible, 0.0);
        correction.rescore(&mut feasible, Some(&[("sm", -0.001)]));
        assert_eq!(feasible, point(Tier::Feasible, 0.0));
        assert_eq!(Correction::from_pairs(&[]), Correction::default());
        assert!(correction.describe().ends_with("rows shifted 0"));
    }

    #[test]
    fn the_engine_start_is_the_median_of_enough_required_factors() {
        let aim = 1.0 + 0.5 * crate::mdo::ENGINE_MATCH_TOLERANCE;
        assert_eq!(engine_start(vec![0.96, f64::NAN, 0.95]), None);
        assert_eq!(engine_start(vec![0.97, 0.95, 0.96]), Some(0.96 * aim));
        assert_eq!(engine_start(vec![1.0, 0.5, 0.75, 0.25]), Some(0.625 * aim));
        assert_eq!(engine_slope(vec![-0.1, 0.95, -0.2]), None);
        assert_eq!(engine_slope(vec![-0.1, -0.3, -0.2, f64::NAN]), Some(-0.2));
    }
}
