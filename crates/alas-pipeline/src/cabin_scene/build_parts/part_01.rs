// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::f64::consts::TAU;

use alas_config::AlasConfig;
use alas_payload::{
    cargo::{self, ContourFidelity},
    geometry::{CabinGeometry, DeckSpec},
    layout::{DeckItem, ItemKind, ItemMeta},
};

use super::*;
use crate::cabin_scene::CabinSceneInputs;

const CONTOUR_SAMPLES: usize = 96;

pub(super) fn build_scene(
    config: &AlasConfig,
    inputs: CabinSceneInputs<'_>,
    cabin: &CabinGeometry,
) -> Result<CabinScene, String> {
    let layout = inputs.layout;
    let mut missing = base_missing();
    let mut recommended_sections = recommended_sections(&layout.items);
    let mut station_x: Vec<f64> = inputs
        .airplane
        .fuselages
        .first()
        .ok_or("analysis airplane has no fuselage")?
        .xsecs
        .iter()
        .map(|section| section.xyz_c[0])
        .chain(recommended_sections.iter().map(|section| section.x_m))
        .filter(|x| x.is_finite())
        .collect();
    station_x.sort_by(f64::total_cmp);
    station_x.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let stations = station_x
        .into_iter()
        .enumerate()
        .map(|(index, x)| station(cabin, index, x))
        .collect::<Vec<_>>();
    for recommendation in &mut recommended_sections {
        recommendation.station_id = stations
            .iter()
            .find(|station| (station.x_m - recommendation.x_m).abs() < 1e-6)
            .map(|station| station.id.clone())
            .ok_or_else(|| format!("recommended station missing at x={}", recommendation.x_m))?;
    }
    let decks = cabin
        .passenger_decks
        .iter()
        .chain(std::iter::once(&cabin.lower_deck))
        .map(|deck| resolved_deck(cabin, deck))
        .collect();
    let (seat_rows, seats) = resolve_seats(&layout.items);
    let overhead = resolve_overhead(&layout.items);
    let cargo = resolve_cargo(&layout.items);
    if overhead.runs.is_empty() {
        missing.push(missing_input(
            "overhead.runs",
            "solver produced no overhead-bin items",
            "resolved cabin fitting result",
        ));
    }
    Ok(CabinScene {
        schema_version: CABIN_SCENE_SCHEMA_VERSION.to_owned(),
        units: SceneUnits {
            length: "m".into(),
            mass: "kg".into(),
            angle: "rad".into(),
        },
        frame: CoordinateFrame {
            origin: "aircraft geometry origin".into(),
            x: "aft".into(),
            y: "starboard".into(),
            z: "up".into(),
            handedness: "right-handed".into(),
        },
        provenance: SceneProvenance {
            producer: "ALAS optimized pipeline".into(),
            source: inputs.source.to_owned(),
            aircraft_preset: (!config.preset.is_empty()).then(|| config.preset.clone()),
            cabin_preset: config.requirements.cabin_preset.clone(),
            optimized_design: inputs.design,
        },
        stations,
        recommended_sections,
        decks,
        seat_rows,
        seats,
        windows: nominal_windows(cabin, &layout.items),
        overhead,
        cargo,
        missing_inputs: missing,
    })
}

fn station(cabin: &CabinGeometry, index: usize, x: f64) -> SectionStation {
    let outer = ellipse(
        cabin.width_at(x) * 0.5,
        cabin.height_at(x) * 0.5,
        cabin.zc_at(x),
    );
    let inner = cabin
        .inner_semi_axes(x)
        .map(|(a, b)| ellipse(a, b, cabin.zc_at(x)));
    let hold = inner.as_ref().map(|_| hold_contour(cabin, x));
    SectionStation {
        id: format!("station-{index}"),
        x_m: x,
        outer: sourced(
            outer,
            "derived",
            "built Airplane fuselage elliptical section",
        ),
        inner: inner.clone().map(|p| {
            sourced(
                p,
                "derived",
                "outer ellipse offset by configured wall thickness",
            )
        }),
        liner: inner.map(|p| {
            sourced(
                p,
                "nominal_fallback",
                "inner envelope reused; no liner mould-line input",
            )
        }),
        hold: hold.map(|p| {
            sourced(
                p,
                "derived",
                "lower-deck chord clipped from wall-inset ellipse",
            )
        }),
    }
}

fn recommended_sections(items: &[DeckItem]) -> Vec<RecommendedSection> {
    let mut result = Vec::new();
    for deck in [alas_payload::layout::MAIN, alas_payload::layout::UPPER] {
        if let Some(x) = best_cabin_station(items, deck) {
            result.push(RecommendedSection {
                station_id: String::new(),
                purpose: "occupied_cabin_with_overhead".into(),
                deck_id: deck.into(),
                x_m: x,
                intersects: intersecting_kinds(items, deck, x),
                selection: "maximizes simultaneous seat-row and overhead-run interval overlap"
                    .into(),
            });
        }
    }
    if let Some(x) = best_cargo_station(items) {
        result.push(RecommendedSection {
            station_id: String::new(),
            purpose: "occupied_hold".into(),
            deck_id: alas_payload::layout::LOWER.into(),
            x_m: x,
            intersects: intersecting_kinds(items, alas_payload::layout::LOWER, x),
            selection: "maximizes occupied cargo-item interval overlap".into(),
        });
    }
    result.sort_by(|a, b| a.x_m.total_cmp(&b.x_m));
    result
}

