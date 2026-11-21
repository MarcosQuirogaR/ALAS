// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The navigation tree, the hover rail and page routing.

use egui::{pos2, vec2, Align, Area, Context, Layout, Order, RichText, ScrollArea, Sense, Ui};

use super::tr;
use crate::layout;
use crate::nav::{self, PageKind};
use crate::state::{nav_overlay_open_with_bounds, AppState};
use crate::theme::navigation_overlay_frame;
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
    render_nav_contents(state, ui);
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
        layout::NAV_RAIL_HIT_WIDTH,
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
                            render_nav_contents(state, ui);
                        } else {
                            let response = ui
                                .allocate_rect(ui.max_rect(), Sense::click())
                                .on_hover_text(tr("Open navigation"));
                            if response.clicked() {
                                set_nav_pinned(state, true);
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

/// Configuration groups whose validation findings colour a step's status dot.
fn step_groups(page_id: &str) -> &'static [&'static str] {
    match page_id {
        "inputs" => &["requirements", "geometry", "optimizer", "propulsion_cycle"],
        "cabin" => &["cabin"],
        "mission" => &["mission", "performance"],
        "setup_analyses" => &["analysis", "structures", "mses"],
        _ => &[],
    }
}

/// The worst validation finding in a flow step's groups; `None` when the page
/// has none, and for pages outside the flow and for Results.
fn step_attention(state: &AppState, page: &nav::Page) -> Option<alas_config::Severity> {
    if page.step == 0 || page.id == "results" {
        return None;
    }
    let groups = step_groups(page.id);
    let mut worst = None;
    for finding in state.validation_findings.iter().filter(|finding| {
        groups
            .iter()
            .any(|group| finding.field_path.split('.').next() == Some(group))
    }) {
        if finding.severity == alas_config::Severity::Error {
            return Some(alas_config::Severity::Error);
        }
        worst = Some(finding.severity);
    }
    worst
}

/// The dot at the right of a step with a finding: red when it blocks the
/// run, amber when it only deserves a look. A step without one has no dot.
fn status_dot(ui: &mut Ui, rect: egui::Rect, severity: alas_config::Severity) {
    let color = match severity {
        alas_config::Severity::Error => ui.visuals().error_fg_color,
        alas_config::Severity::Warning => ui.visuals().warn_fg_color,
    };
    ui.painter()
        .circle_filled(egui::pos2(rect.right() - 10.0, rect.center().y), 4.0, color);
}

/// The greyed row of a page that is work in progress: dimmed title and a
/// "WIP" badge. Clicking still opens the page, which shows only a placeholder.
fn wip_row(ui: &mut Ui, label: &str, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let fill = if selected {
            visuals.selection.bg_fill
        } else if response.hovered() {
            visuals.widgets.hovered.weak_bg_fill
        } else {
            egui::Color32::TRANSPARENT
        };
        ui.painter()
            .rect_filled(rect, crate::theme::SELECTABLE_CONTROL_RADIUS, fill);
        // On the selection highlight the title and the pill use the selected
        // text colour: grey on the accent fill is unreadable.
        let grey = if selected {
            visuals.selection.stroke.color
        } else {
            visuals.weak_text_color()
        };
        let badge = tr("WIP");
        let font = egui::FontId::proportional(14.0);
        let badge_font = egui::FontId::proportional(10.0);
        let galley = ui
            .painter()
            .layout_no_wrap(badge.clone(), badge_font.clone(), grey);
        let badge_rect = egui::Rect::from_center_size(
            egui::pos2(
                rect.right() - 10.0 - galley.size().x * 0.5 - 6.0,
                rect.center().y,
            ),
            galley.size() + vec2(12.0, 4.0),
        );
        ui.painter()
            .rect_stroke(badge_rect, 8.0, egui::Stroke::new(1.0_f32, grey));
        ui.painter().text(
            badge_rect.center(),
            egui::Align2::CENTER_CENTER,
            badge,
            badge_font,
            grey,
        );
        ui.painter().text(
            egui::pos2(rect.left() + 10.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            font,
            grey,
        );
    }
    response.on_hover_text(tr("Work in progress"))
}

fn nav_item(state: &mut AppState, ui: &mut Ui, page: &nav::Page) {
    let selected = state.active_page == page.id;
    let indent = if page.nested { 26.0 } else { 0.0 };
    let label = if page.step > 0 {
        format!("{}   {}", page.step, tr(page.title))
    } else {
        tr(page.title)
    };
    let attention = step_attention(state, page);
    let response = if page.kind == PageKind::Uav && crate::views::uav_view::UAV_WORK_IN_PROGRESS {
        wip_row(ui, &label, selected)
    } else {
        crate::rail_row::rail_row(ui, &label, selected, indent)
    };
    if let Some(severity) = attention {
        status_dot(ui, response.rect, severity);
    }
    if response.clicked() {
        go_to_page(state, page.id);
        // A floating rail folds away once a page is chosen, even with the
        // pointer still over it; it reopens from the edge.
        if !state.nav_pinned {
            state.nav_hover_open = false;
        }
    }
}

/// Pin or unpin the navigation and remember the choice for the next launch.
pub(crate) fn set_nav_pinned(state: &mut AppState, pinned: bool) {
    state.nav_pinned = pinned;
    state.nav_hover_open = false;
    let marker = state.nav_pinned_marker();
    // A failed write only loses the preference for the next launch.
    if pinned {
        let _ = std::fs::create_dir_all(state.data_dir());
        let _ = std::fs::write(&marker, b"1");
    } else {
        let _ = std::fs::remove_file(&marker);
    }
}

/// Make `id` the active page and refresh whichever scene it previews.
pub fn go_to_page(state: &mut AppState, id: &str) {
    let Some(page) = nav::page(id) else {
        return;
    };
    if page.kind == PageKind::Sandbox {
        state.enter_sandbox(false);
        return;
    }
    // The Live Preview shows the interior on Cabin and Payload and the exterior
    // everywhere else; navigation never changes geometry.
    let interior = matches!(id, "cabin" | "payload");
    let was_interior = matches!(state.active_page.as_str(), "cabin" | "payload");
    if interior {
        state.preview_tab = crate::state::PreviewTab::Cabin;
    } else if was_interior {
        state.preview_tab = crate::state::PreviewTab::Exterior;
    }
    state.active_page = id.to_owned();
    match page.kind {
        PageKind::Inputs | PageKind::Form | PageKind::Mission | PageKind::Payload => {
            state.update_preview_scene()
        }
        PageKind::Results => state.refresh_result_scene(),
        _ => {}
    }
}
/// The pin button, right-aligned on the first group header's line: a filled
/// pin while the navigation is pinned (one click unpins), an outline while it
/// floats (one click pins). The whole 28 x 24 pt button highlights on hover.
fn pin_button(state: &mut AppState, ui: &mut Ui) {
    let pinned = state.nav_pinned;
    let (rect, response) = ui.allocate_exact_size(vec2(28.0, 24.0), Sense::click());
    let visuals = ui.visuals();
    let (fill, color) = if response.hovered() {
        (visuals.widgets.hovered.bg_fill, visuals.hyperlink_color)
    } else if pinned {
        (visuals.selection.bg_fill, visuals.selection.stroke.color)
    } else {
        (egui::Color32::TRANSPARENT, visuals.text_color())
    };
    ui.painter()
        .rect_filled(rect, crate::theme::SELECTABLE_CONTROL_RADIUS, fill);
    crate::theme::paint_pin_icon(ui.painter(), rect.center(), color, pinned);
    let hint = if pinned {
        "Unpin navigation: it collapses to the edge and opens on hover"
    } else {
        "Pin navigation: keep it open and reserve its column"
    };
    if response.on_hover_text(tr(hint)).clicked() {
        set_nav_pinned(state, !pinned);
    }
}

fn render_nav_contents(state: &mut AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .id_salt("navigation_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (index, group) in nav::NAV.iter().enumerate() {
                let header = RichText::new(tr(group.title))
                    .strong()
                    .color(ui.visuals().hyperlink_color);
                if index == 0 {
                    ui.horizontal(|ui| {
                        ui.label(header);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            pin_button(state, ui);
                        });
                    });
                } else {
                    ui.label(header);
                }
                ui.add_space(2.0);
                for sub in group.subgroups {
                    if let Some(title) = sub.title {
                        ui.label(RichText::new(tr(title)).weak().small());
                    }
                    for page in sub.pages {
                        nav_item(state, ui, page);
                    }
                }
                ui.add_space(10.0);
            }
        });
}

