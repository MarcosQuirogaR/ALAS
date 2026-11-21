// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Web shear flexibility of the wing-box beam.
//!
//! The Euler-Bernoulli response integrates bending only. A box beam also
//! deflects by the shear strain of its spar webs: for a section whose shear is
//! carried by the webs the unit-load result is `dw/ds = V / (G A_w)`, with
//! `A_w = sum t_w h` over the spars (Megson, *Aircraft Structures for
//! Engineering Students*, 4th ed., 2007, ch. 21-22, shear deflection of thin-
//! walled beams by the unit-load method; Niu, *Airframe Structural Design*,
//! 1988, ch. 8, wing shear deflection). Along the swept axis `ds = dy / cos L`.
//! Cover shear lag and the bending-torsion coupling of the swept box are not
//! in this term.

use super::WingboxSizing;
use alas_config::materials::MaterialSpec;

/// Shear compliance per unit span at each station, `1 / (G A_w cos L)`, 1/N.
/// Zero where no web area is recorded.
pub(crate) fn shear_compliance_per_m(sizing: &WingboxSizing, web: &MaterialSpec) -> Vec<f64> {
    let g = web.g_pa();
    (0..sizing.y_stations.len())
        .map(|j| {
            let area: f64 = sizing
                .spars
                .iter()
                .filter(|s| s.h[j] > 0.0)
                .map(|s| s.t_web[j] * s.h[j])
                .sum();
            let denom = g * area * sizing.axis_cos_at(j).max(1.0e-3);
            if denom.is_finite() && denom > 0.0 {
                1.0 / denom
            } else {
                0.0
            }
        })
        .collect()
}

/// Shear deflection of each station from the root, m, for shear `v` (N, the
/// magnitude of the vertical shear): the trapezoid of `v * compliance`.
pub(crate) fn shear_deflection(y: &[f64], v: &[f64], compliance: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; y.len()];
    for k in 1..y.len() {
        out[k] = out[k - 1]
            + 0.5 * (y[k] - y[k - 1]) * (v[k - 1] * compliance[k - 1] + v[k] * compliance[k]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::shear_deflection;

    #[test]
    fn a_constant_shear_on_a_constant_web_deflects_linearly() {
        // V = 1000 N, 1/(G A) = 2e-6 1/N, L = 10 m: 0.02 m.
        let y: Vec<f64> = (0..=10).map(f64::from).collect();
        let d = shear_deflection(&y, &[1000.0; 11], &[2.0e-6; 11]);
        assert!((d[10] - 0.02).abs() < 1.0e-12);
    }
}
