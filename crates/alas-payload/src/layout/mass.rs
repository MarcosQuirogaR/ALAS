// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mass properties of a laid-out interior: which items are payload,
//! their total mass and centre of gravity, and the declared operating-empty
//! equipment that the furnishings line carries instead.

use super::{DeckItem, ItemMeta, PayloadLayout};

/// Whether `item` is declared operating-empty equipment, which the mass
/// ledger carries in the furnishings line rather than in the payload.
fn is_equipment(item: &DeckItem) -> bool {
    matches!(&item.meta, ItemMeta::Declared(meta)
        if meta.category == alas_config::MassCategory::Equipment)
}

impl PayloadLayout {
    /// The items whose mass is `total_mass`: every massive item but the
    /// declared equipment, which the furnishings line carries.
    pub fn payload_items(&self) -> impl Iterator<Item = &DeckItem> {
        self.items
            .iter()
            .filter(|item| item.mass > 0.0 && !is_equipment(item))
    }
}

/// Total mass and the two centre-of-gravity coordinates of a set of items:
/// computed as a function so a builder can obtain them
/// before it has a summary to construct the layout with.
///
/// Only items with positive mass contribute, so the monuments and exits place
/// themselves without moving the balance.
pub fn mass_properties(items: &[DeckItem]) -> (f64, f64, f64) {
    let mut mass = 0.0;
    let mut moment_x = 0.0;
    let mut moment_y = 0.0;
    for item in items {
        if item.mass > 0.0 && !is_equipment(item) {
            mass += item.mass;
            moment_x += item.mass * item.x;
            moment_y += item.mass * item.y;
        }
    }
    if mass > 0.0 {
        (mass, moment_x / mass, moment_y / mass)
    } else {
        (mass, 0.0, 0.0)
    }
}

impl DeckItem {
    /// Height of the item's mass centroid, m: a seat row's occupants sit at
    /// their own centre of gravity, not at the middle of the row's box.
    pub fn mass_z(&self) -> f64 {
        match &self.meta {
            ItemMeta::Seat(meta) => meta.cg_z,
            _ => self.z,
        }
    }
}

impl PayloadLayout {
    /// The declared items that count against the operating empty mass.
    fn equipment_items(&self) -> impl Iterator<Item = &DeckItem> {
        self.items.iter().filter(|item| {
            matches!(&item.meta, ItemMeta::Declared(meta)
                if meta.category == alas_config::MassCategory::Equipment)
        })
    }

    /// Declared operating-empty equipment (furnished zones, mounted racks), kg.
    /// It is carried by the furnishings line of the mass ledger, not by
    /// [`Self::total_mass`].
    pub fn equipment_mass(&self) -> f64 {
        self.equipment_items().map(|item| item.mass).sum()
    }

    /// The mass-weighted centre of [`Self::equipment_mass`], [x, y, z] m.
    pub fn equipment_cg(&self) -> [f64; 3] {
        let mass = self.equipment_mass();
        if mass <= 0.0 {
            return [0.0; 3];
        }
        let mut moment = [0.0; 3];
        for item in self.equipment_items() {
            moment[0] += item.mass * item.x;
            moment[1] += item.mass * item.y;
            moment[2] += item.mass * item.mass_z();
        }
        [moment[0] / mass, moment[1] / mass, moment[2] / mass]
    }

    /// Every item on one deck, in placement order.
    pub fn by_deck(&self, deck: &str) -> Vec<&DeckItem> {
        self.items.iter().filter(|it| it.deck == deck).collect()
    }

    /// The decks this layout uses, in the order items first appear on them.
    pub fn decks(&self) -> Vec<&'static str> {
        self.items.iter().fold(Vec::new(), |mut seen, item| {
            if !seen.contains(&item.deck) {
                seen.push(item.deck);
            }
            seen
        })
    }
}
