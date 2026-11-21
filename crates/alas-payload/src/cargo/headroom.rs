// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Loose bulk cargo conforms to the local hold clearance instead of needing
//! its own rigid nominal envelope, unlike a ULD container.
//!
//! [`BULK`]'s nominal 1.5 m height does not fit a narrowbody's shallower
//! lower-hold clearance (A320-200 about 1.21 m, A220-300 about 1.01 m under
//! this model's strict-envelope fuselage sampling), so a declared bulk-only
//! hold (`lower_deck_uld == "BLK"`, `preset_flops::declared_cargo_loading`)
//! reported zero capacity for those aircraft even though a real A320/A220
//! carries loose bulk freight. Realizing the position at
//! `min(nominal height, local clear height)` instead, down to
//! [`MIN_BULK_HOLD_CLEAR_HEIGHT_M`], fixes that while still reporting zero
//! for a fuselage with no practical hold at all (the ATR 72-600, whose
//! baggage compartment is aft of the cabin per its factsheet p. 22, not
//! underfloor: its computed clearance, about 0.78 m, is below the floor).
//! The 0.9 m cutoff is a modeling assumption used to distinguish these
//! lower-hold geometries, not a certified ground-handling limit.

use crate::geometry::{CabinGeometry, DeckSpec};

use super::{ContourFidelity, UldType, RECTANGULAR_CONTOUR};

/// True only for [`super::BULK`]: loose freight realized at the local hold
/// clearance, not a rigid container needing its own nominal height whole.
pub(super) fn conforms_to_headroom(uld: &UldType) -> bool {
    uld.code == super::BULK.code
}

/// Whether `uld` is an aircraft pallet (an IATA type code beginning with P):
/// a platform whose netted load is built up to the contour over it, not a
/// rigid shell.
pub(super) fn is_pallet(uld: &UldType) -> bool {
    uld.code.starts_with('P')
}

/// The strip along every edge of a pallet that its load may not cover, m: the
/// edge rail the net attaches to. Swiss WorldCargo ULD pallet specification,
/// P6P/PMC: usable loading area 120 by 91 in on the 125 by 96 in base, 2.5 in
/// in from each edge both ways. AirBridgeCargo ULD Specifications, Pallets and
/// Containers, prints the PMC's volume as 11.5 m3 at 64 in and 21.2 m3 at
/// 118 in: 7.07 m2 of load area, the 120 by 91 in (7.05 m2) within 0.4 %,
/// where the whole 125 by 96 in base would hold 12.6 and 23.2 m3.
const PALLET_EDGE_MARGIN_M: f64 = 2.5 * 0.0254;

/// The area a pallet's load stands on, m2: its base less the edge rail.
pub(super) fn pallet_load_area_m2(uld: &UldType) -> f64 {
    (uld.length - 2.0 * PALLET_EDGE_MARGIN_M).max(0.0)
        * (uld.width - 2.0 * PALLET_EDGE_MARGIN_M).max(0.0)
}

/// Lateral samples across a pallet for its build-up; even, so one lies on its
/// centre line.
const BUILD_SAMPLES_ACROSS: usize = 16;
/// Vertical resolution of the build-up height search, m.
const BUILD_HEIGHT_TOLERANCE_M: f64 = 0.001;

