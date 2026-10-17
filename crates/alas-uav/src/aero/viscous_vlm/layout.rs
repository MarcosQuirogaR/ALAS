// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Strip layout of the main wing: geometry, section blend weights and the
//! thin-airfoil data of the lattice section law.
//!
//! The lattice reproduces, strip by strip, the discrete two-dimensional
//! section law `cl = a (alpha - alpha0L)` for a thin cambered chord line of
//! `n` chordwise panels (vortex at the panel quarter point, collocation at
//! the three-quarter point).  `a` is close to 2 pi and `alpha0L` is the
//! zero-lift angle of the meshed camber line, both relative to the strip's
//! chord line (leading-edge to trailing-edge midpoints).  They are obtained by
//! solving that 2-D system on the strip's real panel coordinates, so the
//! effective angle recovered from the lattice lift
//! `alpha_eff = alpha0L + cl_vlm / a` is exactly the angle at which the
//! lattice would have produced that lift in two dimensions.

use alas_aero::strips::WingStrip;
use alas_aero::vlm::PanelSample;
use alas_geom::aircraft::wing::Wing;

use crate::aero::AeroError;

/// One strip with everything that does not depend on the flow condition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct StripGeom {
    /// Lattice strip (panel range, width, chord, centre).
    pub strip: WingStrip,
    /// Discrete 2-D lift slope of the meshed section, 1/rad.
    pub a: f64,
    /// Discrete 2-D zero-lift angle of the meshed section, rad.
    pub alpha0l: f64,
    /// Index of the root-side wing cross-section of the blend.
    pub section: usize,
    /// Blend weight of the next cross-section, `[0, 1]`.
    pub weight: f64,
    /// Quarter-chord point, geometry axes, m.
    pub quarter_chord: [f64; 3],
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let n = dot(a, a).sqrt();
    (n.is_finite() && n > 0.0).then(|| [a[0] / n, a[1] / n, a[2] / n])
}

fn mid(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        0.5 * (a[0] + b[0]),
        0.5 * (a[1] + b[1]),
        0.5 * (a[2] + b[2]),
    ]
}

/// Solve `m x = b` (row-major `n x n`) by Gaussian elimination with partial
/// pivoting; `None` for a singular system.
fn solve_dense(n: usize, m: &[f64], b: &[f64]) -> Option<Vec<f64>> {
    let mut a = m.to_vec();
    let mut x = b.to_vec();
    for col in 0..n {
        let pivot =
            (col..n).max_by(|&i, &j| a[i * n + col].abs().total_cmp(&a[j * n + col].abs()))?;
        if a[pivot * n + col].abs() < 1e-14 {
            return None;
        }
        if pivot != col {
            for k in 0..n {
                a.swap(col * n + k, pivot * n + k);
            }
            x.swap(col, pivot);
        }
        for row in col + 1..n {
            let f = a[row * n + col] / a[col * n + col];
            for k in col..n {
                a[row * n + k] -= f * a[col * n + k];
            }
            x[row] -= f * x[col];
        }
    }
    for col in (0..n).rev() {
        let mut s = x[col];
        for k in col + 1..n {
            s -= a[col * n + k] * x[k];
        }
        x[col] = s / a[col * n + col];
    }
    Some(x)
}

/// Discrete thin-airfoil `(a, alpha0L)` of the strip's panels.
///
/// Section plane: chordwise axis `e` (leading to trailing edge, aft), span
/// direction `s` (first panel's left to right vertex), and `u = e x s`
/// (up for a wing in the xy plane).  For panel `j` with front and back edge
/// midpoints at `(xi, zeta)` in that plane, the slope is `dzeta/dxi` and the
/// tangency condition reads `alpha - slope_i = sum_j gamma_j / (2 pi (xi_c,i -
/// xi_v,j))` with `gamma = Gamma / V`; the section lift is `cl = 2 sum(gamma)
/// / chord`.
fn thin_section(strip: &WingStrip, panels: &[PanelSample]) -> Option<(f64, f64)> {
    let n = strip.panel_count;
    let first = &panels[0];
    let e = unit(sub(strip.trailing_edge_m, strip.leading_edge_m))?;
    let s = unit(sub(first.front_right, first.front_left))?;
    let u = cross(e, s);
    let le = strip.leading_edge_m;
    let mut xi_vortex = Vec::with_capacity(n);
    let mut xi_colloc = Vec::with_capacity(n);
    let mut slope = Vec::with_capacity(n);
    for p in panels {
        let f = sub(mid(p.front_left, p.front_right), le);
        let b = sub(mid(p.back_left, p.back_right), le);
        let (xf, zf) = (dot(f, e), dot(f, u));
        let (xb, zb) = (dot(b, e), dot(b, u));
        let dx = xb - xf;
        if !(dx.is_finite() && dx > 0.0) {
            return None;
        }
        xi_vortex.push(xf + 0.25 * dx);
        xi_colloc.push(xf + 0.75 * dx);
        slope.push((zb - zf) / dx);
    }
    let two_pi = std::f64::consts::TAU;
    let mut m = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..n {
            m[i * n + j] = 1.0 / (two_pi * (xi_colloc[i] - xi_vortex[j]));
        }
    }
    let ones = vec![1.0; n];
    let neg_slope: Vec<f64> = slope.iter().map(|v| -v).collect();
    let g1 = solve_dense(n, &m, &ones)?;
    let g0 = solve_dense(n, &m, &neg_slope)?;
    let chord = strip.chord_m;
    let a = 2.0 * g1.iter().sum::<f64>() / chord;
    let cl0 = 2.0 * g0.iter().sum::<f64>() / chord;
    (a.is_finite() && a > 0.0).then(|| (a, -cl0 / a))
}

/// Build the layout from the strips and panels of a first solve.
pub(super) fn build(
    wing: &Wing,
    strips: &[WingStrip],
    panels: &[PanelSample],
) -> Result<Vec<StripGeom>, AeroError> {
    let ys: Vec<f64> = wing.xsecs.iter().map(|x| x.xyz_le[1].abs()).collect();
    if ys.len() < 2 || ys.windows(2).any(|w| w[1] <= w[0]) {
        return Err(AeroError::InvalidInput(
            "the main wing cross-sections must have strictly increasing |y| from root to tip"
                .into(),
        ));
    }
    strips
        .iter()
        .map(|strip| {
            let range = strip.first_panel..strip.first_panel + strip.panel_count;
            let (a, alpha0l) = thin_section(strip, &panels[range]).ok_or_else(|| {
                AeroError::Lattice(format!(
                    "degenerate strip geometry at y = {:.4} m",
                    strip.y_m
                ))
            })?;
            let y = strip.y_m.abs();
            let section = ys
                .iter()
                .rposition(|v| *v <= y)
                .unwrap_or(0)
                .min(ys.len() - 2);
            let weight = ((y - ys[section]) / (ys[section + 1] - ys[section])).clamp(0.0, 1.0);
            let qc = [
                strip.leading_edge_m[0]
                    + 0.25 * (strip.trailing_edge_m[0] - strip.leading_edge_m[0]),
                strip.leading_edge_m[1]
                    + 0.25 * (strip.trailing_edge_m[1] - strip.leading_edge_m[1]),
                strip.leading_edge_m[2]
                    + 0.25 * (strip.trailing_edge_m[2] - strip.leading_edge_m[2]),
            ];
            Ok(StripGeom {
                strip: *strip,
                a,
                alpha0l,
                section,
                weight,
                quarter_chord: qc,
            })
        })
        .collect()
}
