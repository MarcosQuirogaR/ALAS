// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Per-stage accounting of a product search and the same-model comparison of
//! its winner with the baseline design.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What the product search claims to be: a local refinement of the loaded
/// preset inside its anchored design box, not a global search.
pub const SEARCH_SCOPE: &str = "local refinement around the preset";

/// What one search stage did, measured rather than configured.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StageSummary {
    /// `screening` or `refinement`.
    pub stage: String,
    /// Configured evaluation budget: the ceiling.
    pub max_evaluations: usize,
    /// The budget `B` the stage planned its schedule on, reserve included:
    /// the measured-throughput plan of a time-limited refinement
    /// (`alas_opt::planned_refinement_budget`), else `max_evaluations`. A
    /// refinement replay sets `replay_planned_evaluations` to it. Zero on an
    /// older record, meaning `max_evaluations`.
    #[serde(default)]
    pub planned_evaluations: usize,
    /// Evaluations of `max_evaluations` kept back for the reporting-fidelity
    /// work after the search (`alas_opt::verification_reserve`); zero for
    /// the screening stage.
    #[serde(default)]
    pub reserved_evaluations: usize,
    /// Configured wall-clock limit, s.
    pub time_limit_s: f64,
    /// Whether the time limit applied. `false` when the run stops on
    /// evaluation budgets only or the stage replayed a recorded count.
    #[serde(default)]
    pub time_limited: bool,
    /// The replay count the budget bounds: pre-gate-passed candidates,
    /// repeats and the reused screening elite and baseline included, so not
    /// [`Self::analysis_evaluations`]. Replaying it as `replay_evaluations`
    /// reproduces the stage bit-identically at any worker count.
    pub evaluations: usize,
    /// The feasibility-restoration share of [`Self::evaluations`], replayed
    /// as `replay_restoration_evaluations` so each phase stops where it did;
    /// zero when restoration did not run.
    #[serde(default)]
    pub restoration_evaluations: usize,
    /// Candidates rejected by the design-vector pre-gate without analysis;
    /// they count against the stage's rejection cap, not its budget.
    pub pre_gate_rejects: usize,
    /// Coupled analyses actually run, the refinement's throughput pilot
    /// included; never a candidate a cancellation stopped before it started.
    pub analysis_evaluations: usize,
    /// Requested candidates a cancellation stopped before any analysis
    /// started: recorded in the history as `cancelled_unstarted`, counted
    /// neither as analyses nor against the budget.
    #[serde(default)]
    pub cancelled_unstarted: usize,
    /// Batches (screening) or generations (refinement) completed.
    pub generations: usize,
    /// Strictly feasible candidates among the stage's evaluations.
    pub feasible: usize,
    /// Screening: members handed to the refinement. Refinement: initial
    /// population.
    pub elite_size: usize,
    /// Measured wall time, s.
    pub wall_time_s: f64,
    /// Mean lane wall time per analysed candidate, s. Each candidate runs on
    /// one single-thread lane, so this is its CPU time when the machine is
    /// not contended (an upper bound otherwise).
    #[serde(default)]
    pub candidate_time_s: f64,
    /// Busy lane time over lanes times `wall_time_s`: the share of the
    /// workers the stage kept busy.
    #[serde(default)]
    pub lane_utilization: f64,
    /// `evaluation_budget`, `time_budget`, `pregate_exhausted`, `cancelled`,
    /// and for the refinement also `converged` or `stagnated`.
    pub termination: String,
    /// Sizing-closure work of the stage's analyses that sized, when any did.
    #[serde(default)]
    pub sizing_work: Option<SizingWorkSummary>,
}

/// Distribution of one work counter over a stage's sized candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkDistribution {
    /// Sum over the candidates.
    pub total: u64,
    /// Median (nearest rank).
    pub p50: u64,
    /// 90th percentile (nearest rank).
    pub p90: u64,
    /// Largest.
    pub max: u64,
}

impl WorkDistribution {
    fn of(mut values: Vec<u64>) -> Self {
        values.sort_unstable();
        // Nearest-rank percentile: the smallest value with at least `p` of
        // the sample at or below it.
        let rank = |p: f64| {
            let index = (p * values.len() as f64).ceil() as usize;
            values[index.clamp(1, values.len()) - 1]
        };
        Self {
            total: values.iter().sum(),
            p50: rank(0.5),
            p90: rank(0.9),
            max: values[values.len() - 1],
        }
    }
}

