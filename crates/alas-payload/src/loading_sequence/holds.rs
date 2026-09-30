// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Per-hold cargo loading sequences.
//!
//! Baggage and freight are loaded compartment by compartment, so a physical
//! loading order finishes one hold before it starts the next. The two
//! extreme orders are forward-hold-first and aft-hold-first; inside a hold
//! the items go front to back or back to front with the same direction.
//! Frames: x is metres aft of the nose tip, masses are kilograms.

use alas_config::CargoDeckConfig;

use super::{LoadingPoint, LoadingSequence};
use crate::cargo::{derive_hold_compartments, CargoLoadManager};
use crate::geometry::CabinGeometry;
use crate::layout::{DeckItem, ItemKind, PayloadLayout};

/// A named longitudinal span that holds cargo, m aft of the nose tip.
#[derive(Debug, Clone, PartialEq)]
pub struct HoldSpan {
    /// Display name of the compartment.
    pub name: String,
    /// Forward station, m.
    pub x_start_m: f64,
    /// Aft station, m.
    pub x_end_m: f64,
}

impl HoldSpan {
    fn centre_x_m(&self) -> f64 {
        0.5 * (self.x_start_m + self.x_end_m)
    }

    fn contains(&self, x_m: f64) -> bool {
        x_m >= self.x_start_m && x_m <= self.x_end_m
    }
}

/// The compartments the baggage engine loaded `layout` into: the declared or
/// derived [`crate::cargo::HoldCompartment`]s of `geometry` and `cargo`,
/// rebuilt the same way the layout builder derived them (container slots of
/// the lower decks, main-deck stowage outside the seats when the aircraft has
/// no under-floor hold).
pub fn layout_hold_spans(
    geometry: &CabinGeometry,
    cargo: &CargoDeckConfig,
    layout: &PayloadLayout,
) -> Vec<HoldSpan> {
    let manager = CargoLoadManager::new(
        geometry,
        CargoDeckConfig {
            use_main_deck: false,
            ..cargo.clone()
        },
    );
    let cabin_extent = layout
        .items
        .iter()
        .filter(|item| {
            matches!(
                item.kind,
                ItemKind::SeatRow
                    | ItemKind::Galley
                    | ItemKind::Lav
                    | ItemKind::AccessibleLav
                    | ItemKind::WheelchairStowage
            )
        })
        .fold(None, |extent, item| {
            let (a, b) = (item.x - item.length * 0.5, item.x + item.length * 0.5);
            Some(extent.map_or((a, b), |(lo, hi): (f64, f64)| (lo.min(a), hi.max(b))))
        });
    derive_hold_compartments(
        geometry,
        &manager.slots,
        cabin_extent,
        &cargo.hold_compartments,
    )
    .into_iter()
    .map(|compartment| HoldSpan {
        name: compartment.name,
        x_start_m: compartment.x_start_m,
        x_end_m: compartment.x_end_m,
    })
    .collect()
}

/// Index of the span an item at `x_m` is loaded into: the one containing it,
/// else the one whose centre is nearest.
fn span_index(spans: &[HoldSpan], x_m: f64) -> usize {
    spans
        .iter()
        .position(|span| span.contains(x_m))
        .or_else(|| {
            spans
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    (a.1.centre_x_m() - x_m)
                        .abs()
                        .total_cmp(&(b.1.centre_x_m() - x_m).abs())
                })
                .map(|(index, _)| index)
        })
        .unwrap_or(0)
}

/// The two per-hold cargo sequences from `dow`: forward-hold-first and
/// aft-hold-first, over the bag and container items of `layout` on any deck.
///
/// Items are assigned to `holds` by station. With no `holds`, all cargo is
/// one hold. Returns an empty vector when `layout` carries no cargo.
pub fn cargo_hold_sequences(
    layout: &PayloadLayout,
    holds: &[HoldSpan],
    dow: LoadingPoint,
) -> Vec<LoadingSequence> {
    let items: Vec<&DeckItem> = layout
        .items
        .iter()
        .filter(|item| {
            item.mass > 0.0
                && item.x.is_finite()
                && matches!(item.kind, ItemKind::Uld | ItemKind::Bag)
        })
        .collect();
    if items.is_empty() {
        return Vec::new();
    }
    let single = [HoldSpan {
        name: "hold".to_owned(),
        x_start_m: f64::NEG_INFINITY,
        x_end_m: f64::INFINITY,
    }];
    let spans: &[HoldSpan] = if holds.is_empty() { &single } else { holds };
    let sequence = |name: &str, forward_first: bool| -> LoadingSequence {
        let mut order: Vec<usize> = (0..spans.len()).collect();
        order.sort_by(|&a, &b| {
            let cmp = spans[a].centre_x_m().total_cmp(&spans[b].centre_x_m());
            if forward_first {
                cmp
            } else {
                cmp.reverse()
            }
        });
        let mut mass_kg = dow.mass_kg;
        let mut moment_kg_m = dow.mass_kg * dow.x_m;
        let mut points = vec![dow];
        for hold in order {
            let mut inside: Vec<&&DeckItem> = items
                .iter()
                .filter(|item| span_index(spans, item.x) == hold)
                .collect();
            inside.sort_by(|a, b| {
                let cmp = a.x.total_cmp(&b.x);
                if forward_first {
                    cmp
                } else {
                    cmp.reverse()
                }
            });
            for item in inside {
                mass_kg += item.mass;
                moment_kg_m += item.mass * item.x;
                points.push(LoadingPoint {
                    mass_kg,
                    x_m: moment_kg_m / mass_kg,
                });
            }
        }
        LoadingSequence {
            name: name.to_owned(),
            points,
        }
    };
    vec![
        sequence("holds forward-first", true),
        sequence("holds aft-first", false),
    ]
}
