// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Results content geometry matching the desktop's navigation/dock panels.

use crate::{layout, state::AppState};
use egui::{Context, Event, FullOutput, Rect};

pub(super) struct Case {
    pub name: &'static str,
    size: [f32; 2],
    navigation: Option<f32>,
    preview: bool,
    maximum_preview: bool,
}

// Shipped minimum/startup windows (`lib.rs`) at 100% zoom, plus the real
// resizable panel endpoints from `layout.rs`; no guessed content widths.
pub(super) const CASES: &[Case] = &[
    Case {
        name: "minimum_window",
        size: [880.0, 560.0],
        navigation: Some(layout::NAV_PANEL_WIDTH),
        preview: true,
        maximum_preview: false,
    },
    Case {
        name: "startup_split",
        size: [1280.0, 820.0],
        navigation: Some(layout::NAV_PANEL_WIDTH),
        preview: true,
        maximum_preview: false,
    },
    Case {
        name: "minimum_content",
        size: [1280.0, 820.0],
        navigation: Some(layout::NAV_PANEL_MAX_WIDTH),
        preview: true,
        maximum_preview: true,
    },
    Case {
        name: "expanded_content",
        size: [1280.0, 820.0],
        navigation: None,
        preview: false,
        maximum_preview: false,
    },
];

pub(super) struct Capture {
    pub output: FullOutput,
    pub content: Rect,
    pub viewport: Rect,
}

pub(super) fn render_frame(
    context: &Context,
    state: &mut AppState,
    case: &Case,
    time: f64,
    events: Vec<Event>,
) -> Capture {
    state.nav_pinned = case.navigation.is_some();
    state.preview_open = case.preview;
    let viewport = Rect::from_min_size(egui::Pos2::ZERO, case.size.into());
    let mut content = Rect::NOTHING;
    let output = context.run(
        egui::RawInput {
            screen_rect: Some(viewport),
            time: Some(time),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::TopBottomPanel::top("menu_bar")
                .exact_height(
                    ctx.style().spacing.interact_size.y + 2.0 * layout::MENU_BAR_VERTICAL_INSET,
                )
                .show(ctx, |_| {});
            if let Some(width) = case.navigation {
                egui::SidePanel::left("nav_panel")
                    .exact_width(width + layout::NAV_CONTENT_GAP)
                    .frame(
                        egui::Frame::side_top_panel(ctx.style().as_ref()).inner_margin(
                            egui::Margin {
                                left: 10.0,
                                right: 10.0 + layout::NAV_CONTENT_GAP,
                                top: 12.0,
                                bottom: 12.0,
                            },
                        ),
                    )
                    .show(ctx, |_| {});
            }
            egui::TopBottomPanel::bottom("control_bar").show(ctx, |ui| {
                crate::views::show_control_bar(state, ui);
            });
            if let Some(range) =
                layout::preview_width_range(ctx.available_rect().width()).filter(|_| case.preview)
            {
                let width = if case.maximum_preview {
                    *range.end()
                } else {
                    layout::PREVIEW_DOCK_DEFAULT_WIDTH.clamp(*range.start(), *range.end())
                };
                egui::SidePanel::right("preview_panel")
                    .exact_width(width)
                    .show(ctx, |_| {});
            }
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::central_panel(ctx.style().as_ref()).inner_margin(egui::Margin {
                        left: 26.0,
                        right: 18.0,
                        top: 16.0,
                        bottom: 14.0,
                    }),
                )
                .show(ctx, |ui| {
                    content = ui.max_rect();
                    super::super::super::show_results_view(state, ui);
                });
        },
    );
    Capture {
        output,
        content,
        viewport,
    }
}
