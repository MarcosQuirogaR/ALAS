// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compact, translatable rows describing the search and its physical objective.

use super::*;

impl OptimizerRunSummary {
    /// `(label, value)` lines that say what the search was and what it did,
    /// for command-line and tabular summaries.
    /// Labels are translatable catalog keys when joined with `": "`; values
    /// are numbers, units or catalog phrases.
    #[must_use]
    pub fn label_lines(&self) -> Vec<(&'static str, String)> {
        let mut lines = vec![
            ("Outcome", self.status.label().to_owned()),
            ("Termination", self.termination_text().to_owned()),
        ];
        for stage in &self.stages {
            let Some([used, wall, stopped]) = stage_labels(&stage.stage, stage.time_limited) else {
                continue;
            };
            let budget = stage
                .schedule_budget()
                .saturating_sub(stage.reserved_evaluations);
            lines.push((used, format!("{} / {budget}", stage.evaluations)));
            lines.push((
                wall,
                if stage.time_limited {
                    format!("{:.1} s / {:.0} s", stage.wall_time_s, stage.time_limit_s)
                } else {
                    format!("{:.1} s", stage.wall_time_s)
                },
            ));
            lines.push((stopped, termination_text(&stage.termination).to_owned()));
        }
        if !self.stages.is_empty() {
            let rule = if self.time_limited() {
                TIME_LIMITED_TEXT
            } else {
                EVALUATIONS_ONLY_TEXT
            };
            lines.push(("Stopping rule", rule.to_owned()));
            let count = |stage| {
                self.replay_evaluations(stage)
                    .map_or_else(|| "-".to_owned(), |count| count.to_string())
            };
            lines.push((
                REPLAY_COUNT_LABEL,
                format!("{} / {}", count("screening"), count("refinement")),
            ));
            let analyses = |stage| self.stage_value(stage, |summary| summary.analysis_evaluations);
            lines.push((
                "Coupled analyses, screening / refinement",
                format!("{} / {}", analyses("screening"), analyses("refinement")),
            ));
            if let Some(refinement) = self.stages.iter().find(|s| s.stage == "refinement") {
                lines.push((
                    PLANNED_BUDGET_LABEL,
                    format!(
                        "{} / {}",
                        refinement.schedule_budget(),
                        refinement.max_evaluations
                    ),
                ));
            }
            let rejects = |stage| self.stage_value(stage, |summary| summary.pre_gate_rejects);
            lines.push((
                "Pre-gate rejections, screening / refinement",
                format!("{} / {}", rejects("screening"), rejects("refinement")),
            ));
        }
        lines.push(("Aeroelastic caveat", AEROELASTIC_CAVEAT_TEXT.to_owned()));
        lines.push(("Buffet margin basis", BUFFET_BASIS_TEXT.to_owned()));
        lines.push(("Wall time", format!("{:.1} s", self.wall_time_s)));
        lines.push((
            "Random seed",
            self.seed
                .map_or_else(|| "not set".to_owned(), |seed| seed.to_string()),
        ));
        if let Some(verification) = self.verification {
            lines.push((
                "Verification analyses / reserve",
                format!("{} / {}", verification.analyses, verification.reserved),
            ));
        }
        let start = |preset, constrained| {
            if self.baseline_constrained {
                constrained
            } else {
                preset
            }
        };
        match self.baseline {
            Some(delta) => {
                lines.push((
                    objective::comparison_label(
                        self.objective_kind,
                        usize::from(self.baseline_constrained),
                    ),
                    format!("{:.4}", delta.baseline_objective),
                ));
                let change = delta
                    .relative_objective_change()
                    .map_or_else(String::new, |change| format!(" ({:+.2} %)", 100.0 * change));
                lines.push((
                    objective::comparison_label(self.objective_kind, 2),
                    format!("{:.4}{change}", delta.winner_objective),
                ));
                if let (Some(baseline), Some(winner)) =
                    (delta.baseline_block_fuel_kg, delta.winner_block_fuel_kg)
                {
                    lines.push((
                        start("Block fuel, preset", "Block fuel, constrained start"),
                        format!("{baseline:.0} kg"),
                    ));
                    lines.push(("Block fuel, result", format!("{winner:.0} kg")));
                }
            }
            None => lines.push(("Same-model baseline", "unavailable".to_owned())),
        }
        if let Some(geometry) = &self.geometry {
            lines.extend(geometry.flag_lines());
        }
        if let Some(reporting) = &self.reporting_baseline {
            if let (Some(baseline), Some(delivered)) = (
                reporting.baseline_trip_fuel_kg,
                reporting.delivered_trip_fuel_kg,
            ) {
                let change = reporting
                    .relative_trip_fuel_change
                    .map_or_else(String::new, |change| format!(" ({:+.2} %)", 100.0 * change));
                lines.push((
                    start(
                        "Trip fuel at reporting fidelity, preset",
                        "Trip fuel at reporting fidelity, constrained start",
                    ),
                    format!("{baseline:.0} kg"),
                ));
                lines.push((
                    "Trip fuel at reporting fidelity, result",
                    format!("{delivered:.0} kg{change}"),
                ));
            }
        }
        lines
    }
}
