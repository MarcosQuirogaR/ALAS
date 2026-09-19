// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The bottom control bar: the run group (Analyze reference / Run), and the
//! status/elapsed readout.
//!
//! A port of the reference desktop app's `ControlBar`.

use egui::{RichText, Ui};

use crate::state::AppState;
use crate::views::tour_data::TourTarget;
use crate::views::tr;
use alas_config::DesignMode;

/// Render the control bar.
pub fn show_control_bar(state: &mut AppState, ui: &mut Ui) {
    // A launcher needs breathing room from the run-log divider and window
    // edge; otherwise its controls look visually welded to both surfaces.
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let running = state.is_running;

        let baseline_mode = state.design_mode() == DesignMode::BaselineSandbox;

        let baseline_response = ui
            .add_enabled(!running, egui::Button::new(tr("Analyze reference")))
            .on_hover_text(tr(
                "Analyze the selected reference aircraft and load case with no redesign",
            ));
        if baseline_response.clicked() {
            state.start_pipeline(true);
        }

        let blocked = state.blocked();
        let run_button = egui::Button::new(
            RichText::new(tr(if running { "Running..." } else { "Run" })).strong(),
        );
        let run_response = ui
            .add_enabled(!running && !blocked, run_button)
            .on_hover_text(if blocked {
                crate::views::notices::run_blocked_hover_text(state)
            } else if baseline_mode {
                tr("Analyze the current design and run the mission without optimization")
            } else {
                tr("Optimize, analyze and run the mission in one pass")
            });
        if run_response.clicked() {
            state.start_pipeline(false);
        }
        // A disabled button explains nothing on its own: say beside it that
        // something blocks the run, and carry the reasons in its hover text.
        if blocked {
            crate::views::notices::show_run_blocked_marker(state, ui);
        }
        if running {
            let cancel = ui
                .add_enabled(
                    !state.cancellation_requested,
                    egui::Button::new(tr(if state.cancellation_requested {
                        "Cancelling..."
                    } else {
                        "Cancel"
                    })),
                )
                .on_hover_text(tr(
                    "Stop at the next safe stage boundary; active external tools finish first",
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
