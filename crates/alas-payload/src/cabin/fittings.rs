// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/cabin_layout.py (`build_passenger_layout`, the
// monument, exit and baggage passes)
// Reference: alas @ rust-port-baseline.

//! Everything the cabin carries that is not a seat: the galley and lavatory
//! complexes, the emergency exits, and the checked baggage in the holds below.
//!
//! All three hang off what the seating pass already decided. The monuments and
//! the exits go into the bays it carved out, so an exit always lands on floor a
//! seat row left free rather than on a position computed independently that
//! would drift out of alignment with it. The baggage is trimmed toward the
//! *seating* centre of gravity, which is what airlines do with bags, so the
//! payload balance stays driven by where the passengers are.

use alas_config::{CargoDeckConfig, DesignRequirements, PassengerCabinConfig};

use super::seating::Seating;
use super::{
    cabin_deck_segments, ceil_div, min_exit_pairs, monument_fill_order, select_exit_type,
    spread_bay_indices, stack_y, Bay, MonumentSide, MONUMENT_LEN, SEAT_BOX_H,
};
use crate::cargo::CargoLoadManager;
use crate::geometry::CabinGeometry;
use crate::layout::{ContainerMeta, DeckItem, ExitMeta, ItemKind, ItemMeta, LOWER};

/// Lateral footprint of a galley bay.
const GALLEY_WIDTH_M: f64 = 0.85;
/// Lateral footprint of a lavatory bay.
const LAV_WIDTH_M: f64 = 0.90;
/// Passengers per lavatory at the standard provisioning ratio.
const PAX_PER_LAV: i64 = 45;
/// Passengers per galley at the same ratio, before the extra one every cabin
/// carries whatever its size.
const PAX_PER_GALLEY: i64 = 100;

/// Longitudinal extent of the loose bulk block the overflow guard places.
const BULK_BLOCK_LEN_M: f64 = 2.0;
/// Its height, which is a lower hold's rather than a container's.
const BULK_BLOCK_HEIGHT_M: f64 = 1.4;
/// How far forward of the cabin's aft end that block sits.
const BULK_BLOCK_INSET_M: f64 = 1.5;
/// Below this the loader treats a position as unloaded, so nothing is drawn or
/// weighed for a container holding a kilogram of nothing.
const MIN_PLACED_MASS_KG: f64 = 1.0;

/// How many monuments a cabin of this size carries.
pub(super) struct MonumentCounts {
    /// Galleys installed.
    pub galleys: i64,
    /// Lavatories installed.
    pub lavatories: i64,
}

/// Distribute the galleys and lavatories across every bay.
///
/// A bay that receives more than one stacks them laterally inward from the
/// wall as a galley complex rather than placing two things at the same
/// coordinates, and galleys and lavatories take opposite sides of the
/// centreline.
pub(super) fn place_monuments(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    bays: &mut [Bay],
    total_pax: i64,
) -> (Vec<DeckItem>, MonumentCounts) {
    let lavatories = if pax.lavatory_count != 0 {
        pax.lavatory_count
    } else {
        ceil_div(total_pax, PAX_PER_LAV).max(1)
    };
    let galleys = if pax.galley_count != 0 {
        pax.galley_count
    } else {
        (ceil_div(total_pax, PAX_PER_GALLEY) + 1).max(1)
    };

    let mut items = Vec::new();
    if bays.is_empty() {
        return (
            items,
            MonumentCounts {
                galleys,
                lavatories,
            },
        );
    }
    let fill_order = monument_fill_order(bays.len());

    for (side, count, width, label, kind) in [
        (
            MonumentSide::Galley,
            galleys,
            GALLEY_WIDTH_M,
            "Galley",
            ItemKind::Galley,
        ),
        (
            MonumentSide::Lav,
            lavatories,
            LAV_WIDTH_M,
            "Lav",
            ItemKind::Lav,
        ),
    ] {
        for i in 0..count.max(0) {
            let bay_index = fill_order[i as usize % fill_order.len()];
            let bay = &mut bays[bay_index];
            // Bays are only ever carved out of a passenger deck, so a bay
            // naming a deck the geometry does not carry cannot arise.
            let Some(deck) = g.passenger_decks.iter().find(|d| d.name == bay.deck) else {
                continue;
            };
            let bay_x = bay.x;
            let bay_deck = bay.deck;
            let (offset, drawn_width) = stack_y(bay, side, width);
            let y = match side {
                MonumentSide::Galley => offset,
                MonumentSide::Lav => -offset,
            };
            items.push(DeckItem {
                kind,
                deck: bay_deck,
                x: bay_x,
                y,
                z: g.item_z(deck, bay_x, SEAT_BOX_H),
                length: MONUMENT_LEN,
                width: drawn_width,
                mass: 0.0,
                height: g.clamp_height(deck, bay_x, SEAT_BOX_H),
                label: label.to_owned(),
                meta: ItemMeta::None,
            });
        }
    }
    (
        items,
        MonumentCounts {
            galleys,
            lavatories,
        },
    )
}

