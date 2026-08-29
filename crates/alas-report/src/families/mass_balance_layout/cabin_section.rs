// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Representative transverse cabin section derived from the shared payload layout.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_payload::geometry::{CabinGeometry, DeckSpec};
use alas_payload::layout::{
    DeckItem, ItemKind, ItemMeta, OverheadBinType, PayloadLayout, SeatMeta, LOWER,
};

use crate::scene::{Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

const WIDTH: f64 = 760.0;
const HEIGHT: f64 = 720.0;

#[derive(Clone, Copy)]
struct SectionMap {
    center: [f64; 2],
    scale: f64,
    z_center: f64,
}

impl SectionMap {
    fn point(self, y: f64, z: f64) -> [f64; 2] {
        [
            self.center[0] + y * self.scale,
            self.center[1] - (z - self.z_center) * self.scale,
        ]
    }
}

fn ellipse(map: SectionMap, width: f64, height: f64, zc: f64) -> Vec<[f64; 2]> {
    (0..=96)
        .map(|index| {
            let theta = std::f64::consts::TAU * index as f64 / 96.0;
            map.point(width * 0.5 * theta.cos(), zc + height * 0.5 * theta.sin())
        })
        .collect()
}

fn polygon(scene: &mut Scene, points: Vec<[f64; 2]>, fill: Color, stroke: Color) {
    scene.add(SceneElement::Polygon {
        points,
        fill: Some(Fill::new(fill)),
        stroke: Some(Stroke::new(stroke, 1.2)),
    });
}

fn text(scene: &mut Scene, value: impl Into<String>, pos: [f64; 2], color: Color, size: f64) {
    scene.add(SceneElement::Text {
        text: value.into(),
        pos,
        font_size: size,
        color,
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
}

#[allow(clippy::too_many_arguments)]
fn rect_physical(
    scene: &mut Scene,
    map: SectionMap,
    y0: f64,
    y1: f64,
    z0: f64,
    z1: f64,
    fill: Color,
    outline: Color,
    radius: f64,
) {
    let top_left = map.point(y0, z1);
    let bottom_right = map.point(y1, z0);
    scene.add(SceneElement::Rect {
        x: top_left[0],
        y: top_left[1],
        width: bottom_right[0] - top_left[0],
        height: bottom_right[1] - top_left[1],
        rx: radius,
        fill: Some(Fill::new(fill)),
        stroke: Some(Stroke::new(outline, 1.0)),
    });
}

fn seat_and_aisle_centers(meta: &SeatMeta) -> (Vec<f64>, Vec<f64>) {
    let blocks = if meta.blocks.is_empty() {
        vec![meta.abreast.max(0)]
    } else {
        meta.blocks.clone()
    };
    let total_width = blocks.iter().sum::<i64>() as f64 * meta.seat_w
        + blocks.len().saturating_sub(1) as f64 * meta.aisle_w;
    let mut cursor = -total_width * 0.5;
    let mut seats = Vec::new();
    let mut aisles = Vec::new();
    let mut remaining = meta.filled.max(0);
    for (block_index, &block) in blocks.iter().enumerate() {
        for _ in 0..block.max(0) {
            if remaining > 0 {
                seats.push(cursor + meta.seat_w * 0.5);
                remaining -= 1;
            }
            cursor += meta.seat_w;
        }
        if block_index + 1 < blocks.len() {
            aisles.push(cursor + meta.aisle_w * 0.5);
            cursor += meta.aisle_w;
        }
    }
    (seats, aisles)
}

fn seat_color(meta: &SeatMeta) -> Color {
    Color::from_hex(match meta.cls {
        "First" => "#8e44ad",
        "Business" => "#2980b9",
        _ => "#27ae60",
    })
}

fn draw_seat(scene: &mut Scene, map: SectionMap, y: f64, floor: f64, width: f64, color: Color) {
    let half = width * 0.40;
    let outline = Color::from_hex("#334155");
    rect_physical(
        scene,
        map,
        y - half,
        y + half,
        floor + 0.28,
        floor + 1.08,
        color,
        outline,
        5.0,
    );
    rect_physical(
        scene,
        map,
        y - half * 1.05,
        y + half * 1.05,
        floor + 0.24,
        floor + 0.43,
        Color::rgba(color.r, color.g, color.b, 245),
        outline,
        3.0,
    );
    for leg_y in [y - half * 0.65, y + half * 0.65] {
        scene.add(SceneElement::Line {
            p1: map.point(leg_y, floor),
            p2: map.point(leg_y, floor + 0.25),
            stroke: Stroke::new(Color::from_hex("#7f8c8d"), 2.0),
        });
    }
}

fn draw_person(scene: &mut Scene, map: SectionMap, y: f64, floor: f64, height: f64) {
    let grey = Color::from_hex("#95a5a6");
    let head = map.point(y, floor + height * 0.89);
    scene.add(SceneElement::Circle {
        center: head,
        radius: height * map.scale * 0.075,
        fill: Some(Fill::new(grey)),
        stroke: None,
    });
    polygon(
        scene,
        vec![
            map.point(y - height * 0.12, floor + height * 0.70),
            map.point(y + height * 0.12, floor + height * 0.70),
            map.point(y + height * 0.09, floor + height * 0.35),
            map.point(y - height * 0.09, floor + height * 0.35),
        ],
        grey,
        grey,
    );
    for offset in [-0.055, 0.055] {
        scene.add(SceneElement::Line {
            p1: map.point(y + height * offset, floor),
            p2: map.point(y + height * offset * 0.8, floor + height * 0.38),
            stroke: Stroke::new(grey, 5.0),
        });
    }
}

fn nearest_seat_row<'a>(layout: &'a PayloadLayout, deck: &str, x: f64) -> Option<&'a DeckItem> {
    layout
        .items
        .iter()
        .filter(|item| {
            item.kind == ItemKind::SeatRow && item.deck == deck && intersects_station(item, x)
        })
        .min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
}

fn intersects_station(item: &DeckItem, station: f64) -> bool {
    (station - item.x).abs() <= item.length.max(0.0) * 0.5 + 1e-9
}

fn envelope_half_width(cabin: &CabinGeometry, station: f64, z0: f64, z1: f64) -> f64 {
    0.5 * cabin
        .usable_width_at_z(station, z0)
        .min(cabin.usable_width_at_z(station, z1))
}

fn draw_bins(scene: &mut Scene, map: SectionMap, layout: &PayloadLayout, deck: &DeckSpec, x: f64) {
    let bins: Vec<&DeckItem> = layout
        .items
        .iter()
        .filter(|item| {
            item.kind == ItemKind::OverheadBin
                && item.deck == deck.name
                && intersects_station(item, x)
        })
        .collect();
    for bin in bins {
        let kind = match &bin.meta {
            ItemMeta::OverheadBin(meta) => meta.bin_type,
            _ => OverheadBinType::Sidewall,
        };
        let half = bin.width * 0.5;
        let bottom = bin.z - bin.height * 0.5;
        let top = bin.z + bin.height * 0.5;
        let color = Color::from_hex(match kind {
            OverheadBinType::Sidewall => "#566573",
            OverheadBinType::Center => "#7b8790",
        });
        let points = match kind {
            OverheadBinType::Sidewall => vec![
                map.point(bin.y - half, bottom + bin.height * 0.22),
                map.point(bin.y - half * 0.75, top),
                map.point(bin.y + half * 0.75, top),
                map.point(bin.y + half, bottom + bin.height * 0.22),
                map.point(bin.y + half * 0.55, bottom),
                map.point(bin.y - half * 0.55, bottom),
            ],
            OverheadBinType::Center => vec![
                map.point(bin.y - half, top),
                map.point(bin.y + half, top),
                map.point(bin.y + half * 0.80, bottom + bin.height * 0.18),
                map.point(bin.y + half * 0.34, bottom),
                map.point(bin.y - half * 0.34, bottom),
                map.point(bin.y - half * 0.80, bottom + bin.height * 0.18),
            ],
        };
        polygon(scene, points, color, Color::from_hex("#d5d8dc"));
    }
}

fn draw_deck(
    scene: &mut Scene,
    map: SectionMap,
    cabin: &CabinGeometry,
    layout: &PayloadLayout,
    deck: &DeckSpec,
    station: f64,
    label_color: Color,
) {
    let floor = cabin.floor_z(deck, station);
    let half_width = envelope_half_width(cabin, station, floor - 0.06, floor + 0.06);
    rect_physical(
        scene,
        map,
        -half_width,
        half_width,
        floor - 0.06,
        floor + 0.06,
        Color::from_hex("#7f8c8d"),
        Color::from_hex("#d5d8dc"),
        0.0,
    );
    if let Some(row) = nearest_seat_row(layout, deck.name, station) {
        if let ItemMeta::Seat(meta) = &row.meta {
            let (seats, aisles) = seat_and_aisle_centers(meta);
            let color = seat_color(meta);
            for y in seats {
                draw_seat(scene, map, y, floor, meta.seat_w, color);
            }
            if let Some(&aisle) = aisles.first() {
                let standing_height = cabin.deck_height(deck, station).min(1.75);
                draw_person(scene, map, aisle, floor, standing_height);
            }
        }
    }
    draw_bins(scene, map, layout, deck, station);
    text(
        scene,
        match deck.name {
            "upper" => "Upper deck",
            _ => "Main deck",
        },
        map.point(-half_width + 0.08, floor + 0.15),
        label_color,
        9.0,
    );
}

fn draw_hold(
    scene: &mut Scene,
    map: SectionMap,
    cabin: &CabinGeometry,
    layout: &PayloadLayout,
    station: f64,
) {
    let deck = &cabin.lower_deck;
    let floor = cabin.floor_z(deck, station);
    let ceiling = cabin.ceil_z(deck, station);
    let floor_half_width = cabin.usable_width_at_z(station, floor) * 0.5;
    let ceiling_half_width = cabin.usable_width_at_z(station, ceiling) * 0.5;
    polygon(
        scene,
        vec![
            map.point(-floor_half_width, floor),
            map.point(floor_half_width, floor),
            map.point(ceiling_half_width, ceiling),
            map.point(-ceiling_half_width, ceiling),
        ],
        Color::rgba(127, 140, 141, 42),
        Color::from_hex("#7f8c8d"),
    );
    let cargo: Vec<&DeckItem> = layout
        .items
        .iter()
        .filter(|item| {
            item.deck == LOWER
                && matches!(item.kind, ItemKind::Bag | ItemKind::Uld)
                && intersects_station(item, station)
        })
        .collect();
    for item in cargo {
        let z0 = (item.z - item.height * 0.5).max(floor);
        let z1 = (item.z + item.height * 0.5).min(ceiling);
        if z1 <= z0 {
            continue;
        }
        let envelope_half = envelope_half_width(cabin, station, z0, z1);
        let y0 = (item.y - item.width * 0.5).max(-envelope_half);
        let y1 = (item.y + item.width * 0.5).min(envelope_half);
        if y1 <= y0 {
            continue;
        }
        rect_physical(
            scene,
            map,
            y0,
            y1,
            z0,
            z1,
            Color::from_hex("#9b9bd0"),
            Color::from_hex("#5b5b91"),
            2.0,
        );
    }
}

/// Draw a representative, dimensioned transverse section of the analyzed cabin.
pub fn figure_cabin_cross_section(
    layout: &PayloadLayout,
    plane: &Airplane,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(WIDTH, HEIGHT, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Cabin Cross-Section".to_owned());
    let Ok(cabin) = CabinGeometry::new(
        plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    ) else {
        text(
            &mut scene,
            "Cabin geometry unavailable",
            [WIDTH * 0.5, HEIGHT * 0.5],
            Color::from_hex(pal.title),
            13.0,
        );
        return scene;
    };
    let midpoint = 0.5 * (cabin.cabin_start_x + cabin.cabin_end_x);
    let station = layout
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::SeatRow)
        .min_by(|a, b| (a.x - midpoint).abs().total_cmp(&(b.x - midpoint).abs()))
        .map_or(midpoint, |item| item.x);
    let width = cabin.width_at(station).max(1.0);
    let height = cabin.height_at(station).max(1.0);
    let zc = cabin.zc_at(station);
    let map = SectionMap {
        center: [WIDTH * 0.5, 360.0],
        scale: (610.0 / width).min(550.0 / height),
        z_center: zc,
    };
    let outline = Color::from_hex(pal.spine);
    polygon(
        &mut scene,
        ellipse(map, width, height, zc),
        Color::from_hex("#aeb6bf"),
        outline,
    );
    polygon(
        &mut scene,
        ellipse(
            map,
            (width - 2.0 * cabin.wall).max(0.2),
            (height - 2.0 * cabin.wall).max(0.2),
            zc,
        ),
        Color::from_hex(pal.bg),
        Color::from_hex("#7f8c8d"),
    );

    for deck in &cabin.passenger_decks {
        draw_deck(
            &mut scene,
            map,
            &cabin,
            layout,
            deck,
            station,
            Color::from_hex(pal.title),
        );
    }
    draw_hold(&mut scene, map, &cabin, layout, station);

    text(
        &mut scene,
        "Cabin Cross-Section",
        [WIDTH * 0.5, 28.0],
        Color::from_hex(pal.title),
        16.0,
    );
    text(
        &mut scene,
        format!(
            "Representative station x = {station:.1} m | outer section {width:.2} x {height:.2} m"
        ),
        [WIDTH * 0.5, 50.0],
        Color::from_hex(pal.tick),
        9.5,
    );
    text(
        &mut scene,
        "Seat color = class | dark bins = sidewall pivot | light bins = center hinge",
        [WIDTH * 0.5, HEIGHT - 20.0],
        Color::from_hex(pal.tick),
        9.5,
    );
    scene
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use alas_geom::builder::AircraftBuilder;
    use alas_payload::build::build_payload_layout;

    #[test]
    fn section_contains_shell_seats_floors_and_labels() {
        let config = AlasConfig::default();
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(None, true)
            .expect("default aircraft builds");
        let layout = build_payload_layout(&plane, &config, 0.0, 0.0)
            .expect("default passenger layout builds");
        let scene = figure_cabin_cross_section(&layout, &plane, &config, Some("dark"));

        assert!(scene.elements.len() > 20);
        assert!(scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, .. } if text == "Cabin Cross-Section"
        )));
        assert!(scene
            .elements
            .iter()
            .any(|element| matches!(element, SceneElement::Polygon { .. })));
    }

    #[test]
    fn section_does_not_draw_non_intersecting_bins_or_cargo() {
        let config = AlasConfig::default();
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(None, true)
            .expect("default aircraft builds");
        let mut layout = build_payload_layout(&plane, &config, 0.0, 0.0)
            .expect("default passenger layout builds");
        let cabin = CabinGeometry::new(
            &plane,
            &config.geometry,
            config.cabin.passenger.wall_thickness_m,
        )
        .expect("default cabin geometry builds");
        let midpoint = 0.5 * (cabin.cabin_start_x + cabin.cabin_end_x);
        let station = layout
            .items
            .iter()
            .filter(|item| item.kind == ItemKind::SeatRow)
            .min_by(|a, b| (a.x - midpoint).abs().total_cmp(&(b.x - midpoint).abs()))
            .map_or(midpoint, |item| item.x);

        for item in &mut layout.items {
            if item.kind == ItemKind::OverheadBin
                || matches!(item.kind, ItemKind::Bag | ItemKind::Uld)
            {
                item.x = station + 10.0;
                item.length = 0.1;
            }
        }
        let scene = figure_cabin_cross_section(&layout, &plane, &config, Some("dark"));
        let bin_colors = [Color::from_hex("#566573"), Color::from_hex("#7b8790")];
        let cargo_color = Color::from_hex("#9b9bd0");

        assert!(!scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Polygon { fill: Some(fill), .. } if bin_colors.contains(&fill.color)
        )));
        assert!(!scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Rect { fill: Some(fill), .. } if fill.color == cargo_color
        )));
    }

    #[test]
    fn longitudinal_span_intersection_includes_edges_only() {
        let config = AlasConfig::default();
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(None, true)
            .expect("default aircraft builds");
        let layout = build_payload_layout(&plane, &config, 0.0, 0.0)
            .expect("default passenger layout builds");
        let item = layout.items.first().expect("layout has physical items");
        let half_length = item.length.max(0.0) * 0.5;

        assert!(intersects_station(item, item.x));
        assert!(intersects_station(item, item.x + half_length));
        assert!(!intersects_station(item, item.x + half_length + 1e-6));
    }
}
