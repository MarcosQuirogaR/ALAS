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
    bulk_height_from_clearance(geometry.deck_height(deck, x), uld.height)
}

fn bulk_height_from_clearance(clear_height_m: f64, nominal_height_m: f64) -> Option<f64> {
    if !clear_height_m.is_finite() || !nominal_height_m.is_finite() {
        return None;
    }
    let realized = clear_height_m.min(nominal_height_m);
    (realized + 1.0e-9 >= MIN_BULK_HOLD_CLEAR_HEIGHT_M).then_some(realized)
}

/// Check a cargo position against local deck and fuselage clearance.
pub(super) fn uld_fits(
    geometry: &CabinGeometry,
    deck: &DeckSpec,
    x: f64,
    y: f64,
    uld: &UldType,
) -> bool {
    let Some(height) = realized_height(geometry, deck, x, uld) else {
        return false;
    };
    if !geometry.enforces_physical_envelope() {
        return geometry.deck_height(deck, x) >= height;
    }
    let z_bottom = geometry.floor_z(deck, x);
    let half_length = uld.length * 0.5;
    let deck_clear = [x - half_length, x, x + half_length]
        .into_iter()
        .all(|sample_x| {
            z_bottom >= geometry.floor_z(deck, sample_x) - 1.0e-9
                && z_bottom + height <= geometry.ceil_z(deck, sample_x) + 1.0e-9
        });
    if !deck_clear {
        return false;
    }
    let fits_orientation = |mirrored| {
        let contour = collision_contour_at(uld, y, z_bottom, uld.width, height, mirrored);
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

/// Assumed minimum clear height for a modeled underfloor bulk position, m.
/// This is not an aircraft-specific handling or certification limit.
pub(super) const MIN_BULK_HOLD_CLEAR_HEIGHT_M: f64 = 0.9;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_clearance_boundary_is_explicit() {
        assert_eq!(bulk_height_from_clearance(0.899, 1.5), None);
        assert_eq!(bulk_height_from_clearance(0.9, 1.5), Some(0.9));
        assert_eq!(bulk_height_from_clearance(1.2, 1.5), Some(1.2));
        assert_eq!(bulk_height_from_clearance(2.0, 1.5), Some(1.5));
        assert_eq!(bulk_height_from_clearance(f64::NAN, 1.5), None);
    }
}
