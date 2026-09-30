// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::sequences::potato_and_paths;
use super::*;
use crate::theme::{PALETTE_DARK, PALETTE_LIGHT};
use alas_payload::loading_sequence::{LoadingPoint, LoadingSequence};

const X_LEMAC: f64 = 15.26;
const MAC: f64 = 4.1935;

/// Ten 1 t items spread along an A320-sized cabin.
fn items() -> Vec<(f64, f64)> {
    (0..10)
        .map(|k| (1_000.0, 8.0 + 2.0 * f64::from(k)))
        .collect()
}

/// An A320-sized sheet: limits 17-40 %MAC, DOW 42 t at 26.5 %MAC.
fn sample() -> LoadTrimSheetData {
    let dow_x = X_LEMAC + 0.265 * MAC;
    let envelope = loading_envelope_polygon(42_000.0, dow_x, &items(), X_LEMAC, MAC);
    let zfw = envelope.iter().map(|p| p.0).fold(0.0, f64::max);
    let dow = LoadingPoint {
        mass_kg: 42_000.0,
        x_m: dow_x,
    };
    let path = |name: &str, order: Vec<(f64, f64)>| {
        let (mut mass, mut moment) = (dow.mass_kg, dow.mass_kg * dow.x_m);
        let mut points = vec![dow];
        for (m, x) in order {
            mass += m;
            moment += m * x;
            points.push(LoadingPoint {
                mass_kg: mass,
                x_m: moment / mass,
            });
        }
        LoadingSequence {
            name: name.to_owned(),
            points,
        }
    };
    let mut reversed = items();
    reversed.reverse();
    let composed = vec![path("fwd", items()), path("aft", reversed)];
    let (potato, sequences) = potato_and_paths(&composed, &[], |x| (x - X_LEMAC) / MAC * 100.0);
    let ground = vec![
        LimitVertex {
            mass_kg: 42_000.0,
            fwd_pct_mac: 5.0,
            aft_pct_mac: 45.0,
        },
        LimitVertex {
            mass_kg: 78_000.0,
            fwd_pct_mac: 5.0,
            aft_pct_mac: 45.0,
        },
    ];
    LoadTrimSheetData {
        title: "test".to_owned(),
        x_lemac_m: X_LEMAC,
        mac_m: MAC,
        index: BalanceIndex::for_aircraft(X_LEMAC, MAC, 78_000.0, 5.0, 45.0),
        ground_limits: ground,
        flight_limits: vec![
            LimitVertex {
                mass_kg: 52_000.0,
                fwd_pct_mac: 12.0,
                aft_pct_mac: 38.0,
            },
            LimitVertex {
                mass_kg: 78_000.0,
                fwd_pct_mac: 12.0,
                aft_pct_mac: 38.0,
            },
        ],
        landing_limits: vec![
            LimitVertex {
                mass_kg: 52_000.0,
                fwd_pct_mac: 14.0,
                aft_pct_mac: 45.0,
            },
            LimitVertex {
                mass_kg: 66_000.0,
                fwd_pct_mac: 14.0,
                aft_pct_mac: 45.0,
            },
        ],
        potato,
        sequences,
        takeoff_limits: vec![
            LimitVertex {
                mass_kg: 42_000.0,
                fwd_pct_mac: 17.0,
                aft_pct_mac: 40.0,
            },
            LimitVertex {
                mass_kg: 60_000.0,
                fwd_pct_mac: 18.0,
                aft_pct_mac: 40.0,
            },
            LimitVertex {
                mass_kg: 78_000.0,
                fwd_pct_mac: 22.0,
                aft_pct_mac: 37.0,
            },
        ],
        zfw_limits: vec![
            LimitVertex {
                mass_kg: 42_000.0,
                fwd_pct_mac: 19.0,
                aft_pct_mac: 35.0,
            },
            LimitVertex {
                mass_kg: 62_500.0,
                fwd_pct_mac: 19.0,
                aft_pct_mac: 35.0,
            },
        ],
        weight_lines: vec![
            WeightLine {
                label: "MTOW".to_owned(),
                mass_kg: 78_000.0,
            },
            WeightLine {
                label: "MLW".to_owned(),
                mass_kg: 66_000.0,
            },
            WeightLine {
                label: "MZFW".to_owned(),
                mass_kg: 62_500.0,
            },
        ],
        fuel_curve: vec![(zfw, 27.0), (60_000.0, 29.0), (70_000.0, 28.0)],
        steps: vec![
            LoadStep {
                item: "Empty".to_owned(),
                state: "DOW".to_owned(),
                mass_kg: 42_000.0,
                pct_mac: 26.5,
            },
            LoadStep {
                item: "Passengers".to_owned(),
                state: "ZFW".to_owned(),
                mass_kg: zfw,
                pct_mac: 27.0,
            },
            LoadStep {
                item: "Fuel".to_owned(),
                state: "TOW".to_owned(),
                mass_kg: 70_000.0,
                pct_mac: 28.0,
            },
        ],
        governance: ["fwd".to_owned(), "aft".to_owned()],
        notes: vec!["note".to_owned()],
    }
}