/// The emergency exits, and how many pairs were installed.
pub(super) struct Exits {
    /// The door cutouts, in placement order.
    pub items: Vec<DeckItem>,
    /// The type every door was drawn at.
    pub exit_type: &'static str,
    /// Pairs installed, summed over the passenger decks.
    pub pairs: i64,
    /// What one door of that type is rated for, per side.
    pub capacity_per_side: i64,
}

/// Size and place one exit-pair set per passenger deck.
///
/// A double-decker's upper deck needs its own evacuation route, so this runs
/// per deck rather than once for the aircraft, and each deck is sized from the
/// passengers actually seated on it after the capacity cap -- never from the
/// raw requested total, which is what a deck that could not seat them all would
/// otherwise be given doors for.
pub(super) fn place_exits(g: &CabinGeometry, seating: &Seating) -> Exits {
    let spec = select_exit_type(g.diameter_m);
    let mut items = Vec::new();
    let mut pairs = 0i64;

    for segment in cabin_deck_segments(g) {
        let deck = segment.deck;
        let deck_pax = seating.on_deck(deck.name);
        if deck_pax <= 0 {
            continue;
        }
        let n_pairs = min_exit_pairs(deck_pax).max(ceil_div(deck_pax, spec.capacity_per_side));

        let mut deck_bays: Vec<&Bay> = seating
            .bays
            .iter()
            .filter(|bay| bay.deck == deck.name)
            .collect();
        deck_bays.sort_by(|a, b| a.x.total_cmp(&b.x));
        let mut exit_xs: Vec<f64> = spread_bay_indices(n_pairs, deck_bays.len())
            .into_iter()
            .map(|i| deck_bays[i].x)
            .collect();

        // More pairs than there are bays to hang them on: the remainder is
        // spread along the whole segment instead, a metre in from each end.
        if n_pairs > exit_xs.len() as i64 {
            let extra = n_pairs - exit_xs.len() as i64;
            let ex0 = segment.x0 - MONUMENT_LEN + 1.0;
            let ex1 = segment.x1 + MONUMENT_LEN - 1.0;
            exit_xs.extend((0..extra).map(|i| ex0 + (ex1 - ex0) * (i as f64 + 0.5) / extra as f64));
        }

        for xe in exit_xs {
            let half = g.usable_width(deck, xe) / 2.0 + g.wall;
            for side in [-1.0, 1.0] {
                items.push(DeckItem {
                    kind: ItemKind::Exit,
                    deck: deck.name,
                    x: xe,
                    y: side * half,
                    z: g.item_z(deck, xe, spec.height_m),
                    length: spec.width_m,
                    width: 0.25,
                    mass: 0.0,
                    height: g.clamp_height(deck, xe, spec.height_m),
                    label: format!("Type {}", spec.name),
                    meta: ItemMeta::Exit(ExitMeta {
                        exit_type: spec.name,
                        door_w: spec.width_m,
                        door_h: spec.height_m,
                    }),
                });
            }
        }
        pairs += n_pairs;
    }

    Exits {
        items,
        exit_type: spec.name,
        pairs,
        capacity_per_side: spec.capacity_per_side,
    }
}

/// What went into the holds.
pub(super) struct Baggage {
    /// The containers and the bulk block, in placement order.
    pub items: Vec<DeckItem>,
    /// Checked baggage asked for.
    pub bag_mass: f64,
    /// Revenue freight that fitted alongside it.
    pub belly_cargo: f64,
    /// What the holds could take.
    pub hold_capacity: f64,
    /// What went into them, the overflow block included.
    pub hold_used: f64,
    /// Containers used.
    pub hold_ulds: i64,
}

