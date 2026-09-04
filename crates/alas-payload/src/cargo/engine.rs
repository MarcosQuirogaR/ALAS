// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/cargo_loader.py (`build_cargo_layout`)
// Reference: alas @ rust-port-baseline.

//! The freighter layout engine: a load plan for a requested payload.
//!
//! # Solving the payload balance backwards
//!
//! What the operator asks for is where the *aircraft* should balance, but what
//! the loader can place is the payload. Given an operating-empty mass and its
//! own centre of gravity, the payload centre of gravity that puts the loaded
//! aircraft on target follows from a moment balance, and that -- not the target
//! itself -- is what the positions are trimmed to. Without an operating-empty
//! mass there is nothing to balance against, so the payload is trimmed to the
//! target directly, which is the branch every caller that only wants the
//! payload's own centre of gravity takes.

use alas_config::{CargoDeckConfig, DesignRequirements};

use super::{CargoLoadManager, CargoMassSemantics, CargoSlot, MIN_LOADED_KG};
use crate::geometry::CabinGeometry;
use crate::layout::{
    mass_properties, CargoSummary, ContainerMeta, DeckItem, ItemKind, ItemMeta, LayoutSummary,
    Mode, PayloadLayout, MAIN,
};

/// Where the load is trimmed to when the configuration asks for no particular
/// point: a conservative quarter chord, which is inside every envelope this
/// program can produce.
const DEFAULT_TARGET_PCT_MAC: f64 = 25.0;

/// Where the main-deck door goes when it is not stated, as a fraction of the
/// cabin length.
const MAIN_DOOR_FRACTION: f64 = 0.55;
/// How far aft of the cabin start the forward hold door goes when unstated.
const FWD_DOOR_INSET_M: f64 = 2.0;
/// How far forward of the cabin end the aft hold door goes when unstated.
const AFT_DOOR_INSET_M: f64 = 2.0;

/// Build the freighter load plan for a fuselage and a requested payload.
///
/// `oew` and `x_oew` are the operating-empty mass and its longitudinal centre
/// of gravity; both may be zero when only the payload's own balance is wanted.
pub fn build_cargo_layout(
    g: &CabinGeometry,
    cargo: &CargoDeckConfig,
    req: &DesignRequirements,
    oew: f64,
    x_oew: f64,
) -> PayloadLayout {
    build_cargo_layout_with_mass_semantics(g, cargo, req, oew, x_oew, CargoMassSemantics::Net)
}

/// Build a cargo layout with the frozen gross-target correction used by the
/// Python parity fixture. Product analyses should call [`build_cargo_layout`].
pub fn build_cargo_layout_reference_compatibility(
    g: &CabinGeometry,
    cargo: &CargoDeckConfig,
    req: &DesignRequirements,
    oew: f64,
    x_oew: f64,
) -> PayloadLayout {
    build_cargo_layout_with_mass_semantics(
        g,
        cargo,
        req,
        oew,
        x_oew,
        CargoMassSemantics::ReferenceGross,
    )
}

