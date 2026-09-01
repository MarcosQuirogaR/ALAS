// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/cargo_loader.py (`CargoLoadManager`)
// Reference: alas @ rust-port-baseline.

//! Where the containers can stand in this fuselage, and how the load is spread
//! across them to trim the aircraft.
//!
//! # The trim loop
//!
//! Filling the highest-priority positions first gets the tonnage aboard and
//! puts the centre of gravity wherever those positions happen to be. The loop
//! then moves load from the heavy side to the light side a step at a time until
//! the balance is close enough, topping the total back up on the way -- because
//! a container that reaches its own limit stops the fill short.
//!
//! That top-up is directional, and deliberately so. Removing excess in priority
//! order can strip exactly the positions the shift step has just filled --
//! whenever the far hold is *further* from the target than the near one, which
//! is the ordinary case for a forward hold across the wing box -- so every
//! shift is undone and the loop stalls at a large error. Adding on the light
//! side and removing from the heavy side instead means the two steps pull the
//! same way.

use alas_config::CargoDeckConfig;

use super::{
    uld_or, CargoSlot, UldType, BULK, LOWER_DECK_DEFAULT, LOWER_HOLD_AUTO_CANDIDATES,
    LOWER_HOLD_FALLBACKS, MAIN_DECK_DEFAULT,
};
use crate::geometry::{CabinGeometry, DeckSpec};
use crate::layout::MAIN;
use crate::numeric::floor_div;

/// Lateral clearance between two containers standing side by side.
const SLOT_GAP_M: f64 = 0.05;
/// Numerical tolerance for a row whose dimensions close exactly.
const SLOT_FIT_TOLERANCE_M: f64 = 1e-9;
/// Longitudinal clearance between two rows in a lower hold.
const LOWER_ROW_GAP_M: f64 = 0.08;
/// The same on a main deck, where the handling system needs more room.
const MAIN_ROW_GAP_M: f64 = 0.20;
/// How far aft of the cabin start the forward hold's first row sits.
const FWD_HOLD_INSET_M: f64 = 0.8;
/// How far aft of the wing box the aft hold's first row sits.
const AFT_HOLD_INSET_M: f64 = 0.2;
/// How far aft of the cabin start a main-deck freighter's first row sits.
const MAIN_DECK_INSET_M: f64 = 1.5;
/// Where the loose bulk position sits, forward of the cabin's aft end.
const BULK_INSET_M: f64 = 0.8;
/// Containers a transverse row may hold, however wide the hold is.
const MAX_ACROSS: i64 = 3;
/// Rows a lower hold may hold, which bounds the loop rather than the geometry.
const MAX_LOWER_ROWS: usize = 40;
/// The same for a main deck.
const MAX_MAIN_ROWS: usize = 60;

/// Equally spaced slot centres for one transverse row.
fn transverse_centers(usable_width: f64, container_width: f64) -> Vec<f64> {
    let count = (((usable_width + SLOT_GAP_M + SLOT_FIT_TOLERANCE_M)
        / (container_width + SLOT_GAP_M))
        .floor() as i64)
        .clamp(0, MAX_ACROSS);
    let pitch = container_width + SLOT_GAP_M;
    (0..count)
        .map(|index| (index as f64 - (count - 1) as f64 * 0.5) * pitch)
        .collect()
}

/// Lexicographic uniform-layout comparison: net capacity, volume, lower tare,
/// then stable type code for reproducibility.
fn candidate_is_better(
    candidate: (&UldType, f64, f64, f64),
    incumbent: (&UldType, f64, f64, f64),
) -> bool {
    let (candidate_type, capacity, volume, tare) = candidate;
    let (best_type, best_capacity, best_volume, best_tare) = incumbent;
    capacity.total_cmp(&best_capacity).is_gt()
        || (capacity == best_capacity && volume.total_cmp(&best_volume).is_gt())
        || (capacity == best_capacity
            && volume == best_volume
            && tare.total_cmp(&best_tare).is_lt())
        || (capacity == best_capacity
            && volume == best_volume
            && tare == best_tare
            && candidate_type.code < best_type.code)
}

/// Total-mass error the trim loop will correct for before shifting load.
const MASS_CORRECTION_KG: f64 = 5.0;
/// Error at which the correction stops adding or removing.
const MASS_SETTLED_KG: f64 = 1.0;
/// Centre-of-gravity error the loop treats as trimmed.
const CG_SETTLED_M: f64 = 0.05;
/// Total-mass error the loop treats as loaded.
const MASS_CONVERGED_KG: f64 = 10.0;

/// Which payload role a cargo request represents at the solver boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CargoMassSemantics {
    /// The product contract: requested and converged cargo exclude ULD tare.
    Net,
    /// The frozen Python reference contract: the correction loop converges on
    /// gross loaded mass even though each slot's initial fill is net cargo.
    ReferenceGross,
}

/// Builds the loading positions from a fuselage and solves the load across
/// them.
pub struct CargoLoadManager<'g> {
    geometry: &'g CabinGeometry,
    config: CargoDeckConfig,
    /// Every position, in the order the decks were walked.
    pub slots: Vec<CargoSlot>,
    /// The container the lower holds ended up taking, after the fit check.
    pub lower_uld: &'static UldType,
}