/// Containerise the checked bags and whatever belly freight fits with them.
///
/// Bags go into the same container grid the freighter loader builds, degrading
/// where a narrowbody hold fails the fit check, and the load is trimmed toward
/// `seat_cg` so the payload centre of gravity follows the seating rather than
/// the hold geometry.
///
/// The belly is then filled with revenue freight up to the airframe's maximum
/// *structural* payload, on top of the passengers and their bags, so the
/// residual fuel matches the real aircraft's max-payload design point. The
/// structural cap is what makes that safe: a widebody belly holds far more
/// volumetrically than the airframe may carry, and filling to geometric
/// capacity overshoots by tens of tonnes. An explicit `belly_cargo_kg` larger
/// than the auto-fill still wins, for a deliberate overload study.
pub(super) fn place_baggage(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    req: &DesignRequirements,
    seated: i64,
    seat_mass: f64,
    seat_cg: f64,
) -> Baggage {
    let bag_mass = seated as f64 * pax.checked_bag_mass_kg;
    let belly_explicit = pax.belly_cargo_kg.max(0.0);
    let max_struct_payload = req.max_structural_payload_kg;
    let belly_to_max = if max_struct_payload > 0.0 {
        (max_struct_payload - seat_mass - bag_mass).max(0.0)
    } else {
        0.0
    };
    let mut belly_cargo = belly_explicit.max(belly_to_max);

    let mut items = Vec::new();
    let mut hold_capacity = 0.0;
    let mut hold_used = 0.0;
    let mut hold_ulds = 0i64;

    if bag_mass + belly_cargo > 0.0 {
        let holds_only = CargoDeckConfig {
            use_main_deck: false,
            ..Default::default()
        };
        let mut manager = CargoLoadManager::new(g, holds_only);
        hold_capacity = manager.total_capacity();
        belly_cargo = belly_cargo.min((hold_capacity - bag_mass).max(0.0));
        let hold_mass = (bag_mass + belly_cargo).min(hold_capacity);
        manager.solve(hold_mass, seat_cg, &|slot| (slot.x - seat_cg).abs(), true);

        let low = &g.lower_deck;
        for slot in &manager.slots {
            if slot.payload <= MIN_PLACED_MASS_KG {
                continue;
            }
            let total_weight = slot.total_weight();
            items.push(DeckItem {
                kind: ItemKind::Bag,
                deck: LOWER,
                x: slot.x,
                y: slot.y,
                z: g.item_z(low, slot.x, slot.uld.height),
                length: slot.uld.length,
                width: slot.uld.width,
                mass: total_weight,
                height: g.clamp_height(low, slot.x, slot.uld.height),
                label: format!("{} {} kg", slot.uld.code, total_weight as i64),
                meta: ItemMeta::Container(ContainerMeta {
                    uld: slot.uld.code,
                    fill: if slot.max_net() > 0.0 {
                        slot.payload / slot.max_net()
                    } else {
                        1.0
                    },
                    color: slot.uld.color,
                    net: None,
                }),
            });
            hold_ulds += 1;
        }
        let (placed, _cg, _used) = manager.mass_props();
        hold_used = placed;

        // Mass past what the containers can hold is still carried: it drives
        // the payload, the fuel and the balance, so it goes in as a loose
        // block at the aft hold rather than disappearing from the total.
        let leftover = (bag_mass + belly_cargo) - hold_used;
        if leftover > MIN_PLACED_MASS_KG {
            let xx = g.cabin_end_x - BULK_BLOCK_INSET_M;
            items.push(DeckItem {
                kind: ItemKind::Bag,
                deck: LOWER,
                x: xx,
                y: 0.0,
                z: g.item_z(low, xx, BULK_BLOCK_HEIGHT_M),
                length: BULK_BLOCK_LEN_M,
                width: g.usable_width(low, xx),
                mass: leftover,
                height: g.clamp_height(low, xx, BULK_BLOCK_HEIGHT_M),
                label: "Bulk overflow".to_owned(),
                meta: ItemMeta::BulkBag,
            });
            hold_used += leftover;
        }
    }

    Baggage {
        items,
        bag_mass,
        belly_cargo,
        hold_capacity,
        hold_used,
        hold_ulds,
    }
}
