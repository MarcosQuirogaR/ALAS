// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


/// Containerise the checked bags and whatever belly freight fits with them.
///
/// Bags go into the same container grid the freighter loader builds, degrading
/// where a narrowbody hold fails the fit check, and the load is trimmed toward
/// the seat CG for layout-only calls. Product mass-analysis calls additionally
/// pass the empty-aircraft mass and CG, in which case the hold load is solved
/// to keep the complete aircraft at the empty-aircraft balance target.
///
/// The belly then carries exactly the revenue freight requested through
/// `pax.belly_cargo_kg`, subject to the physical hold capacity alongside the
/// bags: a zero request means zero revenue freight, not an automatic fill to
/// the structural limit. The [`CargoMassSemantics::ReferenceGross`] replay is
/// the one exception -- it reproduces the frozen Python fixture's behaviour of
/// auto-filling the belly to the airframe's maximum *structural* payload, and
/// must keep doing so for that historical parity target.
#[allow(clippy::too_many_arguments)] // The final optional aircraft-CG target is
                                    // kept explicit so layout-only callers and
                                    // mass-analysis callers share one payload pass.
pub(super) fn place_baggage(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    req: &DesignRequirements,
    seated: i64,
    seat_mass: f64,
    seat_cg: f64,
    mass_semantics: CargoMassSemantics,
    aircraft_cg_target: Option<(f64, f64)>,
    cargo: &CargoDeckConfig,
) -> Baggage {
    let bag_mass = seated as f64 * pax.checked_bag_mass_kg;
    let belly_explicit = pax.belly_cargo_kg.max(0.0);
    let max_struct_payload = req.max_structural_payload_kg;
    // Auto-filling to the structural cap is the frozen reference-compatibility
    // replay's behaviour, not the product contract: an ordinary baseline with
    // `belly_cargo_kg == 0` must carry zero revenue freight, no matter how much
    // structural or hold capacity remains.
    let belly_to_max = if mass_semantics == CargoMassSemantics::ReferenceGross
        && max_struct_payload > 0.0
    {
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
            ..cargo.clone()
        };
        let mut manager = CargoLoadManager::new(g, holds_only);
        hold_capacity = manager.total_capacity();
        belly_cargo = belly_cargo.min((hold_capacity - bag_mass).max(0.0));

        // The structural payload limit is a gross aircraft payload limit: it
        // includes ULD tare, not only the revenue mass placed in the bins.
        // The original auto-fill solved to `MZFW - OEW` in net mass and then
        // added every container's tare on top, which is why a 787 load could
        // exceed its published MZFW while appearing to respect the configured
        // structural payload. Re-solve a product load with a reduced belly
        // target until seats, baggage, revenue freight, ULD tare, and any
        // loose-bulk remainder close below that limit. Explicit belly freight
        // above the auto-fill remains available as a deliberate overload
        // study and is reported by the pipeline feasibility check.
        let enforce_auto_structural_cap = mass_semantics == CargoMassSemantics::Net
            && max_struct_payload.is_finite()
            && max_struct_payload > 0.0
            && belly_explicit <= belly_to_max + 1.0e-9;
        for _ in 0..8 {
            let hold_mass = (bag_mass + belly_cargo).min(hold_capacity);
            let target_cg = hold_target_cg(
                aircraft_cg_target,
                seat_mass,
                seat_cg,
                hold_mass,
            );
            solve_baggage_load(&mut manager, mass_semantics, hold_mass, target_cg);

            // ULD tare changes the moment carried by the variable hold load.
            // Recompute the target with the actual gross mass and solve once
            // more, so the complete product payload rather than only its net
            // freight is balanced.
            if let Some((oew, x_oew)) = aircraft_cg_target {
                let gross_hold_mass = manager.mass_props().0;
                let corrected_target = hold_target_cg(
                    Some((oew, x_oew)),
                    seat_mass,
                    seat_cg,
                    gross_hold_mass,
                );
                if gross_hold_mass.is_finite()
                    && gross_hold_mass > 0.0
                    && corrected_target.is_finite()
                    && (corrected_target - target_cg).abs() > 1.0e-12
                {
                    solve_baggage_load(
                        &mut manager,
                        mass_semantics,
                        hold_mass,
                        corrected_target,
                    );
                }
            }

            if !enforce_auto_structural_cap {
                break;
            }
            let loaded_net = manager
                .slots
                .iter()
                .filter(|slot| slot.payload > MIN_PLACED_MASS_KG)
                .map(|slot| slot.payload)
                .sum::<f64>();
            let loaded_gross = manager.mass_props().0;
            let loose_remainder = (bag_mass + belly_cargo - loaded_net).max(0.0);
            let total_payload = seat_mass + loaded_gross + loose_remainder;
            if !total_payload.is_finite()
                || total_payload <= max_struct_payload + 1.0e-6
                || belly_cargo <= 0.0
            {
                break;
            }
            let excess = total_payload - max_struct_payload;
            let reduced_belly = (belly_cargo - excess).max(0.0);
            if (reduced_belly - belly_cargo).abs() <= 1.0e-9 {
                break;
            }
            belly_cargo = reduced_belly;
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
                meta: if mass_semantics == CargoMassSemantics::Net && slot.uld.code == "BLK" {
                    ItemMeta::BulkBag
                } else {
                    ItemMeta::Container(ContainerMeta {
                        uld: slot.uld.code,
                        fill: if slot.max_net() > 0.0 {
                            slot.payload / slot.max_net()
                        } else {
                            1.0
                        },
                        color: slot.uld.color,
                        net: None,
                    })
                },
            });
            if mass_semantics == CargoMassSemantics::ReferenceGross || slot.uld.code != "BLK" {
                hold_ulds += 1;
            }
        }
        let (placed, _cg, _used) = manager.mass_props();
        hold_used = placed;
        let loaded_net = manager
            .slots
            .iter()
            .filter(|slot| slot.payload > MIN_PLACED_MASS_KG)
            .map(|slot| slot.payload)
            .sum::<f64>();

        // Mass past what the containers can hold is still carried: it drives
        // the payload, the fuel and the balance, so it goes in as a loose
        // block at the aft hold rather than disappearing from the total.
        // The product target is net cargo; the frozen compatibility target was
        // historically compared against gross container mass and must retain
        // that exact convention for its parity fixtures.
        let leftover = if mass_semantics == CargoMassSemantics::Net {
            (bag_mass + belly_cargo) - loaded_net
        } else {
            (bag_mass + belly_cargo) - hold_used
        };
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

