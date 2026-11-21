// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the reporting-fidelity verification may deliver: the search's
//! ranked hard-feasible candidates, and the registered aircraft when none of
//! them beats it.

use alas_config::design_variables::DesignVector;

use super::{OptimizationResult, REGISTERED_DESIGN_RETAINED, START_DESIGN_RETAINED};

impl OptimizationResult {
    /// Record that the run delivers the registered aircraft `design`
    /// because no candidate of the search was verified and at least as good
    /// at reporting fidelity. Call after
    /// [`Self::record_delivered_acceptance`]. When its reporting analysis was
    /// accepted (`verified`), the result is a delivered feasible one whatever
    /// the search's own candidates were; otherwise the aircraft is delivered
    /// flagged, its acceptance verdict and violated constraints as recorded.
    /// The search's history and diagnostics are kept as they were.
    pub fn record_registered_design_retained(&mut self, design: DesignVector, verified: bool) {
        self.record_retained(design, REGISTERED_DESIGN_RETAINED, verified);
    }

    /// [`Self::record_registered_design_retained`] for a start that is not
    /// the registered aircraft, so the run does not claim to deliver it.
    pub fn record_start_design_retained(&mut self, design: DesignVector, verified: bool) {
        self.record_retained(design, START_DESIGN_RETAINED, verified);
    }

    fn record_retained(&mut self, design: DesignVector, termination: &str, verified: bool) {
        self.best_design = design;
        self.termination = termination.to_owned();
        if !verified {
            self.best_valid = false;
            return;
        }
        self.best_valid = true;
        if let Some(acceptance) = self.delivered_acceptance.as_mut() {
            acceptance.verified = true;
            acceptance.delivered_is_search_finalist = false;
            acceptance.delivered_rejected_by.clear();
        }
    }

    /// Hard-feasible candidates from this run's own history, best first, for
    /// a caller that must re-verify more than the finalist.
    ///
    /// Ranked exactly as the search ranks: a candidate with any violated hard
    /// residual is never offered, so a fallback can only ever be a design the
    /// search itself considered admissible. `best_design` is placed first
    /// whatever its history row says, because it is the design the search
    /// returned, and the winner's own history row is skipped. Duplicate
    /// vectors are removed on their exact bit pattern so a design is not
    /// verified twice.
    pub fn ranked_hard_feasible_candidates(&self, limit: usize) -> Vec<DesignVector> {
        let winner_row = self
            .search_diagnostics
            .as_ref()
            .and_then(|diagnostics| diagnostics.winner_history_row);
        let mut ranked: Vec<(usize, f64, DesignVector)> = self
            .history
            .design_vectors
            .iter()
            .enumerate()
            // A row with no design (an unparsed candidate) cannot be verified.
            .filter_map(|(index, design)| design.as_ref().map(|design| (index, design)))
            .filter(|(index, _)| {
                Some(*index) != winner_row
                    && self.history.valid.get(*index).copied().unwrap_or(false)
                    && self
                        .history
                        .hard_violation
                        .get(*index)
                        .copied()
                        .is_some_and(|violation| violation <= 0.0)
                    && self
                        .history
                        .cost
                        .get(*index)
                        .copied()
                        .is_some_and(f64::is_finite)
            })
            .map(|(index, design)| (index, self.history.cost[index], *design))
            .collect();
        ranked.sort_by(|left, right| left.1.total_cmp(&right.1).then(left.0.cmp(&right.0)));

        let key = |design: &DesignVector| {
            design
                .to_array()
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<u64>>()
        };
        let mut seen = vec![key(&self.best_design)];
        let mut candidates = vec![self.best_design];
        for (_, _, design) in ranked {
            if candidates.len() >= limit.max(1) {
                break;
            }
            let candidate_key = key(&design);
            if seen.contains(&candidate_key) {
                continue;
            }
            seen.push(candidate_key);
            candidates.push(design);
        }
        candidates
    }
}
