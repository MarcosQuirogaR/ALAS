// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Workflow navigation and execution controls for the fixed-wing UAV page.

use alas_uav::{TopologyAvailability, UavAnalysisPath, UavTopology};
use egui::{Button, Color32, RichText, Ui};

use crate::theme::{card_frame, selectable_button, success_color};
use crate::uav::{
    ComponentRole, MissionPlanMode, PropulsionInputMode, UavExecutionStatus, UavSection,
    UavWorkflowOutcome, UavWorkflowState,
};

use super::super::{tr, tr_fields};
use super::presentation::card_column_count;

/// Smallest width for one workflow-tab button.
const MIN_WORKFLOW_TAB_WIDTH: f32 = 156.0;
/// Capping the row at three tabs keeps labels easy to scan at wide sizes.
const MAX_WORKFLOW_TAB_COLUMNS: usize = 3;

/// The fixed-wing UAV page is visible for product discovery but remains gated
/// until its catalogue and engineering inputs are release-ready.
pub(super) const UAV_RELEASE_BLOCKED: bool = true;

/// Make the release gate explicit before any editable or executable workflow
/// control is shown.
pub(super) fn release_blocker(ui: &mut Ui) {
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.heading(tr("Fixed-Wing UAV"));
        ui.colored_label(
            ui.visuals().warn_fg_color,
            RichText::new(tr("Work in progress")).strong(),
        );
        ui.label(RichText::new(tr("UAV workflow is blocked for this release.")).strong());
        ui.label(
            RichText::new(tr(
                "The fixed-wing UAV module is unavailable in the release app while its engineering and catalogue inputs are completed.",
            ))
            .weak(),
        );
    });
}

/// Render the page identity and its responsive task navigation.
pub(super) fn workflow_header(state: &mut UavWorkflowState, ui: &mut Ui) {
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.heading(tr("Fixed-Wing UAV"));
        ui.label(
            RichText::new(tr(
                "Catalogue omissions remain unverified; no mass, rating, airfoil, propulsion map, or structural property is inferred.",
            ))
            .weak(),
        );
        ui.add_space(8.0);
        ui.label(RichText::new(tr("UAV workflow")).strong());
        ui.add_space(2.0);
        workflow_tabs(state, ui);
    });
}

/// Render the selected page's purpose without duplicating the form-card title.
pub(super) fn section_intro(section: UavSection, ui: &mut Ui) {
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr(section.short_title())).strong().size(18.0));
        ui.label(RichText::new(tr(section.description())).weak());
    });
}

fn workflow_tabs(state: &mut UavWorkflowState, ui: &mut Ui) {
    let columns = workflow_tab_column_count(ui.available_width()).min(UavSection::ALL.len());
    for row in UavSection::ALL.chunks(columns) {
        ui.columns(columns, |column_uis| {
            for (index, section) in row.iter().enumerate() {
                let selected = state.active_section == *section;
                let response = column_uis[index].add_sized(
                    [column_uis[index].available_width(), 30.0],
                    selectable_button(
                        RichText::new(format!(
                            "{}  {}",
                            UavSection::ALL
                                .iter()
                                .position(|candidate| candidate == section)
                                .unwrap_or_default()
                                + 1,
                            tr(section.short_title())
                        )),
                        selected,
                    ),
                );
                let clicked = response.clicked();
                let color = match section_status(state, *section) {
                    WorkflowStatus::Complete => success_color(column_uis[index].visuals()),
                    WorkflowStatus::NeedsAttention => column_uis[index].visuals().warn_fg_color,
                    WorkflowStatus::Pending => column_uis[index].visuals().weak_text_color(),
                };
                column_uis[index].painter().circle_filled(
                    response.rect.left_center() + egui::vec2(11.0, 0.0),
                    3.5,
                    color,
                );
                response.on_hover_text(tr(section.description()));
                if clicked {
                    // Switching sections is always safe. Editing remains disabled
                    // in the individual section while a worker is active.
                    state.active_section = *section;
                }
            }
        });
        ui.add_space(4.0);
    }
}