/// What the sizing closures of one stage's candidates spent
/// (`SizedCandidate::work`), and the per-candidate cap they ran under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SizingWorkSummary {
    /// Candidates that sized, over which the distributions are taken.
    pub candidates: usize,
    /// Complete mission-profile integrations per candidate.
    pub trip_flights: WorkDistribution,
    /// Propulsion-deck evaluations per candidate.
    pub deck_evals: WorkDistribution,
    /// The cap on trip flights per candidate, when one applied.
    pub cap_trip_flights: Option<u32>,
    /// The cap on deck evaluations per candidate, when one applied.
    pub cap_deck_evals: Option<u64>,
}

impl SizingWorkSummary {
    /// The summary of the history rows `(trip_flights, deck_evals)` of one
    /// stage; rows with no work (not sized) are left out. `None` when no row
    /// sized. The cap is the caller's to fill in.
    pub(crate) fn from_rows(trip_flights: &[u64], deck_evals: &[u64]) -> Option<Self> {
        let (flights, evals): (Vec<u64>, Vec<u64>) = trip_flights
            .iter()
            .zip(deck_evals)
            .filter(|(flights, evals)| **flights > 0 || **evals > 0)
            .map(|(flights, evals)| (*flights, *evals))
            .unzip();
        (!flights.is_empty()).then(|| Self {
            candidates: flights.len(),
            trip_flights: WorkDistribution::of(flights),
            deck_evals: WorkDistribution::of(evals),
            cap_trip_flights: None,
            cap_deck_evals: None,
        })
    }

    /// The summary of the rows from `start` of `history`.
    pub(crate) fn of_history(history: &crate::OptimizationHistory, start: usize) -> Option<Self> {
        Self::from_rows(
            history.trip_flights.get(start..).unwrap_or_default(),
            history.deck_evals.get(start..).unwrap_or_default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_work_summary_skips_unsized_rows_and_orders_its_percentiles() {
        let flights = [0, 12, 30, 0, 18, 15, 60, 14, 13, 16, 17, 20];
        let evals = [
            0, 1_200, 3_000, 0, 1_800, 1_500, 6_000, 1_400, 1_300, 1_600, 1_700, 2_000,
        ];
        let summary = SizingWorkSummary::from_rows(&flights, &evals).expect("rows sized");
        assert_eq!(summary.candidates, 10);
        assert_eq!(summary.trip_flights.total, flights.iter().sum::<u64>());
        assert_eq!(summary.trip_flights.max, 60);
        for work in [summary.trip_flights, summary.deck_evals] {
            assert!(work.p50 <= work.p90 && work.p90 <= work.max);
        }
        // Nearest rank on ten values: the fifth and the ninth smallest.
        assert_eq!(
            (summary.trip_flights.p50, summary.trip_flights.p90),
            (16, 30)
        );
        assert!(SizingWorkSummary::from_rows(&[0, 0], &[0, 0]).is_none());
    }
}

impl StageSummary {
    /// The budget the stage's schedule ran on, reserve included:
    /// [`Self::planned_evaluations`], or `max_evaluations` on an older
    /// record.
    #[must_use]
    pub fn schedule_budget(&self) -> usize {
        if self.planned_evaluations == 0 {
            self.max_evaluations
        } else {
            self.planned_evaluations
        }
    }
}

/// Pre-gate rejections of one stage by the first check each candidate
/// failed, in the order the gate runs them, so the counts sum to the
/// stage's `pre_gate_rejects`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreGateReasons {
    /// Outside the search box, not finite, or of the wrong dimension.
    pub design_box: usize,
    /// A main-wing planform the geometry builder cannot construct, such as
    /// a chord growing outboard.
    pub planform: usize,
    /// Exposed trailing edge, side-of-body (or centreline) to kink, running
    /// forward of the 90 degree limit.
    pub trailing_edge_angle: usize,
    /// Span above the aerodrome reference code limit.
    pub span_code: usize,
}

impl PreGateReasons {
    /// Every rejection counted.
    #[must_use]
    pub fn total(&self) -> usize {
        self.design_box + self.planform + self.trailing_edge_angle + self.span_code
    }
}

/// Why candidates of one stage failed: before analysis (the design-vector
/// pre-gate) and in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageRejections {
    /// `screening` or `refinement`, as in [`StageSummary::stage`].
    pub stage: String,
    /// The stage's pre-gate rejection cap; reaching it stops the stage as
    /// `pregate_exhausted`.
    pub max_pregate_rejects: usize,
    /// The pre-gate rejections by reason.
    pub pre_gate: PreGateReasons,
    /// Among the stage's analyses (history rows): how many violated each
    /// hard constraint, by residual id, or failed to close, by failure
    /// reason. A candidate violating several constraints counts in each.
    #[serde(default)]
    pub analysed_failures: BTreeMap<String, usize>,
}

