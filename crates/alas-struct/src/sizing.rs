// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/structural_sizing.py
// Reference: alas @ rust-port-baseline.

//! Direct strength-based wingbox sizing.
//!
//! [`size_wingbox`] sizes the spar caps directly from strength -- margin of
//! safety zero by construction at the root, the bending-critical station --
//! with no mass-target bisection, then applies the spar-cap taper law and the
//! geometric cap width/height limits, sizes the webs from root shear, fixes
//! the skin at its configured minimum, and derives the rib spacing from a
//! panel-buckling criterion.
//!
//! Loads come from [`crate::loads`] (elliptic distribution, no inertial
//! relief -- the conservative choice for strength sizing). Moment and shear
//! are split across the spars weighted by each spar's local section depth, so
//! a deeper spar carries proportionally more of the bending moment and a
//! partial-span spar, zeroed outboard of the break, carries none of it there.

use alas_config::materials::MaterialSpec;
use alas_config::{DesignRequirements, StructuresConfig};
use alas_geom::wing_structure::WingStructureGeometry;

use crate::loads::{self, LoadCase};

/// Per-spar sizing result, sampled at [`WingboxSizing::y_stations`].
#[derive(Debug, Clone, PartialEq)]
pub struct SparSizing {
    /// The spar's chordwise position, as a fraction of local chord.
    pub chord_fraction: f64,
    /// Free web height at each station, m.
    pub h: Vec<f64>,
    /// Cap flange width (tapered) at each station, m.
    pub w_cap: Vec<f64>,
    /// Cap flange thickness (tapered) at each station, m.
    pub t_cap: Vec<f64>,
    /// One-flange cap area at each station, m^2.
    pub a_cap: Vec<f64>,
    /// Uniform web thickness, m.
    pub t_web: f64,
    /// Bending-moment fraction this spar carries at each station.
    pub frac_moment: Vec<f64>,
    /// Margin of safety at each station -- `+inf` where the local demand is
    /// below 1 N.m (near the tip). Expected `>= 0` near the root.
    pub margin_of_safety: Vec<f64>,
}

/// The sized wingbox: per-station geometry, rib layout and mass breakdown.
#[derive(Debug, Clone, PartialEq)]
pub struct WingboxSizing {
    /// Spanwise stations, m.
    pub y_stations: Vec<f64>,
    /// Normalized spanwise stations, `y / semi_span`.
    pub eta_stations: Vec<f64>,
    /// Local chord at each station, m.
    pub chord: Vec<f64>,
    /// The spar chordwise fractions, in the geometry's sorted order.
    pub spar_fracs: Vec<f64>,
    /// Per-spar sizing.
    pub spars: Vec<SparSizing>,
    /// Skin thickness, m.
    pub t_skin: f64,
    /// Number of ribs.
    pub num_ribs: i64,
    /// Panel-buckling rib spacing, m.
    pub rib_spacing_m: f64,
    /// Semi-wing mass by component, kg.
    pub mass_breakdown_kg: MassBreakdown,
    /// Total semi-wing structural mass, kg.
    pub total_mass_kg: f64,
    /// The name of the load case that sized the box.
    pub sizing_load_case: &'static str,
}

/// The four semi-wing mass components upstream keys by name in its
/// `mass_breakdown_kg` dict.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassBreakdown {
    /// Spar caps, kg.
    pub spar_caps: f64,
    /// Spar webs, kg.
    pub spar_webs: f64,
    /// Skin, kg.
    pub skin: f64,
    /// Ribs, kg.
    pub ribs: f64,
}

/// NumPy `linspace(start, stop, n)` with `endpoint=True`: `n` evenly spaced
/// points, the last pinned exactly to `stop`.
fn linspace(start: f64, stop: f64, n: usize) -> Vec<f64> {
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![start];
    }
    let step = (stop - start) / (n - 1) as f64;
    let mut values: Vec<f64> = (0..n).map(|i| start + i as f64 * step).collect();
    values[n - 1] = stop;
    values
}

