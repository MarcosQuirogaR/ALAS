// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Methods/Aerodynamics/Common/Fidelity_Zero/Lift/
// compute_wing_induced_velocity.py.
// Upstream: mission analysis model 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! The influence of every horseshoe vortex on every control point.
//!
//! This is the row's kernel: an `n_cp` by `n_cp` matrix of three-component
//! velocities, built once per distinct Mach number, from which the boundary
//! condition assembles the matrix the circulation solve inverts.
//!
//! # This runs in `f32`, and that is the whole reason for the row's tier
//!
//! `compute_wing_induced_velocity` opens by casting every coordinate it uses
//! to `np.float32`: explicitly, with `dtype=np.float32` on each of
//! twenty-three arrays, including the Mach number, and every quantity
//! derived from them stays in single precision through to the returned
//! `C_mn`. That is not a storage decision like the panelization's: the
//! division by `DENOM`, the two square roots and the reciprocals of `RTV1`
//! and `RTV2` are all evaluated in single precision, and a control point
//! close to a vortex leg is exactly where that shows. So this module computes
//! in `f32`, and `alas-testkit`'s `f32` tier is what the row is checked at.
//!
//! Everything downstream is `f64` again: `VLM` multiplies `C_mn` by
//! double-precision direction cosines, so the influence matrix and the
//! circulation solve are both double, over single-precision entries.
//!
//! # Scope: subsonic only
//!
//! `compute_wing_induced_velocity` splits on the sign of `mach^2 - 1` and has
//! a second, much longer kernel for the supersonic side: Mach cones, sonic
//! vortex detection, the `RFLAG` row-zeroing that averages a sonic
//! horseshoe's strength between its neighbours, and a separate in-plane
//! case. None of it runs here. `Vortex_Lattice.__defaults__` trains on
//! sixteen Mach numbers of which eight are supersonic, and
//! `Fidelity_Zero.__defaults__` then **overrides that grid** with
//! `[0.0, 0.1, 0.2, 0.3, 0.5, 0.75, 0.85, 0.9]`: subsonic throughout. The
//! mission never evaluates the surrogate outside it either, because
//! `build_surrogate` builds only the subsonic spline when the supersonic
//! training block is empty, and that spline clamps at its edge knots.
//!
//! `RFLAG` is therefore all ones and is absent rather than carried: it exists
//! to switch off a sonic vortex, and there are none. `gen_aero_vorlax.py`
//! refuses to write a fixture whose training grid reaches Mach 1, so the
//! boundary is checked rather than asserted.

/// The influence of every panel on every control point, at one Mach number.
///
/// Row `m` is the velocity induced at control point `m` by a unit-strength
/// horseshoe on each panel `n`, in aircraft axes.
pub struct InducedVelocity {
    /// `n_cp * n_cp * 3`, indexed as `(receiver * n_cp + sender) * 3 + axis`.
    pub c_mn: Vec<f32>,
    /// VORLAX's `EW`: the same normalwash, in the *sending* panel's dihedral
    /// frame rather than the aircraft's. Only the leading-edge rows are read,
    /// by the leading-edge suction term.
    pub ew: Vec<f32>,
}

impl InducedVelocity {
    /// The velocity induced at receiver `m` by sender `n`.
    pub fn at(&self, n_cp: usize, m: usize, n: usize) -> [f32; 3] {
        let base = (m * n_cp + n) * 3;
        [self.c_mn[base], self.c_mn[base + 1], self.c_mn[base + 2]]
    }
}

/// Half the projected length of each panel's bound vortex, VORLAX's `s`.
///
/// Upstream returns this as an `n_cp` by `n_cp` array whose rows are all
/// identical (it repeats a row vector to make a later broadcast work) and
/// then reads only row zero. It is one value per panel and is returned as
/// one.
pub struct BoundVortexGeometry {
    /// Half the projected bound-vortex length, per panel.
    pub semispan: Vec<f32>,
}

