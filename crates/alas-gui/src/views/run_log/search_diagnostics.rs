// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the design search spent its budget on, in the run log's Timings tab.
//!
//! The search has always returned this record and nothing displayed it, so
//! the only visible account of a slow or short run was its wall clock. Three
//! states are distinguished here rather than collapsed into one, because
//! they mean different things:
//!
//! - the run included no design search, so there is nothing to report;
//! - a search ran but its method reports no diagnostics, which is a gap in
//!   what that method records rather than a search that did nothing;
//! - a search ran and reported, in which case every field is shown with its
//!   own unit, and the two optional costs say "not reached" instead of zero
//!   when the search never found a feasible point.

use alas_opt::SearchDiagnostics;
use egui::{Grid, RichText, Ui};

use crate::state::AppState;
use crate::views::{tr, tr_fields};

/// The search diagnostics for the run currently loaded, if any ran.
pub(super) fn current(state: &AppState) -> Option<&SearchDiagnostics> {
    state
        .pipeline_result
        .as_ref()?
        .optimization_result
        .as_ref()?
        .search_diagnostics
        .as_ref()
}

/// Whether the loaded run included a design search at all.
pub(super) fn search_ran(state: &AppState) -> bool {
    state
        .pipeline_result
        .as_ref()
        .is_some_and(|result| result.optimization_result.is_some())
}

/// Render the diagnostics block above the stage timings.
pub(super) fn show(state: &AppState, ui: &mut Ui) {
    ui.label(RichText::new(tr("Search diagnostics")).strong().small());
    let Some(diagnostics) = current(state) else {
        let message = if search_ran(state) {
            // A search method outside the staged MADS driver returns no
            // lifecycle record. Saying so is not the same as reporting zeros.
            tr("This run's search method reports no diagnostics.")
        } else {
            tr("This run included no design search.")
        };
        ui.label(RichText::new(message).weak().small());
        ui.add_space(6.0);
        return;
    };
    Grid::new("run_log_search_diagnostics")
        .num_columns(2)
        .striped(true)
        .spacing([12.0, 3.0])
        .show(ui, |ui| {
            row(
                ui,
                tr("Reached its convergence criterion"),
                if diagnostics.converged {
                    tr("Yes")
                } else {
                    tr("No: stopped on budget, iterations, mesh floor or watchdog")
                },
            );
            row(
                ui,
                tr("Coupled full-fidelity analyses"),
                count(diagnostics.analysis_evaluations),
            );
            row(
                ui,
                tr("Cached mesh nodes reused"),
                count(diagnostics.cache_hits),
            );
            row(
                ui,
                tr("Poll iterations completed"),
                count(diagnostics.poll_iterations),
            );
            row(
                ui,
                tr("Reduced-model scan evaluations"),
                count(diagnostics.screening_evaluations),
            );
            row(
                ui,
                tr("Scan candidates feasible under the reduced model"),
                count(diagnostics.screening_feasible),
            );
            row(
                ui,
                tr("Full-fidelity verifications of scan finalists"),
                count(diagnostics.verification_evaluations),
            );
            row(
                ui,
                tr("Scan wall time"),
                seconds(diagnostics.scan_wall_time_s),
            );
            row(
                ui,
                tr("MADS wall time"),
                seconds(diagnostics.search_wall_time_s),
            );
            row(
                ui,
                tr("Worker threads per evaluation block"),
                count(diagnostics.workers),
            );
            row(
                ui,
                tr("Points per poll block"),
                count(diagnostics.poll_block_size),
            );
            row(
                ui,
                tr("Objective at the first feasible point"),
                optional(diagnostics.first_feasible_cost, 6),
            );
            row(
                ui,
                tr("Relative improvement over it"),
                optional(diagnostics.relative_improvement, 4),
            );
        });
    ui.label(
        RichText::new(tr(
            "Scan evaluations use a coarser mesh and a looser sizing closure, so they are not comparable with the full-fidelity count.",
        ))
        .weak()
        .small(),
    );
    ui.add_space(6.0);
}

fn row(ui: &mut Ui, label: String, value: String) {
    ui.label(RichText::new(label).small());
    ui.label(RichText::new(value).monospace().small());
    ui.end_row();
}

fn count(value: usize) -> String {
    value.to_string()
}

fn seconds(value: f64) -> String {
    if value.is_finite() {
        tr_fields("{value} s", &[("value", format!("{value:.2}"))])
    } else {
        tr("unavailable")
    }
}

/// A cost the search reports only when it reached a feasible point.
///
/// `None` is printed as "not reached" rather than as zero, because a search
/// that never became feasible and one that started at zero cost are
/// different runs and the reader has to be able to tell them apart.
fn optional(value: Option<f64>, decimals: usize) -> String {
    match value {
        Some(value) if value.is_finite() => format!("{value:.decimals$}"),
        Some(_) => tr("unavailable"),
        None => tr("not reached"),
    }
}
