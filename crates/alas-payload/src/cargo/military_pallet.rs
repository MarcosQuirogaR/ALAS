// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The military air-cargo pallet of a freight hold, kept out of the airliner
//! container catalogue in [`super`].

use super::{ContourFidelity, UldContour, UldType};

/// The 463L pallet is a flat rectangle, 88 x 108 in, carrying its load to a
/// rectangular envelope: its contour is that rectangle, drawn as one.
const P463L_CONTOUR: UldContour = UldContour {
    vertices: &[[-1.0, 0.0], [1.0, 0.0], [1.0, 1.0], [-1.0, 1.0]],
    source: "Jane's All the World's Aircraft 2008-2009, A400M entry p.297 and C-295 entry p.544: 463L pallet 88 x 108 in; rectangular footprint and load envelope",
    fidelity: ContourFidelity::VisualizationOnly,
    mirrorable: false,
};

/// The 463L master pallet, the military air-cargo pallet, loaded in a single
/// row down a freight hold.
///
/// Footprint 88 x 108 in (2.235 x 2.743 m): Jane's All the World's Aircraft
/// 2008-2009, A400M entry (book p.297, PDF p.341) lists "nine pallets (88 x
/// 108 in military ...)", and the 463L name appears on the same edition's
/// C-130J entry (PDF p.846), whose "seven 463L pallets plus one on ramp" in
/// a single row is the same convention (PDF p.705 states "nine 463L pallets
/// plus two on ramp in single row" for a larger airlifter).
/// The 88 in side is laid along the aircraft axis: seven pallets at that
/// pitch fill the A400M's 17.71 m floor and two more fill its 5.40 m ramp,
/// which is the "nine pallets" the manufacturer and Jane's state; the 108 in
/// side lies across the 4.00 m hold, where a second pallet cannot stand.
///
/// ENGINEERING VALUES NOT FOUND ON A SOURCE PAGE IN THE LOCAL LIBRARY: the
/// 4,536 kg (10,000 lb) rated load, the 96 in (2.44 m) loaded height and the
/// nominal volume. The rated load is not binding for any registered freight
/// hold (nine pallets carry 40.8 t, above the 37 t maximum payload the A400M
/// sources give) and no pallet tare is declared: military payload figures are
/// quoted as the load on the floor, so restraint and pallet mass are inside
/// the payload and are not added again.
pub(super) const P463L: UldType = UldType {
    key: "463L",
    code: "463",
    name: "463L Master Pallet (88x108)",
    length: 2.2352,
    width: 2.7432,
    top_width: 2.7432,
    height: 2.44,
    max_gross_weight: 4536.0,
    tare_weight: 0.0,
    color: "#556b2f",
    volume_m3: 13.6,
    contour: P463L_CONTOUR,
};

/// The same pallet turned a quarter, 88 in across and 108 in along the aircraft:
/// how a hold narrower than 108 in takes it. Not a catalogue entry of its own,
/// since nobody chooses it: [`pallet_for_hold`] does.
pub(super) static P463L_TURNED: UldType = UldType {
    length: P463L.width,
    width: P463L.length,
    top_width: P463L.length,
    ..P463L
};

/// The orientation of `uld` that stands across a hold of `clear_width_m`.
pub(super) fn pallet_for_hold(uld: &'static UldType, clear_width_m: f64) -> &'static UldType {
    if uld.key == P463L.key && clear_width_m < uld.width && clear_width_m >= P463L_TURNED.width {
        &P463L_TURNED
    } else {
        uld
    }
}
