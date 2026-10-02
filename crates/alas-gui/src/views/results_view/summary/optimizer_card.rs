// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the design search was and what it did, above the result figures.
//!
//! The search is a bounded local refinement of the preset, so the card says
//! so and states, per stage, the budget, the evaluations used, the wall time
//! and why it stopped; then the stopping rule and the counts that replay the
//! run, the seed, the verification against its reserve, and the change
//! against the preset scored by the same model and at reporting fidelity.
//! The rows are the ones the CLI and the report print. The status word never
//! reads "Completed" for a design that is not feasible.

use alas_pipeline::optimizer_summary::{OptimizerRunSummary, SCOPE_LABEL};
use alas_pipeline::SolverOptimizationStatus;
use egui::{Grid, RichText, Ui};

use super::widgets::semantic_frame;
use crate::views::tr;

/// Label and localized value of each row, in display order. Labels are
/// catalog keys.
pub(super) fn rows(summary: &OptimizerRunSummary) -> Vec<(&'static str, String)> {
    summary
        .label_lines()
        .into_iter()
        .map(|(label, value)| (label, tr(&value)))
        .collect()
}

pub(super) fn show(ui: &mut Ui, result: &alas_pipeline::PipelineResult) {
    let Some(summary) = OptimizerRunSummary::from_pipeline(result) else {
        return;
    };
    let color = if summary.status == SolverOptimizationStatus::Completed {
        crate::theme::success_color(ui.visuals())
    } else {
        ui.visuals().error_fg_color
    };
    semantic_frame(ui, color).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr(SCOPE_LABEL)).strong().size(16.0))
            .on_hover_text(tr(
                "The search moves the design inside a bounded envelope around the preset. It does not look for the best aircraft of its class.",
            ));
        Grid::new("optimizer_outcome_card")
            .num_columns(2)
            .striped(true)
            .spacing([12.0, 3.0])
            .show(ui, |ui| {
                for (label, value) in rows(&summary) {
                    ui.label(RichText::new(tr(label)).small());
                    ui.label(RichText::new(value).monospace().small());
                    ui.end_row();
                }
            });
    });
    ui.add_space(10.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::AlasConfig;

    fn stage(name: &str, limited: bool) -> alas_opt::StageSummary {
        alas_opt::StageSummary {
            stage: name.to_owned(),
            max_evaluations: 100,
            planned_evaluations: 100,
            reserved_evaluations: 0,
            time_limit_s: 30.0,
            time_limited: limited,
            evaluations: 50,
            restoration_evaluations: 0,
            cancelled_unstarted: 0,
            pre_gate_rejects: 0,
            analysis_evaluations: 50,
            generations: 2,
            feasible: 10,
            elite_size: 4,
            wall_time_s: 3.0,
            candidate_time_s: 1.0,
            lane_utilization: 0.5,
            termination: if limited {
                "time_budget"
            } else {
                "evaluation_budget"
            }
            .to_owned(),
            sizing_work: None,
        }
    }

    fn summary(feasible: bool, limited: bool) -> OptimizerRunSummary {
        let mut summary = OptimizerRunSummary::from_result(
            &alas_opt::OptimizationResult {
                best_design: alas_config::DesignVector::default(),
                best_cost: 8_600.0,
                best_valid: feasible,
                history: alas_opt::OptimizationHistory::new(),
                wall_time_s: 3.0,
                method: "differential_evolution".to_owned(),
                strategy: String::new(),
                termination: "evaluation_budget".to_owned(),
                pareto_front: Vec::new(),
                search_diagnostics: None,
                delivered_acceptance: None,
            },
            &AlasConfig::default(),
            Some(5),
        );
        summary.stages = vec![stage("screening", limited), stage("refinement", limited)];
        summary
    }

    #[test]
    fn card_states_both_stages_the_stopping_rule_replay_counts_and_seed() {
        let rows = rows(&summary(true, true));
        let labels: Vec<&str> = rows.iter().map(|(label, _)| *label).collect();
        for expected in [
            "Outcome",
            "Termination",
            "Screening, evaluations used / budget",
            "Screening, wall time / limit",
            "Refinement, evaluations used / budget",
            "Refinement, termination",
            "Stopping rule",
            alas_pipeline::optimizer_summary::REPLAY_COUNT_LABEL,
            "Coupled analyses, screening / refinement",
            alas_pipeline::optimizer_summary::PLANNED_BUDGET_LABEL,
            "Wall time",
            "Random seed",
        ] {
            assert!(labels.contains(&expected), "missing row {expected}");
        }
    }

    #[test]
    fn an_infeasible_outcome_row_never_reads_completed() {
        let rows = rows(&summary(false, true));
        let outcome = &rows[0].1;
        assert!(
            !outcome.to_ascii_lowercase().contains("completed"),
            "{outcome}"
        );
    }

    #[test]
    fn every_card_string_has_spanish_provenance() {
        let base = alas_i18n::es::catalog();
        let desktop = alas_i18n::es::desktop_catalog();
        let mut keys: Vec<&str> = vec![
            SCOPE_LABEL,
            "Same-model baseline",
            "Verification analyses / reserve",
            "Objective, preset",
            "Objective, result",
            "Block fuel, preset",
            "Block fuel, result",
            "Trip fuel at reporting fidelity, preset",
            "Trip fuel at reporting fidelity, result",
            alas_pipeline::optimizer_summary::ASPECT_RATIO_FLAG_LABEL,
            alas_pipeline::optimizer_summary::SWEEP_FLAG_LABEL,
            "Objective, constrained start",
            "Block fuel, constrained start",
            "Trip fuel at reporting fidelity, constrained start",
            alas_pipeline::optimizer_summary::TIME_LIMITED_TEXT,
            alas_pipeline::optimizer_summary::EVALUATIONS_ONLY_TEXT,
            "Aeroelastic caveat",
            alas_pipeline::optimizer_summary::AEROELASTIC_CAVEAT_TEXT,
            "Buffet margin basis",
            alas_pipeline::optimizer_summary::BUFFET_BASIS_TEXT,
            "The search moves the design inside a bounded envelope around the preset. It does not look for the best aircraft of its class.",
        ];
        keys.extend(
            alas_pipeline::optimizer_summary::TERMINATION_TEXT
                .iter()
                .map(|(_, text)| *text),
        );
        for status in [
            SolverOptimizationStatus::NotRequested,
            SolverOptimizationStatus::Completed,
            SolverOptimizationStatus::Infeasible,
            SolverOptimizationStatus::Failed,
        ] {
            keys.push(status.label());
        }
        let lines: Vec<(&'static str, String)> = [true, false]
            .into_iter()
            .flat_map(|limited| {
                let mut unseeded = summary(false, limited);
                unseeded.seed = None;
                [summary(true, limited), unseeded]
            })
            .flat_map(|summary| summary.label_lines())
            .collect();
        let report_keys: Vec<String> = lines
            .iter()
            .flat_map(|(label, value)| [label.to_string(), format!("{label}: "), value.clone()])
            .filter(|key| !key.chars().next().is_some_and(|c| c.is_ascii_digit()))
            .filter(|key| !key.ends_with(" s") && !key.ends_with(" kg") && !key.contains('%'))
            .collect();
        keys.extend(report_keys.iter().map(String::as_str));
        for key in keys {
            assert!(
                base.contains_key(key) || desktop.contains_key(key),
                "no Spanish entry for {key:?}"
            );
        }
    }
}