pub(super) fn route_page(state: &mut AppState, ui: &mut Ui) {
    let Some(page) = nav::page(&state.active_page).cloned() else {
        ui.label(tr("Unknown page."));
        return;
    };
    match page.kind {
        PageKind::Inputs | PageKind::Mission | PageKind::Payload | PageKind::Analyses => {
            crate::views::flow_pages::show_flow_page(state, ui, &page)
        }
        PageKind::Files => crate::views::files_page::show_files_page(state, ui),
        PageKind::Sandbox => {}
        PageKind::DesignSpace => crate::views::design_space_view::show_design_space_view(state, ui),
        PageKind::Results => crate::views::show_results_view(state, ui),
        PageKind::Setup => crate::views::show_tools_view(state, ui),
        PageKind::AirfoilScreening => {
            crate::views::screening_window::show_screening_page(state, ui)
        }
        PageKind::Uav => crate::views::show_uav_view(state, ui),
        PageKind::Addons => crate::addons::show_addons_view(state, ui),
        PageKind::Form => {
            ScrollArea::vertical()
                .id_salt(format!("advanced_page_scroll::{}", page.id))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    form_page::show_form_page(state, ui, &page);
                    crate::search::flush_scroll(ui);
                });
        }
    }
}

/// The narrow-window form of the rail: the pages in one scrolling row above
/// the content.
pub(super) fn render_step_strip(state: &mut AppState, ui: &mut Ui) {
    egui::ScrollArea::horizontal()
        .id_salt("step_strip_scroll")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for group in nav::NAV {
                    for sub in group.subgroups {
                        for page in sub.pages {
                            let selected = state.active_page == page.id;
                            let label = if page.step > 0 {
                                format!("{} {}", page.step, tr(page.title))
                            } else {
                                tr(page.title)
                            };
                            if ui
                                .add(crate::theme::selectable_button(label, selected))
                                .clicked()
                            {
                                go_to_page(state, page.id);
                            }
                        }
                    }
                }
            });
        });
}