/// Build the influence matrix at one Mach number.
///
/// # Panics
///
/// Does not panic. Every division here can produce a non-finite value on
/// degenerate geometry, and each one upstream guards is guarded the same way:
/// `DENOM` against a tolerance floor, `FT1`/`FT2` against a control point
/// on the vortex leg, and `U`/`V` against a control point in the horseshoe's
/// own plane.
pub fn compute(
    vd: &super::types::VortexDistribution,
    mach: f64,
) -> (InducedVelocity, BoundVortexGeometry) {
    let n = vd.n_cp;
    let p = &vd.panels;
    let mach = mach as f32;

    // If the bound vortex runs the other way (the mirrored side of a wing,
    // where `y` decreases outboard) its two ends are swapped so that the
    // circulation sense is the same on both sides.
    let flip: Vec<bool> = (0..n).map(|i| p.yah[i] > p.ybh[i]).collect();
    let pick = |a: &[f32], b: &[f32], i: usize| if flip[i] { b[i] } else { a[i] };

    let xa: Vec<f32> = (0..n).map(|i| pick(&p.xah, &p.xbh, i)).collect();
    let ya: Vec<f32> = (0..n).map(|i| pick(&p.yah, &p.ybh, i)).collect();
    let za: Vec<f32> = (0..n).map(|i| pick(&p.zah, &p.zbh, i)).collect();
    let xb: Vec<f32> = (0..n).map(|i| pick(&p.xbh, &p.xah, i)).collect();
    let yb: Vec<f32> = (0..n).map(|i| pick(&p.ybh, &p.yah, i)).collect();
    let zb: Vec<f32> = (0..n).map(|i| pick(&p.zbh, &p.zah, i)).collect();

    // The dihedral angle of each panel's bound vortex, measured from the
    // horizontal. Past a right angle it is folded back so that the two sides
    // of a wing report the same magnitude with opposite sign.
    let dihedral: Vec<f32> = (0..n)
        .map(|i| {
            let d = ((p.yah[i] - p.ybh[i]).powi(2) + (p.zah[i] - p.zbh[i]).powi(2)).sqrt();
            let angle = ((p.ybh[i] - p.yah[i]) / d).acos();
            if angle > std::f32::consts::FRAC_PI_2 {
                angle - std::f32::consts::PI
            } else {
                angle
            }
        })
        .collect();

    // The sending horseshoe's own frame: its midpoint, and the rotation that
    // lays its bound leg in a plane.
    let mut semispan = vec![0.0f32; n];
    let mut slope = vec![0.0f32; n];
    let mut costheta = vec![0.0f32; n];
    let mut sintheta = vec![0.0f32; n];
    let mut centre = vec![[0.0f32; 3]; n];
    for i in 0..n {
        let xc = 0.5 * (xa[i] + xb[i]);
        let yc = 0.5 * (ya[i] + yb[i]);
        let zc = 0.5 * (za[i] + zb[i]);
        centre[i] = [xc, yc, zc];

        let theta = (zb[i] - za[i]).atan2(yb[i] - ya[i]);
        costheta[i] = theta.cos();
        sintheta[i] = theta.sin();

        let x1bar = xb[i] - xc;
        let y1bar = (yb[i] - yc) * costheta[i] + (zb[i] - zc) * sintheta[i];
        semispan[i] = y1bar.abs();
        slope[i] = x1bar / y1bar;
    }

    let b2 = mach * mach - 1.0;
    let mut c_mn = vec![0.0f32; n * n * 3];
    let mut uvw = vec![[0.0f32; 3]; n * n];

    for m in 0..n {
        let (xo, yo, zo) = (p.xc[m], p.yc[m], p.zc[m]);
        for k in 0..n {
            let xobar = xo - centre[k][0];
            let yobar = (yo - centre[k][1]) * costheta[k] + (zo - centre[k][2]) * sintheta[k];
            let zobar = -(yo - centre[k][1]) * sintheta[k] + (zo - centre[k][2]) * costheta[k];

            let (s, t) = (semispan[k], slope[k]);
            let x1 = xobar + t * s;
            let y1 = yobar + s;
            let x2 = xobar - t * s;
            let y2 = yobar - s;

            // The axial distance between the receiving point projected onto
            // the horseshoe plane and the extension of the skewed leg.
            let xty = xobar - t * yobar;

            let tol = s / 500.0;
            let tolsq = tol * tol;
            let zsq = zobar * zobar;
            let rtv1 = y1 * y1 + zsq;
            let rtv2 = y2 * y2 + zsq;

            let (u, v, w) = subsonic(Horseshoe {
                z: zobar,
                xsq1: x1 * x1,
                ro1: b2 * rtv1,
                xsq2: x2 * x2,
                ro2: b2 * rtv2,
                xty,
                t,
                b2,
                zsq,
                tolsq,
                x1,
                y1,
                x2,
                y2,
                rtv1,
                rtv2,
            });

            uvw[m * n + k] = [u, v, w];
            let base = (m * n + k) * 3;
            c_mn[base] = u;
            c_mn[base + 1] = v * costheta[k] - w * sintheta[k];
            c_mn[base + 2] = v * sintheta[k] + w * costheta[k];
        }
    }

    // `EW` is the same normalwash resolved into the difference between the
    // receiving and sending panels' dihedral angles, which is the frame
    // VORLAX's leading-edge suction term works in.
    let mut ew = vec![0.0f32; n * n];
    for m in 0..n {
        for k in 0..n {
            let delta = dihedral[m] - dihedral[k];
            let [_, v, w] = uvw[m * n + k];
            ew[m * n + k] = w * delta.cos() - v * delta.sin();
        }
    }

    (
        InducedVelocity { c_mn, ew },
        BoundVortexGeometry { semispan },
    )
}

