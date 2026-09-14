// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The window-level overlays: the boot splash, the first-run walkthrough, the
//! advanced walkthrough guide, the storage dialog, and the About window.

use egui::{pos2, vec2, Color32, Context, Frame, Rect, RichText, ScrollArea, Stroke, Vec2, Window};

use crate::state::AppState;
use crate::views::guide_data::CHAPTERS;
use crate::views::tour_data::TOUR_STEPS;

const WALKTHROUGH_ORDER: egui::Order = egui::Order::Foreground;
const WALKTHROUGH_WINDOW_HIGHLIGHT_ID: &str = "walkthrough_window_highlight";

/// Consistent margin, in points, between splash content and the window edge.
const SPLASH_MARGIN: f32 = 24.0;
/// Fixed height reserved at the bottom for the wordmark and the
/// author/license line beneath it, so the independently centred main symbol
/// above can never grow tall enough to overlap them.
const SPLASH_FOOTER_HEIGHT: f32 = 96.0;
/// Keep the central mark visually subordinate to the footer at ordinary
/// desktop sizes. The bounds still shrink with the client area below these
/// caps, including when Windows reports a short high-DPI client rectangle.
const SPLASH_SYMBOL_MAX_WIDTH: f32 = 460.0;
const SPLASH_SYMBOL_MAX_HEIGHT: f32 = 220.0;
const SPLASH_WORDMARK_MAX_WIDTH: f32 = 320.0;
const SPLASH_WORDMARK_MAX_HEIGHT: f32 = 56.0;

fn tr(text: &str) -> String {
    alas_i18n::t(Some(text), None).into_owned()
}

fn tr_fields(template: &str, fields: &[(&str, String)]) -> String {
    fields.iter().fold(tr(template), |text, (name, value)| {
        text.replace(&format!("{{{name}}}"), value)
    })
}

/// Render the boot splash while `boot_frames_remaining` is still counting down.
///
/// The main three-stripe symbol and the smaller symbol/wordmark footer are
/// positioned independently -- one centred in the full client area, the
/// other anchored to the bottom with a consistent margin -- rather than as
/// one fused composite image, so each keeps sensible proportions as the
/// window is resized instead of both clustering toward the top.
pub fn show_splash(state: &mut AppState, ctx: &Context) {
    if state.boot_frames_remaining == 0 {
        return;
    }
    state.boot_frames_remaining -= 1;
    egui::CentralPanel::default().show(ctx, |ui| {
        let panel = ui.max_rect();

        let footer_top = (panel.max.y - SPLASH_FOOTER_HEIGHT).max(panel.min.y);
        let footer_rect = Rect::from_min_max(
            pos2(panel.min.x, footer_top),
            pos2(panel.max.x, panel.max.y - SPLASH_MARGIN),
        );
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(footer_rect), |ui| {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.label(author_license_line());
                ui.add_space(6.0);
                if let Some(image) = crate::branding::text_logo_image(ctx) {
                    let width =
                        (panel.width() - 2.0 * SPLASH_MARGIN).clamp(1.0, SPLASH_WORDMARK_MAX_WIDTH);
                    ui.add(image.max_size(vec2(width, SPLASH_WORDMARK_MAX_HEIGHT)));
                }
            });
        });

        // Centred on the full client area, but capped to whichever of the
        // top or bottom clearance is tighter so it can never reach the
        // footer above, even in a short window.
        if let (Some(image), Some(natural)) = (
            crate::branding::logo_image(ctx),
            crate::branding::logo_natural_size(),
        ) {
            let size = splash_symbol_size(natural, panel, footer_top);
            ui.put(Rect::from_center_size(panel.center(), size), image);
        }
    });
}

/// Choose a capped symbol size that remains centred while leaving the footer
/// clear on both sides of the client area.
fn splash_symbol_size(natural: Vec2, panel: Rect, footer_top: f32) -> Vec2 {
    let top_half = (panel.center().y - panel.min.y - SPLASH_MARGIN).max(0.0);
    let bottom_half = (footer_top - SPLASH_MARGIN - panel.center().y).max(0.0);
    let bounds = vec2(
        (panel.width() - 2.0 * SPLASH_MARGIN).clamp(1.0, SPLASH_SYMBOL_MAX_WIDTH),
        (2.0 * top_half.min(bottom_half)).clamp(1.0, SPLASH_SYMBOL_MAX_HEIGHT),
    );
    fit_within(natural, bounds)
}

