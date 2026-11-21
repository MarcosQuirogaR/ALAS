// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cargo drawn in a cabin section: the main-deck freight row and the hold.
//!
//! An item is drawn only where the section plane cuts it, that is where the
//! station lies inside its x-extent. Nothing is drawn for a neighbour.

use super::*;

/// Slack when asking whether a drawn outline lies inside the lining, m.
const INSIDE_TOLERANCE_M: f64 = 0.005;

/// Whether every vertex of `ring` lies inside the lining, to a few millimetres.
fn ring_inside(hull: Hull, ring: &[[f64; 2]]) -> bool {
    ring.iter()
        .all(|&[y, z]| y.abs() <= hull.chord_half(z) + INSIDE_TOLERANCE_M)
}

/// Clearance kept between a built-up load and the lining, m.
const LOAD_CLEARANCE_M: f64 = 0.05;

/// Thickness of the 463L pallet itself, m (2.25 in).
const PALLET_THICKNESS_M: f64 = 0.057;

fn is_463l(item: &DeckItem) -> bool {
    matches!(&item.meta, ItemMeta::Container(meta) if meta.uld == "463")
}

/// The 463L pallet's footprint on the floor: its full width, as a slab.
pub(super) fn pallet_base_ring(item: &DeckItem) -> Vec<[f64; 2]> {
    let (half_w, z0) = (item.width * 0.5, item.z - item.height * 0.5);
    vec![
        [item.y - half_w, z0],
        [item.y + half_w, z0],
        [item.y + half_w, z0 + PALLET_THICKNESS_M],
        [item.y - half_w, z0 + PALLET_THICKNESS_M],
    ]
}

/// The load built up on a pallet: as wide as the pallet, held inside the
/// lining less [`LOAD_CLEARANCE_M`], up to the item's usable height (a freight
/// hold's declared clear height: the C-295's 1.90 m, Jane's All the World's
/// Aircraft 2008-2009 p.545 "Max height", carried by the layout).
pub(super) fn built_up_load(hull: Hull, item: &DeckItem) -> Vec<[f64; 2]> {
    const STEPS: usize = 32;
    let half_w = item.width * 0.5;
    let z0 = item.z - item.height * 0.5 + PALLET_THICKNESS_M;
    let z1 = item.z + item.height * 0.5;
    let at = |k: usize| z0 + (z1 - z0) * k as f64 / STEPS as f64;
    let half = |k: usize| half_w.min(hull.chord_half(at(k)) - LOAD_CLEARANCE_M);
    let last = (0..=STEPS)
        .take_while(|&k| half(k) > 0.05)
        .last()
        .unwrap_or(0);
    let mut ring: Vec<[f64; 2]> = (0..=last).map(|k| [item.y - half(k), at(k)]).collect();
    ring.extend((0..=last).rev().map(|k| [item.y + half(k), at(k)]));
    ring
}

/// The outline a freight item is drawn with. The 463L pallet is a flat
/// rectangle at its loaded size; other items take the shared catalogue
/// profile.
pub(super) fn freight_ring(item: &DeckItem) -> Vec<[f64; 2]> {
    let rectangle = || {
        let (half_w, half_h) = (item.width * 0.5, item.height * 0.5);
        vec![
            [item.y - half_w, item.z - half_h],
            [item.y + half_w, item.z - half_h],
            [item.y + half_w, item.z + half_h],
            [item.y - half_w, item.z + half_h],
        ]
    };
    let is_463l = matches!(&item.meta, ItemMeta::Container(meta) if meta.uld == "463");
    if is_463l {
        return rectangle();
    }
    asset_for_item(item).map_or_else(rectangle, |asset| asset.profile_yz)
}

/// What one freight row drew.
#[derive(Default)]
pub(super) struct FreightRow {
    /// Items drawn.
    pub(super) drawn: usize,
    /// Items whose base (the part standing on the floor) crosses the lining.
    pub(super) base_escaping: usize,
}

/// Draw the freight items of one deck that the plane at `station` cuts.
pub(super) fn draw_cargo_row(
    scene: &mut Scene,
    map: SectionMap,
    hull: Hull,
    layout: &PayloadLayout,
    deck: &str,
    station: f64,
) -> FreightRow {
    let mut row = FreightRow::default();
    for item in layout
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Uld && item.deck == deck && cut_by(item, station))
    {
        let fill = match &item.meta {
            ItemMeta::Container(meta) => Color::from_hex(meta.color),
            _ => Color::from_hex("#9b9bd0"),
        };
        row.drawn += 1;
        let outline = Color::from_hex("#334155");
        if is_463l(item) {
            // The pallet stands on its footprint; the load built up on it
            // follows the hold's inner contour less a clearance, to the usable
            // height the layout gave it.
            let base = pallet_base_ring(item);
            if !ring_inside(hull, &base) {
                row.base_escaping += 1;
            }
            polygon(
                scene,
                ring_points(map, &built_up_load(hull, item)),
                fill,
                outline,
            );
            polygon(
                scene,
                ring_points(map, &base),
                Color::from_hex("#3b4a21"),
                outline,
            );
        } else {
            let ring = freight_ring(item);
            let floor = ring.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
            let base: Vec<[f64; 2]> = ring
                .iter()
                .copied()
                .filter(|p| p[1] <= floor + 1e-9)
                .collect();
            if !ring_inside(hull, &base) {
                row.base_escaping += 1;
            }
            polygon(scene, ring_points(map, &ring), fill, outline);
        }
        plated_text(scene, cargo_label(item), map.point(item.y, item.z), 9.0);
    }
    row
}

