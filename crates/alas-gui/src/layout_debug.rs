// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Development-only layout diagnostics for the native desktop shell.
//!
//! This module is compiled only when debug assertions are enabled. It paints
//! the rectangles ALAS gives to egui panels and important scrollable regions,
//! while also exposing egui's own widget and viewport inspectors. Keeping the
//! instrumentation here prevents a diagnostic surface from becoming part of
//! the packaged application or changing production layout decisions.

use egui::{
    Align2, Color32, Context, Grid, Id, Key, LayerId, Order, Pos2, Rect, RichText, ScrollArea,
    Stroke, Ui, Vec2, Window,
};

const FRAME_ID: &str = "alas_layout_debug_frame";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegionKind {
    Window,
    Viewport,
    Available,
    Menu,
    Navigation,
    Rail,
    Content,
    Preview,
    RunLog,
    Controls,
    Scroll,
    Figure,
    Clip,
}

impl RegionKind {
    fn color(self) -> Color32 {
        match self {
            Self::Window => Color32::from_rgb(255, 255, 255),
            Self::Viewport => Color32::from_rgb(255, 128, 0),
            Self::Available => Color32::from_rgb(255, 0, 255),
            Self::Menu => Color32::from_rgb(255, 96, 0),
            Self::Navigation => Color32::from_rgb(0, 170, 255),
            Self::Rail => Color32::from_rgb(170, 0, 255),
            Self::Content => Color32::from_rgb(0, 255, 96),
            Self::Preview => Color32::from_rgb(0, 235, 235),
            Self::RunLog => Color32::from_rgb(255, 220, 0),
            Self::Controls => Color32::from_rgb(255, 0, 150),
            Self::Scroll => Color32::from_rgb(120, 255, 0),
            Self::Figure => Color32::from_rgb(255, 64, 128),
            Self::Clip => Color32::from_rgb(255, 32, 32),
        }
    }

    fn draws_fill(self) -> bool {
        !matches!(self, Self::Clip | Self::Window | Self::Viewport)
    }

    fn draws_label(self) -> bool {
        !matches!(self, Self::Clip | Self::Viewport)
    }

    fn stroke_width(self) -> f32 {
        if matches!(self, Self::Clip) {
            1.0
        } else {
            2.0
        }
    }
}

#[derive(Clone, Debug)]
struct Region {
    label: String,
    rect: Rect,
    kind: RegionKind,
}

#[derive(Clone, Debug, Default)]
struct FrameData {
    enabled: bool,
    show_clips: bool,
    regions: Vec<Region>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LayoutDebug {
    enabled: bool,
    show_fills: bool,
    show_labels: bool,
    show_clips: bool,
    show_widget_hover: bool,
    show_interactive_widgets: bool,
    show_expand_width: bool,
    show_expand_height: bool,
    show_unaligned: bool,
    show_inspector: bool,
    show_memory: bool,
}

impl Default for LayoutDebug {
    fn default() -> Self {
        Self {
            enabled: false,
            show_fills: true,
            show_labels: true,
            show_clips: true,
            show_widget_hover: false,
            show_interactive_widgets: false,
            show_expand_width: false,
            show_expand_height: false,
            show_unaligned: false,
            show_inspector: false,
            show_memory: false,
        }
    }
}

impl LayoutDebug {
    /// Toggle the overlay from the development keyboard shortcut.
    pub(crate) fn handle_shortcuts(&mut self, ctx: &Context) {
        let toggle = ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, Key::L)
        });
        if toggle {
            self.enabled = !self.enabled;
        }
    }

    /// Start a fresh record of this frame and synchronize egui's own flags.
    pub(crate) fn begin_frame(&self, ctx: &Context) {
        ctx.data_mut(|data| {
            data.insert_temp(
                Id::new(FRAME_ID),
                FrameData {
                    enabled: self.enabled,
                    show_clips: self.show_clips,
                    regions: Vec::new(),
                },
            );
        });
        apply_egui_debug_options(self, ctx);
        if self.enabled {
            record(
                ctx,
                "window screen_rect",
                ctx.screen_rect(),
                RegionKind::Window,
            );
            let (inner_rect, outer_rect) =
                ctx.input(|input| (input.viewport().inner_rect, input.viewport().outer_rect));
            if let Some(rect) = inner_rect {
                record(
                    ctx,
                    "viewport inner_rect (monitor space)",
                    rect,
                    RegionKind::Viewport,
                );
            }
            if let Some(rect) = outer_rect {
                record(
                    ctx,
                    "viewport outer_rect (monitor space)",
                    rect,
                    RegionKind::Viewport,
                );
            }
        }
    }

    /// Add the menu controls and the built-in egui diagnostic switches.
    pub(crate) fn show_menu(&mut self, ui: &mut Ui) {
        ui.separator();
        ui.label(RichText::new("Development diagnostics").strong());
        ui.checkbox(&mut self.enabled, "Layout debug overlay")
            .on_hover_text("Also available with Ctrl+Shift+L.");
        ui.checkbox(&mut self.show_fills, "Tint allocated regions");
        ui.checkbox(&mut self.show_labels, "Label region rectangles");
        ui.checkbox(&mut self.show_clips, "Show egui clip rectangles");
        ui.separator();
        ui.checkbox(&mut self.show_widget_hover, "Inspect widget under pointer");
        ui.checkbox(
            &mut self.show_interactive_widgets,
            "Outline interactive widgets",
        );
        ui.checkbox(&mut self.show_expand_width, "Mark width expansion");
        ui.checkbox(&mut self.show_expand_height, "Mark height expansion");
        ui.checkbox(&mut self.show_unaligned, "Mark unaligned coordinates");
        ui.separator();
        ui.checkbox(&mut self.show_inspector, "Open egui input inspector");
        ui.checkbox(&mut self.show_memory, "Open egui area/memory inspector");
    }

    /// Paint the current frame and show the diagnostic windows.
    pub(crate) fn finish_frame(&mut self, ctx: &Context) {
        if !self.enabled {
            return;
        }
        let regions = ctx
            .data(|data| data.get_temp::<FrameData>(Id::new(FRAME_ID)))
            .map(|frame| frame.regions)
            .unwrap_or_default();
        paint_regions(ctx, &regions, self.show_fills, self.show_labels);
        show_diagnostics(self, ctx, &regions);
    }
}

