// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The window-level overlays: the boot splash, the first-run walkthrough, the
//! advanced walkthrough guide, the storage dialog, and the About window.

use egui::{pos2, vec2, Color32, Context, Frame, Rect, RichText, ScrollArea, Stroke, Window};

use crate::state::AppState;
use crate::views::guide_data::CHAPTERS;
use crate::views::tour_data::TOUR_STEPS;

const WALKTHROUGH_ORDER: egui::Order = egui::Order::Foreground;
const WALKTHROUGH_WINDOW_HIGHLIGHT_ID: &str = "walkthrough_window_highlight";

fn tr(text: &str) -> String {
    alas_i18n::t(Some(text), None).into_owned()
}

fn tr_fields(template: &str, fields: &[(&str, String)]) -> String {
    fields.iter().fold(tr(template), |text, (name, value)| {
        text.replace(&format!("{{{name}}}"), value)
    })
}

/// Render the boot splash while `boot_frames_remaining` is still counting down.
pub fn show_splash(state: &mut AppState, ctx: &Context) {
    if state.boot_frames_remaining == 0 {
        return;
    }
    state.boot_frames_remaining -= 1;
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                if let Some(image) = crate::branding::composite_image(ctx) {
                    ui.add(image.max_size(vec2(460.0, 290.0)));
                }
                ui.label(tr("Aircraft Layout and Analysis Suite"));
            });
        });
    });
}

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
    // The Spanish copy can be taller than the English source. Reserve the
    // maximum practical callout height when choosing a position so its buttons
    // never land beyond the visible window on the final steps.
    let panel_pos = walkthrough_panel_position(target, screen, vec2(420.0, 280.0));
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
            ui.label(RichText::new(tr(step.title)).strong().size(17.0));
            ui.add_space(4.0);
            ui.label(tr(step.body));
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

fn walkthrough_panel_position(
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

/// Render the advanced walkthrough guide window, if it is open.
pub fn show_advanced_guide(state: &mut AppState, ctx: &Context) {
    if !state.show_advanced_guide {
        return;
    }
    let mut open = true;
    Window::new(tr("Advanced Walkthrough"))
        .open(&mut open)
        .default_size(egui::vec2(880.0, 620.0))
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(220.0);
                    ScrollArea::vertical().id_salt("guide_nav").show(ui, |ui| {
                        for (i, chapter) in CHAPTERS.iter().enumerate() {
                            let selected = state.guide_chapter == i;
                            if ui
                                .selectable_label(
                                    selected,
                                    format!("{}. {}", i + 1, tr(chapter.title)),
                                )
                                .on_hover_text(tr(chapter.blurb))
                                .clicked()
                            {
                                state.guide_chapter = i;
                            }
                        }
                    });
                });
                ui.separator();
                ui.vertical(|ui| {
                    let chapter = &CHAPTERS[state.guide_chapter.min(CHAPTERS.len() - 1)];
                    ScrollArea::vertical()
                        .id_salt("guide_content")
                        .show(ui, |ui| {
                            ui.heading(tr(chapter.title));
                            for section in chapter.sections {
                                ui.add_space(8.0);
                                ui.label(RichText::new(tr(section.heading)).strong());
                                for para in section.body {
                                    ui.add_space(4.0);
                                    ui.label(tr(para));
                                }
                            }
                            ui.add_space(16.0);
                            ui.horizontal(|ui| {
                                if state.guide_chapter > 0 && ui.button(tr("<- Back")).clicked() {
                                    state.guide_chapter -= 1;
                                }
                                ui.label(format!(
                                    "{} / {}",
                                    state.guide_chapter + 1,
                                    CHAPTERS.len()
                                ));
                                if state.guide_chapter + 1 < CHAPTERS.len()
                                    && ui.button(tr("Next ->")).clicked()
                                {
                                    state.guide_chapter += 1;
                                }
                            });
                        });
                });
            });
        });
    state.show_advanced_guide = open;
}

