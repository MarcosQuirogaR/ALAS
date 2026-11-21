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

use super::{CurveKind, LoadingCurve, NamedPath, PotatoLevel};

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
    // A takeoff fuel at the closure mass can exceed what the tanks hold; the
    // curve follows the tank sequence up to the usable capacity.
    let fuel_kg = fuel_kg.max(0.0).min(tanks.usable_capacity_kg());
    let state = tanks.distribute(fuel_kg).ok()?;
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

/// The loading curves of the potato from the composed orders: the cargo path
/// from the dry operating weight to the weight with the holds loaded, and the
/// four seat orders (window or aisle seats first, front to back or back to
/// front) from there to the end of the cabin loading.
///
/// An order is recognised by the name its generator gives it; the fuel leg and
/// everything after `cabin_end_kg` are not part of a curve. `hold_kg` is
/// `None` for a run without lower-hold cargo.
pub(super) fn loading_curves(
    composed: &[LoadingSequence],
    hold_kg: Option<f64>,
    cabin_end_kg: f64,
    pct_mac: impl Fn(f64) -> f64,
) -> Vec<LoadingCurve> {
    let mut curves: Vec<LoadingCurve> = Vec::new();
    let mut seen_cargo: Vec<&str> = Vec::new();
    let mut seen_seats: Vec<(bool, bool)> = Vec::new();
    let cut = |points: &[LoadingPoint], lo: f64, hi: f64| -> Vec<(f64, f64)> {
        points
            .iter()
            .filter(|p| p.mass_kg >= lo - 0.5 && p.mass_kg <= hi + 0.5 && p.x_m.is_finite())
            .map(|p| (p.mass_kg, pct_mac(p.x_m)))
            .collect()
    };
    for sequence in composed {
        let parts: Vec<&str> = sequence.name.split(" | ").collect();
        let seats = parts.iter().find(|p| p.contains("-first,"));
        let cargo = parts
            .iter()
            .find(|p| !p.contains("-first,") && **p != "fuel" && !p.is_empty());
        if let (Some(hold_kg), Some(cargo)) = (hold_kg, cargo) {
            if !seen_cargo.contains(cargo) {
                seen_cargo.push(cargo);
                let points = cut(&sequence.points, f64::NEG_INFINITY, hold_kg);
                if points.len() >= 2 {
                    curves.push(LoadingCurve {
                        kind: CurveKind::Cargo,
                        points,
                    });
                }
            }
        }
        if let Some(seats) = seats {
            let key = (
                seats.ends_with("front-to-back"),
                seats.starts_with("window-first"),
            );
            if !seen_seats.contains(&key) {
                seen_seats.push(key);
                let lo = hold_kg.unwrap_or(f64::NEG_INFINITY);
                let points = cut(&sequence.points, lo, cabin_end_kg);
                if points.len() >= 2 {
                    curves.push(LoadingCurve {
                        kind: CurveKind::Seats {
                            front_to_back: key.0,
                            window_first: key.1,
                        },
                        points,
                    });
                }
            }
        }
    }
    curves
}
