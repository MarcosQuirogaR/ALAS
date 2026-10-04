// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The navigation tree, the hover rail and page routing.

use egui::{pos2, vec2, Align, Area, Context, Layout, Order, RichText, ScrollArea, Sense, Ui};

use super::tr;
use crate::layout;
use crate::nav::{self, PageKind};
use crate::state::{nav_overlay_open_with_bounds, AppState};
use crate::theme::{card_frame, navigation_overlay_frame};
use crate::views::form_page;
use crate::views::tour_data::TourTarget;

pub(super) fn render_nav(state: &mut AppState, ui: &mut Ui) {
    #[cfg(debug_assertions)]
    crate::layout_debug::record_ui(
        ui.ctx(),
        "navigation content",
        ui,
        crate::layout_debug::RegionKind::Navigation,
    );
    ui.set_min_width(ui.available_width());
    render_nav_contents(state, ui, true);
}

// These are deliberately shorter than a page transition. The rail is an
// affordance users may cross on the way to the canvas, so it must confirm
// entry without making a cursor detour feel sticky. Egui mirrors `cubic_out`
// for the closing direction: that makes withdrawal start promptly, while its
// final pixels still settle continuously instead of popping away.
const NAV_OVERLAY_OPEN_DURATION_S: f32 = 0.14;
const NAV_OVERLAY_CLOSE_DURATION_S: f32 = 0.10;

pub(super) fn render_nav_rail(state: &mut AppState, ctx: &Context, body_rect: egui::Rect) {
    let pointer = ctx.pointer_hover_pos();
    let width = layout::expanded_navigation_width(
        (body_rect.width() - layout::NAV_CONTENT_GAP).max(layout::NAV_RAIL_WIDTH),
    );
    let pointer_x = pointer.filter(|p| body_rect.contains(*p)).map(|p| p.x);
    let hovered = nav_overlay_open_with_bounds(
        state.nav_hover_open,
        pointer_x,
        body_rect.left(),
        layout::NAV_RAIL_WIDTH,
        width,
    );
    state.nav_hover_open = hovered;
    let expansion = if state.reduced_animations {
        // This is an accessibility preference, not a slower motion setting:
        // every rail state change completes in the current frame.
        if hovered {
            1.0
        } else {
            0.0
        }
    } else {
        ctx.animate_bool_with_time_and_easing(
            egui::Id::new("nav_rail_expansion"),
            hovered,
            if hovered {
                NAV_OVERLAY_OPEN_DURATION_S
            } else {
                NAV_OVERLAY_CLOSE_DURATION_S
            },
            egui::emath::easing::cubic_out,
        )
    };
    let panel_height = (body_rect.height() - 2.0 * layout::NAV_OVERLAY_MARGIN).max(1.0);
    // Slide a stable, full-width surface out of the rail rather than resizing
    // it. Resizing would repeatedly reflow the navigation labels and controls
    // while the cursor is already trying to select them. At rest only the
    // rightmost eight-point rail remains visible.
    let panel_pos = pos2(
        body_rect.left() - (width - layout::NAV_RAIL_WIDTH) * (1.0 - expansion),
        body_rect.top() + layout::NAV_OVERLAY_MARGIN,
    );

    let rail = Area::new(egui::Id::new("nav_rail"))
        .order(Order::Foreground)
        .constrain(false)
        .fade_in(false)
        .fixed_pos(panel_pos)
        .show(ctx, |ui| {
            ui.allocate_ui_with_layout(
                vec2(width, panel_height),
                Layout::top_down(Align::Min),
                |ui| {
                    let frame = navigation_overlay_frame(ui, expansion);
                    frame.show(ui, |ui| {
                        if expansion > 0.01 {
                            render_nav_contents(state, ui, false);
                        } else {
                            let response = ui
                                .allocate_rect(ui.max_rect(), Sense::click())
                                .on_hover_text(tr("Open navigation"));
                            if response.clicked() {
                                state.nav_pinned = true;
                            }
                        }
                    });
                },
            );
        });
    state.record_walkthrough_target(TourTarget::Navigation, rail.response.rect);
    #[cfg(debug_assertions)]
    crate::layout_debug::record(
        ctx,
        "navigation hover rail",
        rail.response.rect,
        crate::layout_debug::RegionKind::Rail,
    );
}

fn render_nav_contents(state: &mut AppState, ui: &mut Ui, pinned: bool) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr("Navigation"))
                .strong()
                .color(ui.visuals().hyperlink_color),
        );
        let label = if pinned { tr("Unpin") } else { tr("Pin") };
        let hint = if pinned {
            tr("Collapse navigation to an 8 px hover rail")
        } else {
            tr("Keep navigation open and reserve its column")
        };
        if ui.small_button(label).on_hover_text(hint).clicked() {
            state.nav_pinned = !state.nav_pinned;
        }
    });
    ui.add_space(6.0);
    ScrollArea::vertical()
        .id_salt("navigation_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for group in nav::NAV {
                card_frame(ui).show(ui, |ui| {
                    let group_width = ui.available_width();
                    ui.set_min_width(group_width);
                    ui.set_max_width(group_width);
                    ui.label(
                        RichText::new(tr(group.title))
                            .strong()
                            .color(ui.visuals().hyperlink_color),
                    );
                    for sub in group.subgroups {
                        if let Some(title) = sub.title {
                            ui.add_space(2.0);
                            ui.label(RichText::new(tr(title)).weak().small());
                        }
                        for page in sub.pages {
                            let selected = state.active_page == page.id;
                            let response = ui.selectable_label(
                                selected,
                                RichText::new(tr(page.title)).size(14.0),
                            );
                            if response.clicked() {
                                state.active_page = page.id.to_owned();
                                match page.kind {
                                    PageKind::Inputs | PageKind::Form => {
                                        state.update_preview_scene()
                                    }
                                    PageKind::Results => state.update_result_scene(),
                                    _ => {}
                                }
                            }
                        }
                    }
                });
                ui.add_space(4.0);
            }
        });
}

pub(super) fn route_page(state: &mut AppState, ui: &mut Ui) {
    let Some(page) = nav::page(&state.active_page).cloned() else {
        ui.label(tr("Unknown page."));
        return;
    };
    match page.kind {
        PageKind::Inputs => crate::views::show_inputs_view(state, ui),
        PageKind::DesignSpace => crate::views::design_space_view::show_design_space_view(state, ui),
        PageKind::Results => crate::views::show_results_view(state, ui),
        PageKind::Setup => crate::views::show_tools_view(state, ui),
        PageKind::Analyses => crate::views::show_analyses_view(state, ui),
        PageKind::AirfoilScreening => {
            crate::views::screening_window::show_screening_page(state, ui)
        }
        PageKind::Uav => crate::views::show_uav_view(state, ui),
        PageKind::Form => {
            ScrollArea::vertical()
                .id_salt(format!("advanced_page_scroll::{}", page.id))
                .auto_shrink([false, false])
                .show(ui, |ui| form_page::show_form_page(state, ui, &page));
        }
    }
}
