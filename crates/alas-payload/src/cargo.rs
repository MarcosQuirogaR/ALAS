// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/cargo_loader.py

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
//! position rather than a container clamped through the structure, which is
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

mod capacity;
mod compartments;
mod engine;
mod headroom;
pub(crate) use headroom::lacks_underfloor_hold;
mod manager;
mod military_pallet;
mod slot;
mod turned;
pub use turned::{turned_pallet, MAIN_DECK_AUTO_CANDIDATES};

pub use capacity::CargoCapacity;
pub use compartments::{
    deck_spec, derive_hold_compartments, span_volume_m3, HoldCompartment,
    AFT_BULKHEAD_TAILCONE_FRACTION, BULK_STOWAGE_FACTOR, FLIGHT_DECK_BULKHEAD_M,
    MIN_MAIN_DECK_COMPARTMENT_M, STOWAGE_DENSITY_KG_M3,
};
pub(crate) use engine::DEFAULT_TARGET_PCT_MAC;
pub use engine::{build_cargo_layout, build_cargo_layout_reference_compatibility};
pub use manager::CargoMassSemantics;
pub use manager::{freight_deck_spec, CargoLoadManager};
use military_pallet::P463L;
pub use slot::CargoSlot;
pub(crate) use slot::MIN_LOADED_KG;

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
    /// Extent along y of the footprint the position is placed by, m: the base
    /// of the contour for a contoured container (its lower outboard corner is
    /// cut), the full width for a pallet. The loader counts and spaces
    /// positions by this width.
    pub width: f64,
    /// Widest lateral extent of the contour, m, at the top of a contoured
    /// container: 79 in for an LD3 whose base is 60.4 in. Equals `width` for a
    /// straight-sided position. The drawn contour and the 3D preview span it.
    pub top_width: f64,
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
/// `y = +/-1` denotes the profile's lateral bounding sides and `z = 0..1`
/// runs from base to top. A contoured lower outboard corner can therefore sit
/// inside `y = +/-1` at the base while still reaching the full side at height.
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
    /// Dimensions (top width, base width, depth, height) printed by a
    /// manufacturer document, with the chamfer printed or taken at 45 degrees.
    /// Collision and fit use this polygon (base, chamfer, top width), placed on
    /// the footprint and mirrored chamfer-outboard on the port side; it is
    /// still not an aircraft- and position-specific certified trace.
    PublishedStandard,
    /// Approximation that must never increase solver feasibility.
    VisualizationOnly,
}

const RECTANGULAR_CONTOUR: UldContour = UldContour {
    vertices: &[[-1.0, 0.0], [1.0, 0.0], [1.0, 1.0], [-1.0, 1.0]],
    source: "ALAS legacy SI extents; conservative bounding rectangle pending aircraft WBM data",
    fidelity: ContourFidelity::ConservativeEnvelope,
    mirrorable: false,
};

// The public IATA LD3 illustration identifies the E contour by its lower
// outboard corner tapering toward the fuselage. The exact certified contour is
// aircraft- and position-specific (and normally comes from IATA ULDR or an
// operator WBM/CAD package), so these normalized silhouettes are deliberately
// visualization-only. `collision_contour` keeps using the full rectangle until
// a certified station/orientation dataset is available; a prettier profile
// must never make the loading solver accept an impossible position.
const IATA_E_CONTAINER_CONTOUR: UldContour = UldContour {
    vertices: &[
        [-1.0, 0.0],
        [0.56, 0.0],
        [1.0, 0.44],
        [1.0, 0.78],
        [1.0, 1.0],
        [0.0, 1.0],
        [-1.0, 1.0],
        [-1.0, 0.50],
    ],
    source: concat!(
        "IATA public LD3 illustration (AKE/AVE/AKN/RKN), E contour; ",
        "normalized visualization approximation, not aircraft-specific WBM/CAD",
    ),
    fidelity: ContourFidelity::VisualizationOnly,
    mirrorable: true,
};

