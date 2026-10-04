// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Assemble the cabin scene from a completed analysis.

mod deck_items;
mod inputs;
mod recommendations;

use deck_items::*;
use inputs::*;
use recommendations::recommended_sections;

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
