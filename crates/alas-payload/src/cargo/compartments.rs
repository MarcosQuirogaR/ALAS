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
//! (a turboprop or business jet with no under-floor hold), baggage is stowed
//! in one main-deck compartment between the last seat or monument and the end
//! of the constant-section cabin barrel, outside the seat and monument extent.
//! A compartment shorter than [`MIN_MAIN_DECK_COMPARTMENT_M`] is dropped.
//! A body whose source draws its compartments declares them instead
//! (`HoldCompartmentConfig`). A declared compartment behind the seats (a
//! business jet's aft cabin baggage area placed at its certified loading arm)
//! keeps the derived compartment's bounds: it starts no further forward than
//! the last seat or monument and ends no further aft than the cabin barrel.

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
/// Stowage density of freight and checked baggage, kg/m3: the ICAO default
/// conversion factor "if no conversion factor is available" (ICAO DATA+ glossary,
/// p. 2 "Default mass / densities values":
/// freight 161 kg/m3, baggage 161 kg/m3; p. 4 "Freight mass": the same factor for
/// passenger baggage). A central estimate of a stowed density (ICAO publishes it
/// as a traffic-statistics conversion, not as a stowage limit; the 10 lb/ft3
/// volumetric-payload convention agrees): a bag-loaded container of 35-45 bags
/// stows 135-200 kg/m3, and dense freight (machinery, metals) more, so the cap it
/// sets is conservative for both; a declared `stowage_density_kg_m3` replaces it.
/// A container carries at most its usable volume times this density
/// (`CargoLoadManager::new`).
pub const STOWAGE_DENSITY_KG_M3: f64 = 161.0;
/// Share of a compartment's net volume that loose bulk baggage and freight
/// fill: Torenbeek, Synthesis of Subsonic Airplane Design (1982), section
/// 3.3.2 "Payload density and volume of the freight hold", p.84: "The stowage
/// factor represents the percentage of usable space in relation to the net
/// volume. This allows for space losses for storage nets, clearance between
/// cargo and structure, room for inspection, etc. Depending on the fuselage
/// shape and the particular nature of the freight, this factor may vary
/// between .70 and .85." A lower-lobe hold is a lined compartment of nearly
/// constant section loaded with bags and loose freight, the regular end of
/// that range; the usable volumes the airframers print for bulk-loaded
/// compartments are geometric volume less the door-net zone and clearances
/// (Airbus A220 ACP issue 013 section 6.2: "based on geometric volume and
/// accounts for the unusable area in the vicinity of the cargo doors").
/// Applied to every loose-bulk position whose compartment volume is
/// integrated over the built hold; a compartment volume a source declares is
/// already its usable volume and is not reduced.
pub const BULK_STOWAGE_FACTOR: f64 = 0.85;
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
    /// Volume, m3: the declared one where the configuration prints it (taken
    /// as the usable volume the source states), otherwise the net volume the
    /// built hold encloses inside its linings.
    pub volume_m3: f64,
    /// Whether [`Self::volume_m3`] was declared rather than integrated over the
    /// built hold.
    pub volume_declared: bool,
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

    /// Usable stowage volume, m3: a declared volume is already the usable one
    /// the source states; an integrated net volume is reduced by
    /// [`BULK_STOWAGE_FACTOR`] (storage nets, clearances, room for inspection).
    pub fn usable_volume_m3(&self) -> f64 {
        if self.volume_declared {
            self.volume_m3
        } else {
            BULK_STOWAGE_FACTOR * self.volume_m3
        }
    }

    /// Station of the compartment's centre, m.
    pub fn centroid_x_m(&self) -> f64 {
        0.5 * (self.x_start_m + self.x_end_m)
    }

    /// Whether station `x` lies inside the compartment.
    pub fn contains(&self, x: f64) -> bool {
        x >= self.x_start_m && x <= self.x_end_m
    }

    /// A declared compartment behind the last seat row, bounded as the
    /// derived main-deck compartment is: from no further forward than the last
    /// seat or monument (`seat_end`) to no further aft than the end of the
    /// constant-section barrel. A barrel that ends forward of the declared aft
    /// station (a shortened fuselage) moves the compartment forward with its
    /// aft end, at its declared length, instead of cutting it. `None` when
    /// less than [`MIN_MAIN_DECK_COMPARTMENT_M`] of it is left.
    fn behind_seats(
        g: &CabinGeometry,
        config: &HoldCompartmentConfig,
        seat_end: Option<f64>,
    ) -> Option<Self> {
        let shift = (config.x_end_m - g.cabin_end_x).max(0.0);
        let (start, x_end) = (config.x_start_m - shift, config.x_end_m - shift);
        let x_start = seat_end.map_or(start, |end| start.max(end));
        if x_end - x_start < MIN_MAIN_DECK_COMPARTMENT_M {
            return None;
        }
        let bounded = HoldCompartmentConfig {
            x_start_m: x_start,
            x_end_m: x_end,
            ..config.clone()
        };
        Some(Self::from_config(g, &bounded))
    }

    fn from_config(g: &CabinGeometry, config: &HoldCompartmentConfig) -> Self {
        let volume_m3 = config.volume_m3.unwrap_or_else(|| {
            span_volume_within_m3(
                g,
                deck_spec(g, config.deck),
                config.x_start_m,
                config.x_end_m,
                |x| config.clear_width_at(x),
            )
        });
        Self {
            name: config.name.clone(),
            x_start_m: config.x_start_m,
            x_end_m: config.x_end_m,
            volume_m3,
            volume_declared: config.volume_m3.is_some(),
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
    span_volume_within_m3(g, deck, x_start, x_end, |_| None)
}

/// [`span_volume_m3`] inside a clear width the source draws, m, where
/// `clear_width` gives one: where it is narrower than the deck's usable width
/// the section takes that share of its area (exact for the rectangular
/// section of a hold without a drawn polygon).
pub fn span_volume_within_m3(
    g: &CabinGeometry,
    deck: &DeckSpec,
    x_start: f64,
    x_end: f64,
    clear_width: impl Fn(f64) -> Option<f64>,
) -> f64 {
    if !x_start.is_finite() || !x_end.is_finite() || x_end <= x_start {
        return 0.0;
    }
    let steps = ((x_end - x_start) / VOLUME_STEP_M).ceil().max(1.0);
    let dx = (x_end - x_start) / steps;
    let mut volume = 0.0;
    let mut index = 0.0;
    while index < steps {
        let x = x_start + (index + 0.5) * dx;
        let area = g.section_area(deck, x);
        let share = match clear_width(x) {
            Some(width) => {
                let usable = g.usable_width(deck, x);
                if usable > width {
                    width / usable
                } else {
                    1.0
                }
            }
            None => 1.0,
        };
        volume += area * share * dx;
        index += 1.0;
    }
    volume
}

/// The compartments of this fuselage.
///
/// `slots` are the container positions the loader built, and
/// `cabin_extent` is the longitudinal extent `(x_min, x_max)` of the seats and
/// monuments. A non-empty `declared` list of valid compartments replaces
/// every derived one; a declared compartment behind the seats keeps the
/// derived compartment's bounds (`HoldCompartment::behind_seats`).
pub fn derive_hold_compartments(
    g: &CabinGeometry,
    slots: &[CargoSlot],
    cabin_extent: Option<(f64, f64)>,
    declared: &[HoldCompartmentConfig],
) -> Vec<HoldCompartment> {
    let seat_end = cabin_extent.map(|(_, end)| end);
    let declared: Vec<HoldCompartment> = declared
        .iter()
        .filter(|config| config.is_valid())
        .filter_map(|config| {
            if config.behind_seats {
                HoldCompartment::behind_seats(g, config, seat_end)
            } else {
                Some(HoldCompartment::from_config(g, config))
            }
        })
        .filter(|compartment| compartment.volume_m3 > 0.0)
        .collect();
    let external = declared.iter().all(|hold| hold.x_start_m >= g.cabin_end_x);
    if !declared.is_empty() && !external {
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
    compartments.extend(declared);
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
                volume_declared: false,
                max_net_kg: None,
                deck: HoldDeck::Lower,
            });
        }
    }
    out
}

