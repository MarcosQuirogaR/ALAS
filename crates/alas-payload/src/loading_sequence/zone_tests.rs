// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_mass::tanks::FuelVectorPoint;

use super::tests::narrowbody_layout;
use super::*;
use crate::layout::{ExitMeta, ItemKind, ItemMeta, LOWER};

const DOW: LoadingPoint = LoadingPoint {
    mass_kg: 40_000.0,
    x_m: 17.0,
};

/// Twelve rows 1 m apart from x = 10: business rows 0-2, a lavatory between
/// rows 3 and 4, an exit between rows 7 and 8, row masses that differ.
fn zoned_layout() -> PayloadLayout {
    let mut layout = narrowbody_layout(12);
    for (k, item) in layout.items.iter_mut().enumerate() {
        item.mass = 250.0 + 37.0 * k as f64;
        if k < 3 {
            if let ItemMeta::Seat(seat) = &mut item.meta {
                seat.cls = "business";
            }
        }
    }
    let template = layout.items[0].clone();
    layout.items.push(DeckItem {
        kind: ItemKind::Lav,
        x: 13.5,
        mass: 0.0,
        meta: ItemMeta::None,
        ..template.clone()
    });
    layout.items.push(DeckItem {
        kind: ItemKind::Exit,
        x: 17.5,
        mass: 0.0,
        meta: ItemMeta::Exit(ExitMeta {
            exit_type: "III",
            door_w: 0.51,
            door_h: 0.92,
        }),
        ..template
    });
    layout
}

fn hold_item(x: f64, mass_kg: f64) -> DeckItem {
    let mut item = narrowbody_layout(1).items.remove(0);
    item.kind = ItemKind::Bag;
    item.deck = LOWER;
    item.x = x;
    item.mass = mass_kg;
    item.meta = ItemMeta::BulkBag;
    item
}

/// (mass kg, station m) chunks of every window, middle and aisle share.
fn chunks(rows: &[RowShare]) -> Vec<(f64, f64)> {
    rows.iter()
        .flat_map(|row| {
            [row.window_kg, row.middle_kg, row.aisle_kg]
                .into_iter()
                .map(|kg| (kg, row.x_m))
        })
        .filter(|(kg, _)| *kg > 0.0)
        .collect()
}

/// Moment about the nose of the forward-first (`aft == false`) or aft-first
/// chain of `items` from `start`, interpolated linearly in mass. The chain
/// bounds every ordering of the items: a moment is linear in mass along one
/// item, and the extreme chains are the greedy orders by station.
fn chain_moment(start: LoadingPoint, items: &[(f64, f64)], aft: bool, mass_kg: f64) -> f64 {
    let mut sorted = items.to_vec();
    sorted.sort_by(|a, b| a.1.total_cmp(&b.1));
    if aft {
        sorted.reverse();
    }
    let (mut mass, mut moment) = (start.mass_kg, start.mass_kg * start.x_m);
    for (m, x) in sorted {
        if mass_kg <= mass + m {
            return moment + (mass_kg - mass) * x;
        }
        mass += m;
        moment += m * x;
    }
    moment
}

fn assert_within_chains(
    start: LoadingPoint,
    items: &[(f64, f64)],
    points: &[LoadingPoint],
    label: &str,
) {
    for point in points {
        let moment = point.mass_kg * point.x_m;
        let lo = chain_moment(start, items, false, point.mass_kg);
        let hi = chain_moment(start, items, true, point.mass_kg);
        let tol = 1.0e-9 * hi.abs().max(1.0);
        assert!(
            moment >= lo - tol && moment <= hi + tol,
            "{label}: moment {moment} outside [{lo}, {hi}] at {} kg",
            point.mass_kg
        );
    }
}

#[test]
fn zones_are_cut_by_class_lavatory_and_exit() {
    let zones = boarding_zones(&zoned_layout());
    let rows: Vec<usize> = zones.iter().map(BoardingZone::row_count).collect();
    assert_eq!(rows, vec![3, 1, 4, 4], "{zones:?}");
    assert_eq!(zones[0].cls, "business");
    assert!(zones.windows(2).all(|w| w[0].x_start_m < w[1].x_start_m));
    let total: f64 = zones.iter().map(BoardingZone::mass_kg).sum();
    let expected: f64 = (0..12).map(|k| 250.0 + 37.0 * f64::from(k)).sum();
    assert!((total - expected).abs() < 1e-9);
}

#[test]
fn zone_sequences_conserve_mass_and_stay_inside_the_reorder_extremes() {
    let layout = zoned_layout();
    let sequences = zone_boarding_sequences(&layout, DOW);
    assert_eq!(sequences.len(), 2);
    let zones = boarding_zones(&layout);
    let all: Vec<RowShare> = zones.iter().flat_map(|z| z.rows.clone()).collect();
    let items = chunks(&all);
    let total: f64 = items.iter().map(|c| c.0).sum();
    for sequence in &sequences {
        let end = sequence.points.last().unwrap();
        assert!((end.mass_kg - DOW.mass_kg - total).abs() < 1e-6);
        assert_within_chains(DOW, &items, &sequence.points, &sequence.name);
    }
    let (front, back) = (&sequences[0], &sequences[1]);
    let (a, b) = (front.points.last().unwrap(), back.points.last().unwrap());
    assert!(
        (a.x_m - b.x_m).abs() < 1e-9,
        "both orders end at one ZFW CG"
    );
    // Mid-boarding the front-to-back order is forward of back-to-front.
    let mid = DOW.mass_kg + 0.5 * total;
    let at = |s: &LoadingSequence| interpolate_x(s, mid).unwrap();
    assert!(at(front) < at(back) - 0.1);
}