/// The mean height a pallet's load is built to over its load area
/// ([`pallet_load_area_m2`]) standing at (`x`, `y`) on `deck`, or `None`
/// where it cannot stand. The load's volume is that area times this height.
///
/// A netted pallet load follows the section over it: it is built up to the
/// lining, and no higher than the main-deck cargo door it is loaded through
/// lets pass (`door_clear_height_m`, when the source prints it), so a crown
/// that falls over the pallet's outboard edge only takes that share of its
/// volume. The pallet stands where its base lies inside the section along the
/// whole footprint and its nominal height clears over its centre line.
pub(super) fn pallet_build_height(
    geometry: &CabinGeometry,
    deck: &DeckSpec,
    x: f64,
    y: f64,
    uld: &UldType,
    door_clear_height_m: Option<f64>,
) -> Option<f64> {
    let (z_bottom, ceil) = footprint_clearance(geometry, deck, x, uld.length);
    let door = door_clear_height_m.filter(|height| height.is_finite() && *height > 0.0);
    let cap = door.map_or(ceil - z_bottom, |door| door.min(ceil - z_bottom));
    if !cap.is_finite() || cap + 1.0e-9 < uld.height {
        return None;
    }
    let (half_length, half_width) = (0.5 * uld.length, 0.5 * uld.width);
    let base = [[y - half_width, z_bottom], [y + half_width, z_bottom]];
    geometry
        .check_polygon_containment(x - half_length, x + half_length, &base)
        .ok()?;
    let load_length = (uld.length - 2.0 * PALLET_EDGE_MARGIN_M).max(0.0);
    let load_width = (uld.width - 2.0 * PALLET_EDGE_MARGIN_M).max(0.0);
    let steps = (load_length / FOOTPRINT_STEP_M).ceil().max(1.0) as usize;
    let heights: Vec<f64> = (0..=BUILD_SAMPLES_ACROSS)
        .map(|sample| {
            let across = sample as f64 / BUILD_SAMPLES_ACROSS as f64;
            let sample_y = y + load_width * (across - 0.5);
            (0..=steps)
                .map(|step| {
                    let along = step as f64 / steps as f64;
                    let sample_x = x + load_length * (along - 0.5);
                    build_height_at(geometry, sample_x, sample_y, z_bottom, cap)
                })
                .fold(cap, f64::min)
        })
        .collect();
    if heights[BUILD_SAMPLES_ACROSS / 2] + BUILD_HEIGHT_TOLERANCE_M < uld.height {
        return None;
    }
    let area: f64 = heights
        .windows(2)
        .map(|pair| 0.5 * (pair[0] + pair[1]))
        .sum();
    Some(area / BUILD_SAMPLES_ACROSS as f64)
}

/// How high a load standing at `z_bottom` reaches inside the section at
/// (`x`, `y`), up to `cap`.
fn build_height_at(geometry: &CabinGeometry, x: f64, y: f64, z_bottom: f64, cap: f64) -> f64 {
    if geometry.contains_point(x, y, z_bottom + cap) {
        return cap;
    }
    if !geometry.contains_point(x, y, z_bottom) {
        return 0.0;
    }
    let (mut low, mut high) = (0.0, cap);
    while high - low > BUILD_HEIGHT_TOLERANCE_M {
        let mid = 0.5 * (low + high);
        if geometry.contains_point(x, y, z_bottom + mid) {
            low = mid;
        } else {
            high = mid;
        }
    }
    low
}

/// The height `uld` occupies at `x`: its own nominal height for a rigid
/// container, or that clamped to the local clear headroom for conforming
/// bulk cargo, `None` below [`MIN_BULK_HOLD_CLEAR_HEIGHT_M`].
pub(super) fn realized_height(
    geometry: &CabinGeometry,
    deck: &DeckSpec,
    x: f64,
    uld: &UldType,
) -> Option<f64> {
    if !conforms_to_headroom(uld) {
        return Some(uld.height);
    }
    let clear = if geometry.enforces_physical_envelope() {
        let (floor, ceil) = footprint_clearance(geometry, deck, x, uld.length);
        ceil - floor
    } else {
        geometry.deck_height(deck, x)
    };
    bulk_height_from_clearance(clear, uld.height)
}

/// The width `uld` occupies standing on `deck` at `x`: its own nominal width
/// for a rigid container, or that clamped to the hold's floor width for
/// conforming bulk cargo, which fills the hold it is loaded into.
pub(super) fn realized_width(
    geometry: &CabinGeometry,
    deck: &DeckSpec,
    x: f64,
    uld: &UldType,
) -> f64 {
    if conforms_to_headroom(uld) && geometry.enforces_physical_envelope() {
        // Conforming bulk fills the narrowest floor width under its footprint,
        // not the width at its centre alone: where the belly rises the chord at
        // the floor narrows along the position.
        let half_length = 0.5 * uld.length;
        uld.width
            .min(geometry.usable_width_over(deck, x - half_length, x + half_length))
            .max(0.0)
    } else {
        uld.width
    }
}

