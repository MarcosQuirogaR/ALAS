// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cover inertia along the actual airfoil contour between the spars.
//!
//! The straight-panel cover model of [`super::section`] takes the cover depth as
//! linear between the two spar heights. A real cover follows the airfoil, whose
//! depth between 25 and 70 percent chord exceeds that chord-wise linear
//! interpolation, so the straight panels understate the cover stiffness (by
//! 19 to 36 percent for the presets). The thin-walled section property of a
//! cover of thickness `t` about the mid-depth plane is `I = integral t z^2 ds`
//! along its contour; Torenbeek, *Advanced Aircraft Design*, Wiley, 2013,
//! Sec. 11.3.2, Eqs. 11.31-11.32, p. 331, expresses the same contour effect as the
//! section effectiveness factor from the front- and aft-spar heights. With the
//! covers at `z = +-h/2` about that plane, `h` the local airfoil depth, both covers
//! give `I / t = integral (h^2 / 4) (ds_u + ds_l)` per unit `t`, evaluated here.
//!
//! The factor stored per station is this inertia over the straight-panel one,
//! so every consumer of the straight-panel formula scales by it (1 when absent).
//! Camber is not credited: the common mid-plane is the mean of the surfaces, as
//! in the straight-panel model.

use alas_geom::wing_structure::WingStructureGeometry;

/// Chordwise integration points per cover bay.
const SEGMENTS: usize = 200;

/// Largest airfoil depth `h(x/c)` between the first and last active spar at `eta`, m,
/// for active spars given as `(chord_fraction, height_m)`; 0 without a bay.
///
/// The cover of the idealised curved section carries the bending stress
/// `|sigma| = M t / (2 I)`, with the neutral axis midway between the panels and
/// `t` the section thickness (Torenbeek, *Advanced Aircraft Design*, Wiley, 2013,
/// Sec. 11.3.2, Eq. 11.30, p. 331), so the extreme cover fibre lies at half the
/// deepest contour depth, which mid-bay exceeds the spar heights. The inertia `I` of [`cover_shape_factor`] already
/// follows that contour; this is the matching fibre distance.
pub(crate) fn cover_depth_m(wsg: &WingStructureGeometry, eta: f64, active: &[(f64, f64)]) -> f64 {
    let (Some(first), Some(last)) = (active.first(), active.last()) else {
        return 0.0;
    };
    let mut depth = active.iter().map(|&(_, h)| h).fold(0.0_f64, f64::max);
    let dxc = (last.0 - first.0) / SEGMENTS as f64;
    for k in 0..=SEGMENTS {
        depth = depth.max(wsg.spar_height(eta, first.0 + k as f64 * dxc));
    }
    if depth.is_finite() {
        depth
    } else {
        0.0
    }
}

/// Ratio of the contour cover inertia to the straight-panel inertia at `eta`,
/// for consecutive active spars given as `(chord_fraction, height_m)`. 1 when
/// there is no bay or the straight-panel value is not positive.
pub(crate) fn cover_shape_factor(
    wsg: &WingStructureGeometry,
    eta: f64,
    chord_m: f64,
    active: &[(f64, f64)],
) -> f64 {
    let (mut straight, mut contour) = (0.0, 0.0);
    for pair in active.windows(2) {
        let ((fa, ha), (fb, hb)) = (pair[0], pair[1]);
        let width = (fb - fa) * chord_m;
        straight += width * (ha * ha + ha * hb + hb * hb) / 6.0;
        let dxc = (fb - fa) / SEGMENTS as f64;
        for k in 0..SEGMENTS {
            let xc = fa + (k as f64 + 0.5) * dxc;
            let h = wsg.spar_height(eta, xc);
            let step = 1.0e-4;
            let (u1, l1) = wsg.airfoil_zu_zl(eta, (xc - step).max(0.0));
            let (u2, l2) = wsg.airfoil_zu_zl(eta, (xc + step).min(1.0));
            let dx = (xc + step).min(1.0) - (xc - step).max(0.0);
            let su = (1.0 + ((u2 - u1) / dx).powi(2)).sqrt();
            let sl = (1.0 + ((l2 - l1) / dx).powi(2)).sqrt();
            contour += 0.25 * h * h * (su + sl) * dxc * chord_m;
        }
    }
    if straight > 0.0 && contour.is_finite() {
        contour / straight
    } else {
        1.0
    }
}
