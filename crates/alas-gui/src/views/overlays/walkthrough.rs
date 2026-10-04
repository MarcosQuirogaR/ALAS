// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The first-run walkthrough: spotlight scrim and step panel.

use egui::{pos2, vec2, Color32, Context, Frame, Rect, RichText, Stroke, Window};

use super::{tr, tr_fields};
use crate::state::AppState;
use crate::views::tour_data::TOUR_STEPS;

pub(super) const WALKTHROUGH_ORDER: egui::Order = egui::Order::Foreground;
pub(super) const WALKTHROUGH_WINDOW_HIGHLIGHT_ID: &str = "walkthrough_window_highlight";

/// Render the first-run walkthrough, if it is open.
pub fn show_walkthrough(state: &mut AppState, ctx: &Context) {
    if !state.show_walkthrough {
        return;
    }
    let step_index = state
        .walkthrough_step
        .min(TOUR_STEPS.len().saturating_sub(1));
    let Some(step) = TOUR_STEPS.get(step_index) else {
        state.finish_walkthrough();
        return;
    };

    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        state.finish_walkthrough();
        return;
    }
    if step_index > 0 && ctx.input(|input| input.key_pressed(egui::Key::ArrowLeft)) {
        state.walkthrough_step -= 1;
        state.prepare_walkthrough_step();
        ctx.request_repaint();
        return;
    }

    let screen = ctx.screen_rect();
    let target = state
        .current_walkthrough_target()
        .map(|rect| rect.expand(8.0).intersect(screen));
    egui::Area::new("walkthrough_scrim".into())
        .fixed_pos(screen.min)
        .order(WALKTHROUGH_ORDER)
        // The scrim is paint-only. If it participates in hit testing it can
        // consume the final tour buttons on some Windows backends.
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_min_size(screen.size());
            draw_walkthrough_scrim(ui.painter(), screen, target);
        });

    let is_last = step_index + 1 == TOUR_STEPS.len();
    let advance_pressed = ctx.input(|input| {
        input.key_pressed(egui::Key::Enter)
            || input.key_pressed(egui::Key::ArrowRight)
            || input.key_pressed(egui::Key::Space)
    });
    let panel_pos = walkthrough_panel_position(target, screen, vec2(420.0, 150.0));
    let window = Window::new(tr("Walkthrough"))
        .title_bar(false)
        .order(WALKTHROUGH_ORDER)
        .resizable(false)
        .collapsible(false)
        .fixed_pos(panel_pos)
        .fixed_size(egui::vec2(420.0, 0.0))
        .frame(
            Frame::window(&ctx.style())
                .stroke(Stroke::new(2.0_f32, Color32::from_rgb(50, 180, 255)))
                .rounding(10.0),
        )
        .show(ctx, |ui| {
            ui.label(
                RichText::new(tr_fields(
                    "Step {current} of {total}",
                    &[
                        ("current", (step_index + 1).to_string()),
                        ("total", TOUR_STEPS.len().to_string()),
                    ],
                ))
                .weak()
                .small(),
            );
            ui.label(RichText::new(tr(step.title)).strong().size(17.0))
                .on_hover_text(tr(step.body));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(tr("Skip")).clicked() {
                    state.finish_walkthrough();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let next_label = if is_last { "Get started" } else { "Next ->" };
                    if ui.button(tr(next_label)).clicked() || advance_pressed {
                        if is_last {
                            state.finish_walkthrough();
                        } else {
                            state.walkthrough_step += 1;
                            state.prepare_walkthrough_step();
                            ctx.request_repaint();
                        }
                    }
                    if step_index > 0 && ui.button(tr("<- Back")).clicked() {
                        state.walkthrough_step -= 1;
                        state.prepare_walkthrough_step();
                        ctx.request_repaint();
                    }
                });
            });
        });
    if let Some(window) = window {
        // Keep the callout visibly distinct from the window frame itself. A
        // dedicated foreground layer is intentional: the scrim is also in
        // the foreground order, and a debug painter stroke can be hidden by
        // another foreground window on some egui backends.
        let painter = ctx.layer_painter(egui::LayerId::new(
            WALKTHROUGH_ORDER,
            egui::Id::new(WALKTHROUGH_WINDOW_HIGHLIGHT_ID),
        ));
        painter.rect_stroke(
            window.response.rect.expand(5.0),
            13.0,
            Stroke::new(4.0_f32, Color32::from_rgb(19, 61, 96)),
        );
        painter.rect_stroke(
            window.response.rect.expand(3.0),
            12.0,
            Stroke::new(2.0_f32, Color32::from_rgb(91, 220, 255)),
        );
    }
}

fn draw_walkthrough_scrim(painter: &egui::Painter, screen: Rect, target: Option<Rect>) {
    let shade = Color32::from_black_alpha(160);
    if let Some(target) = target {
        let top = Rect::from_min_max(screen.min, pos2(screen.max.x, target.min.y));
        let bottom = Rect::from_min_max(pos2(screen.min.x, target.max.y), screen.max);
        let left = Rect::from_min_max(
            pos2(screen.min.x, target.min.y),
            pos2(target.min.x, target.max.y),
        );
        let right = Rect::from_min_max(
            pos2(target.max.x, target.min.y),
            pos2(screen.max.x, target.max.y),
        );
        for rect in [top, bottom, left, right] {
            painter.rect_filled(rect, 0.0, shade);
        }
        painter.rect_stroke(
            target,
            6.0,
            Stroke::new(2.5_f32, Color32::from_rgb(50, 180, 255)),
        );
    } else {
        painter.rect_filled(screen, 0.0, shade);
    }
}

pub(super) fn walkthrough_panel_position(
    target: Option<Rect>,
    screen: Rect,
    panel_size: egui::Vec2,
) -> egui::Pos2 {
    let margin = 16.0;
    let clamp_to_screen = |candidate: egui::Pos2| {
        pos2(
            candidate
                .x
                .clamp(screen.min.x + margin, screen.max.x - panel_size.x - margin),
            candidate
                .y
                .clamp(screen.min.y + margin, screen.max.y - panel_size.y - margin),
        )
    };
    let centered = clamp_to_screen(pos2(
        screen.center().x - panel_size.x * 0.5,
        screen.center().y - panel_size.y * 0.5,
    ));
    let Some(target) = target else {
        return centered;
    };
    let below = target.max.y + 14.0;
    let y = if below + panel_size.y <= screen.max.y - margin {
        below
    } else {
        (target.min.y - panel_size.y - 14.0).max(screen.min.y + margin)
    };
    clamp_to_screen(pos2(
        target
            .min
            .x
            .clamp(screen.min.x + margin, screen.max.x - panel_size.x - margin),
        y,
    ))
}
