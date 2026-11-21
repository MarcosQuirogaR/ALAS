// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Horizontal and vertical tail volume coefficients: closed-form `f64`
//! arithmetic over the built geometry, no factorization anywhere in it.

use alas_geom::aircraft::airplane::Airplane;

use super::AC_CHORD_FRACTION;

/// Area definitions: the tailplane uses its reference planform through the
/// fuselage (Raymer p. 159: "commonly measured to the aircraft centerline");
/// the fin uses its built planform above its root, so a fin seated on a
/// pod (DC-10) excludes the part below the pod top and reads lower than a
/// crown-referenced area (30.9 m2 built, 36.7 m2 to the crown, 53 m2 to the
/// fuselage axis from the preset drawing).
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
    let s_ref = airplane
        .s_ref
        .max(crate::reference_dimensions::REFERENCE_AREA_FLOOR_M2);
    let c_bar = airplane
        .c_ref
        .max(crate::reference_dimensions::REFERENCE_CHORD_FLOOR_M);
    let b_ref = airplane
        .b_ref
        .max(crate::reference_dimensions::REFERENCE_SPAN_FLOOR_M);
    // From the carried box: the plan buried in the body does not lengthen the arm.
    let x_wing_ac = airplane.wings.first().map_or(f64::NAN, |wing| {
        wing.carried_box().aerodynamic_center(AC_CHORD_FRACTION)[0]
    });

    let vh = if airplane.wings.len() > 1 {
        // Volume coefficients are defined on the theoretical tail planform, area
        // and quarter-MAC arm alike (Raymer, Aircraft Design, ch. 6 and 7).
        let hstab = airplane.wings[1].reference_planform();
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
        let vstab = airplane.wings[2].reference_planform();
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
