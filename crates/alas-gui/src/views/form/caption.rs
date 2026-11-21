// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Field captions: the one label band every cell of a form grid draws above
//! its value box, schema field, checkbox or hand-built editor alike.
//!
//! A caption takes one line unless its text wraps. The captions of one grid
//! row share the height of the tallest of them, so every value box of the row
//! starts at one y: the grid publishes the row's band on the cell's `Ui`
//! stack ([`CaptionRow`], measured in the last frame) and every caption
//! reports its own height for the next frame. Outside a grid a caption is as
//! tall as its own text.
//!
//! The text stands at the bottom of the band, so every caption sits
//! [`LABEL_TO_BOX`] above its own box however much shorter it is than the
//! tallest caption of its row. The markers of the box (the modified dot, the
//! reset glyph, a lock, a note) stand right-aligned on the caption's last
//! line, over the box's right edge.

use egui::{
    pos2, vec2, Align, Context, Id, Layout, Rect, Response, Sense, TextStyle, TextWrapMode, Ui,
    UiBuilder, WidgetInfo, WidgetText, WidgetType,
};

#[cfg(doc)]
use crate::theme::space::LABEL_TO_BOX;

/// The `Ui` stack tag of a grid cell: the caption band of its row.
pub(super) const CAPTION_ROW_TAG: &str = "alas_caption_row";

/// Space between two markers on a caption line, pt.
const MARKER_SPACING: f32 = 4.0;
/// Horizontal padding of a button among the markers (the reset glyph), pt:
/// a framed glyph, not a full-size button.
const MARKER_BUTTON_PADDING: f32 = 3.0;

/// What a grid tells the captions of one of its rows.
#[derive(Clone, Debug, Default)]
pub(super) struct CaptionRow {
    /// Height of the row's tallest caption, as measured in the last frame, pt.
    pub(super) band: f32,
    /// Where the captions of the row report their height: the row's own
    /// slot and, for the first row of a grid drawn in a cell of another grid,
    /// the slots of the enclosing row too.
    pub(super) report: Vec<Id>,
}

/// The caption band of the grid row `ui` is drawn in, if any.
pub(super) fn caption_row(ui: &Ui) -> Option<CaptionRow> {
    ui.stack()
        .iter()
        .find_map(|node| node.tags().get_downcast::<CaptionRow>(CAPTION_ROW_TAG))
        .cloned()
}

/// Record a caption `height` in every slot of `keys` (the tallest wins).
fn report(ctx: &Context, keys: &[Id], height: f32) {
    ctx.data_mut(|data| {
        for key in keys {
            let slot = data.get_temp_mut_or_insert_with(*key, || 0.0_f32);
            *slot = slot.max(height);
        }
    });
}

/// Height of one caption line: a line of body text, which the small reset
/// button and the lock icon beside a caption fit in.
pub(crate) fn caption_line_height(ui: &Ui) -> f32 {
    ui.text_style_height(&TextStyle::Body)
        .max(ui.text_style_height(&TextStyle::Button))
}

/// Draw a caption at the top of the cell `ui` and return its response.
///
/// `text` takes the cell's width while its last line leaves the markers
/// their room: `reserve` (kept whether or not they show, so a marker that
/// comes and goes never re-wraps the text) or what `trailing` took in the
/// last frame, whichever is wider. Else it wraps short of that room.
/// `trailing` is drawn right-aligned on the caption's last line. The band
/// under the text is the row's, the text at its bottom, so the box that
/// follows starts on the row's line and the caption stands on its box.
pub(crate) fn caption(
    ui: &mut Ui,
    text: impl Into<WidgetText>,
    reserve: f32,
    trailing: impl FnOnce(&mut Ui),
) -> Response {
    let width = ui.available_width().max(1.0);
    let line = caption_line_height(ui);
    let markers_key = ui.next_auto_id().with("caption_markers");
    let measured = ui
        .ctx()
        .data(|data| data.get_temp::<f32>(markers_key))
        .unwrap_or(0.0);
    let room = reserve.max(measured);
    let text = text.into();
    let layout = |ui: &Ui, text: WidgetText, wrap: f32| {
        text.into_galley(ui, Some(TextWrapMode::Wrap), wrap, TextStyle::Body)
    };
    let mut galley = layout(ui, text.clone(), width);
    let last_line = galley.rows.last().map_or(0.0, |row| row.rect.width());
    if room > 0.0 && last_line + room > width {
        galley = layout(ui, text, (width - room).max(width * 0.3).max(1.0));
    }
    let natural = galley.size().y.max(line);
    let band = match caption_row(ui) {
        Some(row) => {
            report(ui.ctx(), &row.report, natural);
            row.band.max(natural)
        }
        None => natural,
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, band), Sense::hover());
    let label = galley.text().to_owned();
    let text_top = rect.bottom() - galley.size().y.max(line);
    ui.painter().galley(
        pos2(rect.left(), text_top),
        galley,
        ui.visuals().text_color(),
    );
    // The markers on the last line, from the right edge. What they take is
    // kept for the next frame's wrap.
    let mut markers = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(rect.left(), rect.bottom() - line),
                rect.right_bottom(),
            ))
            .layout(Layout::right_to_left(Align::Center)),
    );
    markers.spacing_mut().item_spacing.x = MARKER_SPACING;
    markers.spacing_mut().button_padding.x = MARKER_BUTTON_PADDING;
    trailing(&mut markers);
    let used = markers.min_rect().width();
    let used = if used > 0.5 {
        used + MARKER_SPACING
    } else {
        0.0
    };
    if (used - measured).abs() > 0.5 {
        ui.ctx()
            .data_mut(|data| data.insert_temp(markers_key, used));
        // Only markers wider than the reserve change the wrap.
        if (reserve.max(used) - room).abs() > 0.5 {
            ui.ctx().request_repaint();
        }
    }
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &label));
    response
}
