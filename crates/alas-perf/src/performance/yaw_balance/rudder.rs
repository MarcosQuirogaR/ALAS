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

/// Clamped piecewise-linear interpolation of `ys` over ascending `xs`.
fn interpolate(xs: &[f64], ys: &[f64], x: f64) -> f64 {
    let x = x.clamp(xs[0], xs[xs.len() - 1]);
    for i in 1..xs.len() {
        if x <= xs[i] {
            return ys[i - 1] + (ys[i] - ys[i - 1]) * (x - xs[i - 1]) / (xs[i] - xs[i - 1]);
        }
    }
    ys[ys.len() - 1]
}

/// Empirical large-deflection correction `K'` to the lift effectiveness of a
/// plain trailing-edge flap (elevator or rudder): the ratio of the real to
/// the linear effectiveness at deflection `|delta|` in degrees, for a flap of
/// chord fraction `c_f / c`.
///
/// Source: USAF Stability and Control DATCOM (Finck, AFWAL-TR-83-3048, 1978
/// revision), sec. 6.1.1.1, Fig. 6.1.1.1-40, "Empirical correction for lift
/// effectiveness of plain trailing-edge flaps at high flap deflections"
/// (reproduced as `k'` in Roskam, *Airplane Design* Part VI, Fig. 8.13,
/// p. 228). Digitised by eye from the DTIC ADB072483 scan (leaf n1909),
/// about +/-0.03 (+/-0.05 between 15 and 25 deg), at c_f/c = 0.25, 0.30,
/// 0.40:
///
/// | delta deg | 0.25 | 0.30 | 0.40 |
/// |---|---|---|---|
/// | 0-10 | 1.00 | 1.00 | 1.00 |
/// | 15 | 0.97 | 0.96 | 0.94 |
/// | 20 | 0.85 | 0.82 | 0.77 |
/// | 25 | 0.74 | 0.72 | 0.68 |
/// | 30 | 0.66 | 0.65 | 0.62 |
/// | 40 | 0.57 | 0.56 | 0.53 |
///
/// Linear in both deflection and chord fraction, clamped to the tabulated
/// chord range and constant beyond 40 deg.
#[must_use]
pub fn large_deflection_effectiveness_factor(deflection_deg: f64, chord_fraction: f64) -> f64 {
    const DEFLECTIONS: [f64; 7] = [0.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0];
    const CHORDS: [f64; 3] = [0.25, 0.30, 0.40];
    const FACTORS: [[f64; 7]; 3] = [
        [1.00, 1.00, 0.97, 0.85, 0.74, 0.66, 0.57],
        [1.00, 1.00, 0.96, 0.82, 0.72, 0.65, 0.56],
        [1.00, 1.00, 0.94, 0.77, 0.68, 0.62, 0.53],
    ];
    let delta = if deflection_deg.is_finite() {
        deflection_deg.abs()
    } else {
        DEFLECTIONS[DEFLECTIONS.len() - 1]
    };
    let by_chord: Vec<f64> = FACTORS
        .iter()
        .map(|row| interpolate(&DEFLECTIONS, row, delta))
        .collect();
    let chord = if chord_fraction.is_finite() {
        chord_fraction
    } else {
        CHORDS[CHORDS.len() - 1]
    };
    interpolate(&CHORDS, &by_chord, chord)
}

/// Deflections, deg, of [`RUDDER_SECANT_FACTOR`].
const RUDDER_DEFLECTIONS_DEG: [f64; 6] = [0.0, 5.0, 10.0, 15.0, 25.0, 30.0];
/// Secant fin side force per degree of rudder over its small-deflection
/// value, measured on two transport fins with a quarter-chord rudder
/// (`c_r / c_v` 0.25), exposed-area basis: Obert, *Aerodynamic Design of
/// Transport Aircraft* (IOS Press, 2009), Fig. 33.2, p. 424 (Fokker report
/// L-29-150). Boeing 737-100 low tail: `C_L,v` 0.17 at 5 deg, 0.31 at 10,
/// 0.45 at 14.6 and 0.665 at 24.4 deg (secants 0.034, 0.031, 0.031 and
/// 0.027 per deg); F-29 model 2-5: linear to 0.405 at 15 deg, 0.64 at
/// 30 deg (secant 0.79 of the linear one). Read from the plots, about
/// +/-0.01 in `C_L,v`. Held constant beyond 30 deg.
const RUDDER_SECANT_FACTOR: [f64; 6] = [1.0, 1.0, 0.93, 0.90, 0.785, 0.785];
/// Chord fraction of the fins of [`RUDDER_SECANT_FACTOR`].
const RUDDER_SECANT_CHORD: f64 = 0.25;

/// Large-deflection factor of a rudder: the measured transport-fin secant
/// ([`RUDDER_SECANT_FACTOR`], quarter-chord rudders) times the relative
/// chord trend of the DATCOM plain-flap factor
/// ([`large_deflection_effectiveness_factor`] at `chord_fraction` over its
/// value at 0.25), since a deeper rudder is the less linear (Obert, p. 423).
///
/// The DATCOM factor itself is a sealed-gap airfoil correction (Roskam,
/// *Airplane Design* Part VI, Eq. 8.4, p. 226) that Eq. 10.123a carries to
/// the fin unchanged; on the fins above it falls about 20 % below the
/// measured secant at 25 to 30 deg (0.65 against 0.79), and Roskam's linear
/// control methods hold below about 20 deg (p. 463), so the measured fins
/// govern the rudder. The elevator keeps the DATCOM factor.
#[must_use]
pub fn rudder_large_deflection_factor(deflection_deg: f64, chord_fraction: f64) -> f64 {
    let delta = if deflection_deg.is_finite() {
        deflection_deg.abs()
    } else {
        RUDDER_DEFLECTIONS_DEG[RUDDER_DEFLECTIONS_DEG.len() - 1]
    };
    let measured = interpolate(&RUDDER_DEFLECTIONS_DEG, &RUDDER_SECANT_FACTOR, delta);
    let chord_trend = large_deflection_effectiveness_factor(delta, chord_fraction)
        / large_deflection_effectiveness_factor(delta, RUDDER_SECANT_CHORD);
    measured * chord_trend
}

