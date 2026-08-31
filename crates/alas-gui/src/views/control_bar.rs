// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The bottom control bar: the randomizer group (DOE Sample / Random), the
//! run group (Analyze baseline / Run), and the status/elapsed readout.
//!
//! A port of the reference desktop app's `ControlBar`.

use egui::{RichText, Ui};

use crate::state::{AppState, LogKind};
use crate::views::tour_data::TourTarget;
use crate::views::tr;

/// Render the control bar.
pub fn show_control_bar(state: &mut AppState, ui: &mut Ui) {
    // A launcher needs breathing room from the run-log divider and window
    // edge; otherwise its controls look visually welded to both surfaces.
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let running = state.is_running;

        let doe_response = ui
            .add_enabled(!running, egui::Button::new(tr("DOE Sample")))
            .on_hover_text(tr("Draw one design point within the Design Space bounds"));
        if doe_response.clicked() {
            let sample = state.sample_design(0.0);
            state.design_values = sample;
            state.log(
                tr("DOE Sample: wrote a new design point to the Initial Value column."),
                LogKind::Info,
            );
            state.active_page = "design_space".to_owned();
        }

        let surprise_response = ui
            .add_enabled(!running, egui::Button::new(tr("Random")))
            .on_hover_text(tr(
                "Draw a design +/-30% beyond the bounds, then run the full pipeline",
            ));
        if surprise_response.clicked() {
            let sample = state.sample_design(0.3);
            state.design_values = sample;
            state.log(
                tr("Random: sampled +/-30% beyond the bounds, starting a run."),
                LogKind::Info,
            );
            state.start_pipeline(false);
        }
        state.record_walkthrough_target(
            TourTarget::Randomizer,
            doe_response.rect.union(surprise_response.rect),
        );

        ui.separator();

        let baseline_response = ui
            .add_enabled(!running, egui::Button::new(tr("Analyze baseline")))
            .on_hover_text(tr(
                "Weight and balance + stability of the current design, no optimizer",
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
                tr("Fix error-severity validation issues first")
            } else {
                tr("Optimize, analyze and run the mission in one pass")
            });
        if run_response.clicked() {
            state.start_pipeline(false);
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
    fn walkthrough_records_distinct_button_group_rectangles() {
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

        let randomizer = state.walkthrough_targets[&TourTarget::Randomizer];
        let run = state.walkthrough_targets[&TourTarget::Run];
        assert!(randomizer.max.x < run.min.x);
        assert!(randomizer.width() < 300.0);
        assert!(run.width() < 300.0);
    }
}
