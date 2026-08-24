// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/cargo_loader.py
// Reference: alas @ rust-port-baseline.

//! Freight: which containers exist, where they physically go in this fuselage,
//! and how the load is distributed so the aircraft trims.
//!
//! Cargo is loaded into real unit load devices at real positions rather than
//! as an abstract point mass, because where the load goes decides whether the
//! aircraft is inside its envelope with exactly the same payload. A hold filled
//! front to back and the same tonnage spread across the same positions are
//! different aeroplanes.
//!
//! # The fit check
//!
//! A position exists only where the container's rigid envelope fits the local
//! hold cross-section, in width *and* in height. A station too shallow gets no
//! position rather than a container clamped through the structure -- which is
//! why a narrowbody, whose hold cannot take a full-height LD3, degrades through
//! [`LOWER_HOLD_FALLBACKS`] to the reduced-height container and then to loose
//! bulk instead of reporting a hold it does not have.
//!
//! # References
//!
//! Container dimensions, maximum gross and tare weights and nominal internal
//! volumes follow the IATA ULD tables. The half-width containers of the
//! LD1/LD2/LD3 family share the AKE base footprint; the 3.18 m entries are the
//! double-width and pallet class; M-1 is the twenty-foot-class main-deck
//! freighter box.

mod engine;
mod manager;

pub use engine::build_cargo_layout;
pub use manager::CargoLoadManager;

/// One unit load device type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UldType {
    /// The key the configuration names it by.
    pub key: &'static str,
    /// The IATA code, which is what a load plan prints.
    pub code: &'static str,
    /// The full name.
    pub name: &'static str,
    /// Extent along x.
    pub length: f64,
    /// Extent along y.
    pub width: f64,
    /// Extent along z.
    pub height: f64,
    /// Maximum gross weight, container and contents together.
    pub max_gross_weight: f64,
    /// The empty container's own mass.
    pub tare_weight: f64,
    /// The color a deck plan draws this family in.
    pub color: &'static str,
    /// Nominal internal volume.
    pub volume_m3: f64,
}

impl UldType {
    /// The cargo this container may hold, excluding its own tare.
    pub fn max_net(&self) -> f64 {
        self.max_gross_weight - self.tare_weight
    }
}

/// The LD1 container.
const LD1: UldType = UldType {
    key: "LD1",
    code: "AKC",
    name: "LD1 Container",
    length: 1.56,
    width: 1.53,
    height: 1.63,
    max_gross_weight: 1588.0,
    tare_weight: 120.0,
    color: "#c0392b",
    volume_m3: 5.0,
};
/// The LD2 container.
const LD2: UldType = UldType {
    key: "LD2",
    code: "DPE",
    name: "LD2 Container",
    length: 1.56,
    width: 1.19,
    height: 1.63,
    max_gross_weight: 1225.0,
    tare_weight: 92.0,
    color: "#d35400",
    volume_m3: 3.5,
};
/// The LD3 container, which is what a widebody lower hold is built around.
const LD3: UldType = UldType {
    key: "LD3",
    code: "AKE",
    name: "LD3 Container",
    length: 1.56,
    width: 1.53,
    height: 1.63,
    max_gross_weight: 1588.0,
    tare_weight: 82.0,
    color: "#e74c3c",
    volume_m3: 4.5,
};
/// The reduced-height LD3 that narrowbody holds take. Supplemental to the IATA
/// table above, and included as the fit-check fallback so a narrowbody still
/// containerises its bags rather than carrying them loose.
const LD3_45: UldType = UldType {
    key: "LD3-45",
    code: "AKH",
    name: "LD3-45 Container",
    length: 1.56,
    width: 1.53,
    height: 1.14,
    max_gross_weight: 1134.0,
    tare_weight: 82.0,
    color: "#e57373",
    volume_m3: 3.6,
};
/// The LD6 double-width container.
const LD6: UldType = UldType {
    key: "LD6",
    code: "ALF",
    name: "LD6 Container",
    length: 3.18,
    width: 1.53,
    height: 1.63,
    max_gross_weight: 3175.0,
    tare_weight: 230.0,
    color: "#e67e22",
    volume_m3: 9.1,
};
/// The LD8 double-width container.
const LD8: UldType = UldType {
    key: "LD8",
    code: "DQF",
    name: "LD8 Container",
    length: 3.18,
    width: 1.53,
    height: 1.63,
    max_gross_weight: 2450.0,
    tare_weight: 127.0,
    color: "#f39c12",
    volume_m3: 7.1,
};
/// The LD11 double-width container.
const LD11: UldType = UldType {
    key: "LD11",
    code: "ALP",
    name: "LD11 Container",
    length: 3.18,
    width: 1.53,
    height: 1.63,
    max_gross_weight: 3175.0,
    tare_weight: 185.0,
    color: "#f1c40f",
    volume_m3: 7.4,
};
/// The 88-by-125-inch pallet.
const PAG: UldType = UldType {
    key: "PAG",
    code: "P1P",
    name: "LD7 Pallet (88x125)",
    length: 3.18,
    width: 2.24,
    height: 1.63,
    max_gross_weight: 4626.0,
    tare_weight: 110.0,
    color: "#2980b9",
    volume_m3: 10.5,
};
/// The 96-by-125-inch pallet, which is what a freighter main deck is loaded
/// with by default.
const PMC: UldType = UldType {
    key: "PMC",
    code: "P6P",
    name: "PMC Pallet (96x125)",
    length: 3.18,
    width: 2.44,
    height: 1.63,
    max_gross_weight: 6804.0,
    tare_weight: 120.0,
    color: "#3498db",
    volume_m3: 11.5,
};
/// The twenty-foot-class main-deck box.
const M1: UldType = UldType {
    key: "M1",
    code: "AMA",
    name: "M-1 Main-Deck Box",
    length: 6.06,
    width: 2.44,
    height: 2.44,
    max_gross_weight: 11340.0,
    tare_weight: 1000.0,
    color: "#8e44ad",
    volume_m3: 33.7,
};
/// Loose bulk, which is not a container at all: it carries no tare and is what
/// a hold falls back to when nothing rigid fits.
const BLK: UldType = UldType {
    key: "BLK",
    code: "BLK",
    name: "Bulk Cargo",
    length: 1.50,
    width: 2.00,
    height: 1.50,
    max_gross_weight: 2000.0,
    tare_weight: 0.0,
    color: "#95a5a6",
    volume_m3: 3.0,
};