/// Label of one freight item: "463L pallet · 2,100 kg" or "AKE · 450 kg".
fn cargo_label(item: &DeckItem) -> String {
    let code = match &item.meta {
        ItemMeta::Container(meta) => meta.uld,
        _ => item.label.split(' ').next().unwrap_or(""),
    };
    let kg = item.mass.round() as i64;
    let digits = kg.to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    let name = if code == "463" {
        "463L pallet".to_owned()
    } else {
        code.to_owned()
    };
    format!("{name} \u{b7} {grouped} kg")
}

/// The hold's section between heights `z0` and `z1` at station `x`, as a ring
/// of `[y, z]` points whose sides are the model's hold width at each height
/// (`CabinGeometry::lining_chord_at_z`).
fn hold_ring(cabin: &CabinGeometry, x: f64, z0: f64, z1: f64) -> Vec<[f64; 2]> {
    const STEPS: usize = 16;
    let at = |k: usize| z0 + (z1 - z0) * k as f64 / STEPS as f64;
    let half = |k: usize| 0.5 * cabin.lining_chord_at_z(x, at(k).clamp(z0.min(z1), z0.max(z1)));
    let mut ring: Vec<[f64; 2]> = (0..=STEPS).map(|k| [-half(k), at(k)]).collect();
    ring.extend((0..=STEPS).rev().map(|k| [half(k), at(k)]));
    ring
}

/// Whether `item` is loose bulk rather than a rigid container.
fn is_bulk(item: &DeckItem) -> bool {
    match &item.meta {
        ItemMeta::Container(meta) => meta.uld == "BLK",
        _ => true,
    }
}

pub(super) fn draw_hold(
    scene: &mut Scene,
    map: SectionMap,
    cabin: &CabinGeometry,
    layout: &PayloadLayout,
    station: f64,
    counts: &mut DrawnCounts,
) {
    let deck = &cabin.lower_deck;
    let floor = cabin.floor_z(deck, station);
    let ceiling = cabin.ceil_z(deck, station);
    if ceiling <= floor {
        return;
    }
    let bulk_only = matches!(
        &layout.summary,
        LayoutSummary::Passenger(summary) if summary.hold_ulds == 0
    );
    // Where no hold item stands under the plane, between the forward and aft
    // holds, the lobe holds the centre wing box or the main gear bay: it is
    // outlined and named, not drawn as a hold.
    let hold_x: Vec<(f64, f64)> = layout
        .items
        .iter()
        .filter(|item| item.deck == LOWER && matches!(item.kind, ItemKind::Bag | ItemKind::Uld))
        .map(|item| (item.x - 0.5 * item.length, item.x + 0.5 * item.length))
        .collect();
    let any_cut = layout.items.iter().any(|item| {
        item.deck == LOWER
            && matches!(item.kind, ItemKind::Bag | ItemKind::Uld)
            && cut_by(item, station)
    });
    let first = hold_x.iter().map(|s| s.0).fold(f64::INFINITY, f64::min);
    let last = hold_x.iter().map(|s| s.1).fold(f64::NEG_INFINITY, f64::max);
    let between_holds = !bulk_only && !any_cut && station > first && station < last;
    scene.add(SceneElement::Polygon {
        points: ring_points(map, &hold_ring(cabin, station, floor, ceiling)),
        fill: (!between_holds).then(|| Fill::new(Color::rgba(127, 140, 141, 42))),
        stroke: Some(Stroke::new(Color::from_hex("#7f8c8d"), 1.2)),
    });
    if between_holds {
        plated_text(
            scene,
            "Centre wing box / main gear bay".to_owned(),
            map.point(0.0, 0.5 * (floor + ceiling)),
            9.0,
        );
    }
    let outline = Color::from_hex("#334155");
    for item in layout.items.iter().filter(|item| {
        item.deck == LOWER
            && matches!(item.kind, ItemKind::Bag | ItemKind::Uld)
            && cut_by(item, station)
    }) {
        counts.hold_items += 1;
        if is_bulk(item) || bulk_only {
            // Loose bulk fills the hold the model gives it: the hold's own
            // section up to the height the load reaches.
            let top = (floor + item.height).min(ceiling);
            polygon(
                scene,
                ring_points(map, &hold_ring(cabin, station, floor, top)),
                Color::from_hex("#95a5a6"),
                outline,
            );
            continue;
        }
        counts.hold_ulds += 1;
        let fill = match &item.meta {
            ItemMeta::Container(meta) => Color::from_hex(meta.color),
            _ => Color::from_hex("#9b9bd0"),
        };
        // The section view is driven by the same normalized ULD polygon as the
        // 3D preview, so an LD3's sloped corner stays visible in results.
        let ring =
            asset_for_item(item).map_or_else(|| freight_ring(item), |asset| asset.profile_yz);
        polygon(scene, ring_points(map, &ring), fill, outline);
    }
    if bulk_only {
        // Inside the hold, clear of its ceiling outline, and on no plate.
        text(
            scene,
            "Bulk hold (no ULDs)".to_owned(),
            map.point(0.0, ceiling - 0.3),
            Color::from_hex("#1f2937"),
            9.0,
        );
    }
}

/// Centred text on a pale plate, so it reads over any fill or outline.
fn plated_text(scene: &mut Scene, value: String, pos: [f64; 2], size: f64) {
    let width = value.chars().count() as f64 * size * 0.56 + 8.0;
    let height = size + 6.0;
    scene.add(SceneElement::Rect {
        x: pos[0] - 0.5 * width,
        y: pos[1] - 0.5 * height,
        width,
        height,
        rx: 2.0,
        fill: Some(Fill::new(Color::rgba(255, 255, 255, 225))),
        stroke: None,
    });
    text(scene, value, pos, Color::from_hex("#1f2937"), size);
}
