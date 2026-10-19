// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A search field for long drop-down selectors (aircraft, engines, airports).
//!
//! The field sits at the top of the open list, takes keyboard focus when the
//! list opens, and filters the entries by a case-insensitive substring over
//! every searchable text of an entry (name, ICAO code, manufacturer, ...).
//! The query is kept in egui memory keyed by the selector and cleared when the
//! list closes, so each opening starts unfiltered.

use egui::{Id, Ui};

/// Case-insensitive substring match of `query` over any of `fields`.
///
/// An empty (or all-blank) query matches everything. Matching is on the
/// trimmed query so a trailing space typed by accident does not hide results.
pub fn matches(query: &str, fields: &[&str]) -> bool {
    let needle = query.trim().to_lowercase();
    needle.is_empty()
        || fields
            .iter()
            .any(|field| field.to_lowercase().contains(&needle))
}

fn query_id(salt: &str) -> Id {
    Id::new(("selector_search_query", salt))
}

fn focused_id(salt: &str) -> Id {
    Id::new(("selector_search_focused", salt))
}

/// Draw the search field inside an open list and return the current query.
///
/// The field is focused on the first frame the list is open.
pub fn search_field(ui: &mut Ui, salt: &str) -> String {
    let query_key = query_id(salt);
    let focused_key = focused_id(salt);
    let mut query: String = ui
        .data_mut(|data| data.get_temp(query_key))
        .unwrap_or_default();
    let first_frame = !ui
        .data_mut(|data| data.get_temp::<bool>(focused_key))
        .unwrap_or(false);
    let response = ui.add(
        egui::TextEdit::singleline(&mut query)
            .hint_text(crate::views::tr("Search..."))
            .desired_width(ui.available_width().max(160.0)),
    );
    if first_frame {
        response.request_focus();
        ui.data_mut(|data| data.insert_temp(focused_key, true));
    }
    ui.data_mut(|data| data.insert_temp(query_key, query.clone()));
    ui.separator();
    query
}

/// Reset the search state once the list is closed, so the next opening starts
/// with an empty query and a focused field. Pass the combo box's `inner`
/// (`None` while closed).
pub fn reset_when_closed<R>(ui: &Ui, salt: &str, inner: &Option<R>) {
    if inner.is_none() {
        ui.data_mut(|data| {
            data.remove::<String>(query_id(salt));
            data.remove::<bool>(focused_id(salt));
        });
    }
}

/// Show "no match" text when a filtered list came out empty.
pub fn show_no_match(ui: &mut Ui) {
    ui.label(egui::RichText::new(crate::views::tr("No entry matches the search.")).weak());
}

/// A searchable list of plain-name entries (engines).
///
/// `searchable` yields the texts to match for an entry. Returns the entry the
/// user clicked, if any.
pub fn named_list_combo(
    ui: &mut Ui,
    salt: &str,
    width: Option<f32>,
    selected_text: &str,
    names: &[String],
    current: &str,
) -> Option<String> {
    let mut chosen = None;
    let mut combo = egui::ComboBox::from_id_salt(salt).selected_text(selected_text);
    if let Some(width) = width {
        combo = combo.width(width);
    }
    let response = combo.show_ui(ui, |ui| {
        let query = search_field(ui, salt);
        let mut shown = 0usize;
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                for name in names {
                    if !matches(&query, &[name.as_str()]) {
                        continue;
                    }
                    shown += 1;
                    if ui.selectable_label(name == current, name).clicked() {
                        chosen = Some(name.clone());
                    }
                }
            });
        if shown == 0 {
            show_no_match(ui);
        }
    });
    reset_when_closed(ui, salt, &response.inner);
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_query_matches_everything() {
        assert!(matches("", &["A320"]));
        assert!(matches("   ", &["A320"]));
    }

    #[test]
    fn match_is_case_insensitive_substring_over_any_field() {
        assert!(matches("a32", &["Airbus A320-200", "A320"]));
        assert!(matches("LEMD", &["Madrid-Barajas", "lemd"]));
        assert!(matches("barajas", &["Madrid-Barajas", "LEMD"]));
        assert!(matches("  cfm  ", &["CFM56-5B4"]));
        assert!(!matches("boeing", &["Airbus A320-200", "A320"]));
    }

    #[test]
    fn query_resets_when_the_list_closes() {
        let ctx = egui::Context::default();
        let mut observed = Vec::new();
        for (frame, open) in [true, true, false, true].into_iter().enumerate() {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    if open {
                        if frame == 1 {
                            ui.data_mut(|d| d.insert_temp(query_id("t"), "abc".to_owned()));
                        }
                        let query = search_field(ui, "t");
                        observed.push(query);
                    } else {
                        reset_when_closed::<()>(ui, "t", &None);
                    }
                });
            });
        }
        assert_eq!(observed, vec!["", "abc", ""]);
    }
}
