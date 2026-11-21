// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Load-independent capacity, with container volume kept distinct from hold space.

use super::{derive_hold_compartments, span_volume_m3, CargoLoadManager, BULK};
use crate::layout::MAIN;

/// Capacity of the generated positions, independent of the dispatched load.
///
/// Nominal ULD internal volume is not the aircraft's published hold volume:
/// it excludes space outside containers and may describe a different loading
/// configuration. These quantities must not share a correlation metric, so
/// the enclosed hold volume is reported beside the container volume.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Default)]
pub struct CargoCapacity {
    /// Rigid ULD positions, including empty positions and excluding loose bulk.
    pub uld_positions: usize,
    /// The rigid positions on the main deck (a freighter's pallet deck).
    pub main_deck_uld_positions: usize,
    /// The rigid positions in the lower holds.
    pub lower_deck_uld_positions: usize,
    /// Loose-bulk positions, which carry no container tare.
    pub bulk_positions: usize,
    /// Lower-deck container positions the holds take in the container
    /// configuration the type is offered in: the installed positions where the
    /// holds are container-loaded, and the positions of the optional container
    /// fit (see `alas_config::AircraftPreset::optional_container_lower_uld`)
    /// where the as-delivered holds are loose-bulk and the airport-planning
    /// document prints a container option. Zero where no container fit exists.
    /// It is a capability, not an installation: `lower_deck_uld_positions`
    /// stays the as-configured count.
    pub lower_deck_container_capable_positions: usize,
    /// Sum of nominal internal ULD volumes in cubic metres.
    pub container_internal_volume_m3: f64,
    /// Sum of the rigid positions' usable volumes, m3: a pallet built up to
    /// less than its nominal height holds that much less.
    pub container_usable_volume_m3: f64,
    /// Usable cargo volume, m3. With a declared container format or main-deck pallets it is the
    /// positions' own usable volumes plus the loose bulk compartment's; with
    /// none (loose-loaded holds, main-deck baggage compartments) it is the
    /// compartments' usable volume (`HoldCompartment::usable_volume_m3`:
    /// Torenbeek, Synthesis of Subsonic Airplane Design (1982), section 3.3.2,
    /// p.84, stowage factor 0.70-0.85; a declared volume is already usable),
    /// never less than the loose positions' own.
    pub usable_volume_m3: f64,
    /// Sum of nominal, unadjusted bulk block volumes in cubic metres.
    pub bulk_nominal_volume_m3: f64,
    /// Sum of usable bulk block volumes after local headroom adjustment, cubic meters.
    pub bulk_usable_volume_m3: f64,
    /// Volume the cargo compartments enclose, m3: the declared or derived
    /// lower-hold and baggage compartments, plus the main-deck floor the
    /// pallet positions stand on. Not a ULD sum.
    pub hold_volume_m3: f64,
    /// Net mass limit of all generated positions in kilograms.
    pub net_capacity_kg: f64,
    /// Sum of the published structural net-mass limits of the compartments,
    /// kg, when every compartment declares one and nothing stands on the main
    /// deck, whose positions no declared compartment bounds; `None` otherwise.
    pub structural_limit_kg: Option<f64>,
}

impl CargoCapacity {
    /// The usable cargo volume, m3, as distinct from the enclosed
    /// [`Self::hold_volume_m3`]: see the field [`Self::usable_volume_m3`].
    pub fn usable_volume_m3(&self) -> f64 {
        self.usable_volume_m3
    }
}

