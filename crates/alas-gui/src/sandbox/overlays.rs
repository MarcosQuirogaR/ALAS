// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The action row floating inside the bottom of the viewport, and the
//! derived geometry metrics shown by the Summary card of [`super::panel`].
//!
//! There is no footer: the actions (Quick Analysis, Full Analysis, Cancel
//! while a run is in flight, Undo, Redo, Run log, Results) are floating
//! buttons in one row centred like the camera row (a launch row over an
//! edit row when one row would not fit). Every box registers itself as an
//! overlay, so a gesture on it never orbits the aircraft. The block is
//! inset from the viewport bottom and its rectangle is reserved from the
//! other overlays. The metrics are not drawn beneath the actions: they are
//! listed on demand behind the Summary button below the category stack.
//!
//! Metric conventions are those of [`super::scene`]: `S_ref` projected
//! main-wing planform area including the carry-through, m2; `b` projected
//! tip-to-tip span, m; `MAC` mean aerodynamic chord, m; `AR` `b^2 /
//! S_ref`; `LE sweep` inboard leading-edge sweep design variable, deg,
//! positive aft; `c/4 sweep` area-weighted quarter-chord sweep of the
//! lofted sections, deg; `taper` tip over root chord; `L_fus` overall
//! fuselage length, m.

use egui::{pos2, vec2, Context, Rect, RichText, Ui};

use crate::state::AppState;
use crate::views::tr;

use super::scene::geometry_metrics;
use super::viewport::{
    centered_row, floating_button, floating_control, measured_row_size, OVERLAY_INSET,
};
use super::workspace::{start_full_analysis, start_quick_analysis};

/// Height of the action row, in points.
pub const ACTION_ROW_HEIGHT: f32 = 30.0;
/// Minimum height of every button in the action rows, so Quick analysis, Full
/// analysis, Undo, Redo, Run log and Results are one height.
const ACTION_BUTTON_HEIGHT: f32 = 26.0;

/// Vertical gap between the rows of the block, in points.
pub const ROW_GAP: f32 = 4.0;
/// Nominal width of the full action row (Quick Analysis, Full Analysis, Cancel,
/// Undo, Redo, Run log, Results), used to decide whether it splits.
const ACTIONS_WIDTH: f32 = 720.0;
/// Room at each side of the Cancel label for the run spinner, points.
const SPINNER_ROOM: f32 = 22.0;
/// Hover text of every metric row: the conventions of the eight values.
pub(super) const CONVENTIONS: &str = "S_ref: projected planform area of the main wing including the carry-through, m^2. b: projected tip-to-tip span, m. MAC: mean aerodynamic chord, m. Λ_LE: inboard leading-edge sweep design variable, deg, positive aft. Λ_c4: area-weighted mean quarter-chord sweep of the lofted sections, deg. AR: b^2 / S_ref. taper: tip chord over root chord. L_fus: overall fuselage length, m. Axes: x aft, y right, z up.";

/// Action rows for a viewport width: one, or a launch row over an edit
/// row when the full row would not fit.
pub fn action_rows(viewport_width: f32) -> usize {
    if viewport_width - 2.0 * OVERLAY_INSET >= ACTIONS_WIDTH {
        1
    } else {
        2
    }
}

/// The rows of the block from the top, each with its key and minimum
/// height: the action row, or the launch and edit rows.
fn block_rows(viewport_width: f32) -> Vec<(&'static str, f32)> {
    let mut rows = vec![("actions", ACTION_ROW_HEIGHT)];
    if action_rows(viewport_width) == 2 {
        rows.push(("actions_edit", ACTION_ROW_HEIGHT));
    }
    rows
}

/// The rectangle the action block reserves at the bottom of `viewport`:
/// the rows at the heights they measured last frame (a row wider than the
/// viewport wraps and grows), stacked with [`ROW_GAP`] between them.
pub fn action_block_rect(ctx: &Context, viewport: Rect) -> Rect {
    let rows = block_rows(viewport.width());
    let height = rows
        .iter()
        .map(|(key, default)| measured_row_size(ctx, key, viewport, *default).y)
        .sum::<f32>()
        + (rows.len().saturating_sub(1)) as f32 * ROW_GAP;
    Rect::from_min_max(
        pos2(
            viewport.left() + OVERLAY_INSET,
            viewport.bottom() - OVERLAY_INSET - height,
        ),
        pos2(
            viewport.right() - OVERLAY_INSET,
            viewport.bottom() - OVERLAY_INSET,
        ),
    )
}

