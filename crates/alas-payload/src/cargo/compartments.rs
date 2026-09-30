// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The baggage compartments of a fuselage: where they are, how long they are
//! and how much volume they enclose.
//!
//! Baggage is stowed in compartments, not at a single station, so the
//! longitudinal balance of a hold load depends on how it is shared between
//! them. This module derives the compartments from the same geometry the
//! container slots come from, or takes a declared list.
//!
//! Frames: x is metres aft of the nose tip, volumes are cubic metres, masses
//! are kilograms.
//!
//! # Derivation
//!
//! With lower-deck container positions, the forward, aft and bulk zones take
//! the extent of the slots that carry those identifiers, and the volume is the
//! integral of usable width times deck height over that extent. Without any
//! (a turboprop with no under-floor hold), baggage is stowed on the main deck
//! outside the seat and monument extent: forward between the flight-deck
//! bulkhead and the first seat, and aft between the last seat and the aft
//! pressure bulkhead. A compartment shorter than
//! [`MIN_MAIN_DECK_COMPARTMENT_M`] is dropped.

use alas_config::{HoldCompartmentConfig, HoldDeck};

use super::CargoSlot;
use crate::geometry::{CabinGeometry, DeckSpec};
use crate::layout::MAIN;

/// Distance from the nose tip to the flight-deck rear bulkhead, m
/// (engineering estimate: the flight deck of a regional turboprop is about
/// 2.6 m long including the cockpit door).
pub const FLIGHT_DECK_BULKHEAD_M: f64 = 2.6;
/// Where the aft pressure bulkhead sits, as a fraction of the tailcone length
/// behind the end of the cabin (engineering estimate).
pub const AFT_BULKHEAD_TAILCONE_FRACTION: f64 = 0.5;
/// Shortest main-deck compartment kept, m (engineering estimate: below this
/// nothing larger than a bag fits).
pub const MIN_MAIN_DECK_COMPARTMENT_M: f64 = 0.5;
/// Stowage density of checked baggage in a compartment, kg/m3 (engineering
/// estimate: loaded holds run at roughly 130 to 190 kg/m3 of net volume).
pub const STOWAGE_DENSITY_KG_M3: f64 = 160.0;
/// Longitudinal step of the volume integral, m.
const VOLUME_STEP_M: f64 = 0.05;
/// Length of the fallback loose-bulk compartment, m.
const FALLBACK_LENGTH_M: f64 = 2.0;
/// How far forward of the cabin end the fallback compartment is centred, m.
const FALLBACK_INSET_M: f64 = 1.5;

/// One baggage compartment.
#[derive(Debug, Clone, PartialEq)]
pub struct HoldCompartment {
    /// Display name.
    pub name: String,
    /// Forward station, m.
    pub x_start_m: f64,
    /// Aft station, m.
    pub x_end_m: f64,
    /// Usable volume, m3.
    pub volume_m3: f64,
    /// Net-mass limit, kg, when one is declared.
    pub max_net_kg: Option<f64>,
    /// Which deck it is on.
    pub deck: HoldDeck,
}

impl HoldCompartment {
    /// Longitudinal length, m.
    pub fn length_m(&self) -> f64 {
        self.x_end_m - self.x_start_m
    }

    /// Station of the compartment's centre, m.
    pub fn centroid_x_m(&self) -> f64 {
        0.5 * (self.x_start_m + self.x_end_m)
    }

    /// Whether station `x` lies inside the compartment.
    pub fn contains(&self, x: f64) -> bool {
        x >= self.x_start_m && x <= self.x_end_m
    }

    fn from_config(config: &HoldCompartmentConfig) -> Self {
        Self {
            name: config.name.clone(),
            x_start_m: config.x_start_m,
            x_end_m: config.x_end_m,
            volume_m3: config.volume_m3,
            max_net_kg: config.max_net_kg,
            deck: config.deck,
        }
    }
}

/// The deck geometry a compartment sits on.
pub fn deck_spec(g: &CabinGeometry, deck: HoldDeck) -> &DeckSpec {
    match deck {
        HoldDeck::Lower => &g.lower_deck,
        HoldDeck::Main => g
            .passenger_decks
            .iter()
            .find(|spec| spec.name == MAIN)
            .or_else(|| g.passenger_decks.first())
            .unwrap_or(&g.lower_deck),
    }
}

