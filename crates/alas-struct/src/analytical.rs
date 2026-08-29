// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/structural_analysis.py
// Reference: alas @ rust-port-baseline.

//! Analytical (no-NASTRAN) wingbox deformation, stress and frequency solver.
//!
//! [`analyze_structure`] runs the reference's validation-stage methods on the
//! sized wingbox: the Euler-Bernoulli spanwise deflection curve via the
//! unit-load theorem, the per-spar cap bending stress and margin of safety,
//! and the first cantilever bending-mode frequencies via the Rayleigh
//! quotient. These are always available -- no NASTRAN install is required.
//!
//! Unlike [`crate::sizing`] (which omits inertial relief for a conservative
//! strength check), this module includes relief -- the sized structure's own
//! distributed weight plus wing-mounted engine point masses -- since that is
//! what makes the analytical deflection track a real NASTRAN result.

use alas_config::materials::MaterialSpec;
use alas_config::{DesignRequirements, EngineConfig, MassModelConfig, StructuresConfig};
use alas_geom::wing_structure::WingStructureGeometry;

use crate::loads;
use crate::sizing::WingboxSizing;

/// `(beta*L, sigma)` for the first four cantilever bending modes -- the
/// classical clamped-free eigenvalues and their trial-shape coefficients.
const CANTILEVER_MODES: [(f64, f64); 4] = [
    (1.8751, 0.7341),
    (4.6941, 1.0185),
    (7.8548, 0.9992),
    (10.9955, 1.0000),
];

/// NumPy `trapezoid(y, x)`: the trapezoidal integral of `y` over `x`.
/// Duplicated from [`crate::sizing`]'s private helper for the reason that
/// module keeps its own copy -- it is not part of either module's public
/// surface.
fn trapezoid(y: &[f64], x: &[f64]) -> f64 {
    let mut acc = 0.0;
    for i in 0..y.len().saturating_sub(1) {
        acc += (x[i + 1] - x[i]) * (y[i + 1] + y[i]) / 2.0;
    }
    acc
}

/// Per-spar bending stress and margin of safety at every station.
#[derive(Debug, Clone, PartialEq)]
pub struct SparStressResult {
    /// The spar's chordwise position, as a fraction of local chord.
    pub chord_fraction: f64,
    /// Cap bending stress at each station, Pa.
    pub stress_pa: Vec<f64>,
    /// Margin of safety at each station -- `+inf` where the demand is below
    /// 1 N.m.
    pub margin_of_safety: Vec<f64>,
}

/// One load case's spanwise response.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadCaseResult {
    /// The load-case name.
    pub name: &'static str,
    /// The signed limit load factor.
    pub load_factor: f64,
    /// Spanwise stations, m.
    pub y: Vec<f64>,
    /// Net distributed load (aero minus inertial relief), N/m.
    pub q_net: Vec<f64>,
    /// Shear, N.
    pub shear_n: Vec<f64>,
    /// Bending moment, N.m.
    pub moment_nm: Vec<f64>,
    /// Euler-Bernoulli spanwise deflection curve, m.
    pub deflection_m: Vec<f64>,
    /// Tip deflection, m.
    pub tip_deflection_m: f64,
    /// Per-spar stress.
    pub spar_stress: Vec<SparStressResult>,
}

/// The natural-frequency estimate.
#[derive(Debug, Clone, PartialEq)]
pub struct ModalResult {
    /// Bending-mode natural frequencies, Hz.
    pub frequencies_hz: Vec<f64>,
    /// Normalized (peak = 1) mode shape per mode, on the same `y` grid.
    pub mode_shapes: Vec<Vec<f64>>,
}

/// The full analytical structural report.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralAnalysisReport {
    /// Spanwise stations, m.
    pub y: Vec<f64>,
    /// Static bending stiffness `EI(y)`, N.m^2 -- independent of load case.
    pub ei_nm2: Vec<f64>,
    /// One result per load case, in the load-case order.
    pub load_cases: Vec<LoadCaseResult>,
    /// The modal estimate.
    pub modal: ModalResult,
}

/// Area moment of inertia of a symmetric I-section about its own centroid --
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

