// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// Draw a legible, generated main-deck seat map in longitudinal bands.
///
/// This is deliberately a sizing layout rather than an operator seat map: row
/// positions, class, block breaks, aisles, and incomplete final rows all come
/// from [`PayloadLayout`], while the longitudinal display scale is compressed
/// so each placed identifier remains readable.
pub fn figure_main_deck_seat_map(
    layout: &PayloadLayout,
    plane: &Airplane,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let Ok(cabin) = CabinGeometry::new(
        plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    ) else {
        return empty(theme, "Cabin geometry unavailable");
    };
    let main_rows = layout
        .items
        .iter()
        .filter(|item| item.deck == "main" && item.kind == ItemKind::SeatRow)
        .collect::<Vec<_>>();
    if main_rows.is_empty() {
        return empty(theme, "No main-deck seats available");
    }

    let mut scene = Scene::new(1000.0, 760.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Generated Main-Deck Seat Map".to_owned());
    scene.suppress_derived_title();
    text(
        &mut scene,
        "Generated sizing layout - not a published airline seat map".to_owned(),
        [55.0, 14.0],
        pal,
        10.0,
    );
    let x0 = cabin.x_min - 0.5;
    let x1 = cabin.x_max + 0.5;
    let y0 = -cabin.diameter_m * 0.55;
    let y1 = cabin.diameter_m * 0.55;
    let segment_span = (x1 - x0) / 3.0;
    let mut numbered_rows = Vec::with_capacity(main_rows.len());
    let mut row_number = first_row_number_for_deck(&layout.items, "main");
    for item in main_rows {
        numbered_rows.push((item, row_number));
        row_number = next_row_number(row_number);
    }

    for segment in 0..3 {
        let lo = x0 + segment as f64 * segment_span;
        let hi = if segment == 2 { x1 } else { lo + segment_span };
        let rect = (55.0, 58.0 + segment as f64 * 220.0, 890.0, 175.0);
        let axes = Axes2D::new(rect, (lo, hi), (y0, y1));
        axes.draw_frame_with_labels(&mut scene, pal, "longitudinal X [m]", "lateral Y [m]");
        scene.add(SceneElement::Text {
            text: format!("{} / 3   X={lo:.1}-{hi:.1} m", segment + 1),
            pos: [rect.0 + 6.0, rect.1 + 5.0],
            font_size: 9.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: true,
        });
        for &(item, row) in numbered_rows
            .iter()
            .filter(|(item, _)| item.x >= lo && (item.x < hi || segment == 2))
        {
            let ItemMeta::Seat(meta) = &item.meta else {
                continue;
            };
            let color = item_color(item);
            for (seat_y, letter) in filled_seat_positions(meta) {
                let seat_p0 =
                    axes.map_point(item.x - item.length * 0.42, seat_y - meta.seat_w * 0.42);
                let seat_p1 =
                    axes.map_point(item.x + item.length * 0.42, seat_y + meta.seat_w * 0.42);
                let left = seat_p0[0].min(seat_p1[0]);
                let top = seat_p0[1].min(seat_p1[1]);
                let width = (seat_p1[0] - seat_p0[0]).abs().max(1.0);
                let height = (seat_p1[1] - seat_p0[1]).abs().max(1.0);
                scene.add(SceneElement::Rect {
                    x: left,
                    y: top,
                    width,
                    height,
                    rx: 1.4,
                    fill: Some(Fill::new(color)),
                    stroke: Some(Stroke::new(Color::from_hex(pal.bg), 0.6)),
                });
                let label = format!("{row}{letter}");
                let font_size = (height * 0.78)
                    .min(width / (label.chars().count() as f64 * 0.58))
                    .clamp(4.5, 8.0);
                scene.add(SceneElement::Text {
                    text: label,
                    pos: [left + width * 0.5, top + height * 0.5],
                    font_size,
                    color: Color::rgb(255, 255, 255),
                    align: TextAlign::Center,
                    baseline: TextBaseline::Middle,
                    angle_deg: 0.0,
                    bold: true,
                });
            }
        }
        draw_main_deck_services(&mut scene, &axes, layout, lo, hi, segment == 2);
    }
    text(
        &mut scene,
        "Longitudinal scale compressed for legibility; every identifier comes from the placed layout."
            .to_owned(),
        [55.0, 734.0],
        pal,
        9.0,
    );
    scene
}
