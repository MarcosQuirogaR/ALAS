// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The composed loading order of a whole aircraft: cargo, then passengers,
//! then fuel.
//!
//! A ramp loading order runs hold cargo first, boards the passengers, and
//! fuels last. [`LoadSequenceSet`] keeps each stage's extreme orders built
//! from the empty aircraft and replays every combination end to end
//! ([`LoadSequenceSet::composed`]): the mass and CG each stage adds do not
//! depend on where the aircraft starts, only its increments do, so a stage
//! built from DOW is replayed from the previous stage's end point.
//! Frames: x is metres aft of the nose tip, masses are kilograms.

use alas_mass::tanks::FuelVectorPoint;

use super::{
    cargo_hold_sequences, concat_sequences, passenger_loading_sequences, zone_boarding_sequences,
    HoldSpan, LoadingPoint, LoadingSequence,
};
use crate::layout::PayloadLayout;

impl LoadingSequence {
    /// The (mass kg, station m) of every increment along the path.
    fn increments(&self) -> Vec<(f64, f64)> {
        self.points
            .windows(2)
            .filter_map(|pair| {
                let dm = pair[1].mass_kg - pair[0].mass_kg;
                let dmoment = pair[1].mass_kg * pair[1].x_m - pair[0].mass_kg * pair[0].x_m;
                (dm > 1.0e-9).then(|| (dm, dmoment / dm))
            })
            .collect()
    }

    /// This order's increments replayed from `start`.
    fn replayed(&self, name: &str, start: LoadingPoint) -> LoadingSequence {
        let mut mass_kg = start.mass_kg;
        let mut moment_kg_m = start.mass_kg * start.x_m;
        let mut points = vec![start];
        for (dm, x_m) in self.increments() {
            mass_kg += dm;
            moment_kg_m += dm * x_m;
            points.push(LoadingPoint {
                mass_kg,
                x_m: moment_kg_m / mass_kg,
            });
        }
        LoadingSequence {
            name: name.to_owned(),
            points,
        }
    }
}

/// The fuel loading order as an increment path: cumulative fuel mass against
/// the fuel's own CG, ascending, from an empty tank set.
///
/// `vector` is a fuel-burn CG vector ([`alas_mass::tanks::fuel_vector`]); the
/// fuelling order is its burn order run backwards, so refuelling retraces
/// the burn. Points with no fuel or a non-finite CG are dropped.
pub fn fuel_loading_sequence(vector: &[FuelVectorPoint]) -> LoadingSequence {
    let mut kept: Vec<FuelVectorPoint> = vector
        .iter()
        .copied()
        .filter(|point| point.fuel_kg > 0.0 && point.fuel_kg.is_finite() && point.x_m.is_finite())
        .collect();
    kept.sort_by(|a, b| a.fuel_kg.total_cmp(&b.fuel_kg));
    let mut points = Vec::with_capacity(kept.len() + 1);
    if let Some(first) = kept.first() {
        points.push(LoadingPoint {
            mass_kg: 0.0,
            x_m: first.x_m,
        });
    }
    points.extend(kept.iter().map(|point| LoadingPoint {
        mass_kg: point.fuel_kg,
        x_m: point.x_m,
    }));
    LoadingSequence {
        name: "fuel".to_owned(),
        points,
    }
}

/// Every stage's extreme orders, each built from the empty aircraft.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadSequenceSet {
    /// The empty aircraft every stage starts from.
    pub dow: LoadingPoint,
    /// Per-hold cargo orders (forward-first, aft-first).
    pub cargo: Vec<LoadingSequence>,
    /// Passenger orders: zone-by-zone front-to-back and back-to-front with
    /// window-middle-aisle inside each zone, then the four cabin-wide
    /// category-first extremes.
    pub pax: Vec<LoadingSequence>,
    /// The fuelling order (one path, cumulative fuel mass against fuel CG).
    pub fuel: LoadingSequence,
}

impl LoadSequenceSet {
    /// Build the stage orders of `layout` from `dow`, with cargo assigned to
    /// `holds` and the fuel path from `fuel_vector`.
    pub fn from_layout(
        layout: &PayloadLayout,
        holds: &[HoldSpan],
        fuel_vector: &[FuelVectorPoint],
        dow: LoadingPoint,
    ) -> Self {
        let mut pax = zone_boarding_sequences(layout, dow);
        pax.extend(passenger_loading_sequences(layout, dow));
        Self {
            dow,
            cargo: cargo_hold_sequences(layout, holds, dow),
            pax,
            fuel: fuel_loading_sequence(fuel_vector),
        }
    }

    /// Every cargo -> passengers -> fuel combination as one path from DOW to
    /// takeoff. A missing stage is skipped, not replaced.
    pub fn composed(&self) -> Vec<LoadingSequence> {
        let stage = |list: &[LoadingSequence]| -> Vec<Option<usize>> {
            if list.is_empty() {
                vec![None]
            } else {
                (0..list.len()).map(Some).collect()
            }
        };
        let mut out = Vec::new();
        for cargo in stage(&self.cargo) {
            let cargo_seq = cargo.map(|k| &self.cargo[k]);
            let after_cargo = cargo_seq.cloned().unwrap_or_else(|| LoadingSequence {
                name: String::new(),
                points: vec![self.dow],
            });
            for pax in stage(&self.pax) {
                let mut path = after_cargo.clone();
                let mut name = cargo_seq.map(|s| s.name.clone()).unwrap_or_default();
                if let Some(k) = pax {
                    let end = path.points.last().copied().unwrap_or(self.dow);
                    let leg = self.pax[k].replayed(&self.pax[k].name, end);
                    path = concat_sequences(&name, &path, &leg);
                    if !name.is_empty() {
                        name.push_str(" | ");
                    }
                    name.push_str(&self.pax[k].name);
                }
                if !self.fuel.points.is_empty() {
                    let end = path.points.last().copied().unwrap_or(self.dow);
                    let leg = self.fuel.replayed("fuel", end);
                    path = concat_sequences(&name, &path, &leg);
                    name.push_str(" | fuel");
                }
                path.name = name;
                out.push(path);
            }
        }
        out
    }
}
