// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Advanced Settings > Mission Analysis phase launcher.
//!
//! The twelve solver phases are grouped by flight segment so the card reads
//! as the mission it describes (departure, cruise with its step climbs,
//! descent and approach) instead of one undifferentiated row of buttons.

use egui::{Grid, RichText, Ui};

use super::{open_phase_window, reconcile_route_profile, PHASES};
use crate::state::AppState;
use crate::theme::card_frame;
use crate::views::tr;

/// Flight segment of a solver phase, by its stable identifier.
fn segment_of(phase_id: &str) -> &'static str {
    if phase_id.starts_with("cruise") || phase_id.starts_with("step_climb") {
        "Cruise"
    } else if phase_id.starts_with("descent") || phase_id == "final_approach" {
        "Descent and approach"
    } else {
        "Departure"
    }
}

const SEGMENTS: &[&str] = &["Departure", "Cruise", "Descent and approach"];

/// Render phase-launch buttons in the detached Mission Advanced Settings tab.
/// The advanced tab owns the phase navigation, while the live profile and its
/// route cards remain on Setup > Inputs.
pub(crate) fn show_mission_profile_advanced(state: &mut AppState, ui: &mut Ui) {
    reconcile_route_profile(state);
    ui.add_space(6.0);
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr("Mission phases")).strong());
        ui.label(
            RichText::new(tr(
                "Select a phase to edit its speed, climb or descent rate and altitude in its own window.",
            ))
            .weak(),
        );
        ui.add_space(6.0);
        Grid::new("mission_phase_launcher")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                for segment in SEGMENTS {
                    ui.label(RichText::new(tr(segment)).weak());
                    ui.horizontal_wrapped(|ui| {
                        for phase in PHASES.iter().filter(|p| segment_of(p.id) == *segment) {
                            let selected = state.mission_profile_window.open
                                && state.mission_profile_window.phase_id.as_deref()
                                    == Some(phase.id);
                            if ui
                                .add(crate::theme::selectable_button(tr(phase.title), selected))
                                .clicked()
                            {
                                open_phase_window(state, phase.id);
                            }
                        }
                    });
                    ui.end_row();
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::{segment_of, PHASES, SEGMENTS};

    #[test]
    fn every_phase_belongs_to_one_listed_segment_in_flight_order() {
        let segments: Vec<&str> = PHASES.iter().map(|phase| segment_of(phase.id)).collect();
        for segment in &segments {
            assert!(SEGMENTS.contains(segment));
        }
        // Flight order: segments never interleave.
        let mut order: Vec<&str> = segments.clone();
        order.dedup();
        assert_eq!(order, SEGMENTS);
    }
}