fn build_cargo_layout_with_mass_semantics(
    g: &CabinGeometry,
    cargo: &CargoDeckConfig,
    req: &DesignRequirements,
    oew: f64,
    x_oew: f64,
    mass_semantics: CargoMassSemantics,
) -> PayloadLayout {
    let mut manager = CargoLoadManager::new(g, cargo.clone());
    let payload_target = req.cargo_payload_kg;

    let target_pct = if cargo.target_cg_pct_mac > 0.0 {
        cargo.target_cg_pct_mac
    } else {
        DEFAULT_TARGET_PCT_MAC
    };
    let target_cg_aircraft = g.pct_mac_to_x(target_pct);
    let required_payload_cg = if oew > 0.0 && payload_target > 0.0 {
        ((oew + payload_target) * target_cg_aircraft - oew * x_oew) / payload_target
    } else {
        target_cg_aircraft
    };

    // Zero is not a door at the datum, it is a door the layout places.
    let cabin_len = g.cabin_end_x - g.cabin_start_x;
    let main_door = or_else(
        cargo.main_door_x_m,
        g.cabin_start_x + MAIN_DOOR_FRACTION * cabin_len,
    );
    let fwd_door = or_else(cargo.fwd_door_x_m, g.cabin_start_x + FWD_DOOR_INSET_M);
    let aft_door = or_else(cargo.aft_door_x_m, g.cabin_end_x - AFT_DOOR_INSET_M);

    let strategy = cargo.loading_strategy.clone();
    solve_cargo_load(
        &mut manager,
        &strategy,
        payload_target,
        required_payload_cg,
        main_door,
        fwd_door,
        aft_door,
        mass_semantics,
    );

    // ULD tare contributes to the payload's gross moment. Recompute the
    // required payload CG once after the net load has selected its containers,
    // so the product path balances the aircraft using the actual gross payload
    // rather than silently treating tare as if it were revenue cargo.
    if mass_semantics == CargoMassSemantics::Net && oew > 0.0 && payload_target > 0.0 {
        let gross_payload_mass = manager.mass_props().0;
        if gross_payload_mass.is_finite() && gross_payload_mass > 0.0 {
            let tare_aware_payload_cg = ((oew + gross_payload_mass) * target_cg_aircraft
                - oew * x_oew)
                / gross_payload_mass;
            if tare_aware_payload_cg.is_finite()
                && (tare_aware_payload_cg - required_payload_cg).abs() > 1.0e-12
            {
                solve_cargo_load(
                    &mut manager,
                    &strategy,
                    payload_target,
                    tare_aware_payload_cg,
                    main_door,
                    fwd_door,
                    aft_door,
                    mass_semantics,
                );
            }
        }
    }

    let mut items = Vec::new();
    let mut n_main = 0i64;
    let mut n_lower = 0i64;
    for slot in &manager.slots {
        if slot.payload <= MIN_LOADED_KG {
            continue;
        }
        let deck = if slot.deck == MAIN {
            g.passenger_decks.iter().find(|deck| deck.name == MAIN)
        } else {
            Some(&g.lower_deck)
        };
        // A main-deck position on a body with no main deck cannot arise from
        // this program's own geometry, but the container still has a height to
        // be drawn at if one ever did.
        let (z_item, h_item) = deck.map_or((slot.uld.height / 2.0, slot.uld.height), |deck| {
            (
                g.item_z(deck, slot.x, slot.uld.height),
                g.clamp_height(deck, slot.x, slot.uld.height),
            )
        });
        let total_weight = slot.total_weight();
        items.push(DeckItem {
            kind: if mass_semantics == CargoMassSemantics::Net && slot.uld.code == "BLK" {
                ItemKind::Bag
            } else {
                ItemKind::Uld
            },
            deck: slot.deck,
            x: slot.x,
            y: slot.y,
            z: z_item,
            length: slot.uld.length,
            width: slot.uld.width,
            mass: total_weight,
            height: h_item,
            label: format!("{} {}kg", slot.uld.code, total_weight as i64),
            meta: if mass_semantics == CargoMassSemantics::Net && slot.uld.code == "BLK" {
                ItemMeta::BulkBag
            } else {
                ItemMeta::Container(ContainerMeta {
                    uld: slot.uld.code,
                    fill: if slot.max_net() > 0.0 {
                        slot.payload / slot.max_net()
                    } else {
                        0.0
                    },
                    color: slot.uld.color,
                    net: Some(slot.payload),
                })
            },
        });
        if mass_semantics == CargoMassSemantics::Net && slot.uld.code == "BLK" {
            continue;
        } else if slot.deck == MAIN {
            n_main += 1;
        } else {
            n_lower += 1;
        }
    }

    let capacity = manager.total_capacity();
    let loaded_net_kg: f64 = manager
        .slots
        .iter()
        .filter(|slot| slot.payload > MIN_LOADED_KG)
        .map(|slot| slot.payload)
        .sum();
    let tare_mass_kg: f64 = manager
        .slots
        .iter()
        .filter(|slot| slot.payload > MIN_LOADED_KG)
        .map(|slot| slot.uld.tare_weight)
        .sum();
    let (total_mass, cg_x, cg_y) = mass_properties(&items);
    let summary = CargoSummary {
        payload_t: total_mass / 1000.0,
        requested_net_payload_t: payload_target / 1000.0,
        loaded_net_payload_t: loaded_net_kg / 1000.0,
        tare_mass_t: tare_mass_kg / 1000.0,
        n_ulds: n_main + n_lower,
        n_main_deck: n_main,
        n_lower_deck: n_lower,
        n_slots: manager.slots.len(),
        capacity_t: capacity / 1000.0,
        fill_pct: if capacity > 0.0 {
            100.0 * payload_target / capacity
        } else {
            0.0
        },
        volume_m3: manager.slots.iter().map(|slot| slot.uld.volume_m3).sum(),
        lower_uld: manager.lower_uld.code,
        target_cg_pct_mac: target_pct,
        achieved_cg_pct_mac: if total_mass > 0.0 {
            g.x_to_pct_mac(cg_x)
        } else {
            0.0
        },
        strategy,
    };

    PayloadLayout {
        mode: Mode::Cargo,
        items,
        total_mass,
        cg_x,
        cg_y,
        summary: LayoutSummary::Cargo(Box::new(summary)),
    }
}