/// Longitudinal spacing of the footprint clearance samples, m. Matches the
/// strict-envelope containment step; the footprint also takes the definition
/// stations inside it, as the containment check does.
const FOOTPRINT_STEP_M: f64 = 0.10;

/// The flat bay a position of `length` centred at `x` actually has: the
/// highest floor and the lowest ceiling of `deck` anywhere under its
/// footprint, `(floor_z, ceil_z)` in metres, +up.
///
/// A rigid position rests on one plane and sits under one plane, so the
/// section-by-section floor and ceiling of a lofted body (the floor rises and
/// the crown falls through the nose and tail transitions) bound it at the
/// worst station of its footprint, not at its centre. Sampling only the
/// centre accepted or refused whole rows by millimetres of floor slope.
/// Between definition stations the floor and ceiling are linear, so their
/// extremes lie on the footprint's ends and the stations inside it: a grid
/// that steps over a station where the floor peaks sets the position below
/// the floor there, and the containment check, which takes that station,
/// refuses it (an A350-900 forward-hold row).
fn footprint_clearance(
    geometry: &CabinGeometry,
    deck: &DeckSpec,
    x: f64,
    length: f64,
) -> (f64, f64) {
    let steps = (length / FOOTPRINT_STEP_M).ceil().max(1.0) as usize;
    let start = x - 0.5 * length;
    let mut floor = f64::NEG_INFINITY;
    let mut ceil = f64::INFINITY;
    let grid = (0..=steps).map(|step| start + length * step as f64 / steps as f64);
    for sample_x in grid.chain(geometry.stations_within(start, start + length)) {
        floor = floor.max(geometry.floor_z(deck, sample_x));
        ceil = ceil.min(geometry.ceil_z(deck, sample_x));
    }
    (floor, ceil)
}

/// The clear height over a footprint of `length` centred at `x` on `deck`, m:
/// the lowest ceiling over it less the highest floor under it. A flat pallet is
/// stacked to this height where it is lower than the hold's declared maximum
/// (the crown falls away from the station the declared height is given at).
pub(super) fn footprint_headroom(
    geometry: &CabinGeometry,
    deck: &DeckSpec,
    x: f64,
    length: f64,
) -> f64 {
    let (floor, ceil) = footprint_clearance(geometry, deck, x, length);
    ceil - floor
}

fn bulk_height_from_clearance(clear_height_m: f64, nominal_height_m: f64) -> Option<f64> {
    if !clear_height_m.is_finite() || !nominal_height_m.is_finite() {
        return None;
    }
    let realized = clear_height_m.min(nominal_height_m);
    (realized + BULK_CLEAR_HEIGHT_TOLERANCE_M >= MIN_BULK_HOLD_CLEAR_HEIGHT_M).then_some(realized)
}

/// Check a cargo position against local deck and fuselage clearance. `height_slack_m`
/// lets the lowest ceiling over the footprint fall that far short of the container
/// height (0 for an undeclared hold).
pub(super) fn uld_fits(
    geometry: &CabinGeometry,
    deck: &DeckSpec,
    x: f64,
    y: f64,
    uld: &UldType,
    height_slack_m: f64,
) -> bool {
    let Some(height) = realized_height(geometry, deck, x, uld) else {
        return false;
    };
    if !geometry.enforces_physical_envelope() {
        return geometry.deck_height(deck, x) >= height;
    }
    // The position rests on the highest floor under its footprint and must
    // clear the lowest ceiling over it.
    let half_length = uld.length * 0.5;
    let (z_bottom, ceil_min) = footprint_clearance(geometry, deck, x, uld.length);
    if z_bottom + height > ceil_min + height_slack_m + 1.0e-9 {
        return false;
    }
    let fits_orientation = |mirrored| {
        let contour = collision_contour_at(
            uld,
            y,
            z_bottom,
            realized_width(geometry, deck, x, uld),
            height,
            mirrored,
        );
        geometry
            .check_polygon_containment(x - half_length, x + half_length, &contour)
            .is_ok()
    };
    fits_orientation(false) || (uld.contour.mirrorable && fits_orientation(true))
}

