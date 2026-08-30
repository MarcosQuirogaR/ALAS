// SPDX-License-Identifier: AGPL-3.0-or-later
//! Detailed, physically derived transverse cabin section.

use super::cabin_section_detail as art;
use crate::families::geometry::cabin_assets::asset_for_item;
use crate::scene::{Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_payload::cargo::{uld_by_code, ContourFidelity};
use alas_payload::geometry::{CabinGeometry, DeckSpec};
use alas_payload::layout::{
    DeckItem, ItemKind, ItemMeta, OverheadBinType, PayloadLayout, SeatMeta, LOWER,
};

const WIDTH: f64 = 900.0;
const HEIGHT: f64 = 760.0;

#[derive(Clone, Copy)]
pub(super) struct SectionMap {
    center: [f64; 2],
    pub(super) scale: f64,
    z_center: f64,
}
impl SectionMap {
    pub(super) fn point(self, y: f64, z: f64) -> [f64; 2] {
        [
            self.center[0] + y * self.scale,
            self.center[1] - (z - self.z_center) * self.scale,
        ]
    }
}
fn text(
    s: &mut Scene,
    value: impl Into<String>,
    pos: [f64; 2],
    color: Color,
    size: f64,
    align: TextAlign,
) {
    s.add(SceneElement::Text {
        text: value.into(),
        pos,
        font_size: size,
        color,
        align,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
}
fn intersects(i: &DeckItem, x: f64) -> bool {
    (x - i.x).abs() <= i.length.max(0.0) * 0.5 + 1e-9
}
fn row<'a>(l: &'a PayloadLayout, d: &str, x: f64) -> Option<&'a DeckItem> {
    l.items
        .iter()
        .filter(|i| i.kind == ItemKind::SeatRow && i.deck == d && intersects(i, x))
        .min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
}
fn hold_station(l: &PayloadLayout, fallback: f64) -> f64 {
    let cargo: Vec<_> = l
        .items
        .iter()
        .filter(|i| i.deck == LOWER && matches!(i.kind, ItemKind::Bag | ItemKind::Uld))
        .collect();
    cargo
        .iter()
        .map(|candidate| candidate.x)
        .max_by(|a, b| {
            let count = |x: f64| cargo.iter().filter(|i| intersects(i, x)).count();
            count(*a)
                .cmp(&count(*b))
                .then_with(|| (b - fallback).abs().total_cmp(&(a - fallback).abs()))
        })
        .unwrap_or(fallback)
}
fn representative_cabin_station(l: &PayloadLayout, midpoint: f64) -> f64 {
    let rows: Vec<_> = l
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::SeatRow)
        .collect();
    let mut candidates: Vec<f64> = rows.iter().map(|row| row.x).collect();
    for row in &rows {
        for bin in l.items.iter().filter(|i| {
            i.kind == ItemKind::OverheadBin && i.deck == row.deck && asset_for_item(i).is_some()
        }) {
            let lo = (row.x - row.length * 0.5).max(bin.x - bin.length * 0.5);
            let hi = (row.x + row.length * 0.5).min(bin.x + bin.length * 0.5);
            if lo <= hi {
                candidates.push((lo + hi) * 0.5);
            }
        }
    }
    candidates
        .into_iter()
        .max_by(|a, b| {
            let score = |x: f64| {
                l.items
                    .iter()
                    .filter(|i| {
                        i.kind == ItemKind::OverheadBin
                            && asset_for_item(i).is_some_and(|asset| asset.intersects(x))
                            && rows
                                .iter()
                                .any(|row| row.deck == i.deck && intersects(row, x))
                    })
                    .count()
            };
            score(*a)
                .cmp(&score(*b))
                .then_with(|| (b - midpoint).abs().total_cmp(&(a - midpoint).abs()))
        })
        .unwrap_or(midpoint)
}
#[derive(Debug)]
struct SeatPlacement {
    seats: Vec<f64>,
    aisles: Vec<f64>,
    side_gap: f64,
    seat_width: f64,
    preserved: bool,
}