// Public material does not expose certified transverse traces for every
// registered code in this small catalogue. Keep the confirmed lower-outboard
// taper for the remaining lower-hold container families, but label the result
// as a family fallback rather than claiming that C/F/P share E exactly.
const LOWER_HOLD_CONTAINER_CONTOUR: UldContour = UldContour {
    vertices: &[
        [-1.0, 0.0],
        [0.48, 0.0],
        [1.0, 0.52],
        [1.0, 0.76],
        [1.0, 1.0],
        [0.0, 1.0],
        [-1.0, 1.0],
        [-1.0, 0.50],
    ],
    source: concat!(
        "Public lower-hold ULD family silhouette fallback; ",
        "code-specific ULDR/WBM contour required for certification",
    ),
    fidelity: ContourFidelity::VisualizationOnly,
    mirrorable: true,
};

/// One inch, m.
const INCH_M: f64 = 0.0254;

// The three contours below are normalized to the widest extent (`top_width`),
// `y = +/-1`, and to the height. Their top width, base width, depth and height
// are printed in the manufacturer documents cited; the chamfer is the printed
// one for the LD1 and is taken at the 45 degrees the LD1 drawing prints for the
// LD3 and the LD3-45W, whose chamfer heights are not printed in the documents
// read. Fit and collision use the polygon (`PublishedStandard`).
//
// Base 60.4 in and depth 61.5 in: Airbus A340-200/-300 Aircraft
// Characteristics, Dec 01/25, section 2-6-1 page 3 (PDF p.62), "LD3 60.4 in X
// 61.5 in"; Airbus FAST 39 page 23 (PDF p.14), NAS 3610 ULD figure.

/// The LD1 (AKC): 92 in wide at the top, 60.4 in base, 64 in high, the 45
/// degree chamfer 31.5 in high on the outboard lower corner. Boeing 747-400
/// ACAP D6-58326-1 Rev F, Dec 2024, section 2.6.2 page 2-33 (PDF p.50),
/// half-width container figure: 92 in, 60.4 in, 61.5 in, 64 in, 31.5 in.
/// Base ends at 2 * 60.4 / 92 - 1, chamfer height is 31.5 / 64.
const LD1_CONTOUR: UldContour = UldContour {
    vertices: &[
        [-1.0, 0.0],
        [0.313, 0.0],
        [1.0, 0.492],
        [1.0, 1.0],
        [0.0, 1.0],
        [-1.0, 1.0],
        [-1.0, 0.5],
    ],
    source: concat!(
        "Boeing 747-400 ACAP D6-58326-1 Rev F (Dec 2024) section 2.6.2 p.2-33: ",
        "LD1 92 in top, 60.4 in base, 64 in high, 31.5 in chamfer; ",
        "normalized visualization, not aircraft-specific WBM/CAD",
    ),
    fidelity: ContourFidelity::PublishedStandard,
    mirrorable: true,
};

/// The LD3 (AKE): 79 in wide at the top, 60.4 in base, 64 in high. Airbus FAST
/// 39 page 23 prints 79 in, 60.4 in and 61.5 in for the LD3-45, which shares
/// the LD3's plan. The chamfer height is not printed in the documents read: it
/// is taken at 45 degrees, 18.6 in = (79 - 60.4) in, which is 18.6 / 64 of the
/// height.
const LD3_CONTOUR: UldContour = UldContour {
    vertices: &[
        [-1.0, 0.0],
        [0.529, 0.0],
        [1.0, 0.291],
        [1.0, 1.0],
        [0.0, 1.0],
        [-1.0, 1.0],
        [-1.0, 0.5],
    ],
    source: concat!(
        "Airbus FAST 39 p.23 (PDF p.14) NAS 3610 ULD figure: 79 in top, 60.4 in base, ",
        "61.5 in deep; chamfer height at 45 degrees as the Boeing 747-400 ACAP LD1 ",
        "drawing, not printed for the LD3; normalized visualization, not aircraft-specific WBM/CAD",
    ),
    fidelity: ContourFidelity::PublishedStandard,
    mirrorable: true,
};