fn workflow_tab_column_count(available_width: f32) -> usize {
    ((available_width / MIN_WORKFLOW_TAB_WIDTH).floor() as usize).clamp(1, MAX_WORKFLOW_TAB_COLUMNS)
}

#[derive(Clone, Copy)]
enum WorkflowStatus {
    Complete,
    NeedsAttention,
    Pending,
}

fn section_status(state: &UavWorkflowState, section: UavSection) -> WorkflowStatus {
    let complete = match section {
        UavSection::Mission => {
            let objectives = &state.objectives;
            [
                objectives.endurance_s,
                objectives.range_m,
                objectives.cruise_speed_m_s,
                objectives.maximum_stall_speed_m_s,
                objectives.payload_mass_kg,
                objectives.payload_dimensions.length_m,
                objectives.payload_dimensions.width_m,
                objectives.payload_dimensions.height_m,
            ]
            .into_iter()
            .all(positive_finite)
                && (state.mission_plan_mode == MissionPlanMode::Standard
                    || !state.mission_phases.is_empty())
        }
        UavSection::Hardware => state.selected_evidence_gaps().is_empty(),
        UavSection::Propulsion => match state.propulsion_input_mode {
            PropulsionInputMode::Automatic => {
                state.propulsion_motor_count > 0
                    && [
                        ComponentRole::Battery,
                        ComponentRole::Motor,
                        ComponentRole::Esc,
                        ComponentRole::Propeller,
                    ]
                    .into_iter()
                    .all(|role| state.selected_record(role).is_some())
            }
            PropulsionInputMode::AdvancedManual => {
                state.propulsion_motor_count > 0 && !state.propulsion_evidence.trim().is_empty()
            }
        },
        UavSection::Airframe => matches!(
            state
                .topology
                .availability(UavAnalysisPath::PreliminaryOptimization),
            TopologyAvailability::Available
        ),
        UavSection::Verification => {
            !state.shared_core_inputs.main_airfoil_name.trim().is_empty()
                && !state.shared_core_inputs.tail_airfoil_name.trim().is_empty()
        }
        UavSection::Results => match &state.outcome {
            UavWorkflowOutcome::NotRun => return WorkflowStatus::Pending,
            outcome => outcome_is_verified(outcome),
        },
    };
    if complete {
        WorkflowStatus::Complete
    } else {
        WorkflowStatus::NeedsAttention
    }
}

fn positive_finite(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

/// Select a design convention while keeping unsupported engineering claims explicit.
pub(super) fn topology_selector(state: &mut UavWorkflowState, ui: &mut Ui) {
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr("Design convention")).strong());
        ui.label(
            RichText::new(tr(
                "Choose the airframe arrangement first. A selectable convention is not automatically a supported mass or control model.",
            ))
            .weak()
            .small(),
        );
        ui.add_space(4.0);
        let columns = card_column_count(ui.available_width()).min(UavTopology::ALL.len());
        for row in UavTopology::ALL.chunks(columns) {
            ui.columns(columns, |columns| {
                for (index, topology) in row.iter().enumerate() {
                    let response = columns[index].add_sized(
                        [columns[index].available_width(), 30.0],
                        selectable_button(tr(topology.label()), state.topology == *topology),
                    );
                    let clicked = response.clicked();
                    response.on_hover_text(tr(topology.description()));
                    if clicked {
                        state.topology = *topology;
                    }
                }
            });
            ui.add_space(4.0);
        }
        ui.label(RichText::new(tr(state.topology.description())).weak().small());
        match state
            .topology
            .availability(UavAnalysisPath::PreliminaryOptimization)
        {
            TopologyAvailability::Available => ui.colored_label(
                success_color(ui.visuals()),
                tr("Preliminary sizing is available for this design convention."),
            ),
            TopologyAvailability::Unavailable(reason) => ui.colored_label(
                Color32::from_rgb(220, 150, 70),
                tr_fields(
                    "Preliminary sizing unavailable: {reason}",
                    &[("reason", tr(reason.description()))],
                ),
            ),
        };
    });
}

