// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Boarding zones and zone-by-zone passenger sequences.
//!
//! A boarding zone is a contiguous run of seat rows on one deck and in one
//! cabin class, cut wherever a monument (galley, lavatory, wheelchair
//! stowage) or an emergency exit lies between two consecutive rows. Airlines
//! call zones in a fixed order, front to back or back to front; inside a
//! zone the passengers board window seats first, then middle, then aisle
//! (the window-middle-aisle rule), which keeps the seat-interference
//! cost low. The row-level mass model is that of [`super::seat_category_capacities`].
//! Frames: x is metres aft of the nose tip, masses are kilograms.

use super::{
    passenger_sequence, row_share_of, LoadingPoint, LoadingSequence, PayloadLayout, RowShare,
};
use crate::layout::{DeckItem, ItemKind};

/// One boarding zone.
#[derive(Debug, Clone)]
pub struct BoardingZone {
    /// Display name, e.g. `zone 2 (economy, main)`.
    pub name: String,
    /// The deck the zone is on.
    pub deck: &'static str,
    /// The cabin class of its rows.
    pub cls: &'static str,
    /// Station of the zone's first row, m.
    pub x_start_m: f64,
    /// Station of the zone's last row, m.
    pub x_end_m: f64,
    pub(super) rows: Vec<RowShare>,
}

impl BoardingZone {
    /// Seat-row count of the zone.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Occupant mass of the zone, kg.
    pub fn mass_kg(&self) -> f64 {
        self.rows
            .iter()
            .map(|row| row.window_kg + row.middle_kg + row.aisle_kg)
            .sum()
    }
}

/// Whether an item cuts a zone boundary between two rows.
fn is_zone_break(item: &DeckItem) -> bool {
    matches!(
        item.kind,
        ItemKind::Galley
            | ItemKind::Lav
            | ItemKind::AccessibleLav
            | ItemKind::WheelchairStowage
            | ItemKind::Exit
    ) && item.x.is_finite()
}

/// The boarding zones of `layout`, ordered by the station of their first row.
///
/// A new zone starts at a row when the cabin class or the deck differs from
/// the previous row on that deck, or when a monument or exit lies strictly
/// between the two rows' stations. Empty for a layout without seat rows.
pub fn boarding_zones(layout: &PayloadLayout) -> Vec<BoardingZone> {
    let mut decks: Vec<&'static str> = Vec::new();
    for item in &layout.items {
        if row_share_of(item).is_some() && !decks.contains(&item.deck) {
            decks.push(item.deck);
        }
    }
    let mut zones: Vec<BoardingZone> = Vec::new();
    for deck in decks {
        let mut rows: Vec<(&DeckItem, RowShare, &'static str)> = layout
            .items
            .iter()
            .filter(|item| item.deck == deck)
            .filter_map(|item| {
                let share = row_share_of(item)?;
                let cls = match &item.meta {
                    crate::layout::ItemMeta::Seat(seat) => seat.cls,
                    _ => "",
                };
                Some((item, share, cls))
            })
            .collect();
        rows.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
        let breaks: Vec<f64> = layout
            .items
            .iter()
            .filter(|item| item.deck == deck && is_zone_break(item))
            .map(|item| item.x)
            .collect();
        let mut current: Option<(BoardingZone, f64)> = None;
        for (item, share, cls) in rows {
            let starts_new = current.as_ref().is_none_or(|(zone, last_x)| {
                zone.cls != cls || breaks.iter().any(|&x| x > *last_x && x < item.x)
            });
            if starts_new {
                if let Some((zone, _)) = current.take() {
                    zones.push(zone);
                }
                current = Some((
                    BoardingZone {
                        name: String::new(),
                        deck,
                        cls,
                        x_start_m: item.x,
                        x_end_m: item.x,
                        rows: Vec::new(),
                    },
                    item.x,
                ));
            }
            if let Some((zone, last_x)) = current.as_mut() {
                zone.rows.push(share);
                zone.x_end_m = item.x;
                *last_x = item.x;
            }
        }
        if let Some((zone, _)) = current.take() {
            zones.push(zone);
        }
    }
    zones.sort_by(|a, b| a.x_start_m.total_cmp(&b.x_start_m));
    for (index, zone) in zones.iter_mut().enumerate() {
        zone.name = format!("zone {} ({}, {})", index + 1, zone.cls, zone.deck);
    }
    zones
}

/// The two zone boarding sequences from `dow`: zones front to back and zones
/// back to front, window-middle-aisle inside each zone. Rows of a zone board
/// in the direction of the zone order.
///
/// Empty when `layout` has no seat rows.
pub fn zone_boarding_sequences(layout: &PayloadLayout, dow: LoadingPoint) -> Vec<LoadingSequence> {
    let zones = boarding_zones(layout);
    if zones.is_empty() {
        return Vec::new();
    }
    let sequence = |name: &str, front_to_back: bool| -> LoadingSequence {
        let ordered: Vec<&BoardingZone> = if front_to_back {
            zones.iter().collect()
        } else {
            zones.iter().rev().collect()
        };
        let mut points = vec![dow];
        let mut carried = dow;
        for zone in ordered {
            let leg = passenger_sequence(
                name,
                &zone.rows,
                front_to_back,
                carried,
                |row| row.window_kg,
                |row| row.aisle_kg,
            );
            points.extend(leg.points.iter().skip(1));
            if let Some(last) = points.last() {
                carried = *last;
            }
        }
        LoadingSequence {
            name: name.to_owned(),
            points,
        }
    };
    vec![
        sequence("zones front-to-back, window-middle-aisle", true),
        sequence("zones back-to-front, window-middle-aisle", false),
    ]
}
