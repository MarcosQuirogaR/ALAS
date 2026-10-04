// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The composed loading orders behind the boarding potato: per-hold cargo,
//! zone-by-zone boarding, then fuel, from the run's own payload layout, cabin
//! geometry and tank arrangement.

use alas_config::AlasConfig;
use alas_mass::tanks::{fuel_vector, resolve_product_layout};
use alas_payload::loading_sequence::{
    layout_hold_spans, potato_boundary_at, LoadSequenceSet, LoadingPoint, LoadingSequence,
};
use alas_payload::CabinGeometry;
use alas_pipeline::full_analysis::AnalysisReport;

use super::{NamedPath, PotatoLevel};

/// Number of tank-burn points in the fuel path.
const FUEL_POINTS: usize = 21;
/// Uniform mass levels the potato is sampled at, plus every worked-case mass.
const POTATO_LEVELS: usize = 41;

/// The stage orders of `report`'s loading from `dow`, with `fuel_kg` of fuel
/// distributed by the tank arrangement; `None` when the run has no payload
/// layout or tank arrangement.
pub(super) fn load_sequence_set(
    config: &AlasConfig,
    report: &AnalysisReport,
    dow: LoadingPoint,
    fuel_kg: f64,
) -> Option<LoadSequenceSet> {
    let layout = report.payload_layout.as_ref()?;
    let geometry = CabinGeometry::new(
        &report.airplane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    )
    .ok();
    let holds = geometry
        .as_ref()
        .map(|g| layout_hold_spans(g, &config.cabin.cargo, layout))
        .unwrap_or_default();
    let tanks = resolve_product_layout(config, &report.design, &report.airplane).ok()?;
    let state = tanks.distribute(fuel_kg.max(0.0)).ok()?;
    let vector = fuel_vector(&tanks, &state, FUEL_POINTS);
    Some(LoadSequenceSet::from_layout(layout, &holds, &vector, dow))
}

/// The potato of `composed` at uniform mass levels and at every mass in
/// `extra_levels_kg`, and each order as a (mass, %MAC) path. `pct_mac` maps a
/// station in metres to %MAC.
pub(super) fn potato_and_paths(
    composed: &[LoadingSequence],
    extra_levels_kg: &[f64],
    pct_mac: impl Fn(f64) -> f64,
) -> (Vec<PotatoLevel>, Vec<NamedPath>) {
    let lo = composed
        .iter()
        .filter_map(|s| s.points.first().map(|p| p.mass_kg))
        .fold(f64::INFINITY, f64::min);
    let hi = composed
        .iter()
        .filter_map(|s| s.points.last().map(|p| p.mass_kg))
        .fold(f64::NEG_INFINITY, f64::max);
    if !(lo.is_finite() && hi.is_finite() && hi > lo) {
        return (Vec::new(), Vec::new());
    }
    let mut levels: Vec<f64> = (0..POTATO_LEVELS)
        .map(|k| lo + (hi - lo) * k as f64 / (POTATO_LEVELS - 1) as f64)
        .collect();
    levels.extend(
        extra_levels_kg
            .iter()
            .copied()
            .filter(|m| *m >= lo && *m <= hi),
    );
    levels.sort_by(f64::total_cmp);
    levels.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let potato = potato_boundary_at(composed, &levels)
        .into_iter()
        .map(|p| PotatoLevel {
            mass_kg: p.mass_kg,
            fwd_pct_mac: pct_mac(p.min_x_m),
            aft_pct_mac: pct_mac(p.max_x_m),
        })
        .collect();
    let paths = composed
        .iter()
        .map(|s| NamedPath {
            name: s.name.clone(),
            points: s
                .points
                .iter()
                .map(|p| (p.mass_kg, pct_mac(p.x_m)))
                .collect(),
        })
        .collect();
    (potato, paths)
}