impl StageRejections {
    /// The record of `stage`, its analysis failures read from the history
    /// rows `rows` (each a `+`-joined reason, empty when feasible).
    pub(crate) fn new(
        stage: &str,
        max_pregate_rejects: usize,
        pre_gate: PreGateReasons,
        rows: &[String],
    ) -> Self {
        let mut analysed_failures = BTreeMap::new();
        for reason in rows.iter().flat_map(|row| row.split('+')) {
            if !reason.is_empty() {
                *analysed_failures.entry(reason.to_owned()).or_insert(0) += 1;
            }
        }
        Self {
            stage: stage.to_owned(),
            max_pregate_rejects,
            pre_gate,
            analysed_failures,
        }
    }
}

/// The winner against the baseline design, both scored by the refinement's
/// own full in-loop model, so the difference is a same-model delta.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineComparison {
    /// Whether the baseline passed every hard constraint.
    pub baseline_feasible: bool,
    /// Baseline mission objective (the configured quantity, e.g. block fuel
    /// in kg).
    pub baseline_objective_value: f64,
    /// Winner mission objective, same quantity and units.
    pub winner_objective_value: f64,
    /// `(winner - baseline) / |baseline|`: negative is an improvement under
    /// the minimisation convention. `None` when either value is not finite.
    pub relative_change: Option<f64>,
    /// Sized block fuel of the baseline, kg, from its history row; `None`
    /// when the row recorded none.
    #[serde(default)]
    pub baseline_block_fuel_kg: Option<f64>,
    /// Sized block fuel of the winner, kg, from its history row.
    #[serde(default)]
    pub winner_block_fuel_kg: Option<f64>,
}

/// The unmodified baseline re-evaluated at reporting fidelity beside the
/// delivered design: the same-model delta a release gate reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportingBaseline {
    /// Whether the baseline itself passes the reporting-fidelity check.
    pub accepted: bool,
    /// What rejects the baseline: error findings and replayed hard
    /// residuals.
    pub rejected_by: Vec<String>,
    /// Native-mission trip fuel of the baseline, kg.
    pub baseline_trip_fuel_kg: Option<f64>,
    /// Native-mission trip fuel of the delivered design, kg.
    pub delivered_trip_fuel_kg: Option<f64>,
    /// `(delivered - baseline) / baseline`: negative is a fuel saving.
    pub relative_trip_fuel_change: Option<f64>,
}

impl ReportingBaseline {
    /// The comparison of `delivered` with `baseline` trip fuel.
    #[must_use]
    pub fn new(
        accepted: bool,
        rejected_by: Vec<String>,
        baseline_trip_fuel_kg: Option<f64>,
        delivered_trip_fuel_kg: Option<f64>,
    ) -> Self {
        let relative_trip_fuel_change = baseline_trip_fuel_kg
            .zip(delivered_trip_fuel_kg)
            .filter(|(baseline, delivered)| {
                baseline.is_finite() && delivered.is_finite() && *baseline > 0.0
            })
            .map(|(baseline, delivered)| (delivered - baseline) / baseline);
        Self {
            accepted,
            rejected_by,
            baseline_trip_fuel_kg,
            delivered_trip_fuel_kg,
            relative_trip_fuel_change,
        }
    }
}

impl BaselineComparison {
    pub(crate) fn new(
        baseline_feasible: bool,
        baseline: f64,
        winner: f64,
        (baseline_block_fuel_kg, winner_block_fuel_kg): (Option<f64>, Option<f64>),
    ) -> Self {
        let relative_change = (baseline.is_finite() && winner.is_finite() && baseline != 0.0)
            .then(|| (winner - baseline) / baseline.abs());
        Self {
            baseline_feasible,
            baseline_objective_value: baseline,
            winner_objective_value: winner,
            relative_change,
            baseline_block_fuel_kg,
            winner_block_fuel_kg,
        }
    }
}
