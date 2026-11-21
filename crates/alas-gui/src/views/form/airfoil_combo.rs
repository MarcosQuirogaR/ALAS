// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The airfoil section field: one searchable drop-down. The built-in airfoils
//! come first, then the user's imported ones ("My airfoils"), and an
//! "Import..." entry at the foot asks the page to open the file dialog.

use alas_config::Field;
use egui::{ComboBox, Id, Ui};
use serde_json::Value;

use crate::views::tr;

/// The egui memory slot holding the frame an import was asked for.
fn import_request_id() -> Id {
    Id::new("airfoil_import_requested")
}

/// Take the pending "Import..." request, if one was raised this frame or the
/// one before. The form cannot open the dialog itself (it has no application
/// state), so the page that hosts it calls this after drawing.
pub(crate) fn take_import_request(ctx: &egui::Context) -> bool {
    let requested = ctx
        .data(|data| data.get_temp::<u64>(import_request_id()))
        .is_some_and(|frame| ctx.cumulative_pass_nr() <= frame.saturating_add(2));
    ctx.data_mut(|data| data.remove::<u64>(import_request_id()));
    requested
}

/// An airfoil id as shown: `naca0012` reads "NACA 0012" and `sc20410` or
/// `SC2-0710` reads "SC(2)-0410". Any other id is shown as it is. Display
/// only: the stored value and the preset ids never change.
pub(crate) fn display_airfoil_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("naca") {
        let rest = rest.trim_start_matches(['-', ' ', '_']);
        if !rest.is_empty() && rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return format!("NACA {}", rest.to_ascii_uppercase());
        }
    }
    if let Some(rest) = lower.strip_prefix("sc2") {
        let digits = rest.trim_start_matches(['-', ' ', '_']);
        if digits.len() == 4 && digits.chars().all(|c| c.is_ascii_digit()) {
            return format!("SC(2)-{digits}");
        }
    }
    name.to_owned()
}

/// One row of the open list.
enum Row<'a> {
    Heading(String),
    Airfoil(&'a str),
}

/// Edit an airfoil name. Returns whether the value changed.
pub(super) fn edit_airfoil(
    ui: &mut Ui,
    field: &Field,
    slot: &mut Value,
    id_prefix: &str,
    builtin: &[String],
    current: &str,
) -> bool {
    let salt = format!("{id_prefix}::{}", field.name);
    let mut changed = false;
    let selected_owned = if current.is_empty() {
        "-".to_owned()
    } else {
        display_airfoil_name(current)
    };
    let selected = selected_owned.as_str();
    let response = ComboBox::from_id_salt(&salt)
        .icon(crate::combo_icon::down_arrow)
        .width(ui.available_width())
        .height(crate::selector_search::FIXED_SEARCH_COMBO_HEIGHT)
        .truncate()
        .selected_text(selected)
        .show_ui(ui, |ui| {
            let query = crate::selector_search::search_field(ui, &salt);
            let mine = alas_geom::airfoil_io::names();
            let matching = |name: &str| {
                if query.is_empty() {
                    return true;
                }
                crate::selector_search::matches(&query, &[name, &display_airfoil_name(name)])
            };
            let mut rows: Vec<Row> = Vec::new();
            // A value from an older save stays selectable even when it is in
            // neither list.
            if !current.is_empty()
                && !builtin.iter().any(|name| name == current)
                && !mine.contains(&current)
            {
                rows.push(Row::Airfoil(current));
            }
            rows.extend(
                builtin
                    .iter()
                    .filter(|name| !mine.contains(&name.as_str()) && matching(name))
                    .map(|name| Row::Airfoil(name.as_str())),
            );
            let mine_rows: Vec<&str> = mine.iter().copied().filter(|name| matching(name)).collect();
            if !mine_rows.is_empty() {
                rows.push(Row::Heading(tr("My airfoils")));
                rows.extend(mine_rows.into_iter().map(Row::Airfoil));
            }
            if rows.is_empty() {
                crate::selector_search::show_no_match(ui);
            }
            // Virtualized: the library holds well over a thousand names, so
            // only the visible rows are laid out and none are dropped. The
            // list is the only scrolling part; the search field above and
            // the import entry below stay in place.
            let row_height =
                ui.text_style_height(&egui::TextStyle::Button) + ui.spacing().item_spacing.y;
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .auto_shrink([false, true])
                .show_rows(ui, row_height, rows.len(), |ui, range| {
                    for row in &rows[range] {
                        match row {
                            Row::Heading(text) => {
                                ui.label(egui::RichText::new(text).weak().small());
                            }
                            Row::Airfoil(name) => {
                                if ui
                                    .selectable_label(current == *name, display_airfoil_name(name))
                                    .clicked()
                                {
                                    *slot = Value::String((*name).to_owned());
                                    changed = true;
                                }
                            }
                        }
                    }
                });
            ui.separator();
            if ui
                .selectable_label(false, tr("Import..."))
                .on_hover_text(tr("Import a Selig .dat airfoil file into My airfoils."))
                .clicked()
            {
                let frame = ui.ctx().cumulative_pass_nr();
                ui.ctx()
                    .data_mut(|data| data.insert_temp(import_request_id(), frame));
            }
        });
    crate::selector_search::reset_when_closed(ui, &salt, &response.inner);
    response.response.on_hover_text(selected);
    changed
}

#[cfg(test)]
mod tests {
    use super::display_airfoil_name;

    #[test]
    fn airfoil_ids_share_one_display_convention() {
        assert_eq!(display_airfoil_name("naca0012"), "NACA 0012");
        assert_eq!(display_airfoil_name("NACA64A010"), "NACA 64A010");
        assert_eq!(display_airfoil_name("sc20410"), "SC(2)-0410");
        assert_eq!(display_airfoil_name("SC2-0714"), "SC(2)-0714");
        assert_eq!(display_airfoil_name("e387"), "e387");
        assert_eq!(display_airfoil_name("nacelle"), "nacelle");
    }
}