/// NumPy `gradient(f)` at unit spacing, `edge_order=1`: central differences
/// interior, one-sided at the two ends. For a uniform `y` this is the constant
/// station spacing, but the general form is reproduced so the arithmetic
/// matches upstream bit for bit.
pub(crate) fn gradient_unit(f: &[f64]) -> Vec<f64> {
    let n = f.len();
    let mut g = vec![0.0; n];
    if n < 2 {
        return g;
    }
    for i in 1..n - 1 {
        g[i] = (f[i + 1] - f[i - 1]) / 2.0;
    }
    g[0] = f[1] - f[0];
    g[n - 1] = f[n - 1] - f[n - 2];
    g
}

/// NumPy `trapezoid(y, x)`: the trapezoidal integral of `y` over the sample
/// points `x`.
fn trapezoid(y: &[f64], x: &[f64]) -> f64 {
    let mut acc = 0.0;
    for i in 0..y.len().saturating_sub(1) {
        acc += (x[i + 1] - x[i]) * (y[i + 1] + y[i]) / 2.0;
    }
    acc
}

/// The number of stations needed to keep every uniform rib panel at or below
/// the maximum spacing. Both the root and tip are ribs, so panels plus one is
/// the count. This is the count form of the panel-buckling sizing rule.
fn rib_count_from_max_spacing(semi_span_m: f64, max_spacing_m: f64) -> i64 {
    (semi_span_m / max_spacing_m).ceil() as i64 + 1
}

/// The spar-cap taper law: full section up to `eta_lock`, then linear taper to
/// `tip_fraction` at the tip -- `_cap_taper`.
///
/// Visible to `crate::mesh` as well: the mesh re-derives cap dimensions on its
/// own, finer station grid rather than sampling this module's arrays, and has
/// to apply the same law to do it.
pub(crate) fn cap_taper(eta: &[f64], eta_lock: f64, tip_fraction: f64) -> Vec<f64> {
    let denom = (1.0 - eta_lock).max(1e-9);
    eta.iter()
        .map(|&e| {
            if e <= eta_lock {
                1.0
            } else {
                1.0 - (1.0 - tip_fraction) * (e - eta_lock) / denom
            }
        })
        .collect()
}