fn constrained_centers(m: &SeatMeta, available_half_width: f64) -> SeatPlacement {
    let blocks = if m.blocks.is_empty() {
        vec![m.abreast.max(0)]
    } else {
        m.blocks.clone()
    };
    let nominal_width = blocks.iter().sum::<i64>() as f64 * m.seat_w
        + blocks.len().saturating_sub(1) as f64 * m.aisle_w;
    let asset_half = m.seat_w * 0.504;
    let nominal_gap = (available_half_width - nominal_width * 0.5).clamp(0.05, 0.20);
    let usable = (2.0 * (available_half_width - nominal_gap)).max(0.0);
    let seat_total = blocks.iter().sum::<i64>().max(0) as f64 * m.seat_w;
    let aisle_count = blocks.len().saturating_sub(1);
    let aisle = if aisle_count == 0 {
        0.0
    } else {
        ((usable - seat_total) / aisle_count as f64).max(m.aisle_w)
    };
    let actual_width = seat_total + aisle_count as f64 * aisle;
    let asset_overhang = (asset_half - m.seat_w * 0.5).max(0.0);
    let rendered_half = actual_width * 0.5 + asset_overhang;
    let scale = ((available_half_width - 0.025) / rendered_half.max(1e-9)).clamp(0.0, 1.0);
    // Draw installed seats rather than asymmetric occupancy in a partial row.
    let (mut cur, mut left) = (-actual_width * scale / 2.0, m.abreast.max(0));
    let (mut seats, mut aisles) = (vec![], vec![]);
    for (i, b) in blocks.iter().enumerate() {
        for _ in 0..(*b).max(0) {
            if left > 0 {
                seats.push(cur + m.seat_w * scale / 2.0);
                left -= 1;
            }
            cur += m.seat_w * scale;
        }
        if i + 1 < blocks.len() {
            aisles.push(cur + aisle * scale / 2.0);
            cur += aisle * scale;
        }
    }
    let outer = seats
        .iter()
        .map(|y| y.abs() + asset_half * scale)
        .fold(0.0, f64::max);
    SeatPlacement {
        seats,
        aisles,
        side_gap: available_half_width - outer,
        seat_width: m.seat_w * scale,
        preserved: (scale - 1.0).abs() < 1e-9,
    }
}
fn seat_color(m: &SeatMeta) -> Color {
    Color::from_hex(match m.cls {
        "First" => "#9b6fc2",
        "Business" => "#4b91c7",
        _ => "#39a86b",
    })
}
fn half_width(c: &CabinGeometry, x: f64, z0: f64, z1: f64) -> f64 {
    0.5 * c.usable_width_at_z(x, z0).min(c.usable_width_at_z(x, z1))
}

fn ellipse_roof(width: f64, height: f64, zc: f64, y: f64, inset: f64) -> f64 {
    let a = (width * 0.5 - inset).max(0.1);
    let b = (height * 0.5 - inset).max(0.1);
    zc + b * (1.0 - (y / a).powi(2)).max(0.0).sqrt()
}

