// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cells of the Sandbox station tables (wing and fuselage stations): one
//! boxed numeric cell style, one number format, one button row.

use egui::{vec2, Align, DragValue, Layout, RichText, Ui};

use crate::views::tr;

/// Width of one numeric cell of a station table, points.
const STATION_CELL_WIDTH: f32 = 76.0;
/// Width of the airfoil name box, points: wide enough for names such as
/// `SC2-0410` or `naca64a010` at the table's text size.
const AIRFOIL_CELL_WIDTH: f32 = 130.0;
/// Widest the airfoil box grows for a long name, points.
const AIRFOIL_CELL_MAX: f32 = 240.0;
/// Height of the buttons under a station table, points.
const STATION_BUTTON_HEIGHT: f32 = 24.0;

/// Decimals of a column, from the drag step its cells use: a fraction (step
/// 0.01) has three, a length (0.05) two, an angle or exponent (0.1) one. Every
/// cell of a column reads with the same decimals, so the digits line up.
pub(super) fn column_decimals(speed: f64) -> usize {
    if speed >= 0.1 {
        1
    } else if speed >= 0.05 {
        2
    } else {
        3
    }
}

/// `value` with `decimals` decimals, never `-0`.
pub(super) fn station_number(value: f64, decimals: usize) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    let text = format!("{value:.decimals$}");
    if text
        .strip_prefix('-')
        .is_some_and(|rest| rest.chars().all(|c| c == '0' || c == '.'))
    {
        text[1..].to_owned()
    } else {
        text
    }
}

/// One numeric cell of a station table: always a boxed value of the same
/// width, right-aligned under its right-aligned heading, greyed out when the
/// station derives it.
pub(super) fn station_cell(
    ui: &mut Ui,
    value: &mut f64,
    range: Option<std::ops::RangeInclusive<f64>>,
    speed: f64,
    enabled: bool,
) -> bool {
    let decimals = column_decimals(speed);
    let mut drag = DragValue::new(value)
        .speed(speed)
        .custom_formatter(move |number, _| station_number(number, decimals));
    if let Some(range) = range {
        drag = drag.range(range);
    }
    let height = ui.spacing().interact_size.y;
    ui.add_enabled_ui(enabled, |ui| {
        // A justified column aligned to the right draws the value at the
        // right edge of its box.
        ui.allocate_ui_with_layout(
            vec2(STATION_CELL_WIDTH, height),
            Layout::top_down_justified(Align::Max),
            |ui| ui.add(drag).changed(),
        )
        .inner
    })
    .inner
}

/// The airfoil cell: a text box, greyed out when the station shares an airfoil.
/// The box is wide enough for the whole name (at least [`AIRFOIL_CELL_WIDTH`]),
/// so a name is never cut mid-word; a name longer than [`AIRFOIL_CELL_MAX`] is
/// scrolled inside the box and shows whole as its tooltip.
pub(super) fn airfoil_cell(ui: &mut Ui, airfoil: &mut String, editable: bool) -> bool {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let text_width = ui
        .painter()
        .layout_no_wrap(airfoil.clone(), font, ui.visuals().text_color())
        .size()
        .x;
    let margin = ui.spacing().button_padding.x * 2.0 + 8.0;
    let width = (text_width + margin).clamp(AIRFOIL_CELL_WIDTH, AIRFOIL_CELL_MAX);
    let response = ui.add_enabled(
        editable,
        egui::TextEdit::singleline(airfoil)
            .desired_width(width)
            .min_size(vec2(width, 0.0)),
    );
    #[cfg(test)]
    AIRFOIL_BOXES.with(|boxes| boxes.borrow_mut().push((response.rect, airfoil.clone())));
    let response = if text_width + margin > AIRFOIL_CELL_MAX && !airfoil.is_empty() {
        response.on_hover_text(airfoil.as_str())
    } else {
        response
    };
    response.changed()
}