/// Size the wingbox directly from strength -- `size_wingbox`.
///
/// The load cases come from [`crate::loads::load_cases`]; the box is sized to
/// whichever produces the larger root bending moment.
#[allow(clippy::too_many_arguments)] // mirrors upstream's own signature
pub fn size_wingbox(
    wsg: &WingStructureGeometry,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
    rib_mat: &MaterialSpec,
) -> WingboxSizing {
    let n = cfg.spanwise_stations.max(0) as usize;
    let y = linspace(0.0, wsg.semi_span, n);
    let eta: Vec<f64> = y.iter().map(|&yi| yi / wsg.semi_span).collect();
    let chord: Vec<f64> = eta.iter().map(|&e| wsg.local_chord(e)).collect();

    let cases = loads::load_cases(req, cfg.additional_safety_factor);
    // Sized by whichever case has the larger |root moment|; for an elliptic
    // cantilever this is the largest |total_force_n|, but comparing moments is
    // robust to future load-model changes.
    let mut worst_idx = 0usize;
    let mut worst_m0 = -1.0;
    let mut case_moments: Vec<(Vec<f64>, Vec<f64>)> = Vec::with_capacity(cases.len());
    for (idx, case) in cases.iter().enumerate() {
        let q = loads::elliptic_distributed_load(&y, wsg.semi_span, case.total_force_n);
        let (_, m) = loads::cantilever_shear_moment(&y, &q);
        if m[0].abs() > worst_m0 {
            worst_m0 = m[0].abs();
            worst_idx = idx;
        }
        case_moments.push((q, m));
    }
    let worst_case: &LoadCase = &cases[worst_idx];
    let (q_sizing, m_sizing) = &case_moments[worst_idx];
    let (v_sizing, _) = loads::cantilever_shear_moment(&y, q_sizing);

    // Per-spar section height (n_spars x n), with a partial-span spar zeroed
    // outboard of the break so it carries no moment, shear or mass there.
    let mut h_all: Vec<Vec<f64>> = wsg
        .spar_fracs
        .iter()
        .map(|&f| eta.iter().map(|&e| wsg.spar_height(e, f)).collect())
        .collect();
    for (i, &full_span) in wsg.spar_full_span.iter().enumerate() {
        if !full_span {
            for j in 0..n {
                if eta[j] > wsg.break_eta + 1e-9 {
                    h_all[i][j] = 0.0;
                }
            }
        }
    }
    let h_sum: Vec<f64> = (0..n)
        .map(|j| {
            let s: f64 = h_all.iter().map(|h| h[j]).sum();
            if s > 1e-9 {
                s
            } else {
                1e-9
            }
        })
        .collect();
    let frac_moment_all: Vec<Vec<f64>> = h_all
        .iter()
        .map(|h| (0..n).map(|j| h[j] / h_sum[j]).collect())
        .collect();

    let tau_allow_web = web_mat.f_allow_pa / (2.0 * 3.0_f64.sqrt());
    let taper = cap_taper(&eta, cfg.cap_taper_eta_lock, cfg.cap_taper_tip_fraction);

    let m0 = m_sizing[0].abs();
    let v0 = v_sizing[0].abs();

    let mut spars: Vec<SparSizing> = Vec::with_capacity(wsg.spar_fracs.len());
    for (i, &frac_c) in wsg.spar_fracs.iter().enumerate() {
        let h_i = &h_all[i];
        let frac_m = &frac_moment_all[i];
        let h_eff: Vec<f64> = h_i.iter().map(|&h| h * 0.85).collect();

        // Root cap: MS = 0 by construction.
        let h_eff0 = h_eff[0].max(1e-6);
        let a_cap0 = (frac_m[0] * m0) / (cap_mat.f_allow_pa * h_eff0);
        let w_cap0 = (0.5 * chord[0]).min(h_i[0] * 0.6).max(1e-6);
        let t_cap0 = (a_cap0 / w_cap0).min(h_i[0] * 0.20);

        // Taper outboard; keep width >= thickness and thickness <= H_local/3.
        let t_cap: Vec<f64> = (0..n)
            .map(|j| (t_cap0 * taper[j]).min(h_i[j] / 3.0))
            .collect();
        let w_cap: Vec<f64> = (0..n).map(|j| (w_cap0 * taper[j]).max(t_cap[j])).collect();
        let a_cap: Vec<f64> = (0..n).map(|j| w_cap[j] * t_cap[j]).collect();

        // Web: uniform thickness sized from root shear.
        let t_web = cfg
            .t_web_min_m
            .max((frac_m[0] * v0) / (tau_allow_web * h_eff0));

        // Margin of safety at every station.
        let margin_of_safety: Vec<f64> = (0..n)
            .map(|j| {
                let m_adm = a_cap[j] * cap_mat.f_allow_pa * h_eff[j];
                let demand = (frac_m[j] * m_sizing[j]).abs();
                if demand > 1.0 {
                    m_adm / demand - 1.0
                } else {
                    f64::INFINITY
                }
            })
            .collect();

        spars.push(SparSizing {
            chord_fraction: frac_c,
            h: h_i.clone(),
            w_cap,
            t_cap,
            a_cap,
            t_web,
            frac_moment: frac_m.clone(),
            margin_of_safety,
        });
    }

    // Skin fixed at the configured minimum (no torsional shear-flow upsizing,
    // the same fidelity the analytical model uses).
    let t_skin = cfg.t_skin_min_m;

    // Rib spacing: Euler panel-buckling on the skin between the outermost two
    // spars.
    let frac_min = wsg.spar_fracs.iter().copied().fold(f64::INFINITY, f64::min);
    let frac_max = wsg
        .spar_fracs
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let b_box_root = (frac_max - frac_min) * chord[0];
    let h_root_mid = wsg.spar_height(0.0, 0.5 * (frac_min + frac_max));
    let nx = (m_sizing[0].abs() / h_root_mid.max(1e-6)) / b_box_root.max(1e-6);
    let sig_panel = (nx / t_skin).max(1e6);
    let l_rib = (cfg.rib_radius_of_gyration_m
        * (cfg.rib_buckling_coeff * std::f64::consts::PI.powi(2) * skin_mat.e_pa / sig_panel)
            .sqrt())
    .max(0.5);
    let num_ribs = match cfg.num_ribs_override {
        Some(value) => value,
        None => rib_count_from_max_spacing(wsg.semi_span, l_rib).max(10),
    };

    // Mass breakdown (semi-wing).
    let dy = gradient_unit(&y);
    let mut m_caps = 0.0;
    let mut m_webs = 0.0;
    for s in &spars {
        let caps: f64 = (0..n)
            .map(|j| 2.0 * s.a_cap[j] * cap_mat.rho_kg_m3 * dy[j])
            .sum();
        let webs: f64 = (0..n)
            .map(|j| s.t_web * s.h[j] * web_mat.rho_kg_m3 * dy[j])
            .sum();
        m_caps += caps;
        m_webs += webs;
    }
    let m_skin: f64 = (0..n)
        .map(|j| 2.0 * chord[j] * t_skin * skin_mat.rho_kg_m3 * dy[j])
        .sum();

    let xc_full = linspace(0.01, 0.99, 60);
    let mut m_ribs = 0.0;
    for &eta_r in &linspace(0.0, 1.0, num_ribs.max(2) as usize) {
        let c_r = wsg.local_chord(eta_r);
        let heights: Vec<f64> = xc_full
            .iter()
            .map(|&xc| {
                let (zu, zl) = wsg.airfoil_zu_zl(eta_r, xc);
                (zu - zl) * c_r
            })
            .collect();
        let x: Vec<f64> = xc_full.iter().map(|&xc| xc * c_r).collect();
        m_ribs += trapezoid(&heights, &x) * cfg.t_rib_m * rib_mat.rho_kg_m3;
    }

    let total = m_caps + m_webs + m_skin + m_ribs;

    WingboxSizing {
        y_stations: y,
        eta_stations: eta,
        chord,
        spar_fracs: wsg.spar_fracs.clone(),
        spars,
        t_skin,
        num_ribs,
        rib_spacing_m: l_rib,
        mass_breakdown_kg: MassBreakdown {
            spar_caps: m_caps,
            spar_webs: m_webs,
            skin: m_skin,
            ribs: m_ribs,
        },
        total_mass_kg: total,
        sizing_load_case: worst_case.name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linspace_pins_both_endpoints_and_spaces_evenly() {
        let v = linspace(0.0, 1.0, 5);
        assert_eq!(v, vec![0.0, 0.25, 0.5, 0.75, 1.0]);
        assert_eq!(linspace(2.0, 3.0, 1), vec![2.0]);
        assert!(linspace(0.0, 1.0, 0).is_empty());
    }

    #[test]
    fn gradient_unit_is_the_constant_spacing_for_a_uniform_grid() {
        // NumPy's gradient at unit spacing on a uniform ramp is the step at
        // every station, endpoints included.
        let g = gradient_unit(&[0.0, 2.0, 4.0, 6.0]);
        assert_eq!(g, vec![2.0, 2.0, 2.0, 2.0]);
        // Fewer than two points has no derivative to take.
        assert_eq!(gradient_unit(&[5.0]), vec![0.0]);
    }

    #[test]
    fn trapezoid_integrates_a_line_to_its_exact_area() {
        // Area under y = x from 0 to 1 is 1/2, exact for the trapezoidal rule
        // on a straight line at any sampling.
        let x = vec![0.0, 0.25, 0.5, 0.75, 1.0];
        let y = x.clone();
        assert!((trapezoid(&y, &x) - 0.5).abs() < 1e-15);
    }

    #[test]
    fn automatic_rib_count_uses_ceiling_panels_and_includes_both_end_ribs() {
        assert_eq!(rib_count_from_max_spacing(5.0, 2.0), 4);
        assert_eq!(rib_count_from_max_spacing(6.0, 2.0), 4);
    }

    #[test]
    fn cap_taper_is_flat_inboard_and_reaches_the_tip_fraction_at_the_tip() {
        let taper = cap_taper(&[0.0, 0.5, 1.0], 0.5, 0.3);
        assert_eq!(taper[0], 1.0);
        assert_eq!(taper[1], 1.0);
        assert!((taper[2] - 0.3).abs() < 1e-12);
    }
}