/// Volume enclosed between `x_start` and `x_end` on `deck`: the integral of
/// usable floor width times deck height along the span (midpoint rule).
pub fn span_volume_m3(g: &CabinGeometry, deck: &DeckSpec, x_start: f64, x_end: f64) -> f64 {
    if !x_start.is_finite() || !x_end.is_finite() || x_end <= x_start {
        return 0.0;
    }
    let steps = ((x_end - x_start) / VOLUME_STEP_M).ceil().max(1.0);
    let dx = (x_end - x_start) / steps;
    let mut volume = 0.0;
    let mut index = 0.0;
    while index < steps {
        let x = x_start + (index + 0.5) * dx;
        volume += g.usable_width(deck, x) * g.deck_height(deck, x) * dx;
        index += 1.0;
    }
    volume
}

/// The compartments of this fuselage.
///
/// `slots` are the container positions the loader built, and
/// `cabin_extent` is the longitudinal extent `(x_min, x_max)` of the seats and
/// monuments. A non-empty `declared` list of valid compartments replaces
/// every derived one.
pub fn derive_hold_compartments(
    g: &CabinGeometry,
    slots: &[CargoSlot],
    cabin_extent: Option<(f64, f64)>,
    declared: &[HoldCompartmentConfig],
) -> Vec<HoldCompartment> {
    let declared: Vec<HoldCompartment> = declared
        .iter()
        .filter(|config| config.is_valid())
        .map(HoldCompartment::from_config)
        .collect();
    if !declared.is_empty() {
        return declared;
    }

    let mut compartments = lower_deck_compartments(g, slots);
    if compartments.is_empty() {
        if let Some((seat_start, seat_end)) = cabin_extent {
            compartments = main_deck_compartments(g, seat_start, seat_end);
        }
    }
    if compartments.is_empty() {
        compartments.push(fallback_compartment(g));
    }
    compartments
}

fn lower_deck_compartments(g: &CabinGeometry, slots: &[CargoSlot]) -> Vec<HoldCompartment> {
    let low = &g.lower_deck;
    let mut out = Vec::new();
    for (prefix, name) in [
        ("FWD-", "Forward hold"),
        ("AFT-", "Aft hold"),
        ("BULK", "Bulk hold"),
    ] {
        let mut extent: Option<(f64, f64)> = None;
        for slot in slots
            .iter()
            .filter(|slot| slot.deck == low.name && slot.sid.starts_with(prefix))
        {
            let (a, b) = (
                slot.x - slot.uld.length * 0.5,
                slot.x + slot.uld.length * 0.5,
            );
            extent = Some(extent.map_or((a, b), |(lo, hi)| (lo.min(a), hi.max(b))));
        }
        let Some((x_start, x_end)) = extent else {
            continue;
        };
        let volume = span_volume_m3(g, low, x_start, x_end);
        if volume > 0.0 {
            out.push(HoldCompartment {
                name: name.to_owned(),
                x_start_m: x_start,
                x_end_m: x_end,
                volume_m3: volume,
                max_net_kg: None,
                deck: HoldDeck::Lower,
            });
        }
    }
    out
}

fn main_deck_compartments(
    g: &CabinGeometry,
    seat_start: f64,
    seat_end: f64,
) -> Vec<HoldCompartment> {
    let main = deck_spec(g, HoldDeck::Main);
    let forward = (g.x_min + FLIGHT_DECK_BULKHEAD_M, seat_start);
    let aft = (
        seat_end,
        g.cabin_end_x + AFT_BULKHEAD_TAILCONE_FRACTION * g.tailcone_len,
    );
    let mut out = Vec::new();
    for ((x_start, x_end), name) in [(forward, "Forward baggage"), (aft, "Aft baggage")] {
        if x_end - x_start < MIN_MAIN_DECK_COMPARTMENT_M {
            continue;
        }
        let volume = span_volume_m3(g, main, x_start, x_end);
        if volume > 0.0 {
            out.push(HoldCompartment {
                name: name.to_owned(),
                x_start_m: x_start,
                x_end_m: x_end,
                volume_m3: volume,
                max_net_kg: None,
                deck: HoldDeck::Main,
            });
        }
    }
    out
}

/// A loose-bulk compartment at the aft end of the cabin, for a fuselage with
/// neither container positions nor free main-deck length, so that baggage
/// always has somewhere to go.
fn fallback_compartment(g: &CabinGeometry) -> HoldCompartment {
    let centre = g.cabin_end_x - FALLBACK_INSET_M;
    let (x_start, x_end) = (
        centre - 0.5 * FALLBACK_LENGTH_M,
        centre + 0.5 * FALLBACK_LENGTH_M,
    );
    HoldCompartment {
        name: "Bulk overflow".to_owned(),
        x_start_m: x_start,
        x_end_m: x_end,
        volume_m3: span_volume_m3(g, &g.lower_deck, x_start, x_end),
        max_net_kg: None,
        deck: HoldDeck::Lower,
    }
}