/// Render the start/cancel action in an independent, consistently bounded bar.
pub(super) fn action_bar(state: &mut UavWorkflowState, ui: &mut Ui) {
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        let running = state.is_running();
        if running {
            ui.horizontal(|ui| {
                ui.spinner();
                if ui.button(tr("Cancel UAV search")).clicked() {
                    state.cancel();
                }
            });
            return;
        }

        let unavailable_reason = match state
            .topology
            .availability(UavAnalysisPath::PreliminaryOptimization)
        {
            TopologyAvailability::Available => None,
            TopologyAvailability::Unavailable(reason) => Some(reason),
        };
        let response = ui.add_enabled(
            unavailable_reason.is_none(),
            Button::new(RichText::new(tr("Generate and verify UAV")).strong()),
        );
        if response.clicked() {
            state.start();
        }
        if let Some(reason) = unavailable_reason {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr_fields(
                    "Choose a supported convention before starting preliminary sizing: {reason}",
                    &[("reason", tr(reason.description()))],
                ),
            );
        }
    });
}

/// Render transient and terminal worker state only when it carries information.
pub(super) fn execution_status(state: &UavWorkflowState, ui: &mut Ui) {
    if matches!(state.execution, UavExecutionStatus::Idle) {
        return;
    }
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        match &state.execution {
        UavExecutionStatus::Idle => {}
        UavExecutionStatus::Running(progress) | UavExecutionStatus::CancelRequested(progress) => {
            let fraction = if progress.total_candidates == 0 {
                0.0
            } else {
                progress.evaluated_candidates as f32 / progress.total_candidates as f32
            };
            let text = tr_fields(
                "Evaluated {done} of {total} candidates; {verified} verified.",
                &[
                    ("done", progress.evaluated_candidates.to_string()),
                    ("total", progress.total_candidates.to_string()),
                    ("verified", progress.verified_candidates.to_string()),
                ],
            );
            ui.add(egui::ProgressBar::new(fraction).text(text));
            if matches!(state.execution, UavExecutionStatus::CancelRequested(_)) {
                ui.weak(tr("Cancellation requested; finishing the current candidate."));
            }
        }
        UavExecutionStatus::Completed => {
            if !outcome_is_verified(&state.outcome) {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr("UAV search completed without a verified production result."),
                );
            } else {
                ui.colored_label(success_color(ui.visuals()), tr("UAV search completed."));
            }
        }
        UavExecutionStatus::Cancelled {
            evaluated_candidates,
        } => {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr_fields(
                    "UAV search cancelled after {count} candidates; the last completed result is retained.",
                    &[("count", evaluated_candidates.to_string())],
                ),
            );
        }
            UavExecutionStatus::Failed(message) => {
            ui.colored_label(
                ui.visuals().error_fg_color,
                tr_fields(
                    "UAV search failed: {error}",
                    &[("error", translated_worker_error(message))],
                ),
            );
            ui.weak(tr("The last completed UAV result is retained."));
            }
        }
    });
}

fn translated_worker_error(message: &str) -> String {
    if let Some(error) = message.strip_prefix("Could not start the UAV optimization worker: ") {
        return tr_fields(
            "Could not start the UAV optimization worker: {error}",
            &[("error", error.to_owned())],
        );
    }
    tr(message)
}

/// Whether the latest result passed the independent production-core check.
pub(super) fn outcome_is_verified(outcome: &UavWorkflowOutcome) -> bool {
    matches!(
        outcome,
        UavWorkflowOutcome::PreliminaryFeasible {
            shared_core: Ok(verification),
            ..
        } if verification.lift_verdict == alas_uav::SharedCoreLiftVerdict::Passed
    )
}

#[cfg(test)]
mod tests {
    use super::{workflow_tab_column_count, MIN_WORKFLOW_TAB_WIDTH};

    #[test]
    fn workflow_tabs_wrap_before_labels_become_cramped() {
        assert_eq!(workflow_tab_column_count(MIN_WORKFLOW_TAB_WIDTH - 1.0), 1);
        assert_eq!(workflow_tab_column_count(MIN_WORKFLOW_TAB_WIDTH * 2.0), 2);
        assert_eq!(workflow_tab_column_count(MIN_WORKFLOW_TAB_WIDTH * 4.0), 3);
    }
}