/// One horseshoe's contribution, in its own frame.
struct Horseshoe {
    z: f32,
    xsq1: f32,
    ro1: f32,
    xsq2: f32,
    ro2: f32,
    xty: f32,
    t: f32,
    b2: f32,
    zsq: f32,
    tolsq: f32,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    rtv1: f32,
    rtv2: f32,
}

/// The subsonic horseshoe vortex.
///
/// Miranda, Elliot and Baker, *A generalized vortex lattice method for
/// subsonic and supersonic flow applications*, NASA CR-2865 (1977), and the
/// VORLAX source it describes.
///
/// The three guards are upstream's and each one covers a real singularity:
/// `DENOM` floors at the tolerance so a control point on the skewed leg's
/// extension does not divide by zero; `FT1`/`FT2` drop to zero when the
/// control point sits on a trailing leg; and `U` and `V` are zeroed when the
/// control point is in the horseshoe's own plane, where a planar horseshoe
/// induces only a normal velocity.
fn subsonic(h: Horseshoe) -> (f32, f32, f32) {
    const FOUR_PI: f32 = 4.0 * std::f32::consts::PI;

    let rad1 = (h.xsq1 - h.ro1).sqrt();
    let rad2 = (h.xsq2 - h.ro2).sqrt();

    let tbz = (h.t * h.t - h.b2) * h.zsq;
    let mut denom = h.xty * h.xty + tbz;
    if denom < h.tolsq {
        denom = h.tolsq;
    }

    let fb1 = (h.t * h.x1 - h.b2 * h.y1) / rad1;
    let ft1 = if h.rtv1 < h.tolsq {
        0.0
    } else {
        (h.x1 + rad1) / (rad1 * h.rtv1)
    };

    let fb2 = (h.t * h.x2 - h.b2 * h.y2) / rad2;
    let ft2 = if h.rtv2 < h.tolsq {
        0.0
    } else {
        (h.x2 + rad2) / (rad2 * h.rtv2)
    };

    let qb = (fb1 - fb2) / denom;
    let zetapi = h.z / FOUR_PI;

    let in_plane = h.zsq < h.tolsq;
    let u = if in_plane { 0.0 } else { zetapi * qb };
    let v = if in_plane {
        0.0
    } else {
        zetapi * (ft1 - ft2 - qb * h.t)
    };
    let w = -(qb * h.xty + ft1 * h.y1 - ft2 * h.y2) / FOUR_PI;

    (u, v, w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_horseshoe_in_its_own_plane_induces_no_axial_or_lateral_velocity() {
        let h = Horseshoe {
            z: 0.0,
            xsq1: 4.0,
            ro1: -1.0,
            xsq2: 4.0,
            ro2: -1.0,
            xty: 2.0,
            t: 0.1,
            b2: -1.0,
            zsq: 0.0,
            tolsq: 1e-6,
            x1: 2.0,
            y1: 1.0,
            x2: 2.0,
            y2: -1.0,
            rtv1: 1.0,
            rtv2: 1.0,
        };
        let (u, v, w) = subsonic(h);
        assert_eq!(u, 0.0);
        assert_eq!(v, 0.0);
        assert!(
            w != 0.0,
            "a planar horseshoe still washes the plane it lies in"
        );
    }

    #[test]
    fn the_denominator_never_falls_below_the_tolerance_floor() {
        // A control point sitting exactly on the extension of the skewed leg
        // makes `XTY` zero, and with it the whole denominator; without the
        // floor this is a division by zero.
        let h = Horseshoe {
            z: 1.0,
            xsq1: 4.0,
            ro1: -1.0,
            xsq2: 4.0,
            ro2: -1.0,
            xty: 0.0,
            t: 0.0,
            b2: 0.0,
            zsq: 0.0,
            tolsq: 1e-6,
            x1: 2.0,
            y1: 1.0,
            x2: 2.0,
            y2: -1.0,
            rtv1: 1.0,
            rtv2: 1.0,
        };
        let (u, v, w) = subsonic(h);
        assert!(u.is_finite() && v.is_finite() && w.is_finite());
    }

    #[test]
    fn a_control_point_on_a_trailing_leg_contributes_no_trailing_term() {
        let h = Horseshoe {
            z: 1.0,
            xsq1: 4.0,
            ro1: -1e-12,
            xsq2: 4.0,
            ro2: -1.0,
            xty: 2.0,
            t: 0.0,
            b2: -1.0,
            zsq: 1.0,
            tolsq: 1.0,
            x1: 2.0,
            y1: 0.0,
            x2: 2.0,
            y2: -1.0,
            rtv1: 1e-12,
            rtv2: 1.0,
        };
        // `RTV1` below the tolerance means the point is on the first
        // trailing leg; `FT1` drops out rather than diverging.
        let (_, _, w) = subsonic(h);
        assert!(w.is_finite());
    }
}
