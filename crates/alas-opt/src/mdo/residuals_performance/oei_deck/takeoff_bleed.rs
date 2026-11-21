// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The customer bleed of the air-conditioning packs on the take-off rating,
//! when the configuration flies the take-off with the packs on
//! (`performance.takeoff_packs_on`).
//!
//! The packs deliver the fresh air of 14 CFR 25.831(a), 0.55 lb/min per
//! occupant, bled from the engines (system total; each operating engine feeds
//! its own pack, so the share of the core flow is the same with one engine
//! out). The deck turns it into a take-off-rating thrust loss of
//! `alas_prop::empirical_turbofan::BLEED_THRUST_LOSS_SLOPE` (Raymer Eq. 13.8)
//! times the bled share of the core flow.

use alas_config::{minimum_cabin_crew, AlasConfig};
use alas_prop::empirical_turbofan::FRESH_AIR_PER_OCCUPANT_KG_S;
use alas_prop::system::PropulsionLoads;

/// Flight crew of a two-pilot transport cockpit (14 CFR 25.1523 minimum
/// flight crew of the type-certified transports the presets carry).
const FLIGHT_CREW: usize = 2;

/// The loads the take-off rating is evaluated under: the packs' bleed when
/// the configuration flies the take-off with the packs on, none otherwise.
pub(super) fn packs_loads(config: &AlasConfig) -> PropulsionLoads {
    if !config.performance.takeoff_packs_on {
        return PropulsionLoads::default();
    }
    let seats = usize::try_from(config.requirements.num_passengers.max(0)).unwrap_or(0);
    let occupants = seats + minimum_cabin_crew(seats) + FLIGHT_CREW;
    PropulsionLoads {
        bleed_mass_flow_kg_s: occupants as f64 * FRESH_AIR_PER_OCCUPANT_KG_S,
        ..PropulsionLoads::default()
    }
}
