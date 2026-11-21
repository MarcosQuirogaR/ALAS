// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parameter access floating over the design space.
//!
//! There is no Parameter Panel: the geometry categories are separate
//! buttons floating down the left edge of the viewport, each of which opens
//! the Discipline Window of that category and isolates its component in the
//! preview, exactly as the window's own Open editor action does. The stack
//! (Search box and buttons) is centred about the viewport's horizontal
//! centreline, within the space left free between the camera row and the
//! bottom action block, so it never overlaps either. Above the buttons
//! sits the Search button: it opens the global Ctrl+K palette scoped to the
//! Sandbox parameters (see `sandbox::search`). Below the category buttons
//! sits the Summary button: it toggles a card beside the stack listing the
//! derived geometry metrics (`S_ref`, `b`, `MAC`, `AR`, sweeps, taper and
//! fuselage length), so they no longer clutter the action block.

use egui::{pos2, vec2, Rect, Ui};

use crate::state::AppState;
use crate::views::tr;

use super::fields::Discipline;
use super::overlays::{metric_chips, CONVENTIONS};
use super::viewport::{floating_control, register_overlay_rect, OVERLAY_INSET};

/// Width of the floating column of category buttons, in points.
pub const CATEGORY_COLUMN_WIDTH: f32 = 168.0;
/// Vertical spacing between category buttons, in points.
pub const CATEGORY_SPACING: f32 = 10.0;
/// Smallest vertical gap between stack rows once the free band between the
/// camera row and the action block is shorter than the stack's natural
/// height (a short window). Below `CATEGORY_SPACING` the buttons keep their
/// own size; only the air between them gives way.
const CATEGORY_SPACING_MIN: f32 = 3.0;
/// Automatic gaps inside the stack: one between each of the six rows
/// (Search and the five category buttons).
const STACK_GAP_COUNT: f32 = 5.0;
/// Share of the viewport width the whole-aircraft summary may take before it
/// lays two metrics a row instead of three: it stays in the left corner.
const SUMMARY_WIDTH_SHARE: f32 = 0.25;

/// Open a discipline window and focus its component.
pub fn open_discipline_window(state: &mut AppState, discipline: Discipline) {
    let id = discipline.id().to_owned();
    if !state.sandbox.layout.open_disciplines.contains(&id) {
        state.sandbox.layout.open_disciplines.push(id);
    }
    // A window that is already open may be minimised or behind the main one.
    state.sandbox.raise_discipline = Some(discipline.id().to_owned());
    state.set_sandbox_focus(Some(discipline));
}

/// The tag a category button registers its rectangle under.
fn category_tag(discipline: Discipline) -> &'static str {
    match discipline {
        Discipline::Wing => "category:wing",
        Discipline::HorizontalTail => "category:horizontal_tail",
        Discipline::VerticalTail => "category:vertical_tail",
        Discipline::Fuselage => "category:fuselage",
        Discipline::Propulsion => "category:propulsion",
    }
}

/// The top of the category stack: centred about the viewport's horizontal
/// centreline, clamped to the free band between `free_top` and
/// `free_bottom`; when the band is shorter than the stack, the stack starts
/// at the top of the band.
pub fn stack_top(viewport: Rect, free_top: f32, free_bottom: f32, height: f32) -> f32 {
    let centred = viewport.center().y - height * 0.5;
    centred.clamp(free_top, (free_bottom - height).max(free_top))
}

fn stack_height_id() -> egui::Id {
    egui::Id::new("sandbox_category_stack_height")
}

/// The stack's own last full-spacing height: the reference this shrinks
/// from. Kept separate from [`stack_height_id`] (the *actual*, possibly
/// already-shrunk, height used to position the stack) so a shrunk frame
/// never corrupts the baseline the next frame's shrink is computed from,
/// which would otherwise have the stack alternate between a full-spacing
/// frame that overflows the band and a shrunk frame that fits it forever.
fn stack_natural_height_id() -> egui::Id {
    egui::Id::new("sandbox_category_stack_natural_height")
}

/// The vertical gap between stack rows that keeps the stack's `natural`
/// (full-spacing) height inside `available` points of free band.
///
/// `natural` only ever reflects a frame actually rendered at
/// [`CATEGORY_SPACING`], so this is a pure function of two frame-stable
/// numbers rather than a decision re-made from "does it fit right now",
/// which is what would make it oscillate.
fn category_stack_spacing(natural: f32, available: f32) -> f32 {
    if natural <= available {
        return CATEGORY_SPACING;
    }
    let overflow = natural - available;
    (CATEGORY_SPACING - overflow / STACK_GAP_COUNT).max(CATEGORY_SPACING_MIN)
}

