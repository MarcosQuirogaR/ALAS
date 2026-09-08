// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_payload::geometry::CabinGeometry;
use alas_payload::layout::{DeckItem, ItemKind, ItemMeta, LayoutSummary, PayloadLayout, SeatMeta};

use crate::chart_kit::{draw_horizontal_legend, LegendMarker};
use crate::scene::Axes2D;
use crate::scene::{Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

use super::cabin_seat_map::draw_main_deck_services;
use super::shared::equal_aspect_ranges;
pub(super) fn item_color(item: &DeckItem) -> Color {
    match item.kind {
        ItemKind::SeatRow => match &item.meta {
            ItemMeta::Seat(meta) => Color::from_hex(match meta.cls {
                "First" => "#8e44ad",
                "Business" => "#2980b9",
                "Premium" => "#16a085",
                _ => "#27ae60",
            }),
            _ => Color::from_hex("#27ae60"),
        },
        ItemKind::Uld => match &item.meta {
            ItemMeta::Container(meta) => Color::from_hex(meta.color),
            _ => Color::from_hex("#3498db"),
        },
        ItemKind::Galley => Color::from_hex("#e67e22"),
        ItemKind::Lav => Color::from_hex("#5dade2"),
        ItemKind::AccessibleLav => Color::from_hex("#2471a3"),
        ItemKind::WheelchairStowage => Color::from_hex("#f4d03f"),
        ItemKind::OverheadBin => match &item.meta {
            ItemMeta::OverheadBin(meta) => Color::from_hex(match meta.bin_type {
                alas_payload::layout::OverheadBinType::Sidewall => "#566573",
                alas_payload::layout::OverheadBinType::Center => "#7b8790",
            }),
            _ => Color::from_hex("#566573"),
        },
        ItemKind::Exit => Color::from_hex("#e74c3c"),
        ItemKind::Bag => Color::from_hex("#95a5a6"),
    }
}

const SEAT_LETTERS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";

fn seat_letter(index: usize) -> String {
    SEAT_LETTERS.get(index).map_or_else(
        || format!("S{}", index + 1),
        |letter| char::from(*letter).to_string(),
    )
}

fn next_row_number(row: i64) -> i64 {
    let next = row + 1;
    if next == 13 {
        14
    } else {
        next
    }
}

fn advance_row_number(mut row: i64, count: usize) -> i64 {
    for _ in 0..count {
        row = next_row_number(row);
    }
    row
}

fn first_row_number_for_deck(items: &[DeckItem], deck: &str) -> i64 {
    let mut row = 1_i64;
    for candidate in ["main", "upper", "lower"] {
        if candidate == deck {
            return row;
        }
        let rows = items
            .iter()
            .filter(|item| item.deck == candidate && item.kind == ItemKind::SeatRow)
            .count();
        row = advance_row_number(row, rows);
    }
    row
}

/// Lateral seat centres and letters in the exact outboard-to-outboard block
/// order selected by the cabin engine. Only `filled` positions are returned:
/// the final truncated row must not depict capacity that was not placed.
fn filled_seat_positions(meta: &SeatMeta) -> Vec<(f64, String)> {
    let blocks = if meta.blocks.is_empty() {
        vec![meta.abreast.max(0)]
    } else {
        meta.blocks.clone()
    };
    let seats = blocks.iter().sum::<i64>().max(0);
    let aisle_count = blocks.len().saturating_sub(1) as f64;
    let total_width = seats as f64 * meta.seat_w + aisle_count * meta.aisle_w;
    let mut cursor = -total_width * 0.5;
    let mut position = 0usize;
    let mut remaining = meta.filled.max(0);
    let mut result = Vec::with_capacity(remaining as usize);
    for (block_index, &block_size) in blocks.iter().enumerate() {
        for _ in 0..block_size.max(0) {
            let center_y = cursor + meta.seat_w * 0.5;
            if remaining > 0 {
                result.push((center_y, seat_letter(position)));
                remaining -= 1;
            }
            position += 1;
            cursor += meta.seat_w;
        }
        if block_index + 1 < blocks.len() {
            cursor += meta.aisle_w;
        }
    }
    result
}

fn text(scene: &mut Scene, value: String, pos: [f64; 2], pal: &crate::theme::Palette, size: f64) {
    scene.add(SceneElement::Text {
        text: value,
        pos,
        font_size: size,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
}

fn centered_text(
    scene: &mut Scene,
    value: String,
    pos: [f64; 2],
    pal: &crate::theme::Palette,
    size: f64,
) {
    scene.add(SceneElement::Text {
        text: value,
        pos,
        font_size: size,
        color: Color::from_hex(pal.title),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
}

pub(super) fn empty(theme: Option<&str>, message: &str) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(800.0, 500.0, Some(Color::from_hex(pal.bg)));
    scene.add(SceneElement::Text {
        text: message.to_owned(),
        pos: [400.0, 250.0],
        font_size: 12.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}

/// Draw a plan view of every placed payload item over the actual fuselage.
pub fn figure_cabin_payload(
    layout: &PayloadLayout,
    plane: &Airplane,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    if layout.items.is_empty() {
        return empty(theme, "No payload layout available");
    }
    let Ok(cabin) = CabinGeometry::new(
        plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    ) else {
        return empty(theme, "Cabin geometry unavailable");
    };
    let decks: Vec<&str> = ["upper", "main", "lower"]
        .into_iter()
        .filter(|name| !layout.by_deck(name).is_empty())
        .collect();
    let panel_h = 155.0;
    let panel_gap = 23.0;
    let plot_w = 790.0;
    let side_top = 45.0 + decks.len() as f64 * (panel_h + panel_gap);
    let side_height = 190.0;
    let legend_top = side_top + side_height + 54.0;
    let summary_top = legend_top + 36.0;
    let scene_height = summary_top + 28.0;
    let mut scene = Scene::new(900.0, scene_height, Some(Color::from_hex(pal.bg)));
    scene.title = Some(format!("Cabin / Payload Layout - {}", layout.mode.as_str()));
    scene.suppress_derived_title();
    let x0 = cabin.x_min - 0.5;
    let x1 = cabin.x_max + 0.5;
    let y0 = -cabin.diameter_m * 0.55;
    let y1 = cabin.diameter_m * 0.55;
    let mut draw_deck = |deck: &str, top: f64| {
        let rect = (55.0, top, plot_w, panel_h);
        let ((u0, u1), (v0, v1)) = equal_aspect_ranges(x0, x1, rect.2, y0, y1, rect.3, 0.05);
        let axes = Axes2D::new(rect, (u0, u1), (v0, v1));
        // The shared X label belongs to the bottom side-view panel. Repeating
        // it on every stacked deck panel collides with the panel below.
        axes.draw_frame_with_labels(&mut scene, pal, "", "lateral Y [m]");
        let fus = &plane.fuselages[0];
        let mut outline = Vec::with_capacity(fus.xsecs.len() * 2);
        outline.extend(
            fus.xsecs
                .iter()
                .map(|s| axes.map_point(s.xyz_c[0], s.width * 0.5)),
        );
        outline.extend(
            fus.xsecs
                .iter()
                .rev()
                .map(|s| axes.map_point(s.xyz_c[0], -s.width * 0.5)),
        );
        if outline.len() > 2 {
            scene.add(SceneElement::Polygon {
                points: outline,
                fill: Some(Fill::new(Color::rgba(127, 140, 141, 35))),
                stroke: Some(Stroke::new(Color::from_hex("#7f8c8d"), 1.0)),
            });
        }
        scene.add(SceneElement::Text {
            text: format!("{} deck", deck.to_uppercase()),
            pos: [rect.0 + 6.0, rect.1 + 5.0],
            font_size: 10.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: true,
        });
        let mut row_number = first_row_number_for_deck(&layout.items, deck);
        for item in layout.by_deck(deck) {
            let color = item_color(item);
            let p0 = axes.map_point(item.x - item.length * 0.5, item.y - item.width * 0.5);
            let p1 = axes.map_point(item.x + item.length * 0.5, item.y + item.width * 0.5);
            if item.kind == ItemKind::SeatRow {
                let ItemMeta::Seat(meta) = &item.meta else {
                    continue;
                };
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
                        rx: 1.0,
                        fill: Some(Fill::new(color)),
                        stroke: Some(Stroke::new(Color::from_hex(pal.bg), 0.4)),
                    });
                    let label = format!("{row_number}{letter}");
                    let font_size = (height * 0.72)
                        .min(width / (label.chars().count() as f64 * 0.58))
                        .clamp(2.8, 5.5);
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
                row_number = next_row_number(row_number);
            } else if item.kind == ItemKind::Exit {
                scene.add(SceneElement::Line {
                    p1: [p0[0], (p0[1] + p1[1]) * 0.5],
                    p2: [p1[0], (p0[1] + p1[1]) * 0.5],
                    stroke: Stroke::new(color, 2.0),
                });
            } else if item.kind == ItemKind::OverheadBin {
                // The bin footprint overlaps the seats by design. A faint
                // outline communicates its envelope without hiding the map.
                scene.add(SceneElement::Rect {
                    x: p0[0].min(p1[0]),
                    y: p0[1].min(p1[1]),
                    width: (p1[0] - p0[0]).abs().max(1.0),
                    height: (p1[1] - p0[1]).abs().max(1.0),
                    rx: 1.0,
                    fill: None,
                    stroke: Some(Stroke::dashed(color, 0.55, 2.0, 1.5)),
                });
            } else {
                scene.add(SceneElement::Rect {
                    x: p0[0].min(p1[0]),
                    y: p0[1].min(p1[1]),
                    width: (p1[0] - p0[0]).abs().max(1.0),
                    height: (p1[1] - p0[1]).abs().max(1.0),
                    rx: 1.0,
                    fill: Some(Fill::new(Color::rgba(color.r, color.g, color.b, 190))),
                    stroke: Some(Stroke::new(Color::from_hex(pal.bg), 0.4)),
                });
            }
        }
        let cg = axes.map_point(layout.cg_x, layout.cg_y);
        scene.add(SceneElement::Line {
            p1: [cg[0], rect.1],
            p2: [cg[0], rect.1 + rect.3],
            stroke: Stroke::dashed(Color::from_hex("#e74c3c"), 1.3, 5.0, 3.0),
        });
    };
    for (index, deck) in decks.iter().enumerate() {
        draw_deck(deck, 45.0 + index as f64 * (panel_h + panel_gap));
    }
    let side_rect = (55.0, side_top, plot_w, side_height);
    let z0 = plane.fuselages[0]
        .xsecs
        .iter()
        .map(|s| s.xyz_c[2] - s.height * 0.5)
        .fold(f64::INFINITY, f64::min);
    let z1 = plane.fuselages[0]
        .xsecs
        .iter()
        .map(|s| s.xyz_c[2] + s.height * 0.5)
        .fold(f64::NEG_INFINITY, f64::max);
    let ((sx0, sx1), (sz0, sz1)) =
        equal_aspect_ranges(x0, x1, side_rect.2, z0, z1, side_rect.3, 0.05);
    let side_axes = Axes2D::new(side_rect, (sx0, sx1), (sz0, sz1));
    side_axes.draw_frame_with_labels(&mut scene, pal, "longitudinal X [m]", "height Z [m]");
    let fus = &plane.fuselages[0];
    let mut side_outline = fus
        .xsecs
        .iter()
        .map(|s| side_axes.map_point(s.xyz_c[0], s.xyz_c[2] + s.height * 0.5))
        .collect::<Vec<_>>();
    side_outline.extend(
        fus.xsecs
            .iter()
            .rev()
            .map(|s| side_axes.map_point(s.xyz_c[0], s.xyz_c[2] - s.height * 0.5)),
    );
    scene.add(SceneElement::Polygon {
        points: side_outline,
        fill: Some(Fill::new(Color::rgba(127, 140, 141, 35))),
        stroke: Some(Stroke::new(Color::from_hex("#7f8c8d"), 1.0)),
    });
    for deck in ["upper", "main", "lower"] {
        let Some(spec) = cabin
            .passenger_decks
            .iter()
            .find(|d| d.name == deck)
            .or_else(|| (cabin.lower_deck.name == deck).then_some(&cabin.lower_deck))
        else {
            continue;
        };
        let points = (0..40)
            .map(|i| {
                let x = cabin.x_min + (cabin.x_max - cabin.x_min) * i as f64 / 39.0;
                side_axes.map_point(x, cabin.floor_z(spec, x))
            })
            .collect::<Vec<_>>();
        scene.add(SceneElement::Polyline {
            points,
            stroke: Stroke::dashed(Color::from_hex("#34495e"), 0.9, 3.0, 2.0),
        });
    }
    for item in &layout.items {
        let p0 = side_axes.map_point(item.x - item.length * 0.5, item.z - item.height * 0.5);
        let p1 = side_axes.map_point(item.x + item.length * 0.5, item.z + item.height * 0.5);
        scene.add(SceneElement::Rect {
            x: p0[0].min(p1[0]),
            y: p0[1].min(p1[1]),
            width: (p1[0] - p0[0]).abs().max(1.0),
            height: (p1[1] - p0[1]).abs().max(1.0),
            rx: 1.0,
            fill: Some(Fill::new(item_color(item))),
            stroke: None,
        });
    }
    let cg = side_axes.map_point(layout.cg_x, z0);
    scene.add(SceneElement::Line {
        p1: [cg[0], side_rect.1],
        p2: [cg[0], side_rect.1 + side_rect.3],
        stroke: Stroke::dashed(Color::from_hex("#e74c3c"), 1.5, 5.0, 3.0),
    });
    draw_horizontal_legend(
        &mut scene,
        [55.0, legend_top],
        &[
            (
                "Seat row".to_owned(),
                LegendMarker::Patch(Color::from_hex("#27ae60")),
            ),
            (
                "Galley / lav".to_owned(),
                LegendMarker::Patch(Color::from_hex("#e67e22")),
            ),
            (
                "Emergency exit".to_owned(),
                LegendMarker::Line(Stroke::new(Color::from_hex("#e74c3c"), 2.0)),
            ),
            (
                "Payload CG".to_owned(),
                LegendMarker::Line(Stroke::dashed(Color::from_hex("#e74c3c"), 1.5, 5.0, 3.0)),
            ),
        ],
        pal,
        8.0,
    );
    centered_text(
        &mut scene,
        match &layout.summary {
            LayoutSummary::Passenger(summary) => format!(
                "{} seats, {:.1} t",
                summary.seated_pax,
                layout.total_mass / 1000.0
            ),
            LayoutSummary::Cargo(summary) => format!(
                "{} ULD, {:.1} t",
                summary.n_ulds,
                layout.total_mass / 1000.0
            ),
        },
        [450.0, summary_top],
        pal,
        11.0,
    );
    scene
}
