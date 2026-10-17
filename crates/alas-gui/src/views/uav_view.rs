// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fixed-wing UAV competition page: the preliminary banner, the workflow
//! tabs, the inputs of each section and the result of the last search.

use alas_uav::optimizer::GeneratedGeometry;
use alas_uav::UavDesign;
use egui::{Color32, ScrollArea, Sense, Stroke, Ui, Vec2};

use crate::state::AppState;
use crate::uav::{UavSection, UavWorkflowOutcome, UavWorkflowState};

use super::tr;

mod airframe;
mod airframe_results;
mod competition_results;
mod presentation;
#[cfg(debug_assertions)]
mod preview;
mod results;
mod sections;
mod uav_fields;
mod workflow;

use airframe::airframe_inputs;
use competition_results::competition_result;
use presentation::card;
use sections::mission_inputs;
use workflow::{action_bar, execution_status, preliminary_banner, section_intro, workflow_header};

/// Render the complete fixed-wing UAV workflow.
pub fn show_uav_view(state: &mut AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .id_salt("uav_workflow_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            preliminary_banner(ui);
            ui.add_space(8.0);
            workflow_header(&mut state.uav, ui);
            ui.add_space(8.0);

            let active_section = state.uav.active_section;
            section_intro(active_section, ui);
            ui.add_space(8.0);
            let running = state.uav.is_running();
            match active_section {
                UavSection::Mission => {
                    ui.add_enabled_ui(!running, |ui| mission_inputs(&mut state.uav, ui));
                }
                UavSection::Airframe => {
                    ui.add_enabled_ui(!running, |ui| airframe_inputs(&mut state.uav, ui));
                }
                UavSection::Results => outcome(&state.uav, ui),
            }
            ui.add_space(8.0);
            action_bar(&mut state.uav, ui);
            ui.add_space(8.0);
            execution_status(&state.uav, ui);
            // Developer builds only; a release build compiles none of it.
            #[cfg(debug_assertions)]
            {
                ui.add_space(8.0);
                preview::dev_preview(ui);
            }
        });
}

fn outcome(state: &UavWorkflowState, ui: &mut Ui) {
    match &state.outcome {
        UavWorkflowOutcome::NotRun => {
            card(ui, "UAV result", None, |ui| {
                ui.weak(tr("No UAV search has been run."));
            });
        }
        UavWorkflowOutcome::Competition(result) => competition_result(result, ui),
    }
}

/// Planform and installed-layout figure of one generated airframe.
pub(super) fn layout_plot_geometry(geometry: GeneratedGeometry, design: &UavDesign, ui: &mut Ui) {
    card(ui, "Generated planform and installed layout", None, |ui| {
        let available_width = ui.available_width().max(320.0);
        let desired = Vec2::new(available_width, 300.0);
        let (response, painter) = ui.allocate_painter(desired, Sense::hover());
        let rect = response.rect.shrink(16.0);
        let length = geometry.fuselage.length_m;
        let span = geometry.wing.span_m;
        let scale = (rect.width() / length as f32).min(rect.height() / span as f32);
        let center = rect.center();
        let point = |x_m: f64, y_m: f64| {
            egui::pos2(
                center.x + (x_m - 0.5 * length) as f32 * scale,
                center.y - y_m as f32 * scale,
            )
        };
        let line_color = ui.visuals().text_color();
        let accent = ui.visuals().hyperlink_color;
        let weak = ui.visuals().weak_text_color();
        let wing = geometry.wing;
        let wing_front = wing.leading_edge_x_m;
        let wing_back = wing_front + wing.mean_chord_m;
        painter.rect_stroke(
            egui::Rect::from_two_pos(point(wing_front, -0.5 * span), point(wing_back, 0.5 * span)),
            0.0,
            Stroke::new(2.0_f32, accent),
        );
        let fuselage_half = 0.5 * geometry.fuselage.diameter_m;
        painter.rect_filled(
            egui::Rect::from_two_pos(point(0.0, -fuselage_half), point(length, fuselage_half)),
            3.0,
            Color32::from_rgba_unmultiplied(weak.r(), weak.g(), weak.b(), 80),
        );
        let tail = geometry.empennage;
        if tail.horizontal_area_m2 > 0.0 && tail.horizontal_span_m > 0.0 {
            let tail_x = 0.9 * length;
            let h_chord = tail.horizontal_area_m2 / tail.horizontal_span_m;
            painter.rect_stroke(
                egui::Rect::from_two_pos(
                    point(tail_x - 0.25 * h_chord, -0.5 * tail.horizontal_span_m),
                    point(tail_x + 0.75 * h_chord, 0.5 * tail.horizontal_span_m),
                ),
                0.0,
                Stroke::new(1.5_f32, line_color),
            );
        }
        let main_x = wing.leading_edge_x_m + 0.25 * wing.mean_chord_m;
        let nose_x = main_x - geometry.landing_gear.wheelbase_m;
        for (x_m, y_m) in [
            (main_x, -0.5 * geometry.landing_gear.track_m),
            (main_x, 0.5 * geometry.landing_gear.track_m),
            (nose_x, 0.0),
        ] {
            painter.circle_filled(point(x_m, y_m), 4.0, line_color);
        }
        for placement in std::iter::once(design.battery.placement)
            .chain(std::iter::once(design.motor.placement))
            .chain(std::iter::once(design.esc.placement))
            .chain(std::iter::once(design.propeller.placement))
            .chain(design.additional_propulsors.iter().flat_map(|propulsor| {
                [
                    propulsor.motor.placement,
                    propulsor.esc.placement,
                    propulsor.propeller.placement,
                ]
            }))
            .chain(design.servos.iter().map(|item| item.placement))
            .chain(design.other_items.iter().map(|item| item.placement))
        {
            painter.circle_filled(
                point(placement.center_x_m, placement.center_y_m),
                3.0,
                ui.visuals().warn_fg_color,
            );
        }
    });
}