fn draw_bin(
    s: &mut Scene,
    map: SectionMap,
    item: &DeckItem,
    kind: OverheadBinType,
    _shell: [f64; 3],
    _floor: f64,
) {
    let Some(a) = asset_for_item(item) else {
        return;
    };
    let (body, face) = match kind {
        OverheadBinType::Sidewall => ("#596873", "#87949d"),
        OverheadBinType::Center => ("#74818a", "#aab4ba"),
    };
    let min_y = a
        .profile_yz
        .iter()
        .map(|p| p[0])
        .fold(f64::INFINITY, f64::min);
    let max_y = a
        .profile_yz
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let p: Vec<_> = a.profile_yz.iter().map(|p| map.point(p[0], p[1])).collect();
    art::polygon(
        s,
        p.clone(),
        Color::from_hex(body),
        Color::from_hex("#dce3e7"),
        1.0,
    );
    if p.len() >= 6 {
        art::line(s, p[4], p[5], Color::from_hex(face), 3.0);
        let h = [(p[4][0] + p[5][0]) / 2.0, (p[4][1] + p[5][1]) / 2.0];
        s.add(SceneElement::Circle {
            center: h,
            radius: 2.2,
            fill: Some(Fill::new(Color::from_hex("#d8b45f"))),
            stroke: Some(Stroke::new(Color::from_hex("#303940"), 0.7)),
        });
        for q in [p[1], p[2]] {
            art::line(s, q, [q[0], q[1] - 9.0], Color::from_hex("#9da9b0"), 1.5);
        }
    }
    // Passenger-service-unit strip: continuous visual datum below the bin.
    let psu_z = a
        .profile_yz
        .iter()
        .map(|p| p[1])
        .fold(f64::INFINITY, f64::min)
        - 0.055;
    art::line(
        s,
        map.point(min_y + 0.03, psu_z),
        map.point(max_y - 0.03, psu_z),
        Color::from_hex("#d7dde1"),
        2.5,
    );
    for f in [0.25, 0.5, 0.75] {
        let y = min_y + (max_y - min_y) * f;
        s.add(SceneElement::Circle {
            center: map.point(y, psu_z),
            radius: 1.25,
            fill: Some(Fill::new(Color::from_hex(if f == 0.5 {
                "#f2d36c"
            } else {
                "#91bfd0"
            }))),
            stroke: None,
        });
    }
}

