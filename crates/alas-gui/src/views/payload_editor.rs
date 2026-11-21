// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Payload page's zone and point-mass editor, on the declared payload items.
//!
//! The side view is generated from the built aircraft: the fuselage section
//! by station, the level floor, the cabin and the holds, the seats and the
//! declared items to scale, and the centre of gravity. "+ Zone" and
//! "+ Point Mass" arm placement: the next click on the view places the item
//! at the clicked station snapped to 0.1 m, or is refused when it is outside
//! every valid volume. A Side/Front switch gives the canvas two projections of
//! the airframe envelope, so a point mass can be put anywhere in it (fuselage,
//! belly, under the wing, nose) and its x/y/z read and edited. A zone is
//! dragged along the cabin without ever overlapping another (see [`zones`]).
//! Below the canvas an item list and the picked item's properties sit side by
//! side, and a summary bar shows the payload mass and the live CG against its
//! limits. Every item has a Movable flag, with its station range, that the
//! payload auto-balance honours; Lock Layout freezes the layout
//! and the auto-balance; Save Layout and Load use the Payload Layouts of the
//! file library.

mod camera;
mod canvas;
mod cg_label;
mod envelope;
mod frame;
mod front_camera;
mod front_render;
mod front_view;
mod glyphs;
mod interact;
mod list;
mod placement;
mod props;
mod seat_layer;
mod side_view;
mod summary;
mod toolbar;
mod zones;

use std::sync::Arc;

use alas_config::PayloadItemsConfig;
use egui::{Id, RichText, Ui};
use serde_json::Value;

use crate::state::AppState;
use crate::views::edit_draft;
use crate::views::{tr_fields, tr_label};
#[cfg(test)]
use canvas::Action;
use canvas::Target;
use interact::apply;
use side_view::SideView;

const PILL_SECONDS: f64 = 6.0;
/// Card width from which the canvas and the item panel sit side by side, pt.
const SPLIT_WIDTH: f32 = 1000.0;
/// What the page spends above the canvas (title, toolbar, summary strip), pt.
const CHROME_HEIGHT: f32 = 250.0;
const MAX_CANVAS_HEIGHT: f32 = 900.0;
/// What the "no items yet" line and its note take under the canvas, pt.
const EMPTY_LIST_HEIGHT: f32 = 80.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Idle,
    Zone,
    PointMass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ViewMode {
    #[default]
    Side,
    Front,
}

#[derive(Debug, Clone)]
struct Drag {
    target: Target,
    /// Pointer offset from a zone's start, m.
    grab_m: f64,
    /// Start of the gap a dragged zone would land in on release, when the
    /// pointer is past a neighbour's midpoint.
    landing: Option<f64>,
}

#[derive(Debug, Clone, Default)]
struct EditorUi {
    mode: Mode,
    selected: Option<Target>,
    drag: Option<Drag>,
    view_mode: ViewMode,
    /// The stretch of the body the side view shows.
    window: canvas::Window,
    /// The station of the front view; the middle of the cabin when unset.
    section_x: Option<f64>,
    /// Pan and zoom of the front view.
    front: front_camera::Camera,
    /// The selection the front view last followed.
    followed: Option<Target>,
    status: Option<(String, bool, f64)>,
    /// A declared balance range that leaves its region, recomputed each frame.
    warning: Option<String>,
}

#[derive(Clone)]
struct CachedView {
    key: u64,
    view: Option<Arc<SideView>>,
}

fn ui_id() -> Id {
    Id::new("payload_editor_state")
}

fn view_id() -> Id {
    Id::new("payload_editor_side_view")
}

fn say(editor: &mut EditorUi, ui: &Ui, text: String, ok: bool) {
    editor.status = Some((text, ok, ui.input(|input| input.time)));
}

/// The declared items of the case.
fn read_items(state: &AppState) -> PayloadItemsConfig {
    state
        .config_values
        .pointer("/cabin/payload_items")
        .and_then(|node| serde_json::from_value(node.clone()).ok())
        .unwrap_or_default()
}