fn best_cabin_station(items: &[DeckItem], deck: &str) -> Option<f64> {
    let candidates: Vec<f64> = items
        .iter()
        .filter(|item| item.deck == deck && item.kind == ItemKind::SeatRow)
        .map(|item| item.x)
        .collect();
    let midpoint = extent_midpoint(items, deck, |kind| kind == ItemKind::SeatRow)?;
    candidates.into_iter().min_by(|a, b| {
        cabin_rank(items, deck, *a, midpoint)
            .partial_cmp(&cabin_rank(items, deck, *b, midpoint))
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn best_cargo_station(items: &[DeckItem]) -> Option<f64> {
    let candidates: Vec<f64> = items
        .iter()
        .filter(|item| {
            item.deck == alas_payload::layout::LOWER
                && matches!(item.kind, ItemKind::Uld | ItemKind::Bag)
        })
        .map(|item| item.x)
        .collect();
    let midpoint = extent_midpoint(items, alas_payload::layout::LOWER, |kind| {
        matches!(kind, ItemKind::Uld | ItemKind::Bag)
    })?;
    candidates.into_iter().min_by(|a, b| {
        cargo_rank(items, *a, midpoint)
            .partial_cmp(&cargo_rank(items, *b, midpoint))
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn cabin_rank(items: &[DeckItem], deck: &str, x: f64, midpoint: f64) -> (i64, i64, i64) {
    let seats = overlap_count(items, deck, x, |kind| kind == ItemKind::SeatRow);
    let bins = overlap_count(items, deck, x, |kind| kind == ItemKind::OverheadBin);
    let obstacles = overlap_count(items, deck, x, |kind| {
        matches!(
            kind,
            ItemKind::Exit
                | ItemKind::Galley
                | ItemKind::Lav
                | ItemKind::AccessibleLav
                | ItemKind::WheelchairStowage
        )
    });
    (
        -((seats + bins) as i64),
        obstacles as i64,
        ((x - midpoint).abs() * 1_000_000.0).round() as i64,
    )
}

fn cargo_rank(items: &[DeckItem], x: f64, midpoint: f64) -> (i64, i64) {
    let overlap = overlap_count(items, alas_payload::layout::LOWER, x, |kind| {
        matches!(kind, ItemKind::Uld | ItemKind::Bag)
    });
    (
        -(overlap as i64),
        ((x - midpoint).abs() * 1_000_000.0).round() as i64,
    )
}

fn extent_midpoint(items: &[DeckItem], deck: &str, kind: impl Fn(ItemKind) -> bool) -> Option<f64> {
    let mut x0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    for item in items
        .iter()
        .filter(|item| item.deck == deck && kind(item.kind))
    {
        x0 = x0.min(item.x - item.length * 0.5);
        x1 = x1.max(item.x + item.length * 0.5);
    }
    (x0.is_finite() && x1.is_finite()).then_some((x0 + x1) * 0.5)
}

fn overlap_count(items: &[DeckItem], deck: &str, x: f64, kind: impl Fn(ItemKind) -> bool) -> usize {
    items
        .iter()
        .filter(|item| {
            item.deck == deck && kind(item.kind) && (item.x - x).abs() <= item.length * 0.5 + 1e-9
        })
        .count()
}

fn intersecting_kinds(items: &[DeckItem], deck: &str, x: f64) -> Vec<String> {
    let mut kinds: Vec<String> = items
        .iter()
        .filter(|item| item.deck == deck && (item.x - x).abs() <= item.length * 0.5 + 1e-9)
        .map(|item| item.kind.as_str().to_owned())
        .collect();
    kinds.sort();
    kinds.dedup();
    kinds
}

fn ellipse(a: f64, b: f64, zc: f64) -> Vec<Point2> {
    (0..CONTOUR_SAMPLES)
        .map(|i| {
            let t = TAU * i as f64 / CONTOUR_SAMPLES as f64;
            Point2 {
                y: a * t.cos(),
                z: zc + b * t.sin(),
            }
        })
        .collect()
}

fn hold_contour(c: &CabinGeometry, x: f64) -> Vec<Point2> {
    let deck = &c.lower_deck;
    let floor = c.floor_z(deck, x);
    let ceiling = c.ceil_z(deck, x);
    let half_floor = c.usable_width_at_z(x, floor) * deck.width_factor * 0.5;
    let half_ceiling = c.usable_width_at_z(x, ceiling) * deck.width_factor * 0.5;
    vec![
        Point2 {
            y: -half_floor,
            z: floor,
        },
        Point2 {
            y: half_floor,
            z: floor,
        },
        Point2 {
            y: half_ceiling,
            z: ceiling,
        },
        Point2 {
            y: -half_ceiling,
            z: ceiling,
        },
    ]
}

fn sourced(points: Vec<Point2>, fidelity: &str, source: &str) -> SourcedContour {
    SourcedContour {
        points_yz_m: points,
        fidelity: fidelity.into(),
        source: source.into(),
    }
}

fn resolved_deck(c: &CabinGeometry, deck: &DeckSpec) -> ResolvedDeck {
    let x = (c.cabin_start_x + c.cabin_end_x) * 0.5;
    ResolvedDeck {
        id: deck.name.into(),
        floor_z_m: c.floor_z(deck, x),
        ceiling_z_m: c.ceil_z(deck, x),
        usable_width_m: c.usable_width(deck, x),
        station_x_m: x,
        passenger: deck.is_passenger,
        fidelity: "derived".into(),
        source: "CabinGeometry deck rules at representative constant-body station".into(),
    }
}

fn resolve_seats(items: &[DeckItem]) -> (Vec<SeatRow>, Vec<Seat>) {
    let mut rows = Vec::new();
    let mut seats = Vec::new();
    for (index, item) in items
        .iter()
        .filter(|item| item.kind == ItemKind::SeatRow)
        .enumerate()
    {
        let ItemMeta::Seat(meta) = &item.meta else {
            continue;
        };
        let row_id = format!("seat-row-{index}");
        rows.push(SeatRow {
            id: row_id.clone(),
            deck_id: item.deck.into(),
            envelope: box3(item),
            class: meta.cls.into(),
            abreast: meta.abreast,
            filled: meta.filled,
            blocks: meta.blocks.clone(),
            aisle_width_m: meta.aisle_w,
            fidelity: "solver_resolved_row".into(),
            source: "PayloadLayout DeckItem and SeatMeta".into(),
        });
        let total_width = meta.abreast as f64 * meta.seat_w + meta.aisles as f64 * meta.aisle_w;
        let mut y = -total_width * 0.5 + meta.seat_w * 0.5;
        let mut ordinal = 0_i64;
        for (block_index, count) in meta.blocks.iter().enumerate() {
            for _ in 0..*count {
                seats.push(Seat {
                    id: format!("{row_id}-seat-{ordinal}"),
                    row_id: row_id.clone(),
                    deck_id: item.deck.into(),
                    center_x_m: item.x,
                    center_y_m: y,
                    center_z_m: item.z,
                    width_m: meta.seat_w,
                    occupied: if meta.filled == meta.abreast {
                        Some(true)
                    } else if meta.filled == 0 {
                        Some(false)
                    } else {
                        None
                    },
                    fidelity: "derived_from_solver_row".into(),
                    source: "SeatMeta blocks, seat width and aisle width".into(),
                });
                ordinal += 1;
                y += meta.seat_w;
            }
            if block_index + 1 < meta.blocks.len() {
                y += meta.aisle_w;
            }
        }
    }
    (rows, seats)
}

fn resolve_overhead(items: &[DeckItem]) -> OverheadSystem {
    let mut runs = Vec::new();
    let mut topology = Vec::new();
    for (index, item) in items
        .iter()
        .filter(|item| item.kind == ItemKind::OverheadBin)
        .enumerate()
    {
        let kind = match item.meta {
            ItemMeta::OverheadBin(meta) => meta.bin_type.as_str(),
            _ => "unknown",
        };
        let id = format!("overhead-run-{index}");
        let y0 = item.y - item.width * 0.5;
        let y1 = item.y + item.width * 0.5;
        let z0 = item.z - item.height * 0.5;
        let z1 = item.z + item.height * 0.5;
        runs.push(OverheadRun {
            id: id.clone(),
            deck_id: item.deck.into(),
            kind: kind.into(),
            envelope: box3(item),
            profile_yz_m: vec![
                Point2 { y: y0, z: z0 },
                Point2 { y: y1, z: z0 },
                Point2 { y: y1, z: z1 },
                Point2 { y: y0, z: z1 },
            ],
            fidelity: "solver_envelope".into(),
            source: "PayloadLayout overhead-bin DeckItem".into(),
        });
        topology.push(OverheadTopology {
            run_id: id,
            rail_ids: vec![],
            valance_ids: vec![],
            psu_ids: vec![],
            attachment_ids: vec![],
            status: "missing_supplier_topology".into(),
        });
    }
    OverheadSystem {
        runs,
        topology,
        missing: vec![
            "rail/strongback identifiers and cross-sections".into(),
            "continuous valance and ceiling transition geometry".into(),
            "PSU runs, bin doors, hinges, opening envelopes and structural attachments".into(),
        ],
    }
}