/// Record a named rectangle from the current egui pass.
pub(crate) fn record(ctx: &Context, label: impl Into<String>, rect: Rect, kind: RegionKind) {
    ctx.data_mut(|data| {
        let frame = data.get_temp_mut_or_default::<FrameData>(Id::new(FRAME_ID));
        if frame.enabled && rect.is_positive() {
            frame.regions.push(Region {
                label: label.into(),
                rect,
                kind,
            });
        }
    });
}

/// Record a Ui allocation and its clip rectangle as separate visual layers.
pub(crate) fn record_ui(ctx: &Context, label: &str, ui: &Ui, kind: RegionKind) {
    record(ctx, format!("{label} allocation"), ui.max_rect(), kind);
    let show_clips = ctx
        .data(|data| data.get_temp::<FrameData>(Id::new(FRAME_ID)))
        .is_some_and(|frame| frame.enabled && frame.show_clips);
    if show_clips {
        record(
            ctx,
            format!("{label} clip"),
            ui.clip_rect(),
            RegionKind::Clip,
        );
    }
}

/// Return the geometric overlap area between two layout rectangles.
pub(crate) fn overlap_area(first: Rect, second: Rect) -> f32 {
    let intersection = first.intersect(second);
    if intersection.is_positive() {
        intersection.width() * intersection.height()
    } else {
        0.0
    }
}

fn apply_egui_debug_options(debug: &LayoutDebug, ctx: &Context) {
    let enabled = debug.enabled;
    ctx.set_debug_on_hover(enabled && debug.show_widget_hover);
    ctx.style_mut(|style| {
        style.debug.show_interactive_widgets = enabled && debug.show_interactive_widgets;
        style.debug.show_expand_width = enabled && debug.show_expand_width;
        style.debug.show_expand_height = enabled && debug.show_expand_height;
        style.debug.show_unaligned = enabled && debug.show_unaligned;
    });
}

fn paint_regions(ctx: &Context, regions: &[Region], show_fills: bool, show_labels: bool) {
    let painter = ctx.layer_painter(LayerId::new(
        Order::Foreground,
        Id::new("alas_layout_debug_overlay"),
    ));
    let screen = ctx.screen_rect();
    for region in regions {
        let visible = region.rect.intersect(screen);
        if !visible.is_positive() {
            continue;
        }
        let color = region.kind.color();
        if show_fills && region.kind.draws_fill() {
            painter.rect_filled(visible, 0.0, with_alpha(color, 28));
        }
        painter.rect_stroke(visible, 0.0, Stroke::new(region.kind.stroke_width(), color));
        if show_labels && region.kind.draws_label() {
            let position = visible.min + label_offset(region.kind);
            painter.debug_text(position, Align2::LEFT_TOP, color, &region.label);
        }
    }
}

