// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

/// Print a length in metres with the inch figure the cabin trade quotes.
fn span_text(value_m: f64) -> String {
    format!(
        "{value_m:.2} m ({:.0} in)",
        value_m * INCHES_PER_METRE
    )
}

/// Draw a dimension between two heights, offset laterally from the section.
fn dimension_vertical(
    scene: &mut Scene,
    map: SectionMap,
    y_m: f64,
    interval: [f64; 2],
    text: &str,
    ink: &SectionInk,
) {
    let (low, high) = (map.at(y_m, interval[0]), map.at(y_m, interval[1]));
    let stroke = Stroke::new(ink.muted, 1.1);
    scene.add(SceneElement::Line {
        p1: low,
        p2: high,
        stroke: stroke.clone(),
    });
    for end in [low, high] {
        scene.add(SceneElement::Line {
            p1: [end[0] - 5.0, end[1]],
            p2: [end[0] + 5.0, end[1]],
            stroke: stroke.clone(),
        });
    }
    scene.add(SceneElement::Text {
        text: text.to_owned(),
        pos: [low[0] + if y_m < 0.0 { -7.0 } else { 7.0 }, 0.5 * (low[1] + high[1])],
        font_size: 9.0,
        color: ink.muted,
        align: TextAlign::Center,
        baseline: if y_m < 0.0 {
            TextBaseline::Bottom
        } else {
            TextBaseline::Top
        },
        angle_deg: -90.0,
        bold: false,
    });
}

/// Draw a dimension between two lateral positions, on a line below the section
/// with extension lines back up to what is being measured.
fn dimension_horizontal(
    scene: &mut Scene,
    map: SectionMap,
    z_m: f64,
    interval: [f64; 2],
    measured_z_m: f64,
    text: &str,
    ink: &SectionInk,
) {
    let (left, right) = (map.at(interval[0], z_m), map.at(interval[1], z_m));
    let stroke = Stroke::new(ink.muted, 1.1);
    let thin = Stroke::dashed(ink.muted, 0.7, 3.0, 3.0);
    scene.add(SceneElement::Line {
        p1: left,
        p2: right,
        stroke: stroke.clone(),
    });
    for (end, y_m) in [(left, interval[0]), (right, interval[1])] {
        scene.add(SceneElement::Line {
            p1: [end[0], end[1] - 5.0],
            p2: [end[0], end[1] + 5.0],
            stroke: stroke.clone(),
        });
        scene.add(SceneElement::Line {
            p1: map.at(y_m, measured_z_m),
            p2: [end[0], end[1] - 5.0],
            stroke: thin.clone(),
        });
    }
    label(
        scene,
        text.to_owned(),
        [0.5 * (left[0] + right[0]), left[1] - 7.0],
        ink.muted,
        9.0,
        TextAlign::Center,
        TextBaseline::Bottom,
        false,
    );
}

/// Dimension the cabin the way a cross-section drawing is read: standing height
/// on each passenger deck, floor width, and the depth from the lowest cabin
/// floor to the hold floor.
fn draw_dimensions(scene: &mut Scene, map: SectionMap, slice: &SectionSlice<'_>, ink: &SectionInk) {
    let extent = bounds(&slice.outer);
    let outside_left = extent[0] - 0.22;
    let outside_right = extent[2] + 0.22;
    let passenger: Vec<&DeckSlice<'_>> = slice
        .decks
        .iter()
        .filter(|deck| deck.deck.passenger)
        .collect();
    for (index, deck) in passenger.iter().enumerate() {
        let y_m = if index % 2 == 0 {
            outside_left
        } else {
            outside_right
        };
        dimension_vertical(
            scene,
            map,
            y_m,
            [deck.deck.floor_z_m, deck.deck.ceiling_z_m],
            &span_text(deck.deck.ceiling_z_m - deck.deck.floor_z_m),
            ink,
        );
    }
    if let Some(lowest) = passenger.last() {
        dimension_horizontal(
            scene,
            map,
            extent[1] - 16.0 / map.scale,
            [
                -lowest.deck.usable_width_m * 0.5,
                lowest.deck.usable_width_m * 0.5,
            ],
            lowest.deck.floor_z_m,
            &format!("cabin floor {}", span_text(lowest.deck.usable_width_m)),
            ink,
        );
        if let Some(hold) = slice.decks.iter().find(|deck| !deck.deck.passenger) {
            dimension_vertical(
                scene,
                map,
                outside_left,
                [hold.deck.floor_z_m, lowest.deck.floor_z_m],
                &span_text(lowest.deck.floor_z_m - hold.deck.floor_z_m),
                ink,
            );
        }
    }
}