#[test]
fn every_single_zone_order_lies_between_that_zones_extreme_chains() {
    let zones = boarding_zones(&zoned_layout());
    let mut seed = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for zone in &zones {
        let items = chunks(&zone.rows);
        for trial in 0..200 {
            let mut order = items.clone();
            for k in (1..order.len()).rev() {
                order.swap(k, (next() % (k as u64 + 1)) as usize);
            }
            let (mut mass, mut moment) = (DOW.mass_kg, DOW.mass_kg * DOW.x_m);
            let mut points = Vec::new();
            for (m, x) in order {
                mass += m;
                moment += m * x;
                points.push(LoadingPoint {
                    mass_kg: mass,
                    x_m: moment / mass,
                });
            }
            assert_within_chains(
                DOW,
                &items,
                &points,
                &format!("{} trial {trial}", zone.name),
            );
        }
        let named = passenger_sequence(
            "wma",
            &zone.rows,
            true,
            DOW,
            |row| row.window_kg,
            |row| row.aisle_kg,
        );
        assert_within_chains(DOW, &items, &named.points, &zone.name);
    }
}

#[test]
fn hold_sequences_finish_one_hold_before_the_next_and_agree_at_the_end() {
    let mut layout = narrowbody_layout(1);
    layout.items = vec![
        hold_item(8.0, 300.0),
        hold_item(9.0, 200.0),
        hold_item(27.0, 400.0),
        hold_item(28.0, 100.0),
    ];
    let holds = [
        HoldSpan {
            name: "fwd".to_owned(),
            x_start_m: 7.0,
            x_end_m: 10.0,
        },
        HoldSpan {
            name: "aft".to_owned(),
            x_start_m: 26.0,
            x_end_m: 29.0,
        },
    ];
    let sequences = cargo_hold_sequences(&layout, &holds, DOW);
    assert_eq!(sequences.len(), 2);
    let (fwd, aft) = (&sequences[0], &sequences[1]);
    // After the two forward items only, the forward-first CG is forward of
    // the aft-first CG after its first two (aft) items.
    assert!(fwd.points[2].x_m < DOW.x_m && aft.points[2].x_m > DOW.x_m);
    assert!((fwd.points[2].mass_kg - (DOW.mass_kg + 500.0)).abs() < 1e-9);
    assert!((aft.points[2].mass_kg - (DOW.mass_kg + 500.0)).abs() < 1e-9);
    let (a, b) = (fwd.points.last().unwrap(), aft.points.last().unwrap());
    assert!((a.mass_kg - b.mass_kg).abs() < 1e-9 && (a.x_m - b.x_m).abs() < 1e-9);
    let items: Vec<(f64, f64)> = layout.items.iter().map(|i| (i.mass, i.x)).collect();
    assert_within_chains(DOW, &items, &fwd.points, "fwd");
    assert_within_chains(DOW, &items, &aft.points, "aft");
}

#[test]
fn the_composed_set_runs_cargo_then_passengers_then_fuel_to_one_takeoff_point() {
    let mut layout = zoned_layout();
    layout.items.push(hold_item(8.0, 300.0));
    layout.items.push(hold_item(27.0, 400.0));
    let holds = [
        HoldSpan {
            name: "fwd".to_owned(),
            x_start_m: 7.0,
            x_end_m: 10.0,
        },
        HoldSpan {
            name: "aft".to_owned(),
            x_start_m: 26.0,
            x_end_m: 29.0,
        },
    ];
    let fuel = [
        FuelVectorPoint {
            fuel_kg: 6_000.0,
            x_m: 17.5,
            z_m: 0.0,
        },
        FuelVectorPoint {
            fuel_kg: 3_000.0,
            x_m: 16.0,
            z_m: 0.0,
        },
        FuelVectorPoint {
            fuel_kg: 0.0,
            x_m: 0.0,
            z_m: 0.0,
        },
    ];
    let set = LoadSequenceSet::from_layout(&layout, &holds, &fuel, DOW);
    assert_eq!(set.cargo.len(), 2);
    assert_eq!(set.pax.len(), 6, "2 zone orders + 4 category-first");
    let composed = set.composed();
    assert_eq!(composed.len(), 12);
    let pax_kg: f64 = layout
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::SeatRow)
        .map(|i| i.mass)
        .sum();
    let takeoff = DOW.mass_kg + 700.0 + pax_kg + 6_000.0;
    let first_end = *composed[0].points.last().unwrap();
    for sequence in &composed {
        let end = sequence.points.last().unwrap();
        assert!((end.mass_kg - takeoff).abs() < 1e-6, "{}", sequence.name);
        assert!((end.x_m - first_end.x_m).abs() < 1e-9, "{}", sequence.name);
        assert!(sequence
            .points
            .windows(2)
            .all(|w| w[1].mass_kg >= w[0].mass_kg));
        assert!(sequence.name.ends_with("| fuel"));
    }
    let mid_boarding = DOW.mass_kg + 700.0 + 0.5 * pax_kg;
    let potato = potato_boundary_at(&composed, &[DOW.mass_kg, mid_boarding, takeoff]);
    assert_eq!(potato.len(), 3);
    assert!(potato[1].max_x_m > potato[1].min_x_m + 0.1, "mid boarding");
    assert!(
        (potato[2].max_x_m - potato[2].min_x_m).abs() < 1e-9,
        "one takeoff point"
    );
}
