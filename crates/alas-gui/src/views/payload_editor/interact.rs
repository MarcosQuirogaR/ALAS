// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the canvases' mouse actions do to the declared items: placing, picking
//! and dragging, in the side and in the front view.

use alas_config::PayloadItemsConfig;
use egui::Ui;

use super::canvas::{Action, Target};
use super::side_view::SideView;
use super::{placement, say, zones, Drag, EditorUi, Mode};
use crate::views::tr_label;

/// The section station of the front view: the chosen one, else the middle of
/// the cabin.
pub(super) fn section_x(editor: &EditorUi, view: &SideView) -> f64 {
    editor
        .section_x
        .unwrap_or_else(|| f64::midpoint(view.cabin_x[0], view.cabin_x[1]))
}

pub(super) fn apply(
    action: Action,
    ui: &Ui,
    editor: &mut EditorUi,
    items: &mut PayloadItemsConfig,
    view: &SideView,
    locked: bool,
) {
    if locked && !matches!(action, Action::None | Action::Select(_)) {
        if !matches!(
            action,
            Action::DragTo { .. } | Action::DragToFront { .. } | Action::DragEnd
        ) {
            say(editor, ui, tr_label("Layout Locked"), false);
        }
        editor.drag = None;
        return;
    }
    match action {
        Action::None => {}
        Action::Select(target) => editor.selected = target,
        Action::Click { x, z } => {
            let placed = match editor.mode {
                Mode::Zone => placement::place_zone(items, view, x, z).map(Target::Zone),
                Mode::PointMass => {
                    placement::place_point_mass(items, view, x, z).map(Target::PointMass)
                }
                Mode::Idle => return,
            };
            finish_placing(
                editor,
                ui,
                placed.map_err(|refusal| tr_label(refusal.text())),
            );
        }
        Action::ClickFront { y, z } => {
            let x = section_x(editor, view);
            let placed = match editor.mode {
                Mode::PointMass => placement::place_point_mass_at(items, view, x, y, z)
                    .map(Target::PointMass)
                    .map_err(|refusal| tr_label(refusal.text())),
                Mode::Zone => Err(tr_label("Zones Are Placed In The Side View")),
                Mode::Idle => return,
            };
            finish_placing(editor, ui, placed);
        }
        Action::DragStart { target, x } => start_drag(editor, items, target, x),
        Action::DragTo { x, z } => drag_to(editor, items, view, x, z),
        Action::DragToFront { y, z } => {
            if let Some(Target::PointMass(index)) = editor.drag.as_ref().map(|d| &d.target) {
                if let Some(mass) = items.point_masses.get_mut(*index) {
                    placement::drag_point_mass_front(mass, view, y, z);
                }
            }
        }
        Action::DragEnd => end_drag(editor, items),
    }
}

fn finish_placing(editor: &mut EditorUi, ui: &Ui, placed: Result<Target, String>) {
    match placed {
        Ok(target) => {
            editor.selected = Some(target);
            editor.mode = Mode::Idle;
        }
        Err(reason) => say(editor, ui, reason, false),
    }
}

fn start_drag(editor: &mut EditorUi, items: &PayloadItemsConfig, target: Target, x: f64) {
    let grab_m = match target {
        Target::Zone(index) => {
            let Some(zone) = items.zones.get(index) else {
                return;
            };
            x - zone.x_start_m
        }
        Target::PointMass(index) => {
            if items.point_masses.get(index).is_none() {
                return;
            }
            0.0
        }
    };
    editor.selected = Some(target.clone());
    editor.drag = Some(Drag {
        target,
        grab_m,
        landing: None,
    });
}

/// Follow the pointer. A zone is held against its neighbours (it stops at
/// the one in its way); once the pointer is past a neighbour's midpoint, the
/// drag records the gap beyond as the landing spot, drawn as a ghost, and
/// release puts the zone there.
fn drag_to(editor: &mut EditorUi, items: &mut PayloadItemsConfig, view: &SideView, x: f64, z: f64) {
    let Some(drag) = editor.drag.as_mut() else {
        return;
    };
    match drag.target {
        Target::Zone(index) => {
            let desired = x - drag.grab_m;
            if let Some((held, landing)) = zones::slide(&items.zones, index, view.cabin_x, desired)
            {
                if let Some(zone) = items.zones.get_mut(index) {
                    zones::set_start(zone, held);
                }
                drag.landing = ((landing - held).abs() > 1.0e-6).then_some(landing);
            }
        }
        Target::PointMass(index) => {
            if let Some(mass) = items.point_masses.get_mut(index) {
                placement::drag_point_mass(mass, view, x, z);
            }
        }
    }
}

fn end_drag(editor: &mut EditorUi, items: &mut PayloadItemsConfig) {
    if let Some(Drag {
        target: Target::Zone(index),
        landing: Some(start),
        ..
    }) = editor.drag.take()
    {
        if let Some(zone) = items.zones.get_mut(index) {
            zones::set_start(zone, start);
        }
    }
}

/// The stretch a dragged zone would land on, for the ghost outline.
pub(super) fn landing_span(editor: &EditorUi, items: &PayloadItemsConfig) -> Option<(f64, f64)> {
    let drag = editor.drag.as_ref()?;
    let Target::Zone(index) = drag.target else {
        return None;
    };
    let start = drag.landing?;
    Some((start, start + items.zones.get(index)?.length_m()))
}