/// Spanwise stations `eta` of [`span_factor`], fraction of the panel span.
const SPAN_ETA: [f64; 11] = [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
/// Panel taper ratios of the [`SPAN_KB`] rows.
const SPAN_TAPER: [f64; 3] = [0.0, 0.5, 1.0];
/// `K_b(eta)` from the panel root to `eta`, Roskam, *Airplane Design* Part
/// VI, Fig. 8.52 "Effect of taper ratio and flap span on K_b" (p. 260,
/// copied from DATCOM), digitised by eye (about +/-0.01) at taper 0, 0.5
/// and 1.
const SPAN_KB: [[f64; 11]; 3] = [
    [
        0.0, 0.153, 0.307, 0.444, 0.570, 0.685, 0.785, 0.865, 0.930, 0.975, 1.0,
    ],
    [
        0.0, 0.142, 0.290, 0.420, 0.535, 0.655, 0.755, 0.840, 0.910, 0.965, 1.0,
    ],
    [
        0.0, 0.130, 0.270, 0.392, 0.490, 0.600, 0.705, 0.800, 0.890, 0.955, 1.0,
    ],
];

/// Flap-span factor `K_b` of a control surface from `eta_i` to `eta_o` of a
/// panel of taper ratio `taper` (Roskam Part VI, Sec. 8.1.4.1, Fig. 8.51:
/// `K_b = K_b(eta_o) - K_b(eta_i)`, Fig. 8.52). For a rudder the panel is
/// the fin measured from the fuselage centreline (Roskam Part VI Eq. 10.123a,
/// p. 461, and Fig. 10.13). `None` unless `0 <= eta_i < eta_o <= 1`.
#[must_use]
pub fn span_factor(eta_inner: f64, eta_outer: f64, taper: f64) -> Option<f64> {
    if !((0.0..1.0).contains(&eta_inner) && eta_outer > eta_inner && eta_outer <= 1.0) {
        return None;
    }
    let taper = if taper.is_finite() { taper } else { 0.5 };
    let at = |eta: f64| {
        let rows: Vec<f64> = SPAN_KB
            .iter()
            .map(|row| interpolate(&SPAN_ETA, row, eta))
            .collect();
        interpolate(&SPAN_TAPER, &rows, taper)
    };
    Some(at(eta_outer) - at(eta_inner))
}

/// Aspect ratios of the [`THREE_D_RATIO`] columns.
const RATIO_ASPECT: [f64; 6] = [1.0, 2.0, 3.0, 4.0, 6.0, 10.0];
/// Section flap effectiveness `(alpha_delta)_cl` of the [`THREE_D_RATIO`] rows.
const RATIO_ALPHA_DELTA: [f64; 6] = [0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
/// `(alpha_delta)_CL / (alpha_delta)_cl`, Roskam, *Airplane Design* Part VI,
/// Fig. 8.53 "Effect of aspect ratio and flap-chord ratio on the
/// three-dimensional flap effectiveness" (p. 261, copied from DATCOM),
/// digitised by eye (about +/-0.02) on the curves `(alpha_delta)_cl` = 0.3
/// to 0.8.
const THREE_D_RATIO: [[f64; 6]; 6] = [
    [1.53, 1.36, 1.28, 1.205, 1.14, 1.10],
    [1.41, 1.257, 1.196, 1.145, 1.10, 1.07],
    [1.30, 1.188, 1.145, 1.105, 1.075, 1.05],
    [1.21, 1.14, 1.107, 1.077, 1.055, 1.04],
    [1.145, 1.10, 1.075, 1.054, 1.04, 1.03],
    [1.10, 1.065, 1.05, 1.035, 1.025, 1.02],
];

/// Ratio of the three- to the two-dimensional flap effectiveness
/// `(alpha_delta)_CL / (alpha_delta)_cl` (Roskam Part VI Fig. 8.53) of a
/// surface of aspect ratio `aspect_ratio` with section effectiveness
/// `alpha_delta_section`; clamped to the digitised range. `None` for
/// non-finite or non-positive inputs.
#[must_use]
pub fn three_dimensional_effectiveness_ratio(
    aspect_ratio: f64,
    alpha_delta_section: f64,
) -> Option<f64> {
    if !(aspect_ratio.is_finite()
        && aspect_ratio > 0.0
        && alpha_delta_section.is_finite()
        && alpha_delta_section > 0.0)
    {
        return None;
    }
    let by_alpha: Vec<f64> = THREE_D_RATIO
        .iter()
        .map(|row| interpolate(&RATIO_ASPECT, row, aspect_ratio))
        .collect();
    Some(interpolate(
        &RATIO_ALPHA_DELTA,
        &by_alpha,
        alpha_delta_section,
    ))
}
