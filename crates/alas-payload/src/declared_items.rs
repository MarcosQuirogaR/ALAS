// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Declared floor zones and point masses in the payload layout.
//!
//! [`alas_config::PayloadItemsConfig`] lets a study add a furnished room or a
//! lumped mass to any cabin. This module turns each declaration into
//! [`DeckItem`]s placed in the built fuselage and folds their masses into the
//! layout's balance:
//!
//! * **payload** items (occupants of a zone, freight, a drone) add to
//!   [`PayloadLayout::total_mass`] and move the payload centre of gravity;
//! * **equipment** items (a zone's furnishing, a mounted rack) stay out of the
//!   payload and are reported through [`PayloadLayout::equipment_mass`] for
//!   the furnishings line of the mass ledger.
//!
//! Zones that take floor from the seats also shorten the seat layout through
//! [`CabinGeometry::reserved_zones`], so a presidential office replaces rows
//! of seats instead of floating over them.
//!
//! Modelling assumptions, stated here because no standard prescribes them:
//! furniture and racks are taken to be [`FURNITURE_HEIGHT_M`] tall with their
//! centre of mass at half that height above the local floor; a point mass is
//! drawn as a 0.4 m block.

use alas_config::{
    AlasConfig, FloorZoneConfig, MassCategory, PayloadItemsConfig, PointMassConfig, ZoneDeck,
};

mod balance;
pub use balance::BalanceOutcome;

use crate::geometry::{CabinGeometry, DeckSpec, ReservedZone};
use crate::layout::{
    mass_properties, DeckItem, DeclaredMeta, ItemKind, ItemMeta, PayloadLayout, LOWER, MAIN, UPPER,
};

/// Height of the furniture and racks of a zone, m (modelling assumption).
pub const FURNITURE_HEIGHT_M: f64 = 1.2;

/// Edge of the block a point mass is drawn as, m.
const POINT_MASS_DRAWN_M: f64 = 0.4;

/// Slices the zone footprint is integrated over.
const ZONE_SLICES: usize = 24;

/// The smallest seat stretch worth keeping between two reserved zones, m.
const MIN_SEAT_STRETCH_M: f64 = 2.5;

fn deck_name(deck: ZoneDeck) -> &'static str {
    match deck {
        ZoneDeck::Main => MAIN,
        ZoneDeck::Upper => UPPER,
    }
}

impl CabinGeometry {
    /// This frame with the floor of `items`' seat-displacing zones reserved.
    /// A zone on an upper deck the body does not have is ignored.
    pub fn with_reserved_zones(mut self, items: &PayloadItemsConfig) -> Self {
        self.reserved_zones = items
            .zones
            .iter()
            .filter(|zone| zone.kind.takes_seat_floor() && zone.carve_seat_floor)
            .map(|zone| ReservedZone {
                deck: deck_name(zone.deck),
                x0: zone.x_start_m,
                x1: zone.x_end_m,
            })
            .filter(|zone| {
                self.passenger_decks
                    .iter()
                    .any(|deck| deck.name == zone.deck)
            })
            .collect();
        self.reserved_zones.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        self
    }

    /// This frame with every declared main-deck baggage compartment reserved, so
    /// seats and monuments stand outside them (the ATR 72-600's forward
    /// compartment ends at 5.51 m, its aft one begins at 20.72 m). Where a
    /// source puts a monument inside a compartment (the ATR's aft lavatory) the
    /// layout stands it at the compartment's edge instead. A compartment
    /// declared behind the seats reserves nothing: it is bounded by the last
    /// seat instead (`derive_hold_compartments`).
    pub fn with_declared_compartments(
        mut self,
        holds: &[alas_config::HoldCompartmentConfig],
    ) -> Self {
        let main = holds.iter().filter(|hold| {
            hold.is_valid() && hold.deck == alas_config::HoldDeck::Main && !hold.behind_seats
        });
        for hold in main {
            self.reserved_zones.push(ReservedZone {
                deck: MAIN,
                x0: hold.x_start_m,
                x1: hold.x_end_m,
            });
        }
        self.reserved_zones.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        self
    }

