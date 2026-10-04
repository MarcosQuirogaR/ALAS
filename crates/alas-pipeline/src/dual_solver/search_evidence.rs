// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Complete search accounting survives reporting failures without delivering a design.

use std::io;
use std::path::Path;

use alas_opt::OptimizationResult;
use serde::Serialize;

#[derive(Serialize)]
struct SearchEvidence<'a> {
    record_kind: &'static str,
    explanation: &'static str,
    delivered_feasible: bool,
    reporting_verified: bool,
    non_finite_numbers: &'static str,
    optimization: &'a OptimizationResult,
}

impl<'a> SearchEvidence<'a> {
    fn new(optimization: &'a OptimizationResult) -> Self {
        Self {
            record_kind: "optimization_search",
            explanation: "Complete native optimizer history before reporting-fidelity verification. Search validity is not a delivered or reporting-verified design verdict.",
            delivered_feasible: false,
            reporting_verified: false,
            non_finite_numbers: "JSON null denotes a non-finite or unavailable numerical diagnostic; it never counts as a finite objective or a feasible candidate.",
            optimization,
        }
    }
}

fn write(directory: &Path, optimization: &OptimizationResult) -> io::Result<()> {
    let bytes =
        serde_json::to_vec_pretty(&SearchEvidence::new(optimization)).map_err(io::Error::other)?;
    let temporary = directory.join("optimization_search.json.tmp");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, directory.join("optimization_search.json"))
}

/// Publish search-only accounting atomically; evidence I/O cannot reject a design.
pub(super) fn persist(directory: Option<&Path>, optimization: &OptimizationResult) {
    if let Some(directory) = directory {
        if let Err(error) = write(directory, optimization) {
            tracing::warn!(%error, directory = %directory.display(), "cannot persist native search evidence");
        }
    }
}

#[cfg(test)]
// Fixture construction and temporary-file failures are test assertions.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use alas_config::DesignVector;
    use alas_opt::OptimizationHistory;

    #[test]
    fn search_evidence_preserves_history_without_claiming_delivery() {
        let result = OptimizationResult {
            best_design: DesignVector::default(),
            best_cost: 1.0,
            best_valid: true,
            history: OptimizationHistory {
                design_vectors: vec![DesignVector::default()],
                valid: vec![true],
                cost: vec![1.0],
                objective_value: vec![2.0],
                hard_violation: vec![0.0],
                ..Default::default()
            },
            wall_time_s: 0.0,
            method: "differential_evolution".into(),
            strategy: "test".into(),
            termination: "evaluation_budget".into(),
            pareto_front: Vec::new(),
            search_diagnostics: None,
            delivered_acceptance: None,
        };
        let directory = std::env::temp_dir().join(format!(
            "alas-native-search-evidence-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).expect("test directory");
        write(&directory, &result).expect("atomic evidence publication");
        let path = directory.join("optimization_search.json");
        let record: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("published evidence"))
                .expect("evidence JSON");
        assert_eq!(record["record_kind"], "optimization_search");
        assert_eq!(record["delivered_feasible"], false);
        assert_eq!(record["reporting_verified"], false);
        let restored: OptimizationResult =
            serde_json::from_value(record["optimization"].clone()).expect("complete search result");
        assert_eq!(restored, result);
        assert!(!directory.join("optimization_search.json.tmp").exists());
        std::fs::remove_file(path).expect("remove test evidence");
        std::fs::remove_dir(directory).expect("remove empty test directory");
    }
}
