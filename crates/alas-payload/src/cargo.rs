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

pub use engine::{build_cargo_layout, build_cargo_layout_reference_compatibility};
pub use manager::CargoLoadManager;
pub use manager::CargoMassSemantics;

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
    /// Normalized transverse contour, scaled by this type's width and height.
    pub contour: UldContour,
}

/// A ULD cross-section in coordinates normalized to half-width and height.
///
/// `y = +/-1` denotes the base sides and `z = 0..1` runs from base to top.
/// Publicly sourced silhouettes are tagged [`ContourFidelity::VisualizationOnly`]
/// until an aircraft/operator WBM supplies a station-specific certified trace;
/// the solver then uses the full bounding rectangle conservatively.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UldContour {
    /// Polygon vertices as normalized `[y, z]` pairs.
    pub vertices: &'static [[f64; 2]],
    /// Traceable status/source for this contour definition.
    pub source: &'static str,
    /// Whether this polygon may be used for physical collision decisions.
    pub fidelity: ContourFidelity,
    /// Whether the loading system may install the reflected contour.
    pub mirrorable: bool,
}

/// Evidence level attached to a contour polygon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContourFidelity {
    /// Source-controlled certified or aircraft-approved contour.
    Authoritative,
    /// Full bounding envelope: conservative for collision checks.
    ConservativeEnvelope,
    /// Approximation that must never increase solver feasibility.
    VisualizationOnly,
}

const RECTANGULAR_CONTOUR: UldContour = UldContour {
    vertices: &[[-1.0, 0.0], [1.0, 0.0], [1.0, 1.0], [-1.0, 1.0]],
    source: "ALAS legacy SI extents; conservative bounding rectangle pending aircraft WBM data",
    fidelity: ContourFidelity::ConservativeEnvelope,
    mirrorable: false,
};

// Public ULD drawings identify the sloped/chamfered upper corners that make an
// aircraft container follow a circular fuselage. The exact certified contour
// is aircraft- and position-specific (and normally comes from IATA ULDR or an
// operator WBM/CAD package), so these normalized silhouettes are deliberately
// visualization-only. `collision_contour` keeps using the full rectangle until
// a certified station/orientation dataset is available; a prettier profile
// must never make the loading solver accept an impossible position.
const NAS3610_CONTAINER_CONTOUR: UldContour = UldContour {
    vertices: &[
        [-1.0, 0.0],
        [1.0, 0.0],
        [1.0, 0.52],
        [0.86, 0.82],
        [0.68, 1.0],
        [-0.68, 1.0],
        [-0.86, 0.82],
        [-1.0, 0.52],
    ],
    source: "Public IATA/NAS 3610-style normalized ULD silhouette; visualization approximation, not aircraft-specific WBM/CAD",
    fidelity: ContourFidelity::VisualizationOnly,
    mirrorable: true,
};

const NAS3610_PALLET_CONTOUR: UldContour = UldContour {
    vertices: &[
        [-1.0, 0.0],
        [1.0, 0.0],
        [1.0, 0.24],
        [0.94, 0.52],
        [0.84, 0.78],
        [0.72, 1.0],
        [-0.72, 1.0],
        [-0.84, 0.78],
        [-0.94, 0.52],
        [-1.0, 0.24],
    ],
    source: "Public NAS 3610-style pallet/net envelope; visualization approximation, not aircraft-specific WBM/CAD",
    fidelity: ContourFidelity::VisualizationOnly,
    mirrorable: true,
};

const MAIN_DECK_BOX_CONTOUR: UldContour = UldContour {
    vertices: &[
        [-1.0, 0.0],
        [1.0, 0.0],
        [1.0, 0.62],
        [0.94, 0.84],
        [0.84, 1.0],
        [-0.84, 1.0],
        [-0.94, 0.84],
        [-1.0, 0.62],
    ],
    source: "Public main-deck ULD envelope convention; visualization approximation, not aircraft-specific WBM/CAD",
    fidelity: ContourFidelity::VisualizationOnly,
    mirrorable: true,
};

const BULK_BAG_CONTOUR: UldContour = UldContour {
    vertices: &[
        [-1.0, 0.0],
        [1.0, 0.0],
        [1.0, 0.46],
        [0.90, 0.78],
        [0.60, 1.0],
        [-0.60, 1.0],
        [-0.90, 0.78],
        [-1.0, 0.46],
    ],
    source: "ALAS bulk-load envelope with public ULD-style crown; visualization approximation",
    fidelity: ContourFidelity::VisualizationOnly,
    mirrorable: true,
};

impl UldType {
    /// The cargo this container may hold, excluding its own tare.
    pub fn max_net(&self) -> f64 {
        self.max_gross_weight - self.tare_weight
    }

    /// Physical `(y, z)` polygon translated to an installed floor position.
    pub fn physical_contour(&self, y_center: f64, z_bottom: f64, mirrored: bool) -> Vec<[f64; 2]> {
        self.physical_contour_with_extents(y_center, z_bottom, self.width, self.height, mirrored)
    }