/// Every container type the loader can place.
pub static ULD_DATABASE: [UldType; 11] = [LD1, LD2, LD3, LD3_45, LD6, LD8, LD11, PAG, PMC, M1, BLK];

/// The lower holds' fit-check fallback chain, tried in order when the
/// configured container's envelope fails at every station.
pub static LOWER_HOLD_FALLBACKS: [&str; 2] = ["LD3-45", "BLK"];

/// The container a key names, if the database has one.
pub fn uld(key: &str) -> Option<&'static UldType> {
    ULD_DATABASE.iter().find(|entry| entry.key == key)
}

/// The container a key names, falling back to `fallback` -- upstream's
/// `_uld(code, fallback)`, which is how an unrecognised code in a saved
/// configuration loads as the sensible default rather than as nothing.
pub fn uld_or(key: &str, fallback: &'static UldType) -> &'static UldType {
    uld(key).unwrap_or(fallback)
}

/// The main deck's default container.
pub const MAIN_DECK_DEFAULT: &UldType = &PMC;
/// The lower holds' default container.
pub const LOWER_DECK_DEFAULT: &UldType = &LD3;
/// Loose bulk, which every hold gets one position of whatever else fits.
pub const BULK: &UldType = &BLK;

/// One loading position: a container of a given type at a station on a deck,
/// and what has been put in it.
#[derive(Debug, Clone, PartialEq)]
pub struct CargoSlot {
    /// The position's identifier, which is what a load plan refers to it by.
    pub sid: String,
    /// Which deck it is on.
    pub deck: &'static str,
    /// Longitudinal centre.
    pub x: f64,
    /// Lateral centre.
    pub y: f64,
    /// The container standing here.
    pub uld: &'static UldType,
    /// Net cargo loaded, excluding the container's own tare.
    pub payload: f64,
}

/// Below this a position counts as empty, so an all-but-unloaded container is
/// not flown, drawn or weighed.
pub(crate) const MIN_LOADED_KG: f64 = 1.0;

impl CargoSlot {
    /// What this position may hold.
    pub fn max_net(&self) -> f64 {
        self.uld.max_net()
    }

    /// What it weighs as loaded, container included -- and nothing at all
    /// while it is empty, since an empty position is not carried.
    pub fn total_weight(&self) -> f64 {
        if self.payload > MIN_LOADED_KG {
            self.payload + self.uld.tare_weight
        } else {
            0.0
        }
    }
}

// A test asserts on the database defined above, so a failed unwrap or expect
// is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_database_key_resolves_to_itself() {
        for entry in &ULD_DATABASE {
            assert_eq!(uld(entry.key).map(|found| found.code), Some(entry.code));
        }
    }

    #[test]
    fn an_unrecognised_code_loads_as_the_default_rather_than_as_nothing() {
        // A saved configuration naming a container this build does not carry
        // must still produce a loadable aircraft.
        assert_eq!(uld("no-such-uld"), None);
        assert_eq!(uld_or("no-such-uld", LOWER_DECK_DEFAULT).code, "AKE");
        assert_eq!(uld_or("PMC", LOWER_DECK_DEFAULT).code, "P6P");
    }

    #[test]
    fn every_fallback_fits_a_hold_the_full_height_container_does_not() {
        // The chain exists to degrade a hold whose cross-section fails the fit
        // check, so a fallback no shorter than the container it replaces would
        // fail it too and leave the hold empty.
        let ld3 = uld("LD3").expect("LD3 is in the database");
        for key in LOWER_HOLD_FALLBACKS {
            let fallback = uld(key).expect("every fallback is in the database");
            assert!(
                fallback.height < ld3.height,
                "{key} is no shorter than an LD3"
            );
        }
        assert_eq!(
            LOWER_HOLD_FALLBACKS.last(),
            Some(&BULK.key),
            "the chain has to end in loose bulk, which needs no container to fit"
        );
    }

    #[test]
    fn bulk_carries_no_container_of_its_own() {
        assert_eq!(BULK.tare_weight, 0.0);
        assert_eq!(BULK.max_net(), BULK.max_gross_weight);
    }

    #[test]
    fn an_all_but_empty_position_weighs_nothing() {
        // A kilogram in a container would otherwise add its whole tare to the
        // payload, which on a widebody is tonnes of containers carrying air.
        let mut slot = CargoSlot {
            sid: "FWD-1-1".to_owned(),
            deck: crate::layout::LOWER,
            x: 10.0,
            y: 0.0,
            uld: LOWER_DECK_DEFAULT,
            payload: 0.5,
        };
        assert_eq!(slot.total_weight(), 0.0);
        slot.payload = 500.0;
        assert_eq!(slot.total_weight(), 500.0 + LOWER_DECK_DEFAULT.tare_weight);
    }
}