#[test]
fn the_index_is_linear_in_mass_along_a_constant_mac_line_and_k_at_the_reference() {
    let data = sample();
    let p = 31.0;
    let (a, b, c) = (
        data.index_at(40_000.0, p),
        data.index_at(60_000.0, p),
        data.index_at(80_000.0, p),
    );
    assert!(
        ((b - a) - (c - b)).abs() < 1e-9,
        "equal mass steps give equal index steps"
    );
    assert!((data.index_at(55_000.0, 25.0) - data.index.k).abs() < 1e-9);
    assert!(
        (data.index_at(0.0, p) - data.index.k).abs() < 1e-12,
        "every %MAC line passes through (K, 0)"
    );
    assert!((data.pct_at(data.index_at(51_234.0, 33.3), 51_234.0) - 33.3).abs() < 1e-9);
}

#[test]
fn the_scale_constant_keeps_the_band_within_forty_units_of_k() {
    let data = sample();
    for pct in [17.0, 40.0] {
        assert!((data.index_at(78_000.0, pct) - data.index.k).abs() <= 40.0 + 1e-9);
    }
    assert!(BalanceIndex::NICE_C.contains(&data.index.c_kg_m));
}

#[test]
fn limits_interpolate_between_vertices_and_hold_beyond_them() {
    let data = sample();
    assert!((limit_at(&data.takeoff_limits, 51_000.0, true) - 17.5).abs() < 1e-12);
    assert!((limit_at(&data.takeoff_limits, 10_000.0, false) - 40.0).abs() < 1e-12);
    assert!((limit_at(&data.takeoff_limits, 90_000.0, true) - 22.0).abs() < 1e-12);
}

#[test]
fn the_loading_envelope_contains_every_loading_order_and_its_chains_are_convex_in_index_space() {
    let data = sample();
    let n = items().len();
    let polygon = loading_envelope_polygon(42_000.0, X_LEMAC + 0.265 * MAC, &items(), X_LEMAC, MAC);
    // Forward chain = first n+1 points; its index slopes per kg increase.
    let fwd: Vec<(f64, f64)> = polygon[..=n]
        .iter()
        .map(|&(m, p)| (m, data.index_at(m, p)))
        .collect();
    let slopes: Vec<f64> = fwd
        .windows(2)
        .map(|w| (w[1].1 - w[0].1) / (w[1].0 - w[0].0))
        .collect();
    assert!(
        slopes.windows(2).all(|s| s[1] >= s[0] - 1e-12),
        "forward chain convex"
    );
    // A few arbitrary orders stay between the chains at every mass.
    let dow_x = X_LEMAC + 0.265 * MAC;
    for order in [
        [0, 9, 1, 8, 2, 7, 3, 6, 4, 5],
        [5, 4, 6, 3, 7, 2, 8, 1, 9, 0],
    ] {
        let (mut mass, mut moment) = (42_000.0, 42_000.0 * dow_x);
        for (k, &item) in order.iter().enumerate() {
            let (m, x) = items()[item];
            mass += m;
            moment += m * x;
            let pct = (moment / mass - X_LEMAC) / MAC * 100.0;
            let lo = polygon[k + 1].1;
            let hi = polygon[2 * n - k - 1].1;
            assert!(
                pct >= lo - 1e-9 && pct <= hi + 1e-9,
                "order point {k}: {pct} not in [{lo}, {hi}]"
            );
        }
    }
}