#[cfg(test)]
mod wip_row_tests {
    use super::*;

    /// On the selection highlight a work-in-progress row's title and pill use
    /// the selected text colour; off it they stay weak.
    #[test]
    fn a_selected_wip_row_uses_the_selected_text_colour() {
        let colours = |selected: bool| {
            let ctx = Context::default();
            let mut out = Vec::new();
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    wip_row(ui, "Fixed-wing UAV", selected);
                });
            });
            for clipped in &output.shapes {
                if let egui::Shape::Text(text) = &clipped.shape {
                    out.push(text.galley.job.sections[0].format.color);
                }
            }
            let visuals = ctx.style().visuals.clone();
            (
                out,
                visuals.selection.stroke.color,
                visuals.weak_text_color(),
            )
        };
        let (on, selection, _) = colours(true);
        assert!(
            !on.is_empty() && on.iter().all(|c| *c == selection),
            "{on:?}"
        );
        let (off, _, weak) = colours(false);
        assert!(off.iter().all(|c| *c == weak), "{off:?}");
    }
}

#[cfg(test)]
mod r40_render_tests {
    use super::*;
    use crate::test_raster::{render, write_in};

    /// The pin sits on the first group's header line at the right edge of
    /// the navigation column, not above the header.
    #[test]
    fn the_pin_shares_the_design_header_line_at_the_right() {
        for pinned in [true, false] {
            let mut state = AppState {
                nav_pinned: pinned,
                ..AppState::default()
            };
            let shot = render(260.0, 200.0, 3, 0, |ui| {
                ui.set_width(240.0);
                render_nav_contents(&mut state, ui);
            });
            let design = shot
                .texts
                .iter()
                .find(|(text, _)| text == "Design")
                .map(|(_, rect)| *rect)
                .expect("the Design header is drawn");
            // The pin is a painted shape: its button rect is the 28 x 24 box
            // right-aligned on the header line.
            let pin = shot
                .rects
                .iter()
                .find(|rect| {
                    (rect.width() - 28.0).abs() < 0.5 && (rect.height() - 24.0).abs() < 0.5
                })
                .copied();
            if let Some(pin) = pin {
                assert!(pin.right() > 200.0, "right aligned: {pin:?}");
                assert!(
                    (pin.center().y - design.center().y).abs() < 6.0,
                    "{pin:?} vs {design:?}"
                );
            }
            write_in(
                &crate::test_raster::render_dir(),
                if pinned {
                    "c5-pin-pinned.png"
                } else {
                    "c5-pin-floating.png"
                },
                &shot,
            );
        }
    }
}