/// The LD3-45W (AKH, NAS 3610 2K2C): 96 in at the top, 60.4 in base, 45 in
/// high, both lower corners cut. Airbus FAST 39 page 23 (PDF p.14): "LD3-45W
/// 96in", "60.4in", "61.5in", "max. 45in", volume 3.6 m3 (127 ft3). The chamfer
/// height is not printed: 45 degrees on each side, (96 - 60.4) / 2 = 17.8 in,
/// which is 17.8 / 45 of the height.
const LD3_45W_CONTOUR: UldContour = UldContour {
    vertices: &[
        [-0.629, 0.0],
        [0.629, 0.0],
        [1.0, 0.396],
        [1.0, 1.0],
        [0.0, 1.0],
        [-1.0, 1.0],
        [-1.0, 0.396],
    ],
    source: concat!(
        "Airbus FAST 39 p.23 (PDF p.14) NAS 3610 ULD figure, LD3-45W (AKH, 2K2C): ",
        "96 in top, 60.4 in base, 61.5 in deep, 45 in high, 3.6 m3; chamfer height ",
        "at 45 degrees, not printed; normalized visualization, not aircraft-specific WBM/CAD",
    ),
    fidelity: ContourFidelity::PublishedStandard,
    mirrorable: false,
};

const NAS3610_PALLET_CONTOUR: UldContour = UldContour {
    vertices: &[
        [-1.0, 0.0],
        [0.60, 0.0],
        [0.84, 0.18],
        [0.97, 0.42],
        [1.0, 0.70],
        [1.0, 1.0],
        [0.0, 1.0],
        [-1.0, 1.0],
        [-1.0, 0.48],
        [-1.0, 0.20],
    ],
    source: concat!(
        "Airbus FAST39 NAS 3610 pallet/net family; ",
        "normalized visualization approximation, not aircraft-specific WBM/CAD",
    ),
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

    /// Where the middle of the footprint on the floor lies in the normalized
    /// contour, `[-1, 1]` across the widest extent. Zero for a type whose top
    /// is no wider than its footprint (the contour is then centred as given).
    fn footprint_centre(&self) -> f64 {
        if (self.top_width - self.width).abs() < 1e-9 {
            return 0.0;
        }
        let base: Vec<f64> = self
            .contour
            .vertices
            .iter()
            .filter(|vertex| vertex[1].abs() < 1e-12)
            .map(|vertex| vertex[0])
            .collect();
        let low = base.iter().copied().fold(f64::INFINITY, f64::min);
        let high = base.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        0.5 * (low + high)
    }

    /// Physical contour using the extents of a placed item.
    ///
    /// `y_center` is the middle of the footprint on the floor and `width` the
    /// realized footprint width: the widest extent of the contour is `width`
    /// scaled by `top_width / self.width`, and the contour is placed so that its
    /// footprint, not its widest extent, is centred on `y_center` (a container
    /// whose lower outboard corner is cut overhangs its footprint at the top).
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
        let top = width * self.top_width / self.width;
        let flip = if mirrored && self.contour.mirrorable {
            -1.0
        } else {
            1.0
        };
        let centre = flip * self.footprint_centre();
        self.contour
            .vertices
            .iter()
            .map(|&[normalized_y, normalized_z]| {
                [
                    y_center + (flip * normalized_y - centre) * top * 0.5,
                    z_bottom + normalized_z * height,
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
    // 61.5 in deep (1.562 m), kept at the 1.56 m row pitch the holds were
    // laid out with.
    length: 1.56,
    // 60.4 in base (1.534 m, kept at 1.53 m) and 92 in top (2.337 m).
    width: 1.53,
    top_width: 92.0 * INCH_M,
    height: 1.63,
    // Boeing 747-400 ACAP D6-58326-1 Rev F section 2.6.2 p.2-33: maximum gross
    // 3,500 lb (1,590 kg), tare 270 lb (123 kg), internal volume 173 cu ft (4.9 m3).
    max_gross_weight: 1590.0,
    tare_weight: 123.0,
    color: "#c0392b",
    volume_m3: 4.9,
    contour: LD1_CONTOUR,
};
/// The LD2 container. Dimensions not sourced in this revision: no printed LD2
/// drawing was found in the documents read; the box and contour are the earlier
/// approximation.
const LD2: UldType = UldType {
    key: "LD2",
    code: "DPE",
    name: "LD2 Container",
    length: 1.56,
    width: 1.19,
    top_width: 1.19,
    height: 1.63,
    max_gross_weight: 1225.0,
    tare_weight: 92.0,
    color: "#d35400",
    volume_m3: 3.5,
    contour: IATA_E_CONTAINER_CONTOUR,
};

/// The 747 full-width lower-hold container, 186 in across the top: the unit
/// Boeing 747-400 ACAP D6-58326-1 Rev F (Dec 2024) section 2.6.1 p.2-32 (PDF
/// p.49) and 2.6.2 p.2-33 (PDF p.50) label "FULL-WIDTH CONTAINER (LD2)". It is
/// not the IATA [`LD2`] (a half-width unit of the LD3 family, 1.19 m): the
/// catalogue keeps the two apart and this one is keyed `ACAP-FW186`.
/// Printed: 186 in (4.72 m) top, 125 in (3.18 m) base, 64 in (1.63 m) high, a
/// 31.5 in (0.80 m) dimension on the side, 60.4 in (1.53 m) at the end-face
/// base (taken as the depth along x), internal volume 350 cu ft (9.9 m3), tare
/// 470 lb (213 kg), maximum cargo 6,530 lb (2,965 kg), maximum gross 7,000 lb
/// (3,180 kg). The 31.5 in is read as the chamfer height on each lower corner,
/// as the same drawing reads for the LD1 contour: 31.5 / 64 = 0.492 of the
/// height; the base ends at 125 / 186 = 0.672 of the half top width. The IATA
/// type code is not printed: `XFW` is a placeholder that collides with no
/// catalogue code (unsourced). Printed gross less tare is 2,967 kg against the
/// 2,965 kg printed for cargo (rounding in the document); the gross is kept.
const ACAP_FW186_CONTOUR: UldContour = UldContour {
    vertices: &[
        [-0.672, 0.0],
        [0.672, 0.0],
        [1.0, 0.492],
        [1.0, 1.0],
        [0.0, 1.0],
        [-1.0, 1.0],
        [-1.0, 0.492],
    ],
    source: concat!(
        "Boeing 747-400 ACAP D6-58326-1 Rev F (Dec 2024) section 2.6.1 p.2-32: ",
        "full-width container, 186 in top, 125 in base, 64 in high, 31.5 in side; ",
        "normalized visualization, not aircraft-specific WBM/CAD",
    ),
    fidelity: ContourFidelity::PublishedStandard,
    mirrorable: false,
};
const ACAP_FW186: UldType = UldType {
    key: "ACAP-FW186",
    code: "XFW",
    name: "747 full-width container 186 in (ACAP LD2)",
    length: 60.4 * INCH_M,
    width: 125.0 * INCH_M,
    top_width: 186.0 * INCH_M,
    height: 1.63,
    max_gross_weight: 3180.0,
    tare_weight: 213.0,
    color: "#a04000",
    volume_m3: 9.9,
    contour: ACAP_FW186_CONTOUR,
};
/// The LD3 container, which is what a widebody lower hold is built around.
const LD3: UldType = UldType {
    key: "LD3",
    code: "AKE",
    name: "LD3 Container",
    length: 1.56,
    // 60.4 in base and 79 in top.
    width: 1.53,
    top_width: 79.0 * INCH_M,
    height: 1.63,
    max_gross_weight: 1588.0,
    tare_weight: 82.0,
    color: "#e74c3c",
    // Boeing 787 ACAP D6-58333 Rev O p.2-14: 158 cu ft (4.5 m3) each.
    volume_m3: 4.5,
    contour: LD3_CONTOUR,
};
/// The reduced-height LD3 that narrowbody holds take. Supplemental to the IATA
/// table above, and included as the fit-check fallback so a narrowbody still
/// containerises its bags rather than carrying them loose.
const LD3_45: UldType = UldType {
    key: "LD3-45",
    code: "AKH",
    name: "LD3-45W Container",
    length: 1.56,
    // 60.4 in base and 96 in top.
    width: 1.53,
    top_width: 96.0 * INCH_M,
    height: 1.14,
    // Airbus FAST 39 p.22: the A320 family's semi-automatic cargo loading
    // system limits each ULD to 2,500 lb (1,134 kg).
    max_gross_weight: 1134.0,
    tare_weight: 82.0,
    color: "#e57373",
    // Airbus FAST 39 p.23: 3.6 m3 (127 cu ft).
    volume_m3: 3.6,
    contour: LD3_45W_CONTOUR,
};
/// The LD6 double-width container. Dimensions not sourced in this revision.
const LD6: UldType = UldType {
    key: "LD6",
    code: "ALF",
    name: "LD6 Container",
    length: 3.18,
    width: 1.53,
    top_width: 1.53,
    height: 1.63,
    max_gross_weight: 3175.0,
    tare_weight: 230.0,
    color: "#e67e22",
    volume_m3: 9.1,
    contour: LOWER_HOLD_CONTAINER_CONTOUR,
};
/// The LD8 double-width container. Dimensions not sourced in this revision.
const LD8: UldType = UldType {
    key: "LD8",
    code: "DQF",
    name: "LD8 Container",
    length: 3.18,
    width: 1.53,
    top_width: 1.53,
    height: 1.63,
    max_gross_weight: 2450.0,
    tare_weight: 127.0,
    color: "#f39c12",
    volume_m3: 7.1,
    contour: LOWER_HOLD_CONTAINER_CONTOUR,
};
/// The LD11 double-width container. Dimensions not sourced in this revision.
const LD11: UldType = UldType {
    key: "LD11",
    code: "ALP",
    name: "LD11 Container",
    length: 3.18,
    width: 1.53,
    top_width: 1.53,
    height: 1.63,
    max_gross_weight: 3175.0,
    tare_weight: 185.0,
    color: "#f1c40f",
    volume_m3: 7.4,
    contour: LOWER_HOLD_CONTAINER_CONTOUR,
};
/// The 88-by-125-inch pallet: 88 in across and 125 in along the aircraft, as
/// Boeing 787 ACAP D6-58333 Rev O p.2-14 prints it (88 x 125 in, 2.24 x 3.18 m)
/// and Airbus A340 AC Dec 01/25 section 2-6-1 p.3. The built-up contour is
/// not sourced.
const PAG: UldType = UldType {
    key: "PAG",
    code: "P1P",
    name: "LD7 Pallet (88x125)",
    length: 3.18,
    width: 2.24,
    top_width: 2.24,
    height: 1.63,
    max_gross_weight: 4626.0,
    tare_weight: 110.0,
    color: "#2980b9",
    volume_m3: 10.5,
    contour: NAS3610_PALLET_CONTOUR,
};
/// The 96-by-125-inch pallet (Boeing 787 ACAP D6-58333 Rev O p.2-14: 96 x 125 in,
/// 2.44 x 3.18 m; the built-up contour is not sourced), which is what a freighter main deck is loaded
/// with by default.
const PMC: UldType = UldType {
    key: "PMC",
    code: "P6P",
    name: "PMC Pallet (96x125)",
    length: 3.18,
    width: 2.44,
    top_width: 2.44,
    height: 1.63,
    max_gross_weight: 6804.0,
    tare_weight: 120.0,
    color: "#3498db",
    volume_m3: 11.5,
    contour: NAS3610_PALLET_CONTOUR,
};
/// The 88-by-108-inch pallet: 88 in across and 108 in along the aircraft (2.24 x 2.74 m),
/// the main-deck format of the DC-10-30 freighter's 30-position arrangement (Boeing, *McDonnell
/// Douglas DC-10-30 Freighters*, TBC-PS-2038 p.8 and the DC-10 Freighters main-deck cargo
/// arrangements, TBC-CA-2066 p.5: "(30) 2.2 x 2.7-m (88 x 108-in) pallet/container"). Gross
/// weight, height and contour are the 88 x 125 in pallet's and the volume scales with its length (10.5 x 108/125); the source does not print them.
const PAP108: UldType = UldType {
    key: "P88x108",
    code: "P88x108",
    name: "Pallet (88x108)",
    length: 2.74,
    width: 2.24,
    top_width: 2.24,
    height: 1.63,
    max_gross_weight: 4626.0,
    tare_weight: 100.0,
    color: "#1f6f9f",
    volume_m3: 9.07,
    contour: NAS3610_PALLET_CONTOUR,
};
/// The twenty-foot-class main-deck box.
const M1: UldType = UldType {
    key: "M1",
    code: "AMA",
    name: "M-1 Main-Deck Box",
    length: 6.06,
    width: 2.44,
    top_width: 2.44,
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
    top_width: 2.00,
    height: 1.50,
    max_gross_weight: 2000.0,
    tare_weight: 0.0,
    color: "#95a5a6",
    volume_m3: 3.0,
    contour: BULK_BAG_CONTOUR,
};

/// Every container type the loader can place.
pub static ULD_DATABASE: [UldType; 14] = [
    LD1, LD2, ACAP_FW186, LD3, LD3_45, LD6, LD8, LD11, PAG, PMC, PAP108, M1, P463L, BLK,
];

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

/// The container a key names, falling back to `fallback`: upstream's
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

// A test asserts on the database defined above, so a failed unwrap or expect
// is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    /// The ACAP full-width unit is its own entry: the IATA LD2 stays the
    /// 1.19 m half-width unit and the two never resolve to one another.
    #[test]
    fn the_acap_full_width_unit_is_distinct_from_the_iata_ld2() {
        let acap = uld("ACAP-FW186").expect("catalogued");
        let iata = uld("LD2").expect("catalogued");
        assert_ne!(acap.code, iata.code);
        assert_eq!(
            (iata.width, iata.volume_m3, iata.tare_weight),
            (1.19, 3.5, 92.0)
        );
        assert!((acap.top_width - 4.72).abs() < 5e-3);
        assert!((acap.width - 125.0 * 0.0254).abs() < 1e-9);
        assert_eq!(
            (acap.volume_m3, acap.tare_weight, acap.max_gross_weight),
            (9.9, 213.0, 3180.0)
        );
        assert!((acap.max_net() - 2965.0).abs() < 5.0);
    }

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
    fn physical_contour_centres_the_footprint_and_spans_the_top_width() {
        let contour = LD3.physical_contour(2.0, -1.0, false);
        let xs: Vec<f64> = contour.iter().map(|point| point[0]).collect();
        let (min_x, max_x) = (
            xs.iter().copied().fold(f64::INFINITY, f64::min),
            xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        // The widest extent is the top width, on the outboard side; the
        // footprint on the floor is centred on the position.
        assert!((max_x - min_x - LD3.top_width).abs() < 1e-9);
        let base: Vec<f64> = contour
            .iter()
            .filter(|point| (point[1] + 1.0).abs() < 1e-12)
            .map(|point| point[0])
            .collect();
        let (base_low, base_high) = (
            base.iter().copied().fold(f64::INFINITY, f64::min),
            base.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        assert!((0.5 * (base_low + base_high) - 2.0).abs() < 1e-9);
        let max_z = contour
            .iter()
            .map(|point| point[1])
            .fold(f64::NEG_INFINITY, f64::max);
        assert_eq!(max_z, -1.0 + LD3.height);
        assert!(LD3.contour.vertices.len() >= 6);
        assert_eq!(LD3.contour.fidelity, ContourFidelity::PublishedStandard);
    }

    #[test]
    fn e_contour_tapers_the_lower_outboard_corner_and_mirrors_for_port() {
        let starboard = LD3.physical_contour(0.0, 0.0, false);
        let port = LD3.physical_contour(0.0, 0.0, true);
        let side_x = |ring: &[[f64; 2]], z: f64, outboard_positive: bool| {
            let values = ring
                .iter()
                .filter(|p| (p[1] - z).abs() < 1e-12)
                .map(|p| p[0]);
            if outboard_positive {
                values.fold(f64::NEG_INFINITY, f64::max)
            } else {
                values.fold(f64::INFINITY, f64::min)
            }
        };
        // Outboard is +y for starboard and -y for port: the base stops short of
        // the top on that side only.
        assert!(side_x(&starboard, 0.0, true) < side_x(&starboard, LD3.height, true));
        assert!(side_x(&port, 0.0, false) > side_x(&port, LD3.height, false));
        // The inboard side is vertical.
        assert!(
            (side_x(&starboard, 0.0, false) - side_x(&starboard, LD3.height, false)).abs() < 1e-12
        );
    }

    #[test]
    fn visualization_contours_keep_the_solver_rectangle_conservative() {
        let rendered = LD2.physical_contour(0.0, 0.0, false);
        let collision =
            headroom::collision_contour_at(&LD2, 0.0, 0.0, LD2.width, LD2.height, false);
        assert!(rendered.len() > collision.len());
        assert_eq!(
            collision,
            RECTANGULAR_CONTOUR
                .vertices
                .iter()
                .map(|&[y, z]| [y * LD2.width * 0.5, z * LD2.height])
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_published_standard_contour_is_the_collision_polygon() {
        for mirrored in [false, true] {
            let collision =
                headroom::collision_contour_at(&LD3, 1.0, 0.0, LD3.width, LD3.height, mirrored);
            assert_eq!(collision, LD3.physical_contour(1.0, 0.0, mirrored));
        }
    }

    #[test]
    fn every_catalogue_uld_exposes_a_polygonal_visual_profile() {
        for entry in ULD_DATABASE {
            // The 463L pallet is a flat rectangle: four corners are its shape.
            if entry.key != "463L" {
                assert!(
                    entry.contour.vertices.len() >= 6,
                    "{} should not regress to a four-corner render box",
                    entry.code
                );
            }
            assert_ne!(entry.contour.fidelity, ContourFidelity::Authoritative);
        }
    }

    /// The sourced containers carry the printed top and base widths, and their
    /// polygon spans exactly those.
    #[test]
    fn the_sourced_containers_carry_their_printed_dimensions() {
        // (type, top width in, base width in)
        for (entry, top, base) in [
            (&LD1, 92.0, 60.4),
            (&LD3, 79.0, 60.4),
            (&LD3_45, 96.0, 60.4),
        ] {
            assert!(
                (entry.top_width - top * 0.0254).abs() < 1e-9,
                "{}",
                entry.code
            );
            // The loader's footprint width is the base, to the 1.53 m it was
            // rounded to.
            assert!((entry.width - base * 0.0254).abs() < 5e-3, "{}", entry.code);
            let ring = entry.physical_contour(0.0, 0.0, false);
            let bottom: Vec<f64> = ring
                .iter()
                .filter(|p| p[1].abs() < 1e-12)
                .map(|p| p[0])
                .collect();
            let span = bottom.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                - bottom.iter().copied().fold(f64::INFINITY, f64::min);
            assert!(
                (span - base * 0.0254).abs() < 2e-3,
                "{} base {span:.4}",
                entry.code
            );
        }
    }

    /// The LD1 chamfer is the printed 31.5 in at 45 degrees.
    #[test]
    fn the_ld1_chamfer_is_the_printed_45_degrees() {
        let contour = LD1.physical_contour(0.0, 0.0, false);
        let (corner, top) = (contour[1], contour[2]);
        let rise = (top[1] - corner[1]) / LD1.height * 64.0;
        let run = (top[0] - corner[0]) / (LD1.top_width) * 92.0;
        assert!((rise - 31.5).abs() < 0.1, "{rise}");
        assert!((run - 31.5).abs() < 0.2, "{run}");
    }

    /// Nominal internal volume sits inside the prism the published contour
    /// encloses (depth 61.5 in), within what a container wall could take.
    #[test]
    fn the_nominal_volume_fits_inside_the_published_contour() {
        for (entry, height_in) in [(&LD1, 64.0), (&LD3, 64.0), (&LD3_45, 45.0)] {
            let ring = entry.physical_contour(0.0, 0.0, false);
            let mut area = 0.0;
            for i in 0..ring.len() {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                area += a[0] * b[1] - b[0] * a[1];
            }
            // Scale the catalogue height (rounded) to the printed one.
            let prism = 0.5 * area.abs() * (height_in * 0.0254 / entry.height) * (61.5 * 0.0254);
            assert!(
                entry.volume_m3 < prism,
                "{}: {} vs {prism}",
                entry.code,
                entry.volume_m3
            );
            assert!(
                entry.volume_m3 > 0.70 * prism,
                "{}: {} vs {prism}",
                entry.code,
                entry.volume_m3
            );
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
            realized_height_m: LOWER_DECK_DEFAULT.height,
            realized_width_m: LOWER_DECK_DEFAULT.width,
            payload: 0.5,
            contour_volume_m3: None,
            net_limit_kg: None,
        };
        assert_eq!(slot.total_weight(), 0.0);
        slot.payload = 500.0;
        assert_eq!(slot.total_weight(), 500.0 + LOWER_DECK_DEFAULT.tare_weight);
    }
}
