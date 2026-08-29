// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared presentation primitives for the fixed-wing UAV workflow.
//!
//! The UAV workflow has more evidence than a normal configuration page. These
//! small helpers keep that density readable by using the desktop's card and
//! responsive-column language instead of a single unbounded form.

use egui::{CollapsingHeader, RichText, Ui};

use crate::theme::card_frame;

use super::super::tr;

/// The narrowest width at which two UAV form cards remain independently legible.
const MIN_UAV_CARD_COLUMN_WIDTH: f32 = 390.0;

/// Render one consistently spaced UAV content card.
pub(super) fn card(
    ui: &mut Ui,
    title: &str,
    description: Option<&str>,
    contents: impl FnOnce(&mut Ui),
) {
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr(title)).strong());
        if let Some(description) = description {
            ui.label(RichText::new(tr(description)).weak().small());
        }
        ui.add_space(4.0);
        contents(ui);
    });
}

/// Render a compact disclosure inside the same card system as primary content.
pub(super) fn collapsing_card(
    ui: &mut Ui,
    id: impl std::hash::Hash,
    title: &str,
    description: Option<&str>,
    default_open: bool,
    contents: impl FnOnce(&mut Ui),
) {
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        CollapsingHeader::new(RichText::new(tr(title)).strong())
            .id_salt(id)
            .default_open(default_open)
            .show(ui, |ui| {
                if let Some(description) = description {
                    ui.label(RichText::new(tr(description)).weak().small());
                    ui.add_space(4.0);
                }
                contents(ui);
            });
    });
}

/// Number of equal-width form-card columns that fit the current page width.
pub(super) fn card_column_count(available_width: f32) -> usize {
    ((available_width / MIN_UAV_CARD_COLUMN_WIDTH).floor() as usize).clamp(1, 2)
}

#[cfg(test)]
mod tests {
    use super::{card_column_count, MIN_UAV_CARD_COLUMN_WIDTH};

    #[test]
    fn uav_cards_expand_only_when_each_column_stays_readable() {
        assert_eq!(card_column_count(MIN_UAV_CARD_COLUMN_WIDTH - 1.0), 1);
        assert_eq!(card_column_count(MIN_UAV_CARD_COLUMN_WIDTH * 2.0), 2);
    }
}