    /// The reserved stretches on `deck`, merged, forward to aft.
    pub(crate) fn reserved_spans(&self, deck: &str) -> Vec<(f64, f64)> {
        let mut spans: Vec<(f64, f64)> = Vec::new();
        for zone in self.reserved_zones.iter().filter(|zone| zone.deck == deck) {
            match spans.last_mut() {
                Some(last) if zone.x0 <= last.1 => last.1 = last.1.max(zone.x1),
                _ => spans.push((zone.x0, zone.x1)),
            }
        }
        spans
    }
}

/// `[x0, x1]` with the reserved `spans` cut out, dropping the remainders
/// shorter than the smallest useful seat stretch. Unchanged without spans.
pub(crate) fn carve(x0: f64, x1: f64, spans: &[(f64, f64)]) -> Vec<(f64, f64)> {
    if spans.is_empty() {
        return vec![(x0, x1)];
    }
    let mut pieces = Vec::new();
    let mut start = x0;
    for &(a, b) in spans {
        if b <= start || a >= x1 {
            continue;
        }
        if a - start >= MIN_SEAT_STRETCH_M {
            pieces.push((start, a));
        }
        start = start.max(b);
    }
    if x1 - start >= MIN_SEAT_STRETCH_M {
        pieces.push((start, x1));
    }
    pieces
}

fn deck_spec<'a>(g: &'a CabinGeometry, name: &str) -> Option<&'a DeckSpec> {
    g.passenger_decks.iter().find(|deck| deck.name == name)
}

/// Footprint of a zone on its deck: area, centroid station, lateral width at
/// the centroid, m2 / m / m.
fn footprint(g: &CabinGeometry, deck: &DeckSpec, zone: &FloorZoneConfig) -> (f64, f64, f64) {
    let dx = zone.length_m() / ZONE_SLICES as f64;
    let (mut area, mut moment, mut width_sum) = (0.0, 0.0, 0.0);
    for i in 0..ZONE_SLICES {
        let x = zone.x_start_m + (i as f64 + 0.5) * dx;
        let usable = g.usable_width(deck, x);
        let w = zone.width_m.map_or(usable, |w| w.min(usable));
        area += w * dx;
        moment += w * dx * x;
        width_sum += w;
    }
    let x_c = if area > 0.0 {
        moment / area
    } else {
        0.5 * (zone.x_start_m + zone.x_end_m)
    };
    (area, x_c, width_sum / ZONE_SLICES as f64)
}

fn zone_items(g: &CabinGeometry, zone: &FloorZoneConfig, occupant_kg: f64) -> Vec<DeckItem> {
    let Some(deck) = deck_spec(g, deck_name(zone.deck)) else {
        return Vec::new();
    };
    let (area, x_c, width) = footprint(g, deck, zone);
    let height = g.clamp_height(deck, x_c, FURNITURE_HEIGHT_M);
    let z_c = g.item_z(deck, x_c, height);
    let furnishing = zone
        .total_mass_kg
        .unwrap_or(zone.furnishing_kg_per_m2 * area);
    let occupants = zone.occupants as f64 * occupant_kg;
    let make = |mass: f64, category: MassCategory, suffix: &str| DeckItem {
        kind: ItemKind::Zone,
        deck: deck.name,
        x: x_c,
        y: zone.y_center_m,
        z: z_c,
        length: zone.length_m(),
        width,
        mass,
        height,
        label: format!("{}{suffix}", zone.name),
        meta: ItemMeta::Declared(DeclaredMeta {
            id: zone.name.clone(),
            category,
            zone_kind: Some(zone.kind),
            releasable: false,
            drag_area_m2: None,
            inertia_kgm2: None,
        }),
    };
    let mut items = Vec::new();
    if furnishing > 0.0 {
        items.push(make(furnishing, MassCategory::Equipment, ""));
    }
    if occupants > 0.0 {
        items.push(make(occupants, MassCategory::Payload, " (occupants)"));
    }
    if items.is_empty() {
        items.push(make(0.0, MassCategory::Equipment, ""));
    }
    items
}