fn show_diagnostics(debug: &mut LayoutDebug, ctx: &Context, regions: &[Region]) {
    let screen = ctx.screen_rect();
    let physical = screen.size() * ctx.pixels_per_point();
    let (inner_rect, outer_rect, native_ppp, maximized, focused) = ctx.input(|input| {
        let viewport = input.viewport();
        (
            viewport.inner_rect,
            viewport.outer_rect,
            viewport.native_pixels_per_point,
            viewport.maximized,
            viewport.focused,
        )
    });
    let overlaps = regions
        .iter()
        .enumerate()
        .flat_map(|(index, first)| {
            regions
                .iter()
                .skip(index + 1)
                .map(move |second| (first, second))
        })
        .filter(|(first, second)| {
            !matches!(first.kind, RegionKind::Clip)
                && !matches!(second.kind, RegionKind::Clip)
                && overlap_area(first.rect, second.rect) > 0.0
        })
        .count();

    Window::new("ALAS layout diagnostics")
        .id(Id::new("alas_layout_diagnostics_window"))
        .order(Order::Tooltip)
        .default_pos(Pos2::new(18.0, 42.0))
        .default_width(500.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.label(
                RichText::new("Debug build only: painted rectangles are egui points.")
                    .strong()
                    .color(Color32::YELLOW),
            );
            ui.horizontal(|ui| {
                ui.label(format!(
                    "logical {:.1} x {:.1}",
                    screen.width(),
                    screen.height()
                ));
                ui.label(format!(
                    "physical {:.0} x {:.0}",
                    physical.x, physical.y
                ));
            });
            ui.horizontal(|ui| {
                ui.label(format!("pixels/point {:.3}", ctx.pixels_per_point()));
                ui.label(format!("zoom {:.3}", ctx.zoom_factor()));
                if let Some(native) = native_ppp {
                    ui.label(format!("native {:.3}", native));
                }
            });
            ui.label(format!(
                "areas: {}  geometric overlaps: {}  pointer layer: {}",
                regions.len(),
                overlaps,
                ctx.pointer_hover_pos()
                    .and_then(|pos| ctx.layer_id_at(pos))
                    .map_or_else(|| "none".to_owned(), |layer| layer.short_debug_format())
            ));
            ui.label(format!(
                "focused: {:?}  maximized: {:?}  inner: {}  outer: {}",
                focused,
                maximized,
                format_rect(inner_rect),
                format_rect(outer_rect)
            ));
            ui.separator();
            ui.label("Color key: window white, viewport orange, nav blue, content green, preview cyan, log yellow, controls pink, figures coral, clips red.");
            ui.separator();
            ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                Grid::new("alas_layout_diagnostics_regions")
                    .striped(true)
                    .num_columns(5)
                    .show(ui, |ui| {
                        for heading in ["Region", "X/Y", "W/H", "Outside", "Overlap"] {
                            ui.label(RichText::new(heading).strong());
                        }
                        ui.end_row();
                        for region in regions {
                            let outside = !screen.contains_rect(region.rect);
                            let overlap = regions
                                .iter()
                                .filter(|other| {
                                    !std::ptr::eq(*other, region)
                                        && overlap_area(region.rect, other.rect) > 0.0
                                })
                                .count();
                            ui.colored_label(region.kind.color(), &region.label);
                            ui.monospace(format!(
                                "{:.1}, {:.1}",
                                region.rect.min.x, region.rect.min.y
                            ));
                            ui.monospace(format!(
                                "{:.1}, {:.1}",
                                region.rect.width(), region.rect.height()
                            ));
                            if outside {
                                ui.colored_label(Color32::RED, "YES");
                            } else {
                                ui.label("no");
                            }
                            if overlap > 0 {
                                ui.colored_label(Color32::YELLOW, overlap.to_string());
                            } else {
                                ui.label("0");
                            }
                            ui.end_row();
                        }
                    });
            });
        });

    let mut show_inspector = debug.show_inspector;
    if show_inspector {
        Window::new("egui input inspector")
            .id(Id::new("alas_egui_input_inspector"))
            .open(&mut show_inspector)
            .resizable(true)
            .show(ctx, |ui| ctx.inspection_ui(ui));
    }
    debug.show_inspector = show_inspector;
    let mut show_memory = debug.show_memory;
    if show_memory {
        Window::new("egui areas and memory")
            .id(Id::new("alas_egui_memory_inspector"))
            .open(&mut show_memory)
            .resizable(true)
            .show(ctx, |ui| ctx.memory_ui(ui));
    }
    debug.show_memory = show_memory;
}

fn label_offset(kind: RegionKind) -> Vec2 {
    match kind {
        RegionKind::Window | RegionKind::Menu | RegionKind::Navigation | RegionKind::Rail => {
            Vec2::new(4.0, 4.0)
        }
        RegionKind::Available | RegionKind::Content | RegionKind::Preview => Vec2::new(4.0, 22.0),
        RegionKind::RunLog | RegionKind::Controls | RegionKind::Scroll | RegionKind::Figure => {
            Vec2::new(4.0, 4.0)
        }
        RegionKind::Viewport | RegionKind::Clip => Vec2::ZERO,
    }
}

fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

fn format_rect(rect: Option<Rect>) -> String {
    rect.map_or_else(
        || "unknown".to_owned(),
        |rect| {
            format!(
                "{:.1},{:.1} {:.1}x{:.1}",
                rect.min.x,
                rect.min.y,
                rect.width(),
                rect.height()
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::overlap_area;
    use egui::{pos2, vec2, Rect};

    #[test]
    fn overlap_area_reports_only_the_positive_intersection() {
        let first = Rect::from_min_size(pos2(10.0, 10.0), vec2(100.0, 40.0));
        let second = Rect::from_min_size(pos2(80.0, 20.0), vec2(100.0, 40.0));

        assert_eq!(overlap_area(first, second), 30.0 * 30.0);
        assert_eq!(
            overlap_area(
                first,
                Rect::from_min_size(pos2(200.0, 200.0), vec2(2.0, 2.0))
            ),
            0.0
        );
    }
}