include!("manager_parts/impl.rs");

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use alas_config::{presets, AlasConfig, CargoDeckConfig, GeometryConfig};
    use alas_geom::builder::AircraftBuilder;

    fn geometry() -> CabinGeometry {
        let builder = AircraftBuilder::new(Some(GeometryConfig::default()));
        let plane = builder.build(None, false).expect("default aircraft builds");
        CabinGeometry::new(&plane, &builder.geometry, 0.15).expect("default cabin samples")
    }

    #[test]
    fn transverse_slots_include_exactly_one_gap_between_adjacent_containers() {
        let container_width = 1.53;
        let required_width = 2.0 * container_width + SLOT_GAP_M;
        let centers = transverse_centers(required_width, container_width);

        assert_eq!(centers.len(), 2);
        assert!((centers[1] - centers[0] - container_width - SLOT_GAP_M).abs() < 1e-12);
        assert!((centers[0] + centers[1]).abs() < 1e-12);
    }

    #[test]
    fn transverse_slots_do_not_count_a_gap_outside_the_row() {
        let container_width = 1.53;
        let centers =
            transverse_centers(2.0 * container_width + SLOT_GAP_M - 1e-6, container_width);

        assert_eq!(centers, vec![0.0]);
    }

    #[test]
    fn an_explicit_lower_format_is_preserved_when_it_fits() {
        let geometry = geometry();
        let config = CargoDeckConfig {
            lower_deck_uld: "LD2".to_owned(),
            ..Default::default()
        };
        let manager = CargoLoadManager::new(&geometry, config);

        assert_eq!(manager.lower_uld.code, "DPE");
        assert!(manager.slots.iter().any(|slot| slot.uld.code == "DPE"));
    }

    #[test]
    fn auto_selects_the_highest_scoring_feasible_uniform_format() {
        let geometry = geometry();
        let config = CargoDeckConfig {
            lower_deck_uld: "AUTO".to_owned(),
            ..Default::default()
        };
        let manager = CargoLoadManager::new(&geometry, config);
        let selected = manager.lower_uld;
        let selected_slots = manager
            .slots
            .iter()
            .filter(|slot| slot.deck == geometry.lower_deck.name && slot.uld.code == selected.code)
            .count();
        let selected_capacity = selected_slots as f64 * selected.max_net();
        for key in LOWER_HOLD_AUTO_CANDIDATES {
            let candidate = super::super::uld(key).expect("auto candidate resolves");
            let mut probe = CargoLoadManager {
                geometry: &geometry,
                config: CargoDeckConfig::default(),
                slots: Vec::new(),
                lower_uld: LOWER_DECK_DEFAULT,
            };
            let slots = probe.lower_candidate_slots(candidate);
            assert!(selected_capacity >= slots.len() as f64 * candidate.max_net());
        }
    }

    #[test]
    fn a380_lower_hold_places_multiple_uld_across_the_widebody_bay() {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A380-800" }))
            .expect("the registered A380 preset loads");
        let preset = presets::get("A380-800").expect("the registered A380 preset resolves");
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("the A380 geometry builds");
        let geometry = CabinGeometry::new(
            &plane,
            &config.geometry,
            config.cabin.passenger.wall_thickness_m,
        )
        .expect("the A380 cabin frame builds");
        let manager = CargoLoadManager::new(
            &geometry,
            CargoDeckConfig {
                use_main_deck: false,
                lower_deck_uld: "LD3".to_owned(),
                ..Default::default()
            },
        );
        let mut per_station = std::collections::BTreeMap::<i64, usize>::new();
        for slot in manager
            .slots
            .iter()
            .filter(|slot| slot.deck == geometry.lower_deck.name)
        {
            *per_station
                .entry((slot.x * 1_000.0).round() as i64)
                .or_default() += 1;
        }
        let maximum_across = per_station.values().copied().max().unwrap_or(0);
        assert!(
            maximum_across >= 2,
            "A380 lower hold should carry two LD3 across, got {maximum_across}"
        );
    }

    #[test]
    fn widebody_lower_holds_use_multiple_transverse_positions() {
        for name in ["A340-300", "A380-800", "B787-9", "DC-10"] {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
                .expect("the registered widebody preset loads");
            let preset = presets::get(name).expect("the registered widebody preset resolves");
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&preset.design_vector), true)
                .expect("the widebody geometry builds");
            let geometry = CabinGeometry::new(
                &plane,
                &config.geometry,
                config.cabin.passenger.wall_thickness_m,
            )
            .expect("the widebody cabin frame builds");
            let manager = CargoLoadManager::new(
                &geometry,
                CargoDeckConfig {
                    use_main_deck: false,
                    lower_deck_uld: "LD3".to_owned(),
                    ..Default::default()
                },
            );
            let mut per_station = std::collections::BTreeMap::<i64, usize>::new();
            for slot in manager
                .slots
                .iter()
                .filter(|slot| slot.deck == geometry.lower_deck.name)
            {
                *per_station
                    .entry((slot.x * 1_000.0).round() as i64)
                    .or_default() += 1;
            }
            let maximum_across = per_station.values().copied().max().unwrap_or(0);
            assert!(
                maximum_across >= 2,
                "{name} lower hold should carry at least two LD3 across, got {maximum_across}"
            );
        }
    }
}
