// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Short-lived, local acknowledgement for schema-generated form fields.
//!
//! The run log records the settled value after an edit gesture becomes idle.
//! This badge provides immediate confirmation inside the control without
//! turning a wheel or drag interaction into a sequence of log messages.

use std::time::Duration;

use egui::{pos2, Align2, FontId, Id, Response, Ui};

use crate::theme::success_color;
use crate::views::tr;

/// How long an edited form field keeps its local acknowledgement visible.
pub(super) const MODIFIED_INDICATOR_DURATION: Duration = Duration::from_millis(1_100);

/// Remember that a field changed, without adding a second line below it.
pub(super) fn record_modified(ui: &mut Ui, id: Id, changed: bool) {
    if !changed {
        return;
    }
    let now = ui.input(|input| input.time);
    ui.memory_mut(|memory| memory.data.insert_temp(id, now));
    ui.ctx().request_repaint();
    ui.ctx().request_repaint_after(MODIFIED_INDICATOR_DURATION);
}

/// Whether the field should render its acknowledgement inside the editor.
pub(super) fn is_modified(ui: &mut Ui, id: Id) -> bool {
    let now = ui.input(|input| input.time);
    let Some(changed_at) = ui.memory(|memory| memory.data.get_temp::<f64>(id)) else {
        return false;
    };
    let remaining = MODIFIED_INDICATOR_DURATION.as_secs_f64() - (now - changed_at);
    if remaining <= 0.0 {
        ui.memory_mut(|memory| memory.data.remove::<f64>(id));
        return false;
    }

    ui.ctx()
        .request_repaint_after(Duration::from_secs_f64(remaining));
    true
}

/// Paint the acknowledgement inside a wide editor, including while a numeric
/// editor has keyboard focus and therefore hides its normal value suffix.
pub(super) fn paint_modified_indicator(ui: &Ui, response: &Response, show: bool) {
    if !show {
        return;
    }
    ui.painter().text(
        pos2(response.rect.right() - 8.0, response.rect.center().y),
        Align2::RIGHT_CENTER,
        tr("Modified"),
        FontId::proportional(11.0),
        success_color(ui.visuals()),
    );
}

#[cfg(test)]
mod tests {
    use super::{is_modified, record_modified, MODIFIED_INDICATOR_DURATION};
    use egui::{CentralPanel, Context, Id, RawInput};

    #[test]
    fn acknowledgement_lasts_long_enough_to_be_seen_without_lingering() {
        assert!(MODIFIED_INDICATOR_DURATION >= std::time::Duration::from_secs(1));
        assert!(MODIFIED_INDICATOR_DURATION < std::time::Duration::from_secs(2));
    }

    #[test]
    fn acknowledgement_survives_into_the_following_render_frame() {
        let context = Context::default();
        let id = Id::new("modified-field");
        context.begin_pass(RawInput {
            time: Some(1.0),
            ..Default::default()
        });
        CentralPanel::default().show(&context, |ui| record_modified(ui, id, true));
        let _ = context.end_pass();

        context.begin_pass(RawInput {
            time: Some(1.1),
            ..Default::default()
        });
        let mut visible = false;
        CentralPanel::default().show(&context, |ui| visible = is_modified(ui, id));
        let _ = context.end_pass();

        assert!(visible);
    }
}