#[test]
fn plotted_points_map_back_to_their_mass_and_cg() {
    let data = sample();
    let frame = frame_for(&data);
    for s in &data.steps {
        let (index, mass) = frame.unmap(frame.map(data.index_at(s.mass_kg, s.pct_mac), s.mass_kg));
        assert!((mass - s.mass_kg).abs() < 1e-6);
        assert!((data.pct_at(index, mass) - s.pct_mac).abs() < 1e-9);
    }
}

#[test]
fn the_sheet_renders_without_non_finite_coordinates_in_both_themes() {
    for pal in [&PALETTE_LIGHT, &PALETTE_DARK] {
        let svg = crate::svg::render_svg(&figure_load_trim_sheet(&sample(), pal));
        assert!(
            !svg.contains("NaN") && !svg.contains("inf"),
            "{} theme",
            pal.name
        );
        assert!(svg.contains("LIMIT DEFINITIONS") && svg.contains("WORKED LOADING CASE"));
        assert!(svg.contains("MTOW 78 000 KG") && svg.contains("%MAC"));
    }
}

fn polylines_with_dash(scene: &crate::scene::Scene, dash: Option<&[f64]>) -> usize {
    scene
        .elements
        .iter()
        .filter(|e| {
            matches!(e, crate::scene::SceneElement::Polyline { stroke, .. }
                if stroke.dash_array.as_deref() == dash && stroke.width > 1.5)
        })
        .count()
}

#[test]
fn every_limit_set_is_drawn_in_its_own_style() {
    let scene = figure_load_trim_sheet(&sample(), &PALETTE_LIGHT);
    // Two lines (forward, aft) per set present in the sample.
    assert_eq!(
        polylines_with_dash(&scene, Some(&[7.0, 4.0])),
        2,
        "ground dashed"
    );
    assert_eq!(
        polylines_with_dash(&scene, Some(&[1.5, 3.5])),
        2,
        "flight dotted"
    );
    assert_eq!(
        polylines_with_dash(&scene, Some(&[8.0, 3.0, 1.5, 3.0])),
        2,
        "landing dash-dot"
    );
    assert_eq!(polylines_with_dash(&scene, None), 2, "takeoff solid");
}

#[test]
fn the_potato_lies_inside_the_reorder_polygon_and_inside_the_ground_limits() {
    let data = sample();
    let polygon = loading_envelope_polygon(42_000.0, X_LEMAC + 0.265 * MAC, &items(), X_LEMAC, MAC);
    let n = items().len();
    let chain = |pts: &[(f64, f64)], m: f64| {
        let (a, b) = pts
            .windows(2)
            .find(|w| m >= w[0].0 - 1e-9 && m <= w[1].0 + 1e-9)
            .map(|w| (w[0], w[1]))
            .expect("mass inside chain");
        a.1 + (m - a.0) / (b.0 - a.0) * (b.1 - a.1)
    };
    let fwd = &polygon[..=n];
    let mut aft: Vec<(f64, f64)> = polygon[n..].to_vec();
    aft.reverse();
    assert!(!data.potato.is_empty());
    for level in &data.potato {
        let (lo, hi) = (chain(fwd, level.mass_kg), chain(&aft, level.mass_kg));
        assert!(
            level.fwd_pct_mac >= lo - 0.05 && level.aft_pct_mac <= hi + 0.05,
            "potato at {} kg outside the reorder polygon",
            level.mass_kg
        );
    }
    assert_eq!(data.potato_ground_exceedance_pct_mac(), 0.0);
    let mut tight = data.clone();
    tight.ground_limits[0].aft_pct_mac = 26.0;
    tight.ground_limits[1].aft_pct_mac = 26.0;
    assert!(tight.potato_ground_exceedance_pct_mac() > 0.0);
}