impl CargoLoadManager<'_> {
    /// Describe generated position capacity without counting only loaded ULDs.
    pub fn capacity_summary(&self) -> CargoCapacity {
        self.capacity_summary_with_cabin(None)
    }

    /// [`Self::capacity_summary`] for a cabin whose seats and monuments span
    /// `cabin_extent`, which bounds the main-deck baggage compartments of a
    /// body without an underfloor hold.
    pub fn capacity_summary_with_cabin(&self, cabin_extent: Option<(f64, f64)>) -> CargoCapacity {
        let mut result = CargoCapacity {
            net_capacity_kg: self.total_capacity(),
            ..CargoCapacity::default()
        };
        let (mut main_start, mut main_end) = (f64::INFINITY, f64::NEG_INFINITY);
        let mut hold_slot_usable = 0.0;
        for slot in &self.slots {
            if slot.uld.code == BULK.code {
                result.bulk_positions += 1;
                result.bulk_nominal_volume_m3 += slot.uld.volume_m3;
                result.bulk_usable_volume_m3 += slot.usable_volume_m3();
                hold_slot_usable += slot.usable_volume_m3();
                continue;
            }
            result.uld_positions += 1;
            result.container_internal_volume_m3 += slot.uld.volume_m3;
            result.container_usable_volume_m3 += slot.usable_volume_m3();
            if slot.deck == MAIN {
                result.main_deck_uld_positions += 1;
                main_start = main_start.min(slot.x - 0.5 * slot.uld.length);
                main_end = main_end.max(slot.x + 0.5 * slot.uld.length);
            } else {
                result.lower_deck_uld_positions += 1;
                hold_slot_usable += slot.usable_volume_m3();
            }
        }
        result.lower_deck_container_capable_positions = result.lower_deck_uld_positions;
        let lower: Vec<_> = self
            .slots
            .iter()
            .filter(|slot| slot.deck != MAIN)
            .cloned()
            .collect();
        let compartments = derive_hold_compartments(
            self.geometry,
            &lower,
            cabin_extent,
            &self.config.hold_compartments,
        );
        // A freight hold (the main deck of a military transport) is its own
        // compartment: it replaces the lower holds, which it has none of.
        let freight_hold = self
            .config
            .freight_hold
            .filter(|hold| self.config.use_main_deck && hold.is_valid());
        result.hold_volume_m3 = if let Some(hold) = freight_hold {
            self.freight_deck().map_or(0.0, |deck| {
                span_volume_m3(
                    self.geometry,
                    &deck,
                    hold.floor_start_x_m,
                    hold.aft_end_x_m(),
                )
            })
        } else {
            let main_deck = self
                .geometry
                .passenger_decks
                .iter()
                .find(|deck| deck.name == MAIN);
            let main_volume = match main_deck {
                Some(deck) if main_end > main_start => {
                    span_volume_m3(self.geometry, deck, main_start, main_end)
                }
                _ => 0.0,
            };
            compartments.iter().map(|c| c.volume_m3).sum::<f64>() + main_volume
        };
        // A body whose container format is declared (an explicit lower-deck
        // format, as the registered widebodies carry from their planning
        // documents) or that carries pallets on the main deck states its
        // usable volume as the sum of those devices' internal volumes (plus
        // the loose bulk compartment): the gross compartment volume is not
        // load volume, because the space between the devices is not. A body
        // with no declared format (an automatic or loose lower deck, baggage
        // compartments on the main deck) takes the usable volume of its
        // compartments, never less than its loose positions.
        let format = self.config.lower_deck_uld.trim();
        let declared_devices = result.main_deck_uld_positions > 0
            || !(format.eq_ignore_ascii_case("auto") || format.eq_ignore_ascii_case("blk"));
        result.usable_volume_m3 = if declared_devices && result.uld_positions > 0 {
            result.container_usable_volume_m3 + result.bulk_usable_volume_m3
        } else if freight_hold.is_some() {
            hold_slot_usable
        } else {
            let compartments_usable: f64 = compartments.iter().map(|c| c.usable_volume_m3()).sum();
            (result.container_usable_volume_m3 + result.bulk_usable_volume_m3)
                .max(compartments_usable)
        };
        let main_deck_unbounded = result.main_deck_uld_positions > 0;
        result.structural_limit_kg =
            if freight_hold.is_some() || compartments.is_empty() || main_deck_unbounded {
                None
            } else {
                compartments
                    .iter()
                    .map(|c| c.max_net_kg)
                    .sum::<Option<f64>>()
            };
        result
    }
}

impl CargoLoadManager<'_> {
    /// Installed positions whose identifier starts with `prefix` (`"FWD-"`
    /// for the forward hold, `"AFT-"` for the aft hold). `bulk` selects the
    /// loose-bulk positions instead of the rigid containers.
    pub fn installed_positions(&self, prefix: &str, bulk: bool) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.sid.starts_with(prefix) && (slot.uld.code == BULK.code) == bulk)
            .count()
    }
}