fn point_mass_item(g: &CabinGeometry, point: &PointMassConfig) -> DeckItem {
    // The deck it hangs from: the highest passenger deck floor beneath it,
    // else the hold deck.
    let deck = g
        .passenger_decks
        .iter()
        .filter(|deck| g.floor_z(deck, point.x_m) <= point.z_m)
        .max_by(|a, b| g.floor_z(a, point.x_m).total_cmp(&g.floor_z(b, point.x_m)))
        .map_or(LOWER, |deck| deck.name);
    DeckItem {
        kind: ItemKind::PointMass,
        deck,
        x: point.x_m,
        y: point.y_m,
        z: point.z_m,
        length: POINT_MASS_DRAWN_M,
        width: POINT_MASS_DRAWN_M,
        mass: point.mass_kg,
        height: POINT_MASS_DRAWN_M,
        label: point.name.clone(),
        meta: ItemMeta::Declared(DeclaredMeta {
            id: point.id.clone(),
            category: point.category,
            zone_kind: None,
            releasable: point.releasable,
            drag_area_m2: point.drag_area_m2,
            inertia_kgm2: point.inertia_kgm2,
        }),
    }
}

/// Append the declared zones and point masses of `config` to `layout` and
/// rebalance it. A configuration that declares nothing leaves `layout`
/// untouched, bit for bit.
pub fn append_declared_items(layout: &mut PayloadLayout, g: &CabinGeometry, config: &AlasConfig) {
    append_declared_items_balanced(layout, g, config, 0.0, 0.0);
}

/// What the payload auto-balance would do for `layout` (the layout before the
/// declared items): the declared items with the movable ones moved, and the
/// outcome. `None` when nothing is movable, the layout is locked or `oew_kg`
/// is zero. The preview a stage editor shows before the layout is rebuilt.
#[must_use]
pub fn auto_balance(
    layout: &PayloadLayout,
    g: &CabinGeometry,
    config: &AlasConfig,
    oew_kg: f64,
    x_oew_m: f64,
) -> Option<(alas_config::PayloadItemsConfig, BalanceOutcome)> {
    let occupant_kg = (config.requirements.passenger_mass_kg
        - config.cabin.passenger.checked_bag_mass_kg)
        .max(0.0);
    balance::balanced(layout, g, config, oew_kg, x_oew_m, occupant_kg)
}

/// [`append_declared_items`] with the payload auto-balance: the movable items
/// are first slid to the zero-fuel centre of gravity target
/// ([`balance`]) given the operating empty mass `oew_kg` at `x_oew_m`. A zero
/// operating empty mass (a caller with none) leaves the items where declared.
pub fn append_declared_items_balanced(
    layout: &mut PayloadLayout,
    g: &CabinGeometry,
    config: &AlasConfig,
    oew_kg: f64,
    x_oew_m: f64,
) {
    if config.cabin.payload_items.is_empty() {
        return;
    }
    let occupant_kg = (config.requirements.passenger_mass_kg
        - config.cabin.passenger.checked_bag_mass_kg)
        .max(0.0);
    let balanced = balance::balanced(layout, g, config, oew_kg, x_oew_m, occupant_kg);
    // The outcome travels with the layout: the feasibility report states a
    // saturated or unsettled balance.
    layout.balance = balanced.as_ref().map(|(_, outcome)| outcome.clone());
    if let Some((_, outcome)) = &balanced {
        if outcome.saturated || !outcome.settled {
            tracing::warn!(
                target_m = outcome.target_x_m,
                reached_m = outcome.reached_x_m,
                saturated = outcome.saturated,
                "the movable payload did not reach the centre of gravity target"
            );
        }
    }
    let declared = &balanced.map_or_else(|| config.cabin.payload_items.clone(), |(items, _)| items);
    // `passenger_mass_kg` is occupant plus checked bag (`cabin.rs`); a zone's
    // occupants carry their body and carry-on mass at the zone, and, like the
    // seated passengers, their checked bags belong in the hold (declare them
    // as belly cargo), not at the zone's station.
    for zone in &declared.zones {
        layout.items.extend(zone_items(g, zone, occupant_kg));
    }
    for point in &declared.point_masses {
        layout.items.push(point_mass_item(g, point));
    }
    let (mass, cg_x, cg_y) = mass_properties(&layout.items);
    layout.total_mass = mass;
    layout.cg_x = cg_x;
    layout.cg_y = cg_y;
}