/// The largest size with `natural`'s aspect ratio that still fits within
/// `bounds` on both axes.
fn fit_within(natural: Vec2, bounds: Vec2) -> Vec2 {
    if natural.x <= 0.0 || natural.y <= 0.0 || bounds.x <= 0.0 || bounds.y <= 0.0 {
        return Vec2::ZERO;
    }
    natural * (bounds.x / natural.x).min(bounds.y / natural.y)
}

/// The splash footer's author/license line, built from this crate's own
/// `Cargo.toml` metadata (workspace `authors`/`license`) rather than a second
/// hard-coded copy of it. Not routed through the translation catalog: a
/// personal name and an SPDX license identifier have no Spanish equivalent,
/// the same treatment the About window already gives this metadata below.
fn author_license_line() -> String {
    let authors = env!("CARGO_PKG_AUTHORS");
    let author = authors
        .split_once('<')
        .map_or(authors, |(name, _)| name)
        .trim();
    format!("{author} \u{b7} {}", env!("CARGO_PKG_LICENSE"))
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
    use super::{
        author_license_line, fit_within, splash_symbol_size, walkthrough_panel_position,
        SPLASH_FOOTER_HEIGHT, SPLASH_MARGIN, SPLASH_SYMBOL_MAX_HEIGHT, SPLASH_SYMBOL_MAX_WIDTH,
        WALKTHROUGH_ORDER, WALKTHROUGH_WINDOW_HIGHLIGHT_ID,
    };
    use egui::{pos2, vec2, Rect, Vec2};

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

    #[test]
    fn fit_within_preserves_aspect_ratio_on_the_tighter_axis() {
        // A wide source in a square box: width is the binding constraint.
        let size = fit_within(vec2(1000.0, 400.0), vec2(500.0, 500.0));
        assert!((size.x - 500.0).abs() < 1e-6);
        assert!((size.y - 200.0).abs() < 1e-6);

        // The same source in a short, wide box: height binds instead.
        let size = fit_within(vec2(1000.0, 400.0), vec2(500.0, 100.0));
        assert!((size.x - 250.0).abs() < 1e-6);
        assert!((size.y - 100.0).abs() < 1e-6);
    }

    #[test]
    fn fit_within_degrades_to_zero_for_a_degenerate_input() {
        assert_eq!(fit_within(vec2(0.0, 400.0), vec2(500.0, 500.0)), Vec2::ZERO);
        assert_eq!(
            fit_within(vec2(1000.0, 400.0), vec2(0.0, 500.0)),
            Vec2::ZERO
        );
    }

    #[test]
    fn author_license_line_names_the_author_and_license_without_the_email() {
        let line = author_license_line();
        assert_eq!(line, "Marcos Quiroga Rodriguez \u{b7} AGPL-3.0-or-later");
        assert!(!line.contains('@'));
        assert!(!line.contains('<'));
    }

    #[test]
    fn splash_symbol_is_capped_and_keeps_the_footer_clear() {
        let panel = Rect::from_min_size(pos2(0.0, 0.0), vec2(1_280.0, 820.0));
        let footer_top = panel.max.y - SPLASH_FOOTER_HEIGHT;
        let natural = vec2(1_511.0, 692.0);
        let size = splash_symbol_size(natural, panel, footer_top);
        let symbol = Rect::from_center_size(panel.center(), size);

        assert_eq!(symbol.center(), panel.center());
        assert!(size.x <= SPLASH_SYMBOL_MAX_WIDTH + f32::EPSILON);
        assert!(size.y <= SPLASH_SYMBOL_MAX_HEIGHT + f32::EPSILON);
        assert!(symbol.min.y >= panel.min.y + SPLASH_MARGIN - f32::EPSILON);
        assert!(symbol.max.y <= footer_top - SPLASH_MARGIN + f32::EPSILON);
    }

    #[test]
    fn splash_symbol_shrinks_for_a_short_client_height() {
        let panel = Rect::from_min_size(pos2(0.0, 0.0), vec2(880.0, 320.0));
        let footer_top = panel.max.y - SPLASH_FOOTER_HEIGHT;
        let size = splash_symbol_size(vec2(1_511.0, 692.0), panel, footer_top);
        let symbol = Rect::from_center_size(panel.center(), size);

        assert!(symbol.min.y >= panel.min.y + SPLASH_MARGIN - f32::EPSILON);
        assert!(symbol.max.y <= footer_top - SPLASH_MARGIN + f32::EPSILON);
        assert!(size.y < SPLASH_SYMBOL_MAX_HEIGHT);
    }
}
