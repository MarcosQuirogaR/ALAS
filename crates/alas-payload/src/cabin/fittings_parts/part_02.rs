// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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
    mass_semantics: CargoMassSemantics,
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
        let priority = |slot: &crate::cargo::CargoSlot| (slot.x - seat_cg).abs();
        match mass_semantics {
            CargoMassSemantics::Net => manager.solve(hold_mass, seat_cg, &priority, true),
            CargoMassSemantics::ReferenceGross => {
                manager.solve_reference_compatibility(hold_mass, seat_cg, &priority, true)
            }
        }

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
            let height = g.clamp_height(low, xx, BULK_BLOCK_HEIGHT_M);
            let bottom = g.floor_z(low, xx);
            let width = if g.enforces_physical_envelope() {
                [bottom, bottom + height]
                    .into_iter()
                    .map(|z| g.usable_width_at_z(xx, z))
                    .fold(f64::INFINITY, f64::min)
                    * low.width_factor
            } else {
                g.usable_width(low, xx)
            };
            if g.check_rectangular_prism(xx, BULK_BLOCK_LEN_M, 0.0, width, bottom, height)
                .is_ok()
            {
                items.push(DeckItem {
                    kind: ItemKind::Bag,
                    deck: LOWER,
                    x: xx,
                    y: 0.0,
                    z: bottom + height * 0.5,
                    length: BULK_BLOCK_LEN_M,
                    width,
                    mass: leftover,
                    height,
                    label: "Bulk overflow".to_owned(),
                    meta: ItemMeta::BulkBag,
                });
                hold_used += leftover;
            }
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

