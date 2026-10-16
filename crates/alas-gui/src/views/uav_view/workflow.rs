// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Workflow navigation and execution controls for the fixed-wing UAV page.

use egui::{Button, RichText, Ui};

use crate::theme::{card_frame, selectable_button, success_color};
use crate::uav::{UavExecutionStatus, UavSection, UavWorkflowState};

use super::super::{tr, tr_fields};

/// Smallest width for one workflow-tab button.
const MIN_WORKFLOW_TAB_WIDTH: f32 = 156.0;
/// Capping the row at three tabs keeps labels easy to scan at wide sizes.
const MAX_WORKFLOW_TAB_COLUMNS: usize = 3;

/// Persistent notice above every UAV page state: the outputs are preliminary
/// and no measured flight data supports them yet.
pub(super) fn preliminary_banner(ui: &mut Ui) {
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.colored_label(
            ui.visuals().warn_fg_color,
            RichText::new(tr(
                "Preliminary: not validated against measured flight data",
            ))
            .strong(),
        );
    });
}

/// Render the page identity and its responsive task navigation.
pub(super) fn workflow_header(state: &mut UavWorkflowState, ui: &mut Ui) {
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.heading(tr("Fixed-Wing UAV")).on_hover_text(tr(
            "Catalogue omissions remain unverified; no mass, rating, airfoil, propulsion map, or structural property is inferred.",
        ));
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
        ui.label(RichText::new(tr(section.short_title())).strong().size(18.0))
            .on_hover_text(tr(section.description()));
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
                state.competition.announced_takeoff_m,
            ]
            .into_iter()
            .all(positive_finite)
                && state.evaluations >= 4
        }
        UavSection::Airframe => true,
        UavSection::Results => match &state.outcome {
            crate::uav::UavWorkflowOutcome::NotRun => return WorkflowStatus::Pending,
            crate::uav::UavWorkflowOutcome::Competition(result) => result
                .optimum
                .best
                .as_ref()
                .is_some_and(|best| best.evaluation.feasible),
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

/// Render the start/cancel action in an independent, consistently bounded bar.
pub(super) fn action_bar(state: &mut UavWorkflowState, ui: &mut Ui) {
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        if state.is_running() {
            ui.horizontal(|ui| {
                ui.spinner();
                if ui.button(tr("Cancel UAV search")).clicked() {
                    state.cancel();
                }
            });
            return;
        }
        let response = ui.add(Button::new(
            RichText::new(tr("Run competition search")).strong(),
        ));
        if response.clicked() {
            state.start();
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
            UavExecutionStatus::Running(progress)
            | UavExecutionStatus::CancelRequested(progress) => {
                let fraction = if progress.total_candidates == 0 {
                    0.0
                } else {
                    progress.evaluated_candidates as f32 / progress.total_candidates as f32
                };
                let text = tr_fields(
                    "Evaluated {done} of {total} candidates; {verified} feasible.",
                    &[
                        ("done", progress.evaluated_candidates.to_string()),
                        ("total", progress.total_candidates.to_string()),
                        ("verified", progress.verified_candidates.to_string()),
                    ],
                );
                ui.add(egui::ProgressBar::new(fraction).text(text));
                if let Some(score) = progress.best_score {
                    ui.weak(tr_fields(
                        "Best feasible score so far: {score} points",
                        &[("score", format!("{score:.1}"))],
                    ));
                }
                if matches!(state.execution, UavExecutionStatus::CancelRequested(_)) {
                    ui.weak(tr("Cancellation requested; finishing the current candidate."));
                }
            }
            UavExecutionStatus::Completed => {
                ui.colored_label(success_color(ui.visuals()), tr("UAV search completed."));
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
                    tr_fields("UAV search failed: {error}", &[("error", message.clone())]),
                );
                ui.weak(tr("The last completed UAV result is retained."));
            }
        }
    });
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