/// [`UldType::collision_contour`] at an explicit realized `width`/`height`
/// rather than `uld`'s own nominal ones, for a conforming bulk position
/// clamped by [`conforms_to_headroom`].
pub(super) fn collision_contour_at(
    uld: &UldType,
    y_center: f64,
    z_bottom: f64,
    width: f64,
    height: f64,
    mirrored: bool,
) -> Vec<[f64; 2]> {
    if uld.contour.fidelity == ContourFidelity::PublishedStandard {
        // The published polygon on the footprint: base, chamfer and top width.
        return uld.physical_contour_with_extents(y_center, z_bottom, width, height, mirrored);
    }
    let contour = if uld.contour.fidelity == ContourFidelity::VisualizationOnly {
        RECTANGULAR_CONTOUR
    } else {
        uld.contour
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
                y_center + oriented_y * width * 0.5,
                z_bottom + normalized_z * height,
            ]
        })
        .collect()
}

/// Minimum clear height for a modeled underfloor bulk position, m: the
/// shallowest drawn hold clearance, 0.71 m (Airbus A220 ACP, forward cargo
/// compartment clearance section D-D, ceiling to the lowest side-wall point;
/// see `MIN_HOLD_HEIGHT_M`). This is not a handling or certification limit.
pub(super) const MIN_BULK_HOLD_CLEAR_HEIGHT_M: f64 = crate::geometry::MIN_HOLD_HEIGHT_M;

/// Tolerance on that minimum, m. The drawn section is 28 in (0.7112 m); the
/// 0.71 m constant already rounds it down by 1.2 mm, and a lofted section
/// built to it (the A220 hold before section A-A replaced D-D: 0.70997 m)
/// lands 3e-5 m below it. A hold built to
/// the shallowest drawn section must not lose every bulk position to loft
/// round-off, so the test accepts 1 mm under the constant (about the
/// precision of a printed 28 in dimension).
const BULK_CLEAR_HEIGHT_TOLERANCE_M: f64 = 1.0e-3;

/// Clear height below which a body on the frozen reference frame has no
/// underfloor hold, m (the legacy parity rule; the product frame decides from
/// its section).
const NO_UNDERFLOOR_HOLD_BELOW_M: f64 = 0.9;

/// Whether the fuselage has no practical under-floor hold: it has no drawn or
/// class hold polygon and the lower deck's clear height mid-cabin is below
/// [`NO_UNDERFLOOR_HOLD_BELOW_M`], so the baggage is stowed on the main deck
/// ahead of and behind the seats (the ATR 72-600's arrangement, factsheet
/// p. 22).
pub(crate) fn lacks_underfloor_hold(geometry: &CabinGeometry) -> bool {
    if geometry.enforces_physical_envelope() {
        return !geometry.underfloor_hold;
    }
    let x_mid = 0.5 * (geometry.cabin_start_x + geometry.cabin_end_x);
    geometry.deck_height(&geometry.lower_deck, x_mid) < NO_UNDERFLOOR_HOLD_BELOW_M
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_clearance_boundary_is_explicit() {
        assert_eq!(bulk_height_from_clearance(0.7099, 1.5), Some(0.7099));
        assert_eq!(bulk_height_from_clearance(0.708, 1.5), None);
        assert_eq!(bulk_height_from_clearance(0.71, 1.5), Some(0.71));
        assert_eq!(bulk_height_from_clearance(1.2, 1.5), Some(1.2));
        assert_eq!(bulk_height_from_clearance(2.0, 1.5), Some(1.5));
        assert_eq!(bulk_height_from_clearance(f64::NAN, 1.5), None);
    }
}