/// Break a sentence into lines that fit the canvas at the notes font size.
fn wrap(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= max_chars => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
}

/// Draw one legend entry and return the x position the next one starts at.
fn legend_entry(scene: &mut Scene, x: f64, y: f64, color: Color, text: &str, ink: &SectionInk) -> f64 {
    scene.add(SceneElement::Rect {
        x,
        y: y - 6.0,
        width: 13.0,
        height: 13.0,
        rx: 2.0,
        fill: Some(Fill::new(color)),
        stroke: Some(Stroke::new(ink.outline, 0.9)),
    });
    label(
        scene,
        text.to_owned(),
        [x + 18.0, y],
        ink.muted,
        9.0,
        TextAlign::Left,
        TextBaseline::Middle,
        false,
    );
    x + 30.0 + text.chars().count() as f64 * 5.4
}

/// Draw the key, the station statement and the geometric findings.
fn draw_notes(scene: &mut Scene, slice: &SectionSlice<'_>, cabin: &CabinScene, ink: &SectionInk) {
    let classes: Vec<&str> = slice
        .decks
        .iter()
        .filter_map(|deck| deck.row.map(|row| row.class.as_str()))
        .collect();
    let mut x = 40.0;
    let key_y = NOTES_TOP;
    for class in ["First", "Business", "Premium", "Economy"] {
        if classes.contains(&class) {
            x = legend_entry(scene, x, key_y, class_color(class), class, ink);
        }
    }
    x = legend_entry(scene, x, key_y, Color::from_hex("#e2ded4"), "Overhead bin", ink);
    x = legend_entry(scene, x, key_y, Color::from_hex("#98a1aa"), "1.75 m occupant", ink);
    if !slice.cargo.is_empty() {
        x = legend_entry(scene, x, key_y, Color::from_hex("#a8a4d8"), "ULD", ink);
    }
    x = legend_entry(scene, x, key_y, Color::from_hex("#8fd3f4"), "Window", ink);
    let _ = legend_entry(scene, x, key_y, ink.structure, "Shell and structure", ink);

    let hold_note = slice.hold.as_ref().map_or_else(
        || "no hold contour at this station".to_owned(),
        |hold| {
            let contained: Vec<f64> = slice
                .cargo
                .iter()
                .filter(|item| ring_within(&item.ring, hold))
                .map(|item| ring_area(&item.ring))
                .collect();
            let area = ring_area(hold);
            let used: f64 = contained.iter().sum();
            let share = if area > 0.0 { used / area * 100.0 } else { 0.0 };
            format!(
                "hold section {area:.2} m2 holds {} of {} cut item(s), covering {:.0}% of its area",
                contained.len(),
                slice.cargo.len(),
                share.max(0.0)
            )
        },
    );
    let mut lines: Vec<(String, Color)> = Vec::new();
    for line in wrap(
        &format!(
            "Station {} at x = {:.2} m, chosen for coverage: seat rows cut on {} of {} passenger deck(s), {} overhead run(s), {} cargo item(s); {hold_note}.",
            slice.choice.id,
            slice.choice.x_m,
            slice.choice.decks_with_rows,
            slice.choice.passenger_decks,
            slice.choice.overhead_runs,
            slice.choice.cargo_items,
        ),
        NOTE_WRAP_CHARS,
    ) {
        lines.push((line, ink.muted));
    }
    for line in wrap(
        &format!(
            "Contours {}; seats, overhead runs and cargo solver-resolved; window apertures {}, {}. Engineering visualization, not certified geometry.",
            cabin
                .stations
                .first()
                .map_or("derived", |station| station.outer.fidelity.as_str()),
            cabin.windows.status,
            if slice.windows_projected {
                "projected onto this plane from the nearest exported longitudinal position and drawn dashed"
            } else {
                "cut by this plane"
            }
        ),
        NOTE_WRAP_CHARS,
    ) {
        lines.push((line, ink.muted));
    }
    let findings = if slice.findings.is_empty() {
        "No geometric finding at this station.".to_owned()
    } else {
        format!(
            "{} finding(s): {}.",
            slice.findings.len(),
            slice.findings.join("; ")
        )
    };
    let finding_color = if slice.findings.is_empty() {
        ink.muted
    } else {
        Color::from_hex("#c2620f")
    };
    for line in wrap(&findings, NOTE_WRAP_CHARS) {
        lines.push((line, finding_color));
    }
    for (index, (line, color)) in lines.iter().enumerate() {
        label(
            scene,
            line.clone(),
            [40.0, key_y + 26.0 + index as f64 * 13.0],
            *color,
            8.5,
            TextAlign::Left,
            TextBaseline::Middle,
            false,
        );
    }
}