/// Render the Search button and the category buttons, all floating over
/// `viewport` within the vertical band `free_top..free_bottom` the other overlays leave free.
pub fn show_parameter_access(
    state: &mut AppState,
    ui: &mut Ui,
    viewport: Rect,
    free_top: f32,
    free_bottom: f32,
) {
    let width = CATEGORY_COLUMN_WIDTH.min((viewport.width() - 2.0 * OVERLAY_INSET).max(1.0));
    let ctx = ui.ctx().clone();
    let measured = ctx
        .data(|d| d.get_temp::<f32>(stack_height_id()))
        .unwrap_or(196.0);
    // The free band between the camera row and the action block: on a short
    // window it can be narrower than the stack's seven rows at their full
    // spacing, so the gap between rows gives way before anything overlaps
    // the block below.
    let available = (free_bottom - free_top).max(0.0);
    let natural = ctx
        .data(|d| d.get_temp::<f32>(stack_natural_height_id()))
        .unwrap_or(measured);
    let spacing = category_stack_spacing(natural, available);
    let top = stack_top(viewport, free_top, free_bottom, measured);
    let column = Rect::from_min_max(
        pos2(viewport.left() + OVERLAY_INSET, top),
        pos2(
            viewport.left() + OVERLAY_INSET + width,
            viewport.bottom() - OVERLAY_INSET,
        ),
    );
    let stack = ui.allocate_new_ui(egui::UiBuilder::new().max_rect(column), |ui| {
        ui.spacing_mut().item_spacing.y = spacing;
        show_search_box(state, ui, width);
        // The selected component is `SandboxSession::focus` and nothing else.
        // The category stack, the camera row's context label and the isolated
        // scene all render from this one read, so they cannot disagree.
        let focus = state.sandbox.focus();
        for discipline in Discipline::ALL {
            let button = egui::Button::new(tr(discipline.title())).min_size(vec2(width, 0.0));
            if floating_control(
                ui,
                category_tag(discipline),
                true,
                focus == Some(discipline),
                button,
            )
            .on_hover_text(tr("Open this discipline in its own window."))
            .clicked()
            {
                if focus == Some(discipline) {
                    // Clicking the isolated component again shows the whole aircraft.
                    state.show_whole_aircraft();
                } else {
                    open_discipline_window(state, discipline);
                }
            }
        }
    });
    let used = stack.response.rect;
    register_overlay_rect(&ctx, "stack", used);
    let height = used.height().max(1.0);
    if (height - measured).abs() > 0.5 {
        ctx.data_mut(|d| d.insert_temp(stack_height_id(), height));
        ctx.request_repaint();
    }
    // Only a frame actually rendered at the full `CATEGORY_SPACING` may
    // update the shrink baseline: a shrunk frame's height must never be
    // mistaken for "how tall the stack wants to be", or the next frame would
    // see it fit, spring back to full spacing, overflow again, and shrink
    // again forever.
    if (spacing - CATEGORY_SPACING).abs() < 0.01 && (height - natural).abs() > 0.5 {
        ctx.data_mut(|d| d.insert_temp(stack_natural_height_id(), height));
        ctx.request_repaint();
    }
}

/// The whole-aircraft summary in the bottom-left corner of the viewport,
/// always present, with no box and no button chrome: "Sandbox from AVE" on
/// its own line over a small table of the derived geometry metrics, a symbol
/// and its right-aligned value per pair, three pairs a row when they fit in
/// the left quarter of the viewport, else two. It never widens toward the
/// viewport's centre: more metrics add rows. Its bottom edge sits at
/// `bottom`; returns its top edge (or `bottom` when there is no aircraft to
/// describe). The rectangle registers as `summary_text` so a gesture on it
/// never orbits.
pub fn show_summary_text(state: &AppState, ui: &mut Ui, viewport: Rect, bottom: f32) -> f32 {
    let metrics = metric_chips(state);
    if metrics.is_empty() {
        return bottom;
    }
    let font = egui::TextStyle::Small.resolve(ui.style());
    let color = ui.visuals().weak_text_color();
    let layout = |text: &str| {
        let mut job = crate::notation::job(text, &font, color);
        job.wrap.max_width = f32::INFINITY;
        ui.fonts(|fonts| fonts.layout_job(job))
    };
    let title = layout(&tr("Sandbox from AVE"));
    let cells: Vec<_> = metrics
        .iter()
        .map(|(symbol, value)| (layout(symbol), layout(value)))
        .collect();
    let symbol_width = cells
        .iter()
        .map(|cell| cell.0.size().x)
        .fold(0.0_f32, f32::max);
    let value_width = cells
        .iter()
        .map(|cell| cell.1.size().x)
        .fold(0.0_f32, f32::max);
    let line = cells
        .iter()
        .map(|cell| cell.0.size().y.max(cell.1.size().y))
        .fold(title.size().y, f32::max);
    let pair = symbol_width + ui.spacing().item_spacing.x + value_width;
    let gutter = crate::theme::space::COLUMN_GUTTER;
    let three = 3.0 * pair + 2.0 * gutter;
    let per_row = if three <= viewport.width() * SUMMARY_WIDTH_SHARE {
        3
    } else {
        2
    };
    let rows = cells.len().div_ceil(per_row);
    let table_width = per_row as f32 * pair + (per_row - 1) as f32 * gutter;
    let height = line * (rows + 1) as f32;
    let rect = Rect::from_min_size(
        pos2(viewport.left() + OVERLAY_INSET, bottom - height),
        vec2(table_width.max(title.size().x), height),
    );
    let painter = ui.painter();
    painter.galley(rect.left_top(), title, color);
    for (index, (symbol, value)) in cells.into_iter().enumerate() {
        let x = rect.left() + (index % per_row) as f32 * (pair + gutter);
        let y = rect.top() + line * (1 + index / per_row) as f32;
        let value_x = x + pair - value.size().x;
        painter.galley(pos2(x, y), symbol, color);
        painter.galley(pos2(value_x, y), value, color);
    }
    register_overlay_rect(ui.ctx(), "summary_text", rect);
    let hover = ui.interact(
        rect,
        egui::Id::new("sandbox_summary_text"),
        egui::Sense::hover(),
    );
    crate::notation::hover(hover, tr(CONVENTIONS));
    rect.top()
}