/// The hold-load CG that makes seats plus hold load balance the empty
/// aircraft at `x_oew`. With no empty-aircraft data, the layout-only contract
/// deliberately falls back to the historical seat-centred target.
fn hold_target_cg(
    aircraft_cg_target: Option<(f64, f64)>,
    seat_mass: f64,
    seat_cg: f64,
    hold_mass: f64,
) -> f64 {
    let Some((oew, x_oew)) = aircraft_cg_target else {
        return seat_cg;
    };
    if !oew.is_finite() || !x_oew.is_finite() || oew <= 0.0 || hold_mass <= 0.0 {
        return seat_cg;
    }
    let result = ((oew + seat_mass + hold_mass) * x_oew
        - oew * x_oew
        - seat_mass * seat_cg)
        / hold_mass;
    if result.is_finite() {
        result
    } else {
        seat_cg
    }
}

/// Solve the checked-bag/ belly load with the product or frozen mass
/// convention selected by the caller.
fn solve_baggage_load(
    manager: &mut CargoLoadManager<'_>,
    mass_semantics: CargoMassSemantics,
    target_mass: f64,
    target_cg: f64,
) {
    let priority = |slot: &crate::cargo::CargoSlot| (slot.x - target_cg).abs();
    match mass_semantics {
        CargoMassSemantics::Net => manager.solve(target_mass, target_cg, &priority, true),
        CargoMassSemantics::ReferenceGross => manager.solve_reference_compatibility(
            target_mass,
            target_cg,
            &priority,
            true,
        ),
    }
}
