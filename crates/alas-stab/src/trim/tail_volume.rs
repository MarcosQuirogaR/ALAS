// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Horizontal and vertical tail volume coefficients: closed-form `f64`
//! arithmetic over the built geometry, no factorization anywhere in it.

use alas_geom::aircraft::airplane::Airplane;

use super::AC_CHORD_FRACTION;

/// Horizontal and vertical tail volume coefficients `(Vh, Vv)`:
/// `tail_volume_coefficients`. `Vh = Sh Lh / (S c_bar)`,
/// `Vv = Sv Lv / (S b)`, with the moment arms taken from the wings' quarter-
/// MAC aerodynamic centres. `None` for either when the airplane has fewer than
/// two/three wings. The tails are identified by position (`wings[1]`,
/// `wings[2]`), as upstream indexes them, not by name.
pub fn tail_volume_coefficients(airplane: &Airplane) -> (Option<f64>, Option<f64>) {
    tail_volume_coefficients_with_reference_mode(airplane, false)
}

/// Frozen translation/parity form of [`tail_volume_coefficients`].
pub fn tail_volume_coefficients_reference_compatibility(
    airplane: &Airplane,
) -> (Option<f64>, Option<f64>) {
    tail_volume_coefficients_with_reference_mode(airplane, true)
}

fn tail_volume_coefficients_with_reference_mode(
    airplane: &Airplane,
    reference_compatibility: bool,
) -> (Option<f64>, Option<f64>) {
    let s_ref = airplane.s_ref.max(1.0);
    let c_bar = airplane.c_ref.max(0.1);
    let b_ref = airplane.b_ref.max(1.0);
    let x_wing_ac = airplane.wings.first().map_or(f64::NAN, |wing| {
        wing.aerodynamic_center(AC_CHORD_FRACTION)[0]
    });

    let vh = if airplane.wings.len() > 1 {
        let hstab = &airplane.wings[1];
        let l_h = (hstab.aerodynamic_center(AC_CHORD_FRACTION)[0] - x_wing_ac).max(0.0);
        let area = if reference_compatibility {
            hstab.unfolded_area()
        } else {
            hstab.reference_area()
        };
        Some(area * l_h / (s_ref * c_bar))
    } else {
        None
    };

    let vv = if airplane.wings.len() > 2 {
        let vstab = &airplane.wings[2];
        let l_v = (vstab.aerodynamic_center(AC_CHORD_FRACTION)[0] - x_wing_ac).max(0.0);
        // A vertical fin's planform is the XZ surface, so projecting it onto
        // the aircraft XY reference plane would collapse its area to zero.
        // Keep its physical fin planform explicit; only the main-wing
        // denominator and lateral arm are aircraft XY reference quantities.
        Some(vstab.unfolded_area() * l_v / (s_ref * b_ref))
    } else {
        None
    };

    (vh, vv)
}
