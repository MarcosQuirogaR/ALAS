// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The evaluation-budget table: per-preset screening and refinement
//! evaluation counts, the seed, and the provenance of the numbers.
//!
//! The embedded `matrix` table is `data/matrix_budgets.json`. A budget is an
//! evaluation count only (`--stop-on-evaluations`), so a cell's result does not
//! depend on host load, worker count or cell parallelism.

use std::collections::BTreeMap;
use std::fs;
use std::io;

use serde::Deserialize;

use super::Cell;

const EMBEDDED: &str = include_str!("../../../data/matrix_budgets.json");

#[derive(Debug, Clone, Deserialize)]
pub struct PresetBudget {
    pub screening: u64,
    pub refinement: u64,
    /// Estimated core-seconds per evaluation, used only to order the queue.
    #[serde(default)]
    pub core_s_per_evaluation: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BudgetTable {
    /// Experiment label recorded in every cell (never a registered solver preset name).
    pub name: String,
    pub seed: u64,
    /// Provenance caveat carried into the summary.
    pub status: String,
    #[serde(default)]
    pub provenance: String,
    /// Per-mode multiplier on both budgets (1.0 when absent).
    #[serde(default)]
    pub mode_scale: BTreeMap<String, f64>,
    /// Per-mode relative cost of one evaluation, used only to order the queue
    /// (heaviest first); 1.0 when absent.
    #[serde(default)]
    pub mode_cost_factor: BTreeMap<String, f64>,
    pub default: PresetBudget,
    pub presets: BTreeMap<String, PresetBudget>,
}

impl BudgetTable {
    /// `matrix` selects the embedded table; anything else is a JSON file path.
    pub fn load(spec: &str) -> io::Result<Self> {
        let text = if spec == "matrix" {
            EMBEDDED.to_owned()
        } else {
            fs::read_to_string(spec)?
        };
        let table: Self = serde_json::from_str(&text).map_err(|error| {
            io::Error::new(io::ErrorKind::InvalidData, format!("{spec}: {error}"))
        })?;
        table.validate()?;
        Ok(table)
    }

    fn validate(&self) -> io::Result<()> {
        let bad = |what: &str| io::Error::new(io::ErrorKind::InvalidData, what.to_owned());
        if self.name.is_empty() || alas_config::solver_presets::get(&self.name).is_ok() {
            return Err(bad(
                "table name must be non-empty and not a registered solver preset",
            ));
        }
        for budget in self.presets.values().chain([&self.default]) {
            if budget.screening == 0 || budget.refinement == 0 {
                return Err(bad("budgets must be positive"));
            }
        }
        if self
            .mode_scale
            .values()
            .any(|s| !s.is_finite() || *s <= 0.0)
        {
            return Err(bad("mode_scale entries must be positive and finite"));
        }
        Ok(())
    }

    /// The cell for `preset` in `mode`: its budget (the table default when the
    /// preset is absent) scaled by the mode factor, rounded, at least 1.
    pub fn cell(&self, preset: &str, mode: &str) -> Cell {
        let budget = self.presets.get(preset).unwrap_or(&self.default);
        let scale = self.mode_scale.get(mode).copied().unwrap_or(1.0);
        let apply = |n: u64| ((n as f64 * scale).round() as u64).max(1);
        let (screening, refinement) = (apply(budget.screening), apply(budget.refinement));
        let core_s = budget
            .core_s_per_evaluation
            .or(self.default.core_s_per_evaluation)
            .unwrap_or(1.0);
        Cell {
            preset: preset.to_owned(),
            mode: mode.to_owned(),
            screening,
            refinement,
            weight: (screening + refinement) as f64
                * core_s
                * self.mode_cost_factor.get(mode).copied().unwrap_or(1.0),
        }
    }

    /// Whether `preset` has its own entry (otherwise the flagged default applies).
    pub fn has_entry(&self, preset: &str) -> bool {
        self.presets.contains_key(preset)
    }
}

#[cfg(test)]
// A failed unwrap in a test is the assertion failing.
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_table_covers_every_registered_preset() {
        let table = BudgetTable::load("matrix").unwrap();
        for preset in alas_config::presets::available() {
            assert!(table.has_entry(preset), "{preset} has no budget entry");
        }
        assert_eq!(table.name, "matrix");
    }

    #[test]
    fn mode_scale_and_default_apply() {
        let mut table = BudgetTable::load("matrix").unwrap();
        table.mode_scale.insert("mtow_band".to_owned(), 0.5);
        let a = table.cell("A320-200", "fixed_requirement");
        let b = table.cell("A320-200", "mtow_band");
        assert_eq!(b.screening, (a.screening as f64 * 0.5).round() as u64);
        let unknown = table.cell("NOPE", "fixed_requirement");
        assert_eq!(unknown.screening, table.default.screening);
    }
}
