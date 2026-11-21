// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The "Modified" marker of a schema-generated form field.
//!
//! The marker states one fact: this field no longer holds the value the
//! schema declares as its default. That is a property of the value, so it is
//! derived from the value on every frame rather than remembered from an edit
//! gesture. A marker timed out after a
//! keystroke would make a field that is still non-default look untouched,
//! and the word must not be written into the editor's own value suffix, or a
//! numeric field would briefly render the non-numeric text "3.5  Modified".
//!
//! The marker is drawn once, on the last line of the field's caption beside
//! the reset arrow, so it never overlaps a value, a combo-box arrow or a
//! slider. It is a dot in the success colour, the word on hover, so it fits
//! the caption's `MARKER_ROOM` with the reset glyph.

use egui::{vec2, Sense, Ui, WidgetInfo, WidgetType};
use serde_json::Value;

use crate::theme::success_color;
use crate::views::tr;

/// Whether an edited slot differs from the schema default it started at.
///
/// This is the same comparison the reset arrow uses, so the two affordances
/// can never disagree about whether a field was modified.
pub(super) fn differs_from_default(slot: &Value, default: &Value) -> bool {
    // `1` and `1.0` are the same quantity.
    match (slot.as_f64(), default.as_f64()) {
        (Some(a), Some(b)) if slot.is_number() && default.is_number() => a != b,
        _ => slot != default,
    }
}

/// Diameter of the dot that marks a modified field, pt.
pub(crate) const MODIFIED_DOT: f32 = 7.0;

/// Draw the marker for a field whose value differs from its default.
pub(super) fn modified_marker(ui: &mut Ui, show: bool) {
    if !show {
        return;
    }
    let (rect, response) = ui.allocate_exact_size(vec2(MODIFIED_DOT, MODIFIED_DOT), Sense::hover());
    ui.painter().circle_filled(
        rect.center(),
        MODIFIED_DOT * 0.5,
        success_color(ui.visuals()),
    );
    let word = tr("Modified");
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &word));
    response.on_hover_text(format!(
        "{word}: {}",
        tr("This value differs from the preset default.")
    ));
}

#[cfg(test)]
mod tests {
    use super::differs_from_default;
    use serde_json::Value;

    #[test]
    fn a_value_equal_to_its_default_is_not_modified() {
        assert!(!differs_from_default(
            &Value::from(0.84),
            &Value::from(0.84)
        ));
        assert!(!differs_from_default(
            &Value::String("Passenger".to_owned()),
            &Value::String("Passenger".to_owned())
        ));
        assert!(!differs_from_default(&Value::Null, &Value::Null));
    }

    #[test]
    fn a_value_away_from_its_default_stays_modified_for_as_long_as_it_differs() {
        let default = Value::from(0.84);
        let edited = Value::from(3.5);
        // The same call any number of frames later returns the same answer:
        // the marker cannot time out while the value is still non-default.
        for _ in 0..1_000 {
            assert!(differs_from_default(&edited, &default));
        }
        assert!(!differs_from_default(&default, &default));
    }

    #[test]
    fn a_restored_default_clears_the_marker() {
        let default = Value::from(11_887.0);
        let mut slot = Value::from(12_000.0);
        assert!(differs_from_default(&slot, &default));
        slot = default.clone();
        assert!(!differs_from_default(&slot, &default));
    }
}