/// Draw a dimensioned transverse cabin section from a resolved cabin scene.
///
/// Every drawn part is cut by one longitudinal plane. The station is chosen for
/// coverage, not for appearance, and nothing is borrowed from a neighbouring
/// station to fill the picture: a deck with no seat row at the drawn station is
/// drawn empty and said to be empty, and an item that does not fit its
/// container is drawn where it is with a finding against it.
pub fn figure_cabin_section(cabin: &CabinScene, theme: Option<&str>) -> Scene {
    let palette = get_palette(theme);
    let ink = SectionInk::new(palette.name);
    let mut scene = Scene::new(WIDTH, HEIGHT, Some(Color::from_hex(palette.bg)));
    scene.title = Some("Cabin Cross-Section".to_owned());
    scene.suppress_derived_title();
    let aircraft = cabin
        .provenance
        .aircraft_preset
        .clone()
        .unwrap_or_else(|| "Aircraft".to_owned());
    label(
        &mut scene,
        format!("{aircraft} cabin cross-section"),
        [WIDTH * 0.5, 30.0],
        ink.text,
        16.0,
        TextAlign::Center,
        TextBaseline::Middle,
        true,
    );
    let Some(slice) = slice_section(cabin) else {
        label(
            &mut scene,
            "No exported station carries an interior contour to section.",
            [WIDTH * 0.5, HEIGHT * 0.5],
            ink.text,
            12.0,
            TextAlign::Center,
            TextBaseline::Middle,
            false,
        );
        return scene;
    };
    let extent = bounds(&slice.outer);
    let abreast: Vec<String> = slice
        .decks
        .iter()
        .filter_map(|deck| deck.row)
        .map(|row| {
            format!(
                "{} abreast {}",
                row.abreast,
                row.blocks
                    .iter()
                    .map(i64::to_string)
                    .collect::<Vec<_>>()
                    .join("-")
            )
        })
        .collect();
    let aisle = slice
        .decks
        .iter()
        .filter_map(|deck| deck.row.map(|row| row.aisle_width_m))
        .fold(f64::NAN, f64::max);
    label(
        &mut scene,
        format!(
            "Outer section {:.2} m wide x {:.2} m high | {}{}",
            extent[2] - extent[0],
            extent[3] - extent[1],
            abreast.join(" and "),
            if aisle.is_nan() {
                String::new()
            } else {
                format!(" | aisle {aisle:.2} m")
            }
        ),
        [WIDTH * 0.5, 52.0],
        ink.muted,
        10.0,
        TextAlign::Center,
        TextBaseline::Middle,
        false,
    );
    let map = SectionMap::fit(&slice.outer);
    draw_structure(&mut scene, map, &slice, &ink);
    for item in &slice.cargo {
        draw_cargo(&mut scene, map, item, &ink);
    }
    for deck in &slice.decks {
        draw_deck(&mut scene, map, deck, &ink);
    }
    for window in &slice.windows {
        draw_window(&mut scene, map, window, slice.windows_projected, &ink);
    }
    draw_dimensions(&mut scene, map, &slice, &ink);
    draw_notes(&mut scene, &slice, cabin, &ink);
    scene
}