/// Distributed mass per unit span, kg/m -- `_mass_per_length`.
fn mass_per_length(
    sizing: &WingboxSizing,
    cap_rho: f64,
    web_rho: f64,
    skin_rho: f64,
    include_ribs: bool,
) -> Vec<f64> {
    let n = sizing.chord.len();
    let mut m_y: Vec<f64> = (0..n)
        .map(|j| 2.0 * sizing.chord[j] * sizing.t_skin * skin_rho)
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

/// Combined bending stiffness `EI(y)`, N.m^2 -- `_EI_curve`.
fn ei_curve(sizing: &WingboxSizing, cap_mat: &MaterialSpec, skin_mat: &MaterialSpec) -> Vec<f64> {
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

/// Spanwise deflection via the unit-load (virtual work) theorem at every
/// station -- `_deflection_curve`, O(N^2).
fn deflection_curve(y: &[f64], m: &[f64], ei: &[f64]) -> Vec<f64> {
    let n = y.len();
    let mut delta = vec![0.0; n];
    let integrand: Vec<f64> = (0..n).map(|j| m[j] / ei[j]).collect();
    for k in 1..n {
        let s_k = y[k];
        let product: Vec<f64> = (0..=k)
            .map(|j| {
                let m_bar = if y[j] <= s_k { s_k - y[j] } else { 0.0 };
                integrand[j] * m_bar
            })
            .collect();
        delta[k] = trapezoid(&product, &y[..=k]);
    }
    delta
}

/// The first `n_modes` cantilever bending-mode frequencies and shapes via the
/// Rayleigh quotient with classical trial shapes -- `_rayleigh_frequencies`.
fn rayleigh_frequencies(
    y: &[f64],
    ei: &[f64],
    m_y: &[f64],
    n_modes: i64,
) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = y.len();
    let length = y[n - 1];
    let n_modes = (n_modes.max(0) as usize).min(CANTILEVER_MODES.len());
    let mut freqs = vec![0.0; n_modes];
    let mut shapes: Vec<Vec<f64>> = Vec::with_capacity(n_modes);
    for (i, &(beta_l, sigma)) in CANTILEVER_MODES.iter().take(n_modes).enumerate() {
        let beta = beta_l / length.max(1e-9);
        let phi: Vec<f64> = y
            .iter()
            .map(|&yj| {
                let by = beta * yj;
                by.cosh() - by.cos() - sigma * (by.sinh() - by.sin())
            })
            .collect();
        let phi_pp: Vec<f64> = y
            .iter()
            .map(|&yj| {
                let by = beta * yj;
                beta.powi(2) * (by.cosh() + by.cos() - sigma * (by.sinh() + by.sin()))
            })
            .collect();
        let num_terms: Vec<f64> = (0..n).map(|j| ei[j] * phi_pp[j].powi(2)).collect();
        let den_terms: Vec<f64> = (0..n).map(|j| m_y[j] * phi[j].powi(2)).collect();
        let num = trapezoid(&num_terms, y);
        let den = trapezoid(&den_terms, y);
        freqs[i] = if den > 1e-30 {
            (num / den).sqrt() / (2.0 * std::f64::consts::PI)
        } else {
            0.0
        };
        let max_abs = phi.iter().fold(0.0_f64, |acc, &v| acc.max(v.abs()));
        let norm = if max_abs == 0.0 { 1.0 } else { max_abs };
        shapes.push(phi.iter().map(|&v| v / norm).collect());
    }
    (freqs, shapes)
}

/// Analyze the sized wingbox: deflection, stress and modal response --
/// `analyze_structure`.
#[allow(clippy::too_many_arguments)] // mirrors upstream's own signature
pub fn analyze_structure(
    wsg: &WingStructureGeometry,
    sizing: &WingboxSizing,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    engine_cfg: &EngineConfig,
    mass_cfg: &MassModelConfig,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
) -> StructuralAnalysisReport {
    analyze_structure_with_rib_mass(
        wsg, sizing, cfg, req, engine_cfg, mass_cfg, skin_mat, web_mat, cap_mat, true,
    )
}

/// Analyze the sized wingbox using the frozen reference mass convention.
///
/// The historical parity path omitted the explicitly sized rib mass from
/// analytical inertial relief and modal mass. It remains available solely for
/// replaying the old fixture; product callers should use [`analyze_structure`].
#[allow(clippy::too_many_arguments)]
pub fn analyze_structure_reference_compatibility(
    wsg: &WingStructureGeometry,
    sizing: &WingboxSizing,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    engine_cfg: &EngineConfig,
    mass_cfg: &MassModelConfig,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
) -> StructuralAnalysisReport {
    analyze_structure_with_rib_mass(
        wsg, sizing, cfg, req, engine_cfg, mass_cfg, skin_mat, web_mat, cap_mat, false,
    )
}

// The helper keeps the reference and product analyses on one explicit path;
// each argument is a distinct geometry, material, or load-model input.
#[allow(clippy::too_many_arguments)]
fn analyze_structure_with_rib_mass(
    wsg: &WingStructureGeometry,
    sizing: &WingboxSizing,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    engine_cfg: &EngineConfig,
    mass_cfg: &MassModelConfig,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
    include_ribs: bool,
) -> StructuralAnalysisReport {
    let y = &sizing.y_stations;
    let n = y.len();
    let semi_span = wsg.semi_span;
    let g = req.gravity_m_s2;

    let ei = ei_curve(sizing, cap_mat, skin_mat);
    let m_y = mass_per_length(
        sizing,
        cap_mat.rho_kg_m3,
        web_mat.rho_kg_m3,
        skin_mat.rho_kg_m3,
        include_ribs,
    );
    let engine_loads = loads::engine_point_loads_n(engine_cfg, mass_cfg, req);

    let m_bar_tip: Vec<f64> = y.iter().map(|&yj| semi_span - yj).collect();

    let mut load_cases: Vec<LoadCaseResult> = Vec::new();
    for case in loads::load_cases(req, cfg.additional_safety_factor) {
        let sign = if case.total_force_n >= 0.0 { 1.0 } else { -1.0 };
        let l_total = case.total_force_n.abs();
        let n_factor = case.load_factor.abs();

        let q_aero = loads::elliptic_distributed_load(y, semi_span, l_total);
        let q_net: Vec<f64> = (0..n).map(|j| q_aero[j] - n_factor * g * m_y[j]).collect();
        let (v, mut m) = loads::cantilever_shear_moment(y, &q_net);

        for &(y_eng, m_eng) in &engine_loads {
            let f_eng = n_factor * m_eng * g;
            for j in 0..n {
                if y[j] <= y_eng {
                    m[j] += -f_eng * (y_eng - y[j]);
                }
            }
        }

        let tip_terms: Vec<f64> = (0..n).map(|j| m[j] * m_bar_tip[j] / ei[j]).collect();
        let tip_deflection_m = trapezoid(&tip_terms, y) * sign;
        let defl_curve = deflection_curve(y, &m, &ei);
        let deflection_m: Vec<f64> = defl_curve.iter().map(|&d| d * sign).collect();
        let m_signed: Vec<f64> = m.iter().map(|&mj| mj * sign).collect();

        let mut spar_stress: Vec<SparStressResult> = Vec::with_capacity(sizing.spars.len());
        for s in &sizing.spars {
            let stress_pa: Vec<f64> = (0..n)
                .map(|j| {
                    let h_eff = s.h[j] * 0.85;
                    (s.frac_moment[j] * m_signed[j]).abs() / (s.a_cap[j] * h_eff).max(1e-12)
                })
                .collect();
            let margin_of_safety: Vec<f64> = (0..n)
                .map(|j| {
                    if (s.frac_moment[j] * m_signed[j]).abs() > 1.0 {
                        cap_mat.f_allow_pa / stress_pa[j].max(1e-9) - 1.0
                    } else {
                        f64::INFINITY
                    }
                })
                .collect();
            spar_stress.push(SparStressResult {
                chord_fraction: s.chord_fraction,
                stress_pa,
                margin_of_safety,
            });
        }

        load_cases.push(LoadCaseResult {
            name: case.name,
            load_factor: case.load_factor,
            y: y.clone(),
            q_net: q_net.iter().map(|&qn| qn * sign).collect(),
            shear_n: v.iter().map(|&vj| vj * sign).collect(),
            moment_nm: m_signed,
            deflection_m,
            tip_deflection_m,
            spar_stress,
        });
    }

    // Modal: engine point masses smeared onto the nearest station.
    let mut m_y_modal = m_y.clone();
    if n > 1 {
        let dy_uniform = y[1] - y[0];
        for &(y_eng, m_eng) in &engine_loads {
            let raw = (y_eng / dy_uniform.max(1e-9)).round_ties_even();
            let idx = (raw as i64).clamp(0, n as i64 - 1) as usize;
            m_y_modal[idx] += m_eng / dy_uniform.max(1e-9);
        }
    }
    let (frequencies_hz, mode_shapes) = rayleigh_frequencies(y, &ei, &m_y_modal, cfg.n_modes);

    StructuralAnalysisReport {
        y: y.clone(),
        ei_nm2: ei,
        load_cases,
        modal: ModalResult {
            frequencies_hz,
            mode_shapes,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sizing::MassBreakdown;

    #[test]
    fn rib_mass_is_conserved_in_the_distributed_analytical_density() {
        let sizing = WingboxSizing {
            y_stations: vec![0.0, 5.0],
            eta_stations: vec![0.0, 1.0],
            chord: vec![2.0, 2.0],
            spar_fracs: Vec::new(),
            spars: Vec::new(),
            t_skin: 0.1,
            num_ribs: 6,
            rib_spacing_m: 1.0,
            mass_breakdown_kg: MassBreakdown {
                spar_caps: 0.0,
                spar_webs: 0.0,
                skin: 1.0,
                ribs: 10.0,
            },
            total_mass_kg: 11.0,
            sizing_load_case: "probe",
        };

        let without_ribs = mass_per_length(&sizing, 1.0, 1.0, 1.0, false);
        let with_ribs = mass_per_length(&sizing, 1.0, 1.0, 1.0, true);
        for (&with, &without) in with_ribs.iter().zip(&without_ribs) {
            assert!((with - without - 2.0).abs() < 1e-12);
        }
    }
}