/// One line of the payload ledger: every item of one kind and label
/// (all rows of a seat class are one line) with its share of the balance.
#[derive(Debug, Clone, PartialEq)]
pub struct LedgerLine {
    /// The item kind's name (`seat_row`, `zone`, `point_mass`, ...).
    pub kind: &'static str,
    /// The label the deck plan writes on it.
    pub label: String,
    /// `payload` (in the payload total) or `equipment` (in the furnishings line).
    pub category: &'static str,
    /// Mass, kg.
    pub mass_kg: f64,
    /// Mass-weighted station, m.
    pub x_m: f64,
    /// Mass-weighted lateral position, m.
    pub y_m: f64,
    /// Mass-weighted height, m.
    pub z_m: f64,
    /// Shift of the payload centre of gravity this line causes relative to
    /// the payload CG, `mass * (x - cg_x) / payload mass`, m (zero for
    /// equipment, which is not in the payload CG).
    pub cg_moment_arm_m: f64,
}

/// The mass ledger of a layout by item kind and label, in placement order of
/// each line's first item. Items with no mass are left out.
pub fn ledger(layout: &PayloadLayout) -> Vec<LedgerLine> {
    let mut lines: Vec<(LedgerLine, [f64; 3])> = Vec::new();
    for item in layout.items.iter().filter(|item| item.mass > 0.0) {
        let category = match &item.meta {
            ItemMeta::Declared(meta) if meta.category == MassCategory::Equipment => "equipment",
            _ => "payload",
        };
        let kind = item.kind.as_str();
        let moments = [
            item.mass * item.x,
            item.mass * item.y,
            item.mass * item.mass_z(),
        ];
        match lines.iter_mut().find(|(line, _)| {
            line.kind == kind && line.label == item.label && line.category == category
        }) {
            Some((line, sums)) => {
                line.mass_kg += item.mass;
                for axis in 0..3 {
                    sums[axis] += moments[axis];
                }
            }
            None => lines.push((
                LedgerLine {
                    kind,
                    label: item.label.clone(),
                    category,
                    mass_kg: item.mass,
                    x_m: 0.0,
                    y_m: 0.0,
                    z_m: 0.0,
                    cg_moment_arm_m: 0.0,
                },
                moments,
            )),
        }
    }
    lines
        .into_iter()
        .map(|(mut line, sums)| {
            line.x_m = sums[0] / line.mass_kg;
            line.y_m = sums[1] / line.mass_kg;
            line.z_m = sums[2] / line.mass_kg;
            if line.category == "payload" && layout.total_mass > 0.0 {
                line.cg_moment_arm_m = line.mass_kg * (line.x_m - layout.cg_x) / layout.total_mass;
            }
            line
        })
        .collect()
}

/// [`ledger`] as a plain-text table with its totals.
pub fn format_ledger(layout: &PayloadLayout) -> String {
    let mut out = String::from(
        "kind              label                         category   mass [kg]   x [m]   y [m]   z [m]  dCG [m]
",
    );
    for line in ledger(layout) {
        out.push_str(&format!(
            "{:<17} {:<29} {:<9} {:>10.1} {:>7.2} {:>7.2} {:>7.2} {:>8.3}
",
            line.kind,
            line.label,
            line.category,
            line.mass_kg,
            line.x_m,
            line.y_m,
            line.z_m,
            line.cg_moment_arm_m,
        ));
    }
    out.push_str(&format!(
        "payload {:.1} kg at x = {:.3} m, y = {:.3} m; equipment {:.1} kg at x = {:.3} m
",
        layout.total_mass,
        layout.cg_x,
        layout.cg_y,
        layout.equipment_mass().abs(),
        layout.equipment_cg()[0],
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carving_nothing_returns_the_stretch() {
        assert_eq!(carve(5.0, 40.0, &[]), vec![(5.0, 40.0)]);
    }

    #[test]
    fn a_middle_zone_splits_the_stretch_in_two() {
        let pieces = carve(5.0, 40.0, &[(15.0, 20.0)]);
        assert_eq!(pieces, vec![(5.0, 15.0), (20.0, 40.0)]);
    }

    #[test]
    fn slivers_between_zones_are_dropped() {
        let pieces = carve(5.0, 40.0, &[(5.0, 15.0), (16.0, 20.0)]);
        assert_eq!(pieces, vec![(20.0, 40.0)]);
    }

    #[test]
    fn a_zone_beyond_the_deck_changes_nothing() {
        assert_eq!(carve(5.0, 40.0, &[(50.0, 60.0)]), vec![(5.0, 40.0)]);
    }
}