/// Store `items`; an empty set removes the node. Returns whether it changed.
fn write_items(state: &mut AppState, items: &PayloadItemsConfig) -> bool {
    let Some(cabin) = state
        .config_values
        .get_mut("cabin")
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    let changed = if items.is_empty() {
        cabin.remove("payload_items").is_some()
    } else {
        let Ok(value) = serde_json::to_value(items) else {
            return false;
        };
        let changed = cabin.get("payload_items") != Some(&value);
        cabin.insert("payload_items".to_owned(), value);
        changed
    };
    if changed {
        state.on_config_modified();
    }
    changed
}

/// What else the side view depends on besides the configuration: the design
/// values, and which run (if any) its masses may come from.
fn inputs_fingerprint(state: &AppState) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}", state.design_values).hash(&mut hasher);
    format!("{:?}", state.pipeline_result_design_values).hash(&mut hasher);
    state.pipeline_result_complete.hash(&mut hasher);
    state.pipeline_result.is_some().hash(&mut hasher);
    state.run_identity.hash(&mut hasher);
    hasher.finish()
}

/// The fingerprint of the whole configuration and the side view's other
/// inputs. The configuration part is the per-frame memo, not a new walk.
fn full_fingerprint(state: &AppState) -> u64 {
    state.config_fingerprint_now() ^ inputs_fingerprint(state).rotate_left(17)
}

/// Fingerprints of the configuration with and without the declared items:
/// the geometry only changes with the second, so a drag reuses the view.
fn fingerprints(state: &mut AppState) -> (u64, u64) {
    use crate::state::state_memo::config_fingerprint;
    let extra = inputs_fingerprint(state);
    let full = full_fingerprint(state);
    let node = state
        .config_values
        .get_mut("cabin")
        .and_then(Value::as_object_mut)
        .and_then(|cabin| cabin.remove("payload_items"));
    let geometry = config_fingerprint(&state.config_values) ^ extra.rotate_left(17);
    if let (Some(node), Some(cabin)) = (
        node,
        state
            .config_values
            .get_mut("cabin")
            .and_then(Value::as_object_mut),
    ) {
        cabin.insert("payload_items".to_owned(), node);
    }
    (full, geometry)
}

fn side_view(state: &mut AppState, ui: &Ui, dragging: bool) -> Option<Arc<SideView>> {
    // The geometry-only key needs a walk of the configuration without the
    // items, so only a drag pays for it; every other frame reads the memo.
    let key = if dragging {
        fingerprints(state).1
    } else {
        full_fingerprint(state)
    };
    if let Some(cached) = ui.ctx().data(|data| data.get_temp::<CachedView>(view_id())) {
        if cached.key == key || (dragging && cached.view.is_some()) {
            return cached.view;
        }
    }
    let view = state
        .typed_config_arc()
        .and_then(|config| side_view::build(state, &config))
        .map(Arc::new);
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            view_id(),
            CachedView {
                key,
                view: view.clone(),
            },
        );
    });
    view
}