/// The metrics in display order, each a symbol and its value with the unit.
pub fn metric_chips(state: &AppState) -> Vec<(&'static str, String)> {
    let (Some(plane), Some(design)) = (&state.sandbox.airplane, state.current_design()) else {
        return Vec::new();
    };
    let m = geometry_metrics(plane, &design);
    vec![
        ("S_ref", format!("{:.1} m\u{b2}", m.reference_area_m2)),
        ("b", format!("{:.2} m", m.span_m)),
        ("MAC", format!("{:.2} m", m.mean_aerodynamic_chord_m)),
        ("AR", format!("{:.2}", m.aspect_ratio)),
        ("Λ_LE", format!("{:.1}°", m.leading_edge_sweep_deg)),
        ("Λ_c4", format!("{:.1}°", m.quarter_chord_sweep_deg)),
        ("taper", format!("{:.3}", m.taper_ratio)),
        ("L_fus", format!("{:.2} m", m.fuselage_length_m)),
    ]
}

/// Render the action row; returns the block rectangle.
pub fn show_action_block(state: &mut AppState, ui: &mut Ui, viewport: Rect) -> Rect {
    let ctx = ui.ctx().clone();
    let block = action_block_rect(&ctx, viewport);
    let mut top = block.top();
    // Rows are placed by the sizes they measured last frame, the same
    // sizes the block rectangle was reserved from.
    let mut advance = |key: &str, default: f32| {
        let placed = top;
        top += measured_row_size(&ctx, key, viewport, default).y + ROW_GAP;
        placed
    };
    if action_rows(viewport.width()) == 1 {
        let at = advance("actions", ACTION_ROW_HEIGHT);
        centered_row(ui, "actions", viewport, at, ACTION_ROW_HEIGHT, |ui| {
            show_launch_actions(state, ui);
            show_edit_actions(state, ui);
        });
    } else {
        let at = advance("actions", ACTION_ROW_HEIGHT);
        centered_row(ui, "actions", viewport, at, ACTION_ROW_HEIGHT, |ui| {
            show_launch_actions(state, ui);
        });
        let at = advance("actions_edit", ACTION_ROW_HEIGHT);
        centered_row(ui, "actions_edit", viewport, at, ACTION_ROW_HEIGHT, |ui| {
            show_edit_actions(state, ui);
        });
    }
    block
}

/// The analysis launches with their running and blocked states, and Cancel
/// with a spinner while the pipeline runs. Keyboard focus and the disabled
/// look are egui's own.
fn show_launch_actions(state: &mut AppState, ui: &mut Ui) {
    {
        let running = state.is_running;
        let blocked = state.blocked();
        let quick = egui::Button::new(RichText::new(tr("Quick Analysis")).strong())
            .min_size(vec2(0.0, ACTION_BUTTON_HEIGHT));
        if floating_control(
            ui,
            "action",
            !blocked && !state.sandbox.estimates.running(),
            false,
            quick,
        )
            .on_hover_text(tr("In-process estimates for the drawn aircraft; first results within seconds, each labelled as the Full Analysis' value or an estimate."))
            .clicked()
        {
            start_quick_analysis(state);
        }
        if floating_control(
            ui,
            "action",
            !running && !blocked,
            false,
            egui::Button::new(tr("Full Analysis")).min_size(vec2(0.0, ACTION_BUTTON_HEIGHT)),
        )
        .on_hover_text(tr("Run the complete pipeline on the drawn aircraft as a fixed design; results open in their own window."))
        .clicked()
        {
            start_full_analysis(state);
        }
        // Cancel keeps its slot, greyed out while nothing runs, and carries
        // the run spinner inside its own box: the centred row never changes
        // width, so no button moves when a run starts or ends.
        let label = tr("Cancel");
        let text_width = ui
            .painter()
            .layout_no_wrap(
                label.clone(),
                egui::TextStyle::Button.resolve(ui.style()),
                ui.visuals().text_color(),
            )
            .size()
            .x;
        let width = text_width + 2.0 * ui.spacing().button_padding.x + 2.0 * SPINNER_ROOM;
        let cancel = floating_control(
            ui,
            "action",
            running && !state.cancellation_requested,
            false,
            egui::Button::new(label).min_size(vec2(width, ACTION_BUTTON_HEIGHT)),
        );
        if running {
            let side = SPINNER_ROOM - 6.0;
            let spinner = Rect::from_center_size(
                pos2(
                    cancel.rect.left() + SPINNER_ROOM * 0.5 + 3.0,
                    cancel.rect.center().y,
                ),
                vec2(side, side),
            );
            egui::Spinner::new().size(side).paint_at(ui, spinner);
        }
        if cancel.clicked() {
            state.request_pipeline_cancel();
        }
    }
}