#[cfg(test)]
// A section is judged by what it refuses to draw, so these tests build real
// preset geometry and assert the refusals as well as the picture. A fixture
// that cannot be built is a failed test, so they panic on it deliberately.
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use alas_config::{presets, AlasConfig};
    use alas_geom::builder::AircraftBuilder;
    use alas_payload::build::build_payload_layout;
    use alas_pipeline::CabinSceneInputs;

    fn scene_for(name: &str) -> CabinScene {
        let preset = presets::get(name).expect("registered preset");
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
            .expect("preset configuration");
        let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("preset geometry");
        let layout = build_payload_layout(&airplane, &config, 0.0, 0.0).expect("payload layout");
        CabinScene::from_parts(
            &config,
            CabinSceneInputs {
                design: preset.design_vector,
                airplane: &airplane,
                layout: &layout,
                source: "test fixture",
            },
        )
        .expect("cabin scene")
    }

    #[test]
    fn containment_and_half_width_agree_with_a_known_square() {
        let square = rectangle(0.0, 2.0, -1.0, 1.0);
        assert!(contains_point(&square, [0.0, 0.0]));
        assert!(!contains_point(&square, [1.5, 0.0]));
        assert!(ring_within(&rectangle(0.0, 1.0, -0.5, 0.5), &square));
        assert!(!ring_within(&rectangle(0.0, 3.0, -0.5, 0.5), &square));
        assert!((ring_area(&square) - 4.0).abs() < 1e-12);
        assert!((half_width_at(&square, 0.0, 1.0).expect("crossing") - 1.0).abs() < 1e-12);
        assert!(half_width_at(&square, 5.0, 1.0).is_none());
    }

    #[test]
    fn the_scale_figure_keeps_its_stated_height() {
        let extent = bounds(&occupant_ring(0.4, 1.0));
        assert!((extent[1] - 1.0).abs() < 1e-12);
        assert!(extent[3] < 1.0 + OCCUPANT_HEIGHT_M);
        assert!((extent[2] - extent[0] - 2.0 * OCCUPANT_HALF_WIDTH_M).abs() < 1e-12);
    }

    #[test]
    fn every_drawn_part_belongs_to_the_one_selected_station() {
        let cabin = scene_for("A320-200");
        let slice = slice_section(&cabin).expect("sectionable station");
        let x = slice.choice.x_m;
        for deck in &slice.decks {
            if let Some(row) = deck.row {
                assert!(spans(&row.envelope, x), "row {} is not cut at x", row.id);
            }
        }
        let cut_items = cabin
            .cargo
            .items
            .iter()
            .filter(|item| spans(&item.envelope, x))
            .count();
        assert_eq!(slice.cargo.len(), cut_items);
    }

    #[test]
    fn a_double_deck_section_cuts_a_seat_row_on_both_decks() {
        let cabin = scene_for("A380-800");
        let slice = slice_section(&cabin).expect("sectionable station");
        assert_eq!(slice.choice.passenger_decks, 2);
        assert_eq!(slice.choice.decks_with_rows, 2);
        assert!(slice
            .decks
            .iter()
            .filter(|deck| deck.deck.passenger)
            .all(|deck| !deck.seats.is_empty()));
    }

    #[test]
    fn a_cabin_too_shallow_for_a_standing_figure_reports_it_instead_of_shrinking_one() {
        let cabin = scene_for("ATR72-600");
        let slice = slice_section(&cabin).expect("sectionable station");
        assert!(slice
            .decks
            .iter()
            .filter(|deck| deck.deck.passenger)
            .all(|deck| deck.occupants.is_empty()));
        assert!(slice
            .findings
            .iter()
            .any(|finding| finding.contains("standing figure")));
    }

    #[test]
    fn every_registered_preset_renders_a_titled_section() {
        for name in presets::available() {
            let cabin = scene_for(name);
            let figure = figure_cabin_section(&cabin, Some("light"));
            assert!(
                figure.elements.len() > 40,
                "{name}: only {} element(s)",
                figure.elements.len()
            );
            assert!(figure.elements.iter().any(|element| matches!(
                element,
                SceneElement::Text { text, .. } if text.contains("cabin cross-section")
            )));
            let repeat = figure_cabin_section(&cabin, Some("light"));
            assert_eq!(figure.elements.len(), repeat.elements.len());
        }
    }
}