/// The Payload page's editor.
pub(crate) fn show(state: &mut AppState, ui: &mut Ui) {
    let mut editor: EditorUi = ui
        .ctx()
        .data_mut(|data| data.get_temp(ui_id()))
        .unwrap_or_default();
    let committed = read_items(state);
    let items_id = Id::new("payload_editor_draft");
    let mut items = edit_draft::begin(ui.ctx(), items_id, committed.clone());
    let before = items.clone();
    let settings_committed = crate::views::mission_stage_editor::layout_settings(state);
    let settings_id = Id::new("payload_editor_settings_draft");
    let (mut locked, mut target) = edit_draft::begin(ui.ctx(), settings_id, settings_committed);
    let view = side_view(state, ui, editor.drag.is_some());
    let theme = state.theme.figure_theme_name();

    let mut remove = None;
    let released_at = crate::views::mission_stage_editor::release_stages(state);
    crate::views::inputs_view::card(ui, "Payload Layout", |ui| {
        toolbar::toolbar(
            state,
            ui,
            &mut editor,
            &mut locked,
            &mut target,
            view.as_deref(),
        );
        ui.add_space(6.0);
        if let Some(view) = view.as_deref() {
            summary::show(ui, view);
            ui.add_space(6.0);
        }
        if ui.available_width() >= SPLIT_WIDTH && items.has_items() {
            let total = ui.available_width();
            let height = canvas_height(ui);
            let left = (total * 0.62).round();
            let right = total - left - 14.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(left, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(left);
                        layout_view(
                            ui,
                            &mut editor,
                            &mut items,
                            view.as_deref(),
                            locked,
                            height,
                            theme,
                        );
                    },
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(right, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(right);
                        remove = items_panel(
                            ui,
                            &mut editor,
                            &mut items,
                            view.as_deref(),
                            &released_at,
                            locked,
                            true,
                        );
                    },
                );
            });
        } else {
            let height = if !items.has_items() {
                // The empty-list line sits under the canvas: the card still fits the viewport.
                (canvas_height(ui) - EMPTY_LIST_HEIGHT).max(canvas::MIN_HEIGHT)
            } else {
                (canvas_height(ui) * 0.8).max(canvas::MIN_HEIGHT)
            };
            layout_view(
                ui,
                &mut editor,
                &mut items,
                view.as_deref(),
                locked,
                height,
                theme,
            );
            ui.add_space(6.0);
            remove = items_panel(
                ui,
                &mut editor,
                &mut items,
                view.as_deref(),
                &released_at,
                locked,
                false,
            );
        }
    });
    match remove {
        Some(Target::Zone(index)) if index < items.zones.len() => {
            items.zones.remove(index);
            editor.selected = None;
        }
        Some(Target::PointMass(index)) if index < items.point_masses.len() => {
            items.point_masses.remove(index);
            editor.selected = None;
        }
        _ => {}
    }

    if let Some(view) = view.as_deref() {
        placement::constrain(&mut items, &before, view);
    }
    placement::normalize(&mut items, Some(&before), view.as_deref());
    editor.warning = view
        .as_deref()
        .and_then(|view| placement::ranges_leaving_regions(&items, view))
        .map(|item| {
            tr_fields(
                "Balance Range Of {item} Leaves Its Region",
                &[("item", item)],
            )
        });
    if let Err(reason) = items.validate() {
        say(&mut editor, ui, reason, false);
        items = before;
    }
    if let Some(items) = edit_draft::finish(ui.ctx(), items_id, &committed, items) {
        write_items(state, &items);
    }
    let settings = (locked, target);
    let committed_settings = crate::views::mission_stage_editor::layout_settings(state);
    if let Some((locked, target)) =
        edit_draft::finish(ui.ctx(), settings_id, &committed_settings, settings)
    {
        crate::views::mission_stage_editor::set_layout_settings(state, locked, target);
    }
    ui.ctx().data_mut(|data| data.insert_temp(ui_id(), editor));
}

/// The canvas for the chosen projection, `height` points tall.
fn layout_view(
    ui: &mut Ui,
    editor: &mut EditorUi,
    items: &mut PayloadItemsConfig,
    view: Option<&SideView>,
    locked: bool,
    height: f32,
    theme: &'static str,
) {
    let Some(view) = view else {
        ui.label(RichText::new(tr_label("Layout Unavailable")).weak());
        return;
    };
    follow_selection(editor, items);
    let picked = editor.selected.clone();
    let (dragging, placing) = (editor.drag.is_some(), editor.mode != Mode::Idle);
    let action = match editor.view_mode {
        ViewMode::Side => canvas::show(
            ui,
            view,
            items,
            picked.as_ref(),
            dragging,
            placing,
            interact::landing_span(editor, items),
            &mut editor.window,
            height,
            theme,
        ),
        ViewMode::Front => {
            let current = interact::section_x(editor, view);
            let mut section = current;
            let action = front_view::show(
                ui,
                view,
                items,
                picked.as_ref(),
                &mut section,
                &mut editor.front,
                dragging,
                placing,
                height,
                theme,
            );
            if (section - current).abs() > 1.0e-9 {
                editor.section_x = Some(section);
            }
            action
        }
    };
    apply(action, ui, editor, items, view, locked);
}