fn draw_continuous_lining(
    s: &mut Scene,
    map: SectionMap,
    c: &CabinGeometry,
    bins: &[&DeckItem],
    x: f64,
    floor: f64,
) {
    if bins.is_empty() {
        return;
    }
    let profiles: Vec<_> = bins.iter().filter_map(|i| asset_for_item(i)).collect();
    let min_y = profiles
        .iter()
        .flat_map(|a| a.profile_yz.iter())
        .map(|p| p[0])
        .fold(f64::INFINITY, f64::min);
    let max_y = profiles
        .iter()
        .flat_map(|a| a.profile_yz.iter())
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let bin_top = profiles
        .iter()
        .flat_map(|a| a.profile_yz.iter())
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max);
    let wall_half = half_width(c, x, floor + 1.45, floor + 1.75);
    let left = min_y.min(-wall_half + 0.04);
    let right = max_y.max(wall_half - 0.04);
    // The continuous centre panel meets the cassette crowns; never place it
    // above the available liner roof in a crown-constrained narrowbody.
    let ceiling = bin_top;
    let mut p = Vec::new();
    for k in 0..=32 {
        let y = left + (right - left) * k as f64 / 32.0;
        p.push(map.point(
            y,
            ellipse_roof(c.width_at(x), c.height_at(x), c.zc_at(x), y, c.wall),
        ));
    }
    p.extend([map.point(right, ceiling), map.point(left, ceiling)]);
    art::polygon(
        s,
        p,
        Color::from_hex("#343f48"),
        Color::from_hex("#c4cdd2"),
        0.9,
    );
    art::line(
        s,
        map.point(left, ceiling),
        map.point(right, ceiling),
        Color::from_hex("#d7dde1"),
        1.4,
    );
    // Subdued rails are structural cues, not separate visible roof towers.
    for y in [min_y, max_y] {
        art::line(
            s,
            map.point(y, bin_top),
            map.point(y, bin_top + 0.08),
            Color::from_hex("#75828a"),
            0.7,
        );
    }
}
fn draw_bins(
    s: &mut Scene,
    map: SectionMap,
    c: &CabinGeometry,
    l: &PayloadLayout,
    d: &DeckSpec,
    x: f64,
    floor: f64,
) {
    let mut bins: Vec<_> = l
        .items
        .iter()
        .filter(|i| {
            i.kind == ItemKind::OverheadBin
                && i.deck == d.name
                && asset_for_item(i).is_some_and(|a| a.intersects(x))
        })
        .collect();
    bins.sort_by(|a, b| a.y.total_cmp(&b.y));
    draw_continuous_lining(s, map, c, &bins, x, floor);
    for i in bins {
        let k = match &i.meta {
            ItemMeta::OverheadBin(m) => m.bin_type,
            _ => OverheadBinType::Sidewall,
        };
        draw_bin(
            s,
            map,
            i,
            k,
            [c.width_at(x), c.height_at(x), c.zc_at(x)],
            floor,
        );
    }
}
fn draw_deck(
    s: &mut Scene,
    map: SectionMap,
    c: &CabinGeometry,
    l: &PayloadLayout,
    d: &DeckSpec,
    x: f64,
    label: Color,
) {
    let floor = c.floor_z(d, x);
    let span = half_width(c, x, floor - 0.08, floor + 0.08);
    art::floor(s, map, span, floor);
    art::windows(s, map, c.width_at(x), c.height_at(x), c.zc_at(x), floor);
    if let Some(r) = row(l, d.name, x) {
        if let ItemMeta::Seat(m) = &r.meta {
            let clearance_half = [floor + 0.59, floor + 1.10]
                .into_iter()
                .map(|z| half_width(c, x, z, z) - 0.025)
                .fold(f64::INFINITY, f64::min);
            let placement = constrained_centers(m, clearance_half);
            for &y in &placement.seats {
                art::seat(s, map, y, floor, placement.seat_width, seat_color(m));
            }
            if let Some(y) = placement.aisles.first() {
                art::person(s, map, *y, floor, c.deck_height(d, x).min(1.78));
            }
            // Retain the diagnostic for future metadata, not overlaid text.
            let _seat_geometry_preserved = placement.preserved;
            let _verified_side_gap_m = placement.side_gap;
        }
    }
    draw_bins(s, map, c, l, d, x, floor);
    text(
        s,
        if d.name == "upper" {
            "UPPER DECK"
        } else {
            "MAIN DECK"
        },
        map.point(-span + 0.05, floor + 0.13),
        label,
        8.0,
        TextAlign::Left,
    );
}
fn draw_hold(s: &mut Scene, map: SectionMap, c: &CabinGeometry, l: &PayloadLayout, x: f64) {
    let d = &c.lower_deck;
    let (floor, ceil) = (c.floor_z(d, x), c.ceil_z(d, x));
    let bottom = c.usable_width_at_z(x, floor) * d.width_factor * 0.5;
    let top = c.usable_width_at_z(x, ceil) * d.width_factor * 0.5;
    art::polygon(
        s,
        [[-bottom, floor], [bottom, floor], [top, ceil], [-top, ceil]]
            .into_iter()
            .map(|p| map.point(p[0], p[1])),
        Color::from_hex("#252d33"),
        Color::from_hex("#7f8c94"),
        1.0,
    );
    art::floor(s, map, bottom, floor);
    for i in l.items.iter().filter(|i| {
        i.deck == LOWER
            && matches!(i.kind, ItemKind::Bag | ItemKind::Uld)
            && asset_for_item(i).map_or_else(|| intersects(i, x), |a| a.intersects(x))
    }) {
        let profile = if let Some(a) = asset_for_item(i) {
            a.profile_yz
        } else if matches!(i.meta, ItemMeta::BulkBag) {
            let (hy, hz) = (i.width * 0.5, i.height * 0.5);
            vec![
                [i.y - hy, i.z - hz],
                [i.y + hy, i.z - hz],
                [i.y + hy, i.z + hz],
                [i.y - hy, i.z + hz],
            ]
        } else {
            continue;
        };
        let cy = profile.iter().map(|p| p[0]).sum::<f64>() / profile.len() as f64;
        let outline: Vec<_> = profile.iter().map(|p| map.point(p[0], p[1])).collect();
        art::polygon(
            s,
            outline,
            Color::from_hex("#8b90c9"),
            Color::from_hex("#d7daf4"),
            1.4,
        );
        let miny = profile.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let maxy = profile
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max);
        let minz = profile.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        for f in [0.25, 0.5, 0.75] {
            art::line(
                s,
                map.point(miny + (maxy - miny) * f, minz + 0.04),
                map.point(miny + (maxy - miny) * f, ceil - 0.05),
                Color::from_hex("#b9bddf"),
                0.6,
            );
        }
        // Diagonal cargo net, corner frame and the actual IATA ULD model.
        art::line(
            s,
            map.point(miny + 0.04, minz + 0.08),
            map.point(maxy - 0.04, ceil - 0.08),
            Color::from_hex("#e4c36b"),
            0.75,
        );
        art::line(
            s,
            map.point(maxy - 0.04, minz + 0.08),
            map.point(miny + 0.04, ceil - 0.08),
            Color::from_hex("#e4c36b"),
            0.75,
        );
        if let ItemMeta::Container(meta) = &i.meta {
            art::label(s, meta.uld, map.point(cy, minz + (ceil - minz) * 0.55), 7.0);
            art::label(
                s,
                format!("{:>3.0}%", meta.fill.clamp(0.0, 1.0) * 100.0),
                map.point(cy, minz + (ceil - minz) * 0.39),
                5.2,
            );
            if let Some(uld) = uld_by_code(meta.uld) {
                let fidelity = match uld.contour.fidelity {
                    ContourFidelity::Authoritative => "approved contour",
                    ContourFidelity::ConservativeEnvelope => "conservative envelope",
                    ContourFidelity::VisualizationOnly => "visual contour",
                };
                art::label(
                    s,
                    format!(
                        "{} · {:.2}×{:.2}×{:.2} m",
                        uld.name, uld.length, uld.width, uld.height
                    ),
                    map.point(cy, minz + 0.16),
                    4.4,
                );
                art::label(s, fidelity, map.point(cy, minz + 0.09), 3.8);
            }
        } else if matches!(i.meta, ItemMeta::BulkBag) {
            art::label(
                s,
                "BULK / LOOSE BAGS",
                map.point(cy, minz + (ceil - minz) * 0.52),
                5.4,
            );
        }
        for f in [0.18, 0.5, 0.82] {
            s.add(SceneElement::Circle {
                center: map.point(miny + (maxy - miny) * f, minz - 0.025),
                radius: 2.0,
                fill: Some(Fill::new(Color::from_hex("#c3cbd0"))),
                stroke: None,
            });
        }
        // Floor locks at both envelope shoulders.
        for y in [miny + 0.04, maxy - 0.04] {
            art::polygon(
                s,
                [
                    map.point(y - 0.025, minz),
                    map.point(y + 0.025, minz),
                    map.point(y, minz + 0.07),
                ],
                Color::from_hex("#d9a73f"),
                Color::from_hex("#332b1d"),
                0.6,
            );
        }
    }
}

