// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rudder angle-of-attack effectiveness.

use std::f64::consts::PI;

/// Rudder angle-of-attack effectiveness `tau_r` against rudder-to-fin chord
/// ratio `C_R / C_V`, the points of Sadraey Fig. 12.12 (book p. 659, "control
/// surface angle of attack effectiveness parameter") that his worked examples
/// read off the curve and print: 0.2 to 0.41 (aileron Step 8, p. 724), 0.3
/// to 0.52 (Example 12.1, pp. 692-693), 0.4 to 0.60 (Example 12.3, pp. 704-705) and
/// 0.49 to 0.664 (elevator Step 11, p. 732). Linear between points; outside
/// them the Glauert thin-airfoil shape is scaled to the nearest point.
const RUDDER_TAU_POINTS: [(f64, f64); 4] = [(0.2, 0.41), (0.3, 0.52), (0.4, 0.60), (0.49, 0.664)];

/// Glauert thin-airfoil flap effectiveness `1 - (theta - sin theta) / pi`,
/// `theta = acos(2 E - 1)`, for a flap chord ratio `E`.
fn glauert_flap_effectiveness(chord_ratio: f64) -> f64 {
    let theta = (2.0 * chord_ratio - 1.0).acos();
    1.0 - (theta - theta.sin()) / PI
}

/// Rudder effectiveness `tau` (lift per unit rudder deflection over lift per
/// unit angle of attack) from [`RUDDER_TAU_POINTS`] (Sadraey Fig. 12.12),
/// linear between the points; outside the range the Glauert thin-airfoil
/// value is scaled to the nearest point. `None` unless `0 < E < 1`.
pub fn rudder_effectiveness(chord_ratio: f64) -> Option<f64> {
    if !(chord_ratio.is_finite() && chord_ratio > 0.0 && chord_ratio < 1.0) {
        return None;
    }
    let (first, last) = (
        RUDDER_TAU_POINTS[0],
        RUDDER_TAU_POINTS[RUDDER_TAU_POINTS.len() - 1],
    );
    if chord_ratio <= first.0 || chord_ratio >= last.0 {
        let anchor = if chord_ratio <= first.0 { first } else { last };
        return Some(
            anchor.1 * glauert_flap_effectiveness(chord_ratio)
                / glauert_flap_effectiveness(anchor.0),
        );
    }
    RUDDER_TAU_POINTS.windows(2).find_map(|pair| {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        (chord_ratio >= x0 && chord_ratio <= x1)
            .then(|| y0 + (y1 - y0) * (chord_ratio - x0) / (x1 - x0))
    })
}
