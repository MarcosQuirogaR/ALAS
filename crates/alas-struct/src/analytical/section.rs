// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Section stiffness and baseline running mass for the analytical solver.
//! Historical and product geometry conventions are selected by the caller.

use crate::sizing::WingboxSizing;
use alas_config::materials::MaterialSpec;
/// Area moment of inertia of a symmetric I-section about its own centroid:
/// `_I_section`.
fn i_section(h: &[f64], bf: &[f64], tf: &[f64], tw: f64) -> Vec<f64> {
    (0..h.len())
        .map(|j| {
            let thin = h[j] <= 2.0 * tf[j];
            if thin {
                bf[j] * h[j].powi(3) / 12.0
            } else {
                let hw = (h[j] - 2.0 * tf[j]).max(0.0);
                tw * hw.powi(3) / 12.0 + 2.0 * bf[j] * tf[j] * ((h[j] - tf[j]) / 2.0).powi(2)
            }
        })
        .collect()
}

/// Distributed mass per unit span, kg/m: `_mass_per_length`.
pub(super) fn mass_per_length(
    sizing: &WingboxSizing,
    cap_rho: f64,
    web_rho: f64,
    skin_rho: f64,
    include_ribs: bool,
    skin_cover_fraction: f64,
) -> Vec<f64> {
    let n = sizing.chord.len();
    let mut m_y: Vec<f64> = (0..n)
        .map(|j| 2.0 * skin_cover_fraction * sizing.chord[j] * sizing.t_skin * skin_rho)
        .collect();
    for s in &sizing.spars {
        for (m, (&h, &a)) in m_y.iter_mut().zip(s.h.iter().zip(&s.a_cap)) {
            *m += s.t_web * h * web_rho + 2.0 * a * cap_rho;
        }
    }
    if include_ribs {
        // Ribs are discrete in the mesh, but the analytical load/mode model
        // uses a spanwise mass density.  Conserving the sized rib mass as a
        // uniform density keeps both inertial relief and the Rayleigh modal
        // denominator on the same mass basis as the sizing result.
        let span = sizing
            .y_stations
            .last()
            .copied()
            .zip(sizing.y_stations.first().copied())
            .map(|(last, first)| last - first)
            .unwrap_or(0.0);
        if span > 0.0 && sizing.mass_breakdown_kg.ribs.is_finite() {
            let rib_density = sizing.mass_breakdown_kg.ribs / span;
            for mass in &mut m_y {
                *mass += rib_density;
            }
        }
    }
    m_y
}

/// Combined bending stiffness `EI(y)`, N.m^2: `_EI_curve`.
pub(super) fn ei_curve(
    sizing: &WingboxSizing,
    cap_mat: &MaterialSpec,
    skin_mat: &MaterialSpec,
) -> Vec<f64> {
    let n = sizing.y_stations.len();
    let mut ei = vec![0.0; n];
    for s in &sizing.spars {
        let i_cap = i_section(&s.h, &s.w_cap, &s.t_cap, 0.0);
        for (e, &ic) in ei.iter_mut().zip(&i_cap) {
            *e += cap_mat.e_pa * ic;
        }
    }

    let frac_min = sizing
        .spar_fracs
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let frac_max = sizing
        .spar_fracs
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let n_spars = sizing.spars.len() as f64;
    for (j, e) in ei.iter_mut().enumerate() {
        let b_box = (frac_max - frac_min) * sizing.chord[j];
        let h_mean: f64 = sizing.spars.iter().map(|s| s.h[j]).sum::<f64>() / n_spars;
        let d_skin = h_mean / 2.0;
        let i_skin = 2.0 * b_box * sizing.t_skin * d_skin.powi(2);
        *e += skin_mat.e_pa * i_skin;
    }
    ei
}
