// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Swept-box bending: the beam lies along the swept elastic axis, not along
//! the streamwise cut.
//!
//! Sources, verified on the page: Torenbeek, *Advanced Aircraft Design*, 2013,
//! book pp. 327-328 (PDF 343-344): Eq. 11.11 `M_BL(y) = int (y' - y)/cos(L_ea)
//! dL(y')` (bending moment about the section normal to the elastic axis),
//! Eq. 11.15 `b_st = b/cos(L_ea)`, with `L_ea` "nearly equal to the mean sweep
//! angle of a straight line midway between the front and the aft spars";
//! Niu, *Airframe Structural Design*, book pp. 31-32 (PDF 37-38, rendered):
//! swept-wing bending `phi = int M/(E I) dy` and torsion `theta = int T/(G J)
//! dy` integrated along the elastic axis.
//!
//! Derived here (not in either book): with section stiffness `EI_n` normal to
//! the axis and `s = y/cos L`, the vertical deflection obeys `w_ss = M_n/EI_n`,
//! so `w_yy = M_x/(EI_n cos^3 L)`; the Rayleigh strain energy gives the same
//! effective stiffness. `EI_n = k EI_s`, where `EI_s` is the streamwise-cut
//! stiffness of the sized section: cap and web material crossed by a normal cut
//! is the cap area and the web depth times thickness, so `k = 1` for them; a
//! cover panel between the spars is `cos L` as wide on a normal cut, so
//! `k = cos L` for it. Against the shell deck (12 presets, FE sections and FE
//! loads) this predicts the extra flexibility within about 15 percent.

use alas_geom::wing_structure::WingStructureGeometry;

/// Cosine of the box-axis sweep at each station, from the slope of the line
/// midway between the outermost spars (Torenbeek, Eq. 11.11 text). Local
/// slope, one-sided at the ends. All stations 1 for a degenerate input.
pub(crate) fn axis_cos(wsg: &WingStructureGeometry, y: &[f64]) -> Vec<f64> {
    let n = y.len();
    if n < 2 || !wsg.semi_span.is_finite() || wsg.semi_span <= 0.0 {
        return vec![1.0; n];
    }
    let (front, rear) = crate::sizing::box_chord_band(wsg);
    let mid = 0.5 * (front + rear);
    let x_mid: Vec<f64> = y
        .iter()
        .map(|&yi| {
            let eta = (yi / wsg.semi_span).clamp(0.0, 1.0);
            wsg.x_le(eta) + mid * wsg.local_chord(eta)
        })
        .collect();
    (0..n)
        .map(|j| {
            let (a, b) = (j.saturating_sub(1), (j + 1).min(n - 1));
            let dy = y[b] - y[a];
            if dy <= 0.0 || !dy.is_finite() {
                return 1.0;
            }
            let tan = (x_mid[b] - x_mid[a]) / dy;
            if tan.is_finite() {
                1.0 / (1.0 + tan * tan).sqrt()
            } else {
                1.0
            }
        })
        .collect()
}
