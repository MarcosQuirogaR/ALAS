// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The bottom control bar: the run group (Analyze reference / Run), and the
//! status/elapsed readout.

use egui::{RichText, Ui};

use crate::state::AppState;
use crate::views::tour_data::TourTarget;
use crate::views::{tr, tr_label};

/// Size both run-group buttons share, so the pair reads as one unit.
const RUN_BUTTON_SIZE: egui::Vec2 = egui::vec2(132.0, 28.0);

/// Render the control bar.
pub fn show_control_bar(state: &mut AppState, ui: &mut Ui) {
    // A launcher needs breathing room from the run-log divider and window
    // edge; otherwise its controls look visually welded to both surfaces.
    ui.add_space(6.0);
    // Workspace pages do not run the design: keep only the status text, so
    // there is no competing Run button.
    if matches!(
        state.active_page.as_str(),
        "files" | "setup_tools" | "addons"
    ) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr(&state.status_message)).weak());
        });
        ui.add_space(6.0);
        return;
    }
    ui.horizontal(|ui| {
        let running = state.is_running;
        // The Fixed-wing UAV page holds a placeholder while its workflow is
        // not ready: a Run there would start something else, so both buttons
        // are off and say why.
        let not_ready = state.active_page == "uav" && crate::views::uav_view::UAV_WORK_IN_PROGRESS;
        let not_ready_hint = tr("The fixed-wing UAV workflow is not ready yet. Open another page to run an aircraft study.");

        let baseline_response = ui
            .add_enabled(
                !running && !not_ready,
                egui::Button::new(tr_label("Analyze reference")).min_size(RUN_BUTTON_SIZE),
            )
            .on_hover_text(if not_ready {
                not_ready_hint.clone()
            } else {
                tr("Analyze the selected reference aircraft and load case with no redesign")
            });
        if baseline_response.clicked() {
            state.start_pipeline(true);
        }

        let blocked = state.blocked();
        let run_button = crate::theme::primary_button(
            RichText::new(tr_label(if running { "Running..." } else { "Run" })).strong(),
        )
        .min_size(RUN_BUTTON_SIZE);
        let run_response = ui
            .add_enabled(!running && !blocked && !not_ready, run_button)
            .on_hover_text(if not_ready {
                not_ready_hint
            } else if blocked {
                crate::views::notices::run_blocked_hover_text(state)
            } else if !crate::views::inputs_mtow::optimize_active(state) {
                tr("Analyze the current design and run the mission without optimization")
            } else {
                tr("Optimize, analyze and run the mission in one pass")
            });
        if run_response.clicked() {
            state.start_pipeline(false);
        }
        // A disabled button explains nothing on its own: say beside it that
        // something blocks the run, and carry the reasons in its hover text.
        if blocked && !not_ready {
            crate::views::notices::show_run_blocked_marker(state, ui);
        }
        if running {
            let cancel = ui
                .add_enabled(
                    !state.cancellation_requested,
                    egui::Button::new(tr_label(if state.cancellation_requested {
                        "Cancelling..."
                    } else {
                        "Cancel"
                    })),
                )
                .on_hover_text(tr(
                    "Stop the run now: running external tools are killed and the run ends at the next safe point",
                ));
            if cancel.clicked() {
                state.request_pipeline_cancel();
            }
        }
        state.record_walkthrough_target(
            TourTarget::Run,
            baseline_response.rect.union(run_response.rect),
        );

        if running {
            ui.spinner();
            let ms = state.elapsed_ms();
            let secs = ms / 1000;
            ui.label(format!("{}:{:02}", secs / 60, secs % 60))
                .on_hover_text(tr(&state.stage));
        } else if !state.stage.is_empty() {
            ui.label(tr(&state.stage));
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let color = if state.status_message.contains("error")
                || state.status_message.contains("Failed")
            {
                Some(ui.visuals().error_fg_color)
            } else {
                None
            };
            let mut text = RichText::new(tr(&state.status_message));
            if let Some(c) = color {
                text = text.color(c);
            }
            ui.label(text);
        });
    });
    ui.add_space(6.0);
}

#[cfg(test)]
mod tests {
    use super::show_control_bar;
    use crate::state::AppState;
    use crate::views::tour_data::TourTarget;

    #[test]
    fn walkthrough_records_the_run_button_group_rectangle() {
        let mut state = AppState::default();
        state.finish_walkthrough();
        state.begin_walkthrough();
        let context = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1_000.0, 120.0),
            )),
            ..egui::RawInput::default()
        };

        let _ = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| show_control_bar(&mut state, ui));
        });

        let run = state.walkthrough_targets[&TourTarget::Run];
        assert!(run.width() < 300.0);
    }
}