/// Render the storage-management dialog, if it is open.
///
/// The reference's dialog is backed by an HTTP maintenance endpoint tracking
/// extracted-runtime caches this port has no counterpart for (it is one
/// binary, nothing to extract); the one real reclaimable location here is the
/// pipeline's own output directory, which this offers to clear directly.
pub fn show_storage_dialog(state: &mut AppState, ctx: &Context) {
    if !state.show_storage {
        return;
    }
    let mut open = true;
    Window::new(tr("Manage storage"))
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .show(ctx, |ui| {
            let out_dir = state
                .pipeline_options
                .output_dir
                .clone()
                .unwrap_or_else(|| std::path::PathBuf::from("outputs"));
            let (exists, size) = directory_size(&out_dir);
            ui.label(tr_fields(
                "Output directory: {path}",
                &[("path", out_dir.display().to_string())],
            ));
            ui.label(if exists {
                tr_fields("{size} on disk", &[("size", format_bytes(size))])
            } else {
                tr("Not created yet.")
            });
            ui.add_space(8.0);
            ui.add_enabled_ui(exists, |ui| {
                if ui.button(tr("Clear exported outputs")).clicked() {
                    match std::fs::remove_dir_all(&out_dir) {
                        Ok(()) => state.log(
                            tr_fields(
                                "Cleared {path}.",
                                &[("path", out_dir.display().to_string())],
                            ),
                            crate::state::LogKind::Info,
                        ),
                        Err(e) => state.log(
                            tr_fields(
                                "Could not clear outputs: {error}",
                                &[("error", e.to_string())],
                            ),
                            crate::state::LogKind::Error,
                        ),
                    }
                }
            });
        });
    state.show_storage = open;
}

/// Render the About window, if it is open.
pub fn show_about(state: &mut AppState, ctx: &Context) {
    if !state.show_about {
        return;
    }
    let mut open = true;
    Window::new(tr("About ALAS"))
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .show(ctx, |ui| {
            if let Some(image) = crate::branding::text_logo_image(ctx) {
                ui.add(image.max_size(vec2(300.0, 40.0)));
            }
            ui.add_space(8.0);
            ui.label(tr(
                "Conceptual transport aircraft sizing, optimization, and multi-disciplinary analysis.",
            ));
            ui.add_space(8.0);
            ui.label("SPDX-License-Identifier: AGPL-3.0-or-later");
            ui.label("Copyright (C) 2026 Marcos Quiroga Rodriguez");
        });
    state.show_about = open;
}

fn directory_size(dir: &std::path::Path) -> (bool, u64) {
    if !dir.exists() {
        return (false, 0);
    }
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    (true, total)
}

fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::{walkthrough_panel_position, WALKTHROUGH_ORDER, WALKTHROUGH_WINDOW_HIGHLIGHT_ID};
    use egui::{pos2, vec2, Rect};

    #[test]
    fn walkthrough_panel_moves_below_a_target_when_room_exists() {
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1_280.0, 820.0));
        let target = Rect::from_min_size(pos2(20.0, 40.0), vec2(220.0, 180.0));
        let position = walkthrough_panel_position(Some(target), screen, vec2(420.0, 220.0));

        assert!(position.y > target.max.y);
        assert!(position.x >= 16.0);
    }

    #[test]
    fn walkthrough_panel_moves_above_a_low_target() {
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1_280.0, 820.0));
        let target = Rect::from_min_size(pos2(20.0, 690.0), vec2(900.0, 100.0));
        let position = walkthrough_panel_position(Some(target), screen, vec2(420.0, 220.0));

        assert!(position.y + 220.0 < target.min.y);
    }

    #[test]
    fn walkthrough_panel_stays_clickable_on_a_short_window() {
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(880.0, 560.0));
        let target = Rect::from_min_size(pos2(10.0, 390.0), vec2(860.0, 100.0));
        let position = walkthrough_panel_position(Some(target), screen, vec2(420.0, 280.0));

        assert!(position.y >= 16.0);
        assert!(position.y + 280.0 <= screen.max.y - 16.0);
    }

    #[test]
    fn overlay_shell_strings_have_spanish_desktop_translations() {
        let catalog = alas_i18n::es::desktop_catalog();
        for key in [
            "Aircraft Layout and Analysis Suite",
            "Walkthrough",
            "Step {current} of {total}",
            "Skip",
            "Get started",
            "Next ->",
            "<- Back",
            "Advanced Walkthrough",
            "Manage storage",
            "Output directory: {path}",
            "{size} on disk",
            "Not created yet.",
            "Clear exported outputs",
            "Cleared {path}.",
            "Could not clear outputs: {error}",
            "About ALAS",
            "ALAS - Aircraft Layout and Analysis Suite",
            "Conceptual transport aircraft sizing, optimization, and multi-disciplinary analysis.",
        ] {
            assert!(catalog.contains_key(key), "missing overlay text: {key}");
        }
    }

    #[test]
    fn walkthrough_scrim_and_later_window_use_the_foreground_layer() {
        assert_eq!(WALKTHROUGH_ORDER, egui::Order::Foreground);
    }

    #[test]
    fn walkthrough_window_highlight_has_a_dedicated_foreground_layer() {
        let layer = egui::LayerId::new(
            WALKTHROUGH_ORDER,
            egui::Id::new(WALKTHROUGH_WINDOW_HIGHLIGHT_ID),
        );
        assert_eq!(layer.order, egui::Order::Foreground);
        assert_eq!(layer.id, egui::Id::new(WALKTHROUGH_WINDOW_HIGHLIGHT_ID));
    }
}