/// Apply one cargo strategy with the requested product or compatibility mass
/// semantics. Keeping this dispatch in one helper ensures the tare-aware
/// second pass above follows exactly the same loading strategy as the first.
// The explicit door coordinates and mass semantics are kept separate here so
// this helper remains a direct translation of the loader's strategy inputs.
#[allow(clippy::too_many_arguments)]
fn solve_cargo_load(
    manager: &mut CargoLoadManager<'_>,
    strategy: &str,
    payload_target: f64,
    required_payload_cg: f64,
    main_door: f64,
    fwd_door: f64,
    aft_door: f64,
    mass_semantics: CargoMassSemantics,
) {
    let solve = |manager: &mut CargoLoadManager<'_>,
                 priority: &dyn Fn(&CargoSlot) -> f64,
                 fill_full: bool| match mass_semantics {
        CargoMassSemantics::Net => {
            manager.solve(payload_target, required_payload_cg, priority, fill_full)
        }
        CargoMassSemantics::ReferenceGross => manager.solve_reference_compatibility(
            payload_target,
            required_payload_cg,
            priority,
            fill_full,
        ),
    };

    match strategy {
        "door_proximity" => {
            let priority = |slot: &CargoSlot| {
                if slot.deck == MAIN {
                    (slot.x - main_door).abs()
                } else {
                    (slot.x - fwd_door).abs().min((slot.x - aft_door).abs())
                }
            };
            solve(manager, &priority, true);
        }
        // The only strategy that spreads the load rather than filling
        // positions in priority order.
        "uniform" => {
            let priority = |slot: &CargoSlot| (slot.x - required_payload_cg).abs();
            solve(manager, &priority, false);
        }
        // `target_cg` and `min_pallets` both concentrate full containers near
        // the required balance point, so they share the fall-through branch
        // any unrecognised name also takes.
        _ => {
            let priority = |slot: &CargoSlot| (slot.x - required_payload_cg).abs();
            solve(manager, &priority, true);
        }
    }
}

/// Upstream's `configured or computed`: a zero means "work it out", and any
/// other value, negative included, is a position the operator stated.
fn or_else(configured: f64, computed: f64) -> f64 {
    if configured == 0.0 {
        computed
    } else {
        configured
    }
}