/// The Search button: opens the global Ctrl+K palette, scoped to the Sandbox
/// parameters while the Sandbox is open. Dimmed like the other floating
/// buttons until it is hovered or the palette is open.
fn show_search_box(state: &mut AppState, ui: &mut Ui, width: f32) {
    let open = state.shell.search.open;
    let label = format!("{}   Ctrl+K", tr("Search"));
    let button = egui::Button::new(label).min_size(vec2(width, 0.0));
    if floating_control(ui, "search", true, open, button)
        .on_hover_text(tr("Search the Sandbox parameters (Ctrl+K)."))
        .clicked()
    {
        crate::views::search_overlay::set_open(state, !open);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stack_centres_on_the_viewport_and_stays_inside_the_free_band() {
        let viewport = Rect::from_min_max(pos2(0.0, 100.0), pos2(1000.0, 700.0));
        // Tall viewport: centred about y = 400.
        assert!((stack_top(viewport, 150.0, 600.0, 200.0) - 300.0).abs() < 1e-6);
        // The bottom block pushes it up when centring would overlap it.
        assert!((stack_top(viewport, 150.0, 450.0, 200.0) - 250.0).abs() < 1e-6);
        // The camera row pushes it down on a short viewport.
        let short = Rect::from_min_max(pos2(0.0, 100.0), pos2(1000.0, 360.0));
        assert!((stack_top(short, 150.0, 300.0, 200.0) - 150.0).abs() < 1e-6);
    }

    /// A sandbox state whose registered-preset protections are still the
    /// ones `enter_sandbox` installs.
    fn sandbox_state() -> AppState {
        let mut state = AppState::default();
        assert!(state.enter_sandbox(true));
        state
    }

    #[test]
    fn one_value_drives_the_category_stack_the_context_label_and_the_scene() {
        let mut state = sandbox_state();
        assert_eq!(state.sandbox.focus(), None, "a new sandbox has no focus");

        for discipline in Discipline::ALL {
            open_discipline_window(&mut state, discipline);
            // The selection the stack highlights, the title the camera row
            // prints and the component the scene isolates are all this read.
            assert_eq!(state.sandbox.focus(), Some(discipline));
            assert_eq!(
                Discipline::ALL
                    .into_iter()
                    .filter(|d| state.sandbox.focus() == Some(*d))
                    .count(),
                1,
                "exactly one category is selected at a time"
            );
        }

        state.show_whole_aircraft();
        assert_eq!(state.sandbox.focus(), None, "Overview clears the selection");
    }

    #[test]
    fn a_focused_component_keeps_its_window_open_and_stays_selected() {
        let mut state = sandbox_state();
        open_discipline_window(&mut state, Discipline::Propulsion);
        assert!(state
            .sandbox
            .layout
            .open_disciplines
            .contains(&Discipline::Propulsion.id().to_owned()));
        // A second selection does not lose the first window, and the
        // selection follows the last chosen component.
        open_discipline_window(&mut state, Discipline::Fuselage);
        assert!(state
            .sandbox
            .layout
            .open_disciplines
            .contains(&Discipline::Propulsion.id().to_owned()));
        assert_eq!(state.sandbox.focus(), Some(Discipline::Fuselage));
    }

    #[test]
    fn every_category_button_has_a_distinct_tag() {
        let tags: std::collections::BTreeSet<&str> =
            Discipline::ALL.into_iter().map(category_tag).collect();
        assert_eq!(tags.len(), Discipline::ALL.len());
    }
}