    /// Physical contour using the extents of a placed item.
    ///
    /// Normal ULD items use [`Self::physical_contour`]. A loose bulk block or
    /// a position clipped by a local deck envelope may carry the same profile
    /// with a different realized width/height; this method keeps that render
    /// asset aligned with the actual [`crate::layout::DeckItem`] dimensions.
    pub fn physical_contour_with_extents(
        &self,
        y_center: f64,
        z_bottom: f64,
        width: f64,
        height: f64,
        mirrored: bool,
    ) -> Vec<[f64; 2]> {
        self.contour
            .vertices
            .iter()
            .map(|&[normalized_y, normalized_z]| {
                let oriented_y = if mirrored && self.contour.mirrorable {
                    -normalized_y
                } else {
                    normalized_y
                };
                [
                    y_center + oriented_y * width * 0.5,
                    z_bottom + normalized_z * height,
                ]
            })
            .collect()
    }

    /// Solver contour; visualization-only shapes fall back to their full box.
    pub(crate) fn collision_contour(
        &self,
        y_center: f64,
        z_bottom: f64,
        mirrored: bool,
    ) -> Vec<[f64; 2]> {
        let contour = if self.contour.fidelity == ContourFidelity::VisualizationOnly {
            RECTANGULAR_CONTOUR
        } else {
            self.contour
        };
        contour
            .vertices
            .iter()
            .map(|&[normalized_y, normalized_z]| {
                let oriented_y = if mirrored && contour.mirrorable {
                    -normalized_y
                } else {
                    normalized_y
                };
                [
                    y_center + oriented_y * self.width * 0.5,
                    z_bottom + normalized_z * self.height,
                ]
            })
            .collect()
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
    contour: NAS3610_CONTAINER_CONTOUR,
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
    contour: NAS3610_CONTAINER_CONTOUR,
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
    contour: NAS3610_CONTAINER_CONTOUR,
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
    contour: NAS3610_CONTAINER_CONTOUR,
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
    contour: NAS3610_CONTAINER_CONTOUR,
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
    contour: NAS3610_CONTAINER_CONTOUR,
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
    contour: NAS3610_CONTAINER_CONTOUR,
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
    contour: NAS3610_PALLET_CONTOUR,
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
    contour: NAS3610_PALLET_CONTOUR,
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
    contour: MAIN_DECK_BOX_CONTOUR,
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
    contour: BULK_BAG_CONTOUR,
};

/// Every container type the loader can place.
pub static ULD_DATABASE: [UldType; 11] = [LD1, LD2, LD3, LD3_45, LD6, LD8, LD11, PAG, PMC, M1, BLK];

/// The lower holds' fit-check fallback chain, tried in order when the
/// configured container's envelope fails at every station.
pub static LOWER_HOLD_FALLBACKS: [&str; 2] = ["LD3-45", "BLK"];

/// Containerized lower-hold formats considered by the physical auto-selector.
/// Bulk is deliberately excluded because it is a hold region, not a ULD.
pub static LOWER_HOLD_AUTO_CANDIDATES: [&str; 8] =
    ["LD1", "LD2", "LD3", "LD3-45", "LD6", "LD8", "LD11", "PAG"];

/// The container a key names, if the database has one.
pub fn uld(key: &str) -> Option<&'static UldType> {
    ULD_DATABASE.iter().find(|entry| entry.key == key)
}

/// Resolve the standards-facing three-letter ULD type code.
pub fn uld_by_code(code: &str) -> Option<&'static UldType> {
    ULD_DATABASE.iter().find(|entry| entry.code == code)
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
    fn every_type_code_resolves_to_the_same_definition() {
        for entry in &ULD_DATABASE {
            assert_eq!(
                uld_by_code(entry.code).map(|found| found.key),
                Some(entry.key)
            );
        }
        assert_eq!(uld_by_code("not-a-type-code"), None);
    }

    #[test]
    fn physical_contour_scales_normalized_coordinates_in_si_units() {
        let contour = LD3.physical_contour(2.0, -1.0, false);
        assert_eq!(contour[0], [2.0 - LD3.width * 0.5, -1.0]);
        let max_y = contour
            .iter()
            .map(|point| point[0])
            .fold(f64::NEG_INFINITY, f64::max);
        let max_z = contour
            .iter()
            .map(|point| point[1])
            .fold(f64::NEG_INFINITY, f64::max);
        assert_eq!(max_y, 2.0 + LD3.width * 0.5);
        assert_eq!(max_z, -1.0 + LD3.height);
        assert!(LD3.contour.vertices.len() >= 6);
        assert_eq!(LD3.contour.fidelity, ContourFidelity::VisualizationOnly);
    }

    #[test]
    fn visualization_contours_keep_the_solver_rectangle_conservative() {
        let rendered = LD3.physical_contour(0.0, 0.0, false);
        let collision = LD3.collision_contour(0.0, 0.0, false);
        assert!(rendered.len() > collision.len());
        assert_eq!(
            collision,
            RECTANGULAR_CONTOUR
                .vertices
                .iter()
                .map(|&[y, z]| [y * LD3.width * 0.5, z * LD3.height])
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn every_catalogue_uld_exposes_a_polygonal_visual_profile() {
        for entry in ULD_DATABASE {
            assert!(
                entry.contour.vertices.len() >= 6,
                "{} should not regress to a four-corner render box",
                entry.code
            );
            assert_eq!(entry.contour.fidelity, ContourFidelity::VisualizationOnly);
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