fn main_deck_compartments(
    g: &CabinGeometry,
    _seat_start: f64,
    seat_end: f64,
) -> Vec<HoldCompartment> {
    // One aft compartment, between the last seat or monument and the end of
    // the constant-section cabin barrel (`cabin_end_x`), where the section
    // starts to close into the tail cone. The tail cone is not stowage: it
    // narrows to nothing and, on a body with rear-mounted engines, holds the
    // engine bay, the APU and the empennage structure. The region ahead of the
    // first seat is the entrance vestibule and flight-deck door zone (the
    // exit-access space 14 CFR 25.813 keeps clear), so it is stowage only where
    // a source declares a compartment there (the ATR 72-600).
    let main = deck_spec(g, HoldDeck::Main);
    let (x_start, x_end) = (seat_end, g.cabin_end_x);
    let mut out = Vec::new();
    if x_end - x_start >= MIN_MAIN_DECK_COMPARTMENT_M {
        let volume = span_volume_m3(g, main, x_start, x_end);
        if volume > 0.0 {
            out.push(HoldCompartment {
                name: "Aft baggage".to_owned(),
                x_start_m: x_start,
                x_end_m: x_end,
                volume_m3: volume,
                volume_declared: false,
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
        volume_declared: false,
        max_net_kg: None,
        deck: HoldDeck::Lower,
    }
}