/// Undo and redo with their availability, the run log toggle and Results
/// once a result exists.
fn show_edit_actions(state: &mut AppState, ui: &mut Ui) {
    {
        if floating_control(
            ui,
            "action",
            state.sandbox.undo.can_undo(),
            false,
            egui::Button::new(tr("Undo")).min_size(vec2(0.0, ACTION_BUTTON_HEIGHT)),
        )
        .clicked()
        {
            state.sandbox_undo();
        }
        if floating_control(
            ui,
            "action",
            state.sandbox.undo.can_redo(),
            false,
            egui::Button::new(tr("Redo")).min_size(vec2(0.0, ACTION_BUTTON_HEIGHT)),
        )
        .clicked()
        {
            state.sandbox_redo();
        }
        if floating_button(
            ui,
            "action",
            egui::Button::new(tr("Run log")).min_size(vec2(0.0, ACTION_BUTTON_HEIGHT)),
        )
        .clicked()
        {
            state.sandbox.layout.log_window_open = !state.sandbox.layout.log_window_open;
            if state.sandbox.layout.log_window_open {
                crate::native_viewport::raise(ui.ctx(), "sandbox_run_log");
            }
        }
        // Results keeps its slot, greyed out until a result exists.
        if floating_control(
            ui,
            "action",
            state.pipeline_result.is_some(),
            false,
            egui::Button::new(tr("Results")).min_size(vec2(0.0, ACTION_BUTTON_HEIGHT)),
        )
        .clicked()
        {
            crate::native_viewport::raise(ui.ctx(), "sandbox_results");
            state.sandbox.results_window_open = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_viewports_split_the_action_row_and_no_metric_rows_are_reserved() {
        assert_eq!(action_rows(1280.0), 1);
        assert_eq!(action_rows(400.0), 2);
        let ctx = Context::default();
        let wide = action_block_rect(
            &ctx,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1280.0, 800.0)),
        );
        let narrow =
            action_block_rect(&ctx, Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 800.0)));
        assert!((wide.height() - ACTION_ROW_HEIGHT).abs() < 1e-6);
        assert!((narrow.height() - (2.0 * ACTION_ROW_HEIGHT + ROW_GAP)).abs() < 1e-6);
        assert!((wide.bottom() - (800.0 - OVERLAY_INSET)).abs() < 1e-6);
    }

    #[test]
    fn metric_chips_carry_values_units_and_conventions() {
        let mut state = AppState::default();
        assert!(state.enter_sandbox(true));
        let chips = metric_chips(&state);
        assert_eq!(chips.len(), 8);
        assert!(chips[0].0 == "S_ref" && chips[0].1.ends_with(" m\u{b2}"));
        assert!(chips[1].0 == "b" && chips[1].1.ends_with(" m"));
        assert!(chips[4].0 == "Λ_LE" && chips[4].1.ends_with('°'));
        assert_eq!(chips[7].0, "L_fus");
    }
}

#[cfg(test)]
mod fixed_position_tests {
    use super::*;

    fn action_rects(state: &mut AppState) -> Vec<Rect> {
        let ctx = Context::default();
        let mut time = 0.0;
        for _ in 0..3 {
            time += 0.05;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1400.0, 900.0))),
                    time: Some(time),
                    ..Default::default()
                },
                |ctx| crate::sandbox::workspace::show_sandbox_workspace(state, ctx),
            );
        }
        crate::sandbox::viewport::overlay_rects_tagged(&ctx, "action")
    }

    /// The row is centred, so a button that appears (Cancel while a run is
    /// active, Results once a result exists) must not move the others: its
    /// slot is always there, greyed out.
    #[test]
    fn the_action_buttons_keep_their_positions_whatever_is_running() {
        let mut idle = AppState::default();
        assert!(idle.enter_sandbox(true));
        let before = action_rects(&mut idle);
        let mut running = AppState::default();
        assert!(running.enter_sandbox(true));
        running.is_running = true;
        let during = action_rects(&mut running);
        // Quick, Full, Cancel, Undo, Redo, Run log, Results.
        assert_eq!(before.len(), 7, "{} idle buttons", before.len());
        assert_eq!(during.len(), before.len());
        for index in 0..before.len() {
            assert!(
                (before[index].left() - during[index].left()).abs() < 0.5,
                "button {index} moved from {} to {}",
                before[index].left(),
                during[index].left()
            );
            assert!((before[index].width() - during[index].width()).abs() < 0.5);
        }
    }
}