/// Draw a detailed, dimensioned representative transverse section.
pub fn figure_cabin_cross_section(
    layout: &PayloadLayout,
    plane: &Airplane,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let mut s = Scene::new(WIDTH, HEIGHT, Some(Color::from_hex(pal.bg)));
    s.title = Some("Cabin Cross-Section".into());
    let Ok(c) = CabinGeometry::new(
        plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    ) else {
        text(
            &mut s,
            "Cabin geometry unavailable",
            [WIDTH / 2.0, HEIGHT / 2.0],
            Color::from_hex(pal.title),
            13.0,
            TextAlign::Center,
        );
        return s;
    };
    let mid = (c.cabin_start_x + c.cabin_end_x) / 2.0;
    let x = representative_cabin_station(layout, mid);
    let (w, h, zc) = (c.width_at(x).max(1.0), c.height_at(x).max(1.0), c.zc_at(x));
    let map = SectionMap {
        center: [WIDTH / 2.0, 390.0],
        scale: (790.0 / w).min(610.0 / h),
        z_center: zc,
    };
    art::shell(&mut s, map, w, h, zc, c.wall.max(0.035));
    for d in &c.passenger_decks {
        draw_deck(&mut s, map, &c, layout, d, x, Color::from_hex(pal.tick));
    }
    let hold_x = hold_station(layout, x);
    draw_hold(&mut s, map, &c, layout, hold_x);
    text(
        &mut s,
        "CABIN CROSS-SECTION",
        [WIDTH / 2.0, 27.0],
        Color::from_hex(pal.title),
        17.0,
        TextAlign::Center,
    );
    text(
        &mut s,
        format!("LOWER-HOLD REFERENCE STATION x = {hold_x:.1} m"),
        [WIDTH / 2.0, HEIGHT - 39.0],
        Color::from_hex(pal.tick),
        6.8,
        TextAlign::Center,
    );
    text(
        &mut s,
        format!(
            "x = {x:.1} m   •   outer {w:.2} × {h:.2} m   •   liner clearance {:.0} mm",
            c.wall * 1000.0
        ),
        [WIDTH / 2.0, 50.0],
        Color::from_hex(pal.tick),
        9.5,
        TextAlign::Center,
    );
    text(
        &mut s,
        "STRUCTURAL SKIN / INSULATION / LINER    ·    ULD CONTOUR FIDELITY SHOWN PER UNIT",
        [WIDTH / 2.0, HEIGHT - 19.0],
        Color::from_hex(pal.tick),
        8.0,
        TextAlign::Center,
    );
    s
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use alas_geom::builder::AircraftBuilder;
    use alas_payload::build::build_payload_layout;
    #[test]
    fn intersection_includes_edges_only() {
        let i = DeckItem {
            kind: ItemKind::Bag,
            deck: "lower",
            x: 2.0,
            y: 0.0,
            z: 0.0,
            length: 1.0,
            width: 1.0,
            height: 1.0,
            mass: 1.0,
            label: String::new(),
            meta: ItemMeta::None,
        };
        assert!(intersects(&i, 2.5));
        assert!(!intersects(&i, 2.500_001));
    }

    #[test]
    fn hold_reference_prefers_a_true_colocated_pair() {
        let base = |x, y| DeckItem {
            kind: ItemKind::Bag,
            deck: LOWER,
            x,
            y,
            z: 0.0,
            length: 1.2,
            width: 1.0,
            height: 1.0,
            mass: 1.0,
            label: String::new(),
            meta: ItemMeta::BulkBag,
        };
        let c = AlasConfig::default();
        let p = AircraftBuilder::new(Some(c.geometry.clone()))
            .build(None, true)
            .expect("aircraft");
        let mut l = build_payload_layout(&p, &c, 0.0, 0.0).expect("layout");
        l.items.retain(|i| i.deck != LOWER);
        l.items
            .extend([base(4.0, -0.6), base(4.0, 0.6), base(12.0, 0.0)]);
        assert_eq!(hold_station(&l, 10.0), 4.0);
    }

    #[test]
    fn cargo_render_identifies_uld_and_contour_fidelity() {
        let c = AlasConfig::default();
        let p = AircraftBuilder::new(Some(c.geometry.clone()))
            .build(None, true)
            .expect("aircraft");
        let l = build_payload_layout(&p, &c, 0.0, 1_000.0).expect("layout");
        let s = figure_cabin_cross_section(&l, &p, &c, Some("dark"));
        let labels: Vec<_> = s
            .elements
            .iter()
            .filter_map(|e| match e {
                SceneElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(labels.iter().any(|v| uld_by_code(v).is_some()));
        assert!(labels
            .iter()
            .any(|v| v.contains("envelope") || v.contains("contour")));
    }
}

#[cfg(test)]
#[path = "cabin_section_regression.rs"]
mod regression;