/// The canvas height: the page's viewport less the title, toolbar and summary
/// strip above it, so the drawing uses the room the page has.
fn canvas_height(ui: &Ui) -> f32 {
    let page = ui.clip_rect().height().min(ui.ctx().screen_rect().height());
    (page - CHROME_HEIGHT).clamp(canvas::MIN_HEIGHT, MAX_CANVAS_HEIGHT)
}

/// The item list and the picked item's properties: one under the other in
/// the narrow column beside the canvas, side by side under it otherwise.
/// Returns the item whose delete button was pressed.
fn items_panel(
    ui: &mut Ui,
    editor: &mut EditorUi,
    items: &mut PayloadItemsConfig,
    view: Option<&SideView>,
    released_at: &std::collections::BTreeMap<String, usize>,
    locked: bool,
    stacked: bool,
) -> Option<Target> {
    let mut out = list::Out::default();
    let empty = !items.has_items();
    let props = |ui: &mut Ui, items: &mut PayloadItemsConfig, editor: &EditorUi| {
        props::show(ui, items, view, editor.selected.as_ref(), locked);
    };
    if stacked || empty || ui.available_width() < 560.0 {
        out = list::show(ui, items, &mut editor.selected, released_at, locked);
        if !empty {
            ui.add_space(6.0);
            ui.separator();
            props(ui, items, editor);
        }
    } else {
        ui.columns(2, |columns| {
            out = list::show(
                &mut columns[0],
                items,
                &mut editor.selected,
                released_at,
                locked,
            );
            props(&mut columns[1], items, editor);
        });
    }
    out.remove
}

/// In the front view, a freshly picked point mass puts the section at its
/// station, so it is on the picture.
fn follow_selection(editor: &mut EditorUi, items: &PayloadItemsConfig) {
    if editor.selected == editor.followed {
        return;
    }
    editor.followed = editor.selected.clone();
    if let (ViewMode::Front, Some(Target::PointMass(index))) =
        (editor.view_mode, editor.selected.as_ref())
    {
        if let Some(mass) = items.point_masses.get(*index) {
            editor.section_x = Some(mass.x_m);
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_more;

#[cfg(test)]
mod tests_audit;

#[cfg(test)]
mod tests_framing;
#[cfg(test)]
mod tests_nav;
#[cfg(test)]
mod tests_perf;
#[cfg(test)]
mod tests_render;
#[cfg(test)]
mod tests_seats;

/// What the other editors' tests need of the Payload editor.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// Declared items with one releasable 800 kg point mass placed in the hold
    /// of the case's side view, as a click and the table would leave it.
    pub(crate) fn releasable_store_in_the_hold(state: &AppState) -> PayloadItemsConfig {
        let config = state.typed_config().expect("typed config");
        let view = side_view::build(state, &config).expect("side view");
        let x = view.cabin_x[1] - 3.0;
        let z = f64::midpoint(view.hold_floor_z(x), view.hold_ceiling_z(x));
        let mut items = read_items(state);
        let index = placement::place_point_mass(&mut items, &view, x, z).expect("a hold station");
        items.point_masses[index].releasable = true;
        items.point_masses[index].mass_kg = 800.0;
        placement::set_point_mass_movable(&mut items.point_masses[index], &view, true);
        items
    }

    /// Whether the view the page would show now (through its cache) carries
    /// the pre-run estimate.
    pub(crate) fn shown_view_is_estimate(state: &mut AppState, ctx: &egui::Context) -> bool {
        let mut estimate = None;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                estimate = side_view(state, ui, false).map(|view| view.estimate);
            });
        });
        estimate.expect("a side view")
    }

    /// The zero-fuel centre of gravity the view marks, m.
    pub(crate) fn view_zero_fuel_cg(state: &AppState) -> Option<f64> {
        let config = state.typed_config().expect("typed config");
        side_view::build(state, &config)
            .expect("side view")
            .zero_fuel_cg
    }

    /// Whether the side view's masses are the pre-run estimate, and its payload
    /// centre of gravity station, m.
    pub(crate) fn view_source(state: &AppState) -> (bool, f64) {
        let config = state.typed_config().expect("typed config");
        let view = side_view::build(state, &config).expect("side view");
        (view.estimate, view.cg_x)
    }
}
#[cfg(test)]
mod tests_r39;