#[cfg(test)]
thread_local! {
    /// The airfoil boxes drawn on this thread, with their names.
    pub(super) static AIRFOIL_BOXES: std::cell::RefCell<Vec<(egui::Rect, String)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// A weak, small column heading of a station table: right-aligned over the
/// right-aligned numbers, left-aligned over the names (station, airfoil).
pub(super) fn station_heading(ui: &mut Ui, text: &str) {
    let label = RichText::new(tr(text)).weak().small();
    if matches!(text, "Station" | "Airfoil") {
        ui.label(label);
    } else {
        ui.allocate_ui_with_layout(
            vec2(STATION_CELL_WIDTH, ui.spacing().interact_size.y),
            Layout::right_to_left(Align::Center),
            |ui| ui.label(label),
        );
    }
}

/// The two buttons under a station table: natural width, one height, one top.
pub(super) fn station_buttons(ui: &mut Ui, create: &str, remove: &str) -> (bool, bool) {
    let mut clicked = (false, false);
    ui.horizontal(|ui| {
        let size = egui::vec2(0.0, STATION_BUTTON_HEIGHT);
        clicked.0 = ui
            .add(egui::Button::new(tr(create)).min_size(size))
            .clicked();
        clicked.1 = ui
            .add(egui::Button::new(tr(remove)).min_size(size))
            .clicked();
    });
    clicked
}

#[cfg(test)]
mod tests {
    use super::{column_decimals, station_number};
    use crate::state::AppState;

    #[test]
    fn every_cell_of_a_column_has_the_same_decimals() {
        assert_eq!(column_decimals(0.01), 3);
        assert_eq!(column_decimals(0.05), 2);
        assert_eq!(column_decimals(0.1), 1);
        let column: Vec<String> = [0.0, 2.4, 13.713, 8.469, -1.586]
            .iter()
            .map(|value| station_number(*value, 2))
            .collect();
        assert_eq!(column, ["0.00", "2.40", "13.71", "8.47", "-1.59"]);
        assert_eq!(station_number(-0.0001, 2), "0.00");
    }

    /// Painted strings with the right edge of each.
    fn painted(output: &egui::FullOutput) -> Vec<(String, f32)> {
        output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some((
                    text.galley.text().trim().to_owned(),
                    text.pos.x + text.galley.rect.right(),
                )),
                _ => None,
            })
            .collect()
    }

    /// Numeric headings end where the numbers under them end, and the Airfoil
    /// box is wide enough for its name.
    #[test]
    fn numeric_headings_are_right_aligned_over_right_aligned_values() {
        let mut state = AppState::default();
        let ctx = egui::Context::default();
        crate::theme::apply_theme(crate::theme::AppTheme::Dark, &ctx);
        let mut output = None;
        for _ in 0..3 {
            output = Some(ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        super::super::show_custom_wing_sections(&mut state, ui);
                    });
                },
            ));
        }
        let texts = painted(&output.expect("frames"));
        let heading = |name: &str| {
            let wanted = crate::views::tr(name);
            texts
                .iter()
                .find(|(text, _)| *text == wanted)
                .unwrap_or_else(|| panic!("{name} heading is not painted"))
                .1
        };
        // The chord column: every painted value ends within a padding of the
        // heading's right edge.
        let chord_right = heading("Chord (m)");
        let values_in_column: Vec<f32> = texts
            .iter()
            .filter(|(text, right)| {
                text.parse::<f64>().is_ok() && (right - chord_right).abs() < 12.0
            })
            .map(|(_, right)| *right)
            .collect();
        assert!(
            values_in_column.len() >= 2,
            "no chord values end at the heading edge {chord_right}: {texts:?}"
        );
        for right in &values_in_column {
            assert!((right - chord_right).abs() < 12.0);
        }
        const { assert!(super::AIRFOIL_CELL_WIDTH >= 120.0) };
    }

    /// The station table of the Sandbox Wing window, as the window lays it out:
    /// a vertical scroll area around a card. Returns the shot and the airfoil
    /// boxes drawn in the last frame.
    fn wing_table(width: f32) -> (crate::test_raster::Shot, Vec<(egui::Rect, String)>) {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        super::AIRFOIL_BOXES.with(|boxes| boxes.borrow_mut().clear());
        let shot = crate::test_raster::render(width, 560.0, 4, 0, |ui| {
            super::AIRFOIL_BOXES.with(|boxes| boxes.borrow_mut().clear());
            egui::ScrollArea::vertical().show(ui, |ui| {
                crate::theme::card_frame(ui).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(egui::RichText::new("Create wing sections").strong());
                    super::super::show_custom_wing_sections(&mut state, ui);
                });
            });
        });
        let boxes = super::AIRFOIL_BOXES.with(|boxes| boxes.borrow().clone());
        (shot, boxes)
    }

    /// The Airfoil column shows every airfoil name whole at the window's
    /// default width (380 pt) and wider, and the window has one heading.
    #[test]
    fn the_airfoil_column_shows_the_whole_name_at_every_window_width() {
        for width in [380.0, 700.0, 740.0, 780.0, 820.0, 1100.0, 1600.0] {
            let (shot, boxes) = wing_table(width);
            crate::test_raster::write_in(
                &crate::test_raster::render_dir(),
                &format!("wing-table-{width:.0}.png"),
                &shot,
            );
            assert!(
                boxes.len() >= 2,
                "{width}: the airfoil boxes were drawn: {boxes:?}"
            );
            for (rect, name) in &boxes {
                let needed = shot
                    .texts
                    .iter()
                    .find(|(text, _)| text == name)
                    .map(|(_, text_rect)| text_rect.width())
                    .unwrap_or(0.0);
                assert!(
                    rect.width() >= needed + 8.0 && rect.width() >= 120.0,
                    "{width}: the box of {name} is {} wide for a {needed} name",
                    rect.width()
                );
            }
            let headings = shot
                .texts
                .iter()
                .filter(|(text, _)| text == "Create wing sections" || text == "Wing stations")
                .count();
            assert_eq!(headings, 1, "{width}: one heading over the table");
        }
    }
}
