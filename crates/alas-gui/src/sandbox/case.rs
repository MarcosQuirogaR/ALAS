// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The case snapshot the sandbox swaps with the guided workspace, and the
//! persisted sandbox window layout.

use std::collections::BTreeMap;

use alas_pipeline::{PipelineResult, RunEvent};
use alas_report::scene::Scene;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::state::{AppState, LogLine, RunLogTab, RunOptions};
use crate::views::results_view::SolverResultView;
/// Everything that identifies one case.
pub struct CaseSnapshot {
    config_values: Value,
    design_values: BTreeMap<String, f64>,
    bounds: BTreeMap<String, (f64, f64)>,
    active_preset: String,
    selected_aux_preset: BTreeMap<String, String>,
    pipeline_result: Option<PipelineResult>,
    run_events: Vec<RunEvent>,
    logs: Vec<LogLine>,
    run_log_tab: RunLogTab,
    validation_findings: Vec<alas_config::ValidationIssue>,
    result_scene: Option<Scene>,
    selected_result_id: String,
    results_tab: String,
    selected_solver_view: SolverResultView,
    active_page: String,
    run_options: RunOptions,
    status_message: String,
    stage: String,
    run_identity: u64,
}

/// Persisted window layout of the sandbox.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SandboxLayout {
    /// Parameter Panel width in points.
    pub parameter_panel_width: f32,
    /// Whether the estimates strip is expanded. Closed on every sandbox
    /// entry; Quick Analysis opens it.
    pub estimates_open: bool,
    /// Whether the Summary card of derived geometry metrics is shown.
    pub summary_open: bool,
    /// Estimates strip width in points.
    pub estimates_width: f32,
    /// Whether the run log window is shown.
    pub log_window_open: bool,
    /// Whether the run log window is minimized to its title bar.
    pub log_window_minimized: bool,
    /// Whether the Advanced Settings window is open.
    pub advanced_settings_open: bool,
    /// Discipline windows that are open, by discipline id.
    pub open_disciplines: Vec<String>,
    /// The focused discipline, by id, or none for the overview.
    pub focus: Option<String>,
}

impl Default for SandboxLayout {
    fn default() -> Self {
        Self {
            parameter_panel_width: 300.0,
            estimates_open: false,
            summary_open: false,
            estimates_width: 300.0,
            log_window_open: false,
            log_window_minimized: false,
            advanced_settings_open: false,
            open_disciplines: Vec::new(),
            focus: None,
        }
    }
}

impl AppState {
    /// Move the active case out of the shared state.
    pub(super) fn take_case(&mut self) -> CaseSnapshot {
        CaseSnapshot {
            config_values: std::mem::take(&mut self.config_values),
            design_values: std::mem::take(&mut self.design_values),
            bounds: std::mem::take(&mut self.bounds),
            active_preset: std::mem::take(&mut self.active_preset),
            selected_aux_preset: std::mem::take(&mut self.selected_aux_preset),
            pipeline_result: self.pipeline_result.take(),
            run_events: std::mem::take(&mut self.run_events),
            logs: std::mem::take(&mut self.logs),
            run_log_tab: self.run_log_tab,
            validation_findings: std::mem::take(&mut self.validation_findings),
            result_scene: self.result_scene.take(),
            selected_result_id: std::mem::take(&mut self.selected_result_id),
            results_tab: std::mem::take(&mut self.results_tab),
            selected_solver_view: std::mem::replace(
                &mut self.selected_solver_view,
                SolverResultView::Vlm,
            ),
            active_page: std::mem::take(&mut self.active_page),
            run_options: self.run_options.clone(),
            status_message: std::mem::take(&mut self.status_message),
            stage: std::mem::take(&mut self.stage),
            run_identity: self.run_identity,
        }
    }

    /// Put a case back into the shared state.
    pub(super) fn restore_case(&mut self, case: CaseSnapshot) {
        self.config_values = case.config_values;
        self.design_values = case.design_values;
        self.bounds = case.bounds;
        self.active_preset = case.active_preset;
        self.selected_aux_preset = case.selected_aux_preset;
        self.pipeline_result = case.pipeline_result;
        self.run_events = case.run_events;
        self.logs = case.logs;
        self.run_log_tab = case.run_log_tab;
        self.validation_findings = case.validation_findings;
        self.result_scene = case.result_scene;
        self.selected_result_id = case.selected_result_id;
        self.results_tab = case.results_tab;
        self.selected_solver_view = case.selected_solver_view;
        self.active_page = case.active_page;
        self.run_options = case.run_options;
        self.status_message = case.status_message;
        self.stage = case.stage;
        // Run identities stay monotonic across cases so figure caches keyed
        // by run never collide between the two workspaces.
        self.run_identity = self.run_identity.max(case.run_identity);
        self.result_figure_cache.clear();
    }
}

impl CaseSnapshot {
    /// The run identity the case last used.
    pub(super) fn run_identity(&self) -> u64 {
        self.run_identity
    }
}
