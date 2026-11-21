// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Representative transverse cabin section derived from the shared payload layout.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_payload::build::product_cabin_geometry;
use alas_payload::cargo::freight_deck_spec;
use alas_payload::geometry::{CabinGeometry, DeckSpec};
use alas_payload::layout::{
    DeckItem, ItemKind, ItemMeta, LayoutSummary, Mode, OverheadBinType, PayloadLayout, SeatMeta,
    LOWER,
};

use crate::families::geometry::cabin_assets::asset_for_item;
use crate::scene::{Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

mod hull;
use hull::{display_blocks, Hull};
mod hold;
use hold::{draw_cargo_row, draw_hold};

pub(super) const WIDTH: f64 = 760.0;
pub(super) const HEIGHT: f64 = 720.0;

#[derive(Clone, Copy)]
pub(super) struct SectionMap {
    center: [f64; 2],
    pub(super) scale: f64,
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

/// A ring of `[y, z]` points mapped to figure coordinates.
fn ring_points(map: SectionMap, ring: &[[f64; 2]]) -> Vec<[f64; 2]> {
    ring.iter().map(|p| map.point(p[0], p[1])).collect()
}

fn polygon(scene: &mut Scene, points: Vec<[f64; 2]>, fill: Color, stroke: Color) {
    scene.add(SceneElement::Polygon {
        points,
        fill: Some(Fill::new(fill)),
        stroke: Some(Stroke::new(stroke, 1.2)),
    });
}

pub(super) fn text(
    scene: &mut Scene,
    value: impl Into<String>,
    pos: [f64; 2],
    color: Color,
    size: f64,
) {
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

// Coordinated analysis inputs are kept explicit at this integration boundary.
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
        display_blocks(meta.abreast, &meta.blocks)
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

/// Draw one seat. The legs stand on the floor inside `leg_limit` (half the
/// lining chord at the floor): a leg that would stand outside it is drawn at the
/// limit, as the seat track is moved inboard. The return is whether one was.
fn draw_seat(
    scene: &mut Scene,
    map: SectionMap,
    y: f64,
    floor: f64,
    width: f64,
    color: Color,
    leg_limit: f64,
) -> bool {
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
    let mut pulled = false;
    for leg_y in [y - half * 0.65, y + half * 0.65] {
        let kept = leg_y.clamp(-leg_limit, leg_limit);
        pulled |= (kept - leg_y).abs() > 1e-9;
        scene.add(SceneElement::Line {
            p1: map.point(kept, floor),
            p2: map.point(kept, floor + 0.25),
            stroke: Stroke::new(Color::from_hex("#7f8c8d"), 2.0),
        });
    }
    pulled
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

/// Whether the section plane at `station` cuts `item`: it falls inside the
/// item's x-extent.
fn cut_by(item: &DeckItem, station: f64) -> bool {
    (item.x - station).abs() <= item.length * 0.5 + 1e-6
}

/// The seat row on `deck` the section plane cuts, if it cuts one.
fn seat_row_at<'a>(layout: &'a PayloadLayout, deck: &str, x: f64) -> Option<&'a DeckItem> {
    layout
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::SeatRow && item.deck == deck && cut_by(item, x))
        .min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
}

fn draw_bins(scene: &mut Scene, map: SectionMap, layout: &PayloadLayout, deck: &DeckSpec, x: f64) {
    // Only the bins the section plane cuts are drawn.
    let bins = layout
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::OverheadBin && item.deck == deck.name)
        .filter(|item| cut_by(item, x));
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

/// What one drawn deck found, for the notes under the section.
#[derive(Default)]
struct DeckReport {
    /// Seats drawn.
    seats: usize,
    /// Seats whose outline, legs included, crosses the lining.
    escaping: usize,
    /// Freight items drawn.
    freight: usize,
    /// Freight items whose base crosses the lining at the floor.
    freight_escaping: usize,
    /// Why the model's floor cannot be drawn, with its values.
    floor_error: Option<String>,
}

/// What the figure drew, for the regression tests.
#[derive(Default, Debug, Clone, PartialEq)]
pub(super) struct DrawnCounts {
    /// The station the section was cut at, m.
    pub(super) station: f64,
    /// Freight items drawn on a freight or main deck.
    pub(super) deck_freight: usize,
    /// Freight and baggage items drawn in the underfloor hold.
    pub(super) hold_items: usize,
    /// Of those, the rigid containers (everything but loose bulk).
    pub(super) hold_ulds: usize,
}

/// Name a deck outside the hull, with a leader line to its floor.
fn side_label(
    scene: &mut Scene,
    map: SectionMap,
    hull: Hull,
    floor: f64,
    name: &str,
    color: Color,
) {
    let rim = map.point(-hull.half_w, floor);
    scene.add(SceneElement::Line {
        p1: [rim[0] - 22.0, rim[1]],
        p2: [rim[0] + 2.0, rim[1]],
        stroke: Stroke::new(color, 0.8),
    });
    scene.add(SceneElement::Text {
        text: name.to_owned(),
        pos: [rim[0] - 26.0, rim[1]],
        font_size: 9.0,
        color,
        align: TextAlign::Right,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
}

/// The deck a section is drawn on: the freight deck of a freighter that
/// declares a freight hold, otherwise the model's own deck.
fn drawn_deck(
    deck: &DeckSpec,
    cabin: &CabinGeometry,
    layout: &PayloadLayout,
    config: &AlasConfig,
) -> DeckSpec {
    match config.cabin.cargo.freight_hold {
        Some(hold)
            if layout.mode == Mode::Cargo
                && deck.name == alas_payload::layout::MAIN
                && hold.is_valid() =>
        {
            freight_deck_spec(cabin, &hold)
        }
        _ => deck.clone(),
    }
}

// Coordinated analysis inputs are kept explicit at this integration boundary.
#[allow(clippy::too_many_arguments)]
fn draw_deck(
    scene: &mut Scene,
    map: SectionMap,
    hull: Hull,
    cabin: &CabinGeometry,
    layout: &PayloadLayout,
    deck: &DeckSpec,
    station: f64,
    label_color: Color,
    counts: &mut DrawnCounts,
) -> DeckReport {
    let mut report = DeckReport::default();
    // The floor is the model's: no default is substituted for a floor the
    // lining cannot carry.
    let floor = cabin.floor_z(deck, station);
    if let Some(problem) = hull.floor_problem(floor) {
        report.floor_error = Some(problem);
        return report;
    }
    let half_width = hull.chord_half(floor) * 0.985;
    rect_physical(
        scene,
        map,
        -half_width,
        half_width,
        floor - 0.06,
        floor,
        Color::from_hex("#7f8c8d"),
        Color::from_hex("#d5d8dc"),
        0.0,
    );
    if let Some(row) = seat_row_at(layout, deck.name, station) {
        if let ItemMeta::Seat(meta) = &row.meta {
            let (seats, aisles) = seat_and_aisle_centers(meta);
            let color = seat_color(meta);
            let half = meta.seat_w * 0.40 * 1.05;
            for y in seats {
                report.seats += 1;
                let inside =
                    hull.seat_rect_inside(y - half, y + half, floor + 0.24, floor + 1.08, floor);
                draw_seat(scene, map, y, floor, meta.seat_w, color, half_width);
                if !inside {
                    report.escaping += 1;
                }
            }
            if let Some(&aisle) = aisles.first() {
                let clearance = hull.zc + hull.half_h - floor - 0.05;
                let standing_height = cabin.deck_height(deck, station).min(1.75).min(clearance);
                draw_person(scene, map, aisle, floor, standing_height);
            }
        }
    }
    draw_bins(scene, map, layout, deck, station);
    let name = match (layout.mode, deck.name) {
        (Mode::Cargo, _) => "Cargo deck",
        (_, "upper") => "Upper deck",
        _ => "Main deck",
    };
    side_label(scene, map, hull, floor, name, label_color);
    if layout.mode == Mode::Cargo {
        let row = draw_cargo_row(scene, map, hull, layout, deck.name, station);
        report.freight += row.drawn;
        report.freight_escaping += row.base_escaping;
        counts.deck_freight += row.drawn;
    }
    report
}

/// Notes printed under the section: what is wrong with it or does not fit.
fn notes_of(reports: &[DeckReport], layout: &PayloadLayout) -> Vec<String> {
    let mut notes = Vec::new();
    let seats: usize = reports.iter().map(|r| r.seats).sum();
    let escaping: usize = reports.iter().map(|r| r.escaping).sum();
    if escaping > 0 {
        notes.push(format!(
            "WARNING: {escaping} of {seats} seats in this row do not fit inside the inner contour at the drawn floor height."
        ));
    }
    let freight: usize = reports.iter().map(|r| r.freight).sum();
    let freight_escaping: usize = reports.iter().map(|r| r.freight_escaping).sum();
    if freight_escaping > 0 {
        notes.push(format!(
            "WARNING: {freight_escaping} of {freight} freight items at this station stand outside the inner contour at the floor."
        ));
    }
    if layout.mode == Mode::Cargo && !layout.items.iter().any(|item| item.kind == ItemKind::Uld) {
        notes.push("No payload loaded in this case: the cargo deck is shown empty.".to_owned());
    }
    notes
}

/// The station the section is cut at, and the class of the row drawn there.
///
/// For a passenger layout it is a full row of the class that seats the most
/// passengers (the cabin's constant section, not a nose or tail row, a premium
/// block or a part-filled last row), chosen where the underfloor hold shows the
/// most of its loaded containers, then nearest the middle of the constant
/// section. A freighter is cut at the freight item nearest the middle.
fn section_station(layout: &PayloadLayout, midpoint: f64) -> (f64, Option<&'static str>) {
    if layout.mode == Mode::Cargo {
        let x = layout
            .items
            .iter()
            .filter(|item| item.kind == ItemKind::Uld && item.deck != LOWER)
            .min_by(|a, b| (a.x - midpoint).abs().total_cmp(&(b.x - midpoint).abs()))
            .map_or(midpoint, |item| item.x);
        return (x, None);
    }
    let rows: Vec<(&DeckItem, &SeatMeta)> = layout
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::SeatRow && item.deck == alas_payload::layout::MAIN)
        .filter_map(|item| match &item.meta {
            ItemMeta::Seat(meta) => Some((item, meta)),
            _ => None,
        })
        .collect();
    let mut by_class: Vec<(&'static str, i64)> = Vec::new();
    for (_, meta) in &rows {
        match by_class.iter_mut().find(|entry| entry.0 == meta.cls) {
            Some(entry) => entry.1 += meta.filled,
            None => by_class.push((meta.cls, meta.filled)),
        }
    }
    let Some(&(class, _)) = by_class.iter().max_by_key(|entry| entry.1) else {
        return (midpoint, None);
    };
    let hold_loaded = |x: f64| {
        layout
            .items
            .iter()
            .filter(|item| {
                item.deck == LOWER
                    && matches!(item.kind, ItemKind::Bag | ItemKind::Uld)
                    && matches!(&item.meta, ItemMeta::Container(meta) if meta.uld != "BLK")
                    && cut_by(item, x)
            })
            .count()
    };
    let decks_seated = |x: f64| {
        let mut decks: Vec<&str> = layout
            .items
            .iter()
            .filter(|item| item.kind == ItemKind::SeatRow && cut_by(item, x))
            .map(|item| item.deck)
            .collect();
        decks.sort_unstable();
        decks.dedup();
        decks.len()
    };
    // Full rows of any class are candidates; ranked by, in order: a seat row on
    // every passenger deck, a container of the hold under the plane (so the
    // section shows passengers over containers, not over a wing box or gear
    // bay), the dominant class, then the most containers and the middle of the
    // cabin. A part-filled row is used only when no row is full.
    let mut candidates: Vec<&(&DeckItem, &SeatMeta)> = rows
        .iter()
        .filter(|(_, meta)| meta.filled == meta.abreast)
        .collect();
    if candidates.is_empty() {
        candidates = rows.iter().filter(|(_, meta)| meta.cls == class).collect();
    }
    let best = candidates.into_iter().max_by(|a, b| {
        let key = |r: &(&DeckItem, &SeatMeta)| {
            (
                decks_seated(r.0.x),
                hold_loaded(r.0.x) > 0,
                r.1.cls == class,
            )
        };
        key(a)
            .cmp(&key(b))
            .then(hold_loaded(a.0.x).cmp(&hold_loaded(b.0.x)))
            .then(
                (b.0.x - midpoint)
                    .abs()
                    .total_cmp(&(a.0.x - midpoint).abs()),
            )
    });
    best.map_or((midpoint, None), |(item, meta)| (item.x, Some(meta.cls)))
}

/// Draw a representative, dimensioned transverse section of the analyzed cabin.
pub fn figure_cabin_cross_section(
    layout: &PayloadLayout,
    plane: &Airplane,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    build_section(layout, plane, config, theme).0
}

/// [`figure_cabin_cross_section`] and what it drew.
pub(super) fn build_section(
    layout: &PayloadLayout,
    plane: &Airplane,
    config: &AlasConfig,
    theme: Option<&str>,
) -> (Scene, DrawnCounts) {
    let pal = get_palette(theme);
    let mut counts = DrawnCounts::default();
    let mut scene = Scene::new(WIDTH, HEIGHT, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Cabin Cross-Section".to_owned());
    scene.suppress_derived_title();
    let Ok(cabin) = product_cabin_geometry(plane, config) else {
        text(
            &mut scene,
            "Cabin geometry unavailable",
            [WIDTH * 0.5, HEIGHT * 0.5],
            Color::from_hex(pal.title),
            13.0,
        );
        return (scene, counts);
    };
    let midpoint = 0.5 * (cabin.cabin_start_x + cabin.cabin_end_x);
    let (station, row_class) = section_station(layout, midpoint);
    counts.station = station;
    let width = cabin.width_at(station).max(1.0);
    let height = cabin.height_at(station).max(1.0);
    let zc = cabin.zc_at(station);
    let map = SectionMap {
        center: [WIDTH * 0.5, 360.0],
        scale: (570.0 / width).min(550.0 / height),
        z_center: zc,
    };
    let Some(hull) = Hull::new(&cabin, station) else {
        text(
            &mut scene,
            "Cabin section cannot be drawn: the lining consumes the section at this station.",
            [WIDTH * 0.5, HEIGHT * 0.5],
            Color::from_hex("#c0392b"),
            13.0,
        );
        return (scene, counts);
    };
    let outline = Color::from_hex(pal.spine);
    // The outer skin is the lining's own contour with the wall added, at the
    // section's superellipse exponent.
    let skin = hull.skin_outline(cabin.wall);
    polygon(
        &mut scene,
        ring_points(map, &skin),
        Color::from_hex("#aeb6bf"),
        outline,
    );
    polygon(
        &mut scene,
        ring_points(map, &hull.outline()),
        Color::from_hex(pal.bg),
        Color::from_hex("#7f8c8d"),
    );

    let mut reports = Vec::new();
    for deck in &cabin.passenger_decks {
        let deck = drawn_deck(deck, &cabin, layout, config);
        reports.push(draw_deck(
            &mut scene,
            map,
            hull,
            &cabin,
            layout,
            &deck,
            station,
            Color::from_hex(pal.title),
            &mut counts,
        ));
    }
    // A freighter's cargo rides the main deck; an underfloor box is drawn only
    // where there is something below the floor to show.
    let lower_in_use =
        layout.mode == Mode::Passenger || layout.items.iter().any(|item| item.deck == LOWER);
    if lower_in_use && cabin.underfloor_hold {
        draw_hold(&mut scene, map, &cabin, layout, station, &mut counts);
    }

    text(
        &mut scene,
        "Cabin Cross-Section",
        [WIDTH * 0.5, 28.0],
        Color::from_hex(pal.title),
        16.0,
    );
    let station_line = match row_class {
        Some(class) => format!(
            "Representative station x = {station:.1} m | {class} row | outer section {width:.2} x {height:.2} m | lining {:.2} x {:.2} m",
            2.0 * hull.half_w,
            2.0 * hull.half_h
        ),
        None => format!(
            "Representative station x = {station:.1} m | outer section {width:.2} x {height:.2} m | lining {:.2} x {:.2} m",
            2.0 * hull.half_w,
            2.0 * hull.half_h
        ),
    };
    text(
        &mut scene,
        station_line,
        [WIDTH * 0.5, 50.0],
        Color::from_hex(pal.tick),
        9.5,
    );
    let mut notes = notes_of(&reports, layout);
    if let Some(problem) = reports.iter().find_map(|r| r.floor_error.clone()) {
        let red = Color::from_hex("#c0392b");
        text(
            &mut scene,
            "Cabin section cannot be drawn: the model's floor does not stand inside the lining.",
            [WIDTH * 0.5, 340.0],
            red,
            12.0,
        );
        text(&mut scene, problem, [WIDTH * 0.5, 360.0], red, 10.0);
        notes.clear();
    }
    for (index, note) in notes.iter().enumerate() {
        text(
            &mut scene,
            note.clone(),
            [WIDTH * 0.5, 662.0 + 16.0 * index as f64],
            Color::from_hex("#c2620f"),
            9.0,
        );
    }
    (scene, counts)
}

#[cfg(test)]
mod tests;
