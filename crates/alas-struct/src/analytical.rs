// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Analytical (no-NASTRAN) wingbox deformation, stress and frequency solver.
//!
//! [`analyze_structure`] runs the reference's validation-stage methods on the
//! sized wingbox: the Euler-Bernoulli spanwise deflection curve via the
//! unit-load theorem, the per-spar cap bending stress and margin of safety,
//! and the first cantilever bending-mode frequencies via the Rayleigh
//! quotient. These are always available, no NASTRAN install is required.
//!
//! The relief this module applies - the sized structure's own distributed
//! weight plus wing-mounted engine point masses - is built from the same
//! [`crate::loads`] primitives [`crate::sizing`] applies, so the deflection is
//! solved under the load model the box was sized under. This module adds the
//! engine point masses the sizing entry point is not given. The original
//! [`analyze_structure`] entry point omits integral fuel; product sizing and
//! acceptance use [`analyze_structure_with_wing_carried_mass`] so response and
//! sizing share one explicit design-gross-mass and fuel state.

use alas_config::materials::MaterialSpec;
use alas_config::{DesignRequirements, EngineConfig, MassModelConfig, StructuresConfig};
use alas_geom::wing_structure::WingStructureGeometry;

use crate::loads;
use crate::sizing::{trapezoid, WingboxSizing};

mod section;
use section::{ei_curve, mass_per_length};

/// `(beta*L, sigma)` for the first four cantilever bending modes: the
/// classical clamped-free eigenvalues and their trial-shape coefficients.
const CANTILEVER_MODES: [(f64, f64); 4] = [
    (1.8751, 0.7341),
    (4.6941, 1.0185),
    (7.8548, 0.9992),
    (10.9955, 1.0000),
];

/// Per-spar bending stress and margin of safety at every station.
#[derive(Debug, Clone, PartialEq)]
pub struct SparStressResult {
    /// The spar's chordwise position, as a fraction of local chord.
    pub chord_fraction: f64,
    /// Cap bending stress at each station, Pa.
    pub stress_pa: Vec<f64>,
    /// Margin of safety at each station: `+inf` where the demand is below
    /// 1 N.m.
    pub margin_of_safety: Vec<f64>,
}

/// One load case's spanwise response.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadCaseResult {
    /// The load-case name.
    pub name: &'static str,
    /// Signed load factor: ultimate for manoeuvres, 1 for level flight.
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
    /// Static bending stiffness `EI(y)`, N.m^2: independent of load case.
    pub ei_nm2: Vec<f64>,
    /// One result per load case, in the load-case order.
    pub load_cases: Vec<LoadCaseResult>,
    /// The modal estimate.
    pub modal: ModalResult,
}

/// Spanwise deflection via the unit-load (virtual work) theorem at every
/// station: `_deflection_curve`, O(N) using accumulated curvature moments.
fn deflection_curve(y: &[f64], m: &[f64], ei: &[f64]) -> Vec<f64> {
    let n = y.len();
    let mut delta = vec![0.0; n];
    // Accumulating the zeroth and first moments of curvature is exactly the
    // same trapezoidal unit-load quadrature as the former O(N^2) loop.
    let mut integral_curvature = 0.0;
    let mut integral_first_moment = 0.0;
    for k in 1..n {
        let dy = y[k] - y[k - 1];
        let left = m[k - 1] / ei[k - 1];
        let right = m[k] / ei[k];
        integral_curvature += 0.5 * dy * (left + right);
        integral_first_moment += 0.5 * dy * (y[k - 1] * left + y[k] * right);
        delta[k] = y[k] * integral_curvature - integral_first_moment;
    }
    delta
}

/// The first `n_modes` cantilever bending-mode frequencies and shapes via the
/// Rayleigh quotient with classical trial shapes: `_rayleigh_frequencies`.
fn rayleigh_frequencies(
    y: &[f64],
    ei: &[f64],
    m_y: &[f64],
    n_modes: i64,
) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = y.len();
    let length = y.last().copied().unwrap_or(0.0);
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

/// Analyze the sized wingbox: deflection, stress and modal response:
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
    let (front, rear) = crate::sizing::box_chord_band(wsg);
    analyze_structure_with_rib_mass(
        wsg,
        sizing,
        cfg,
        req,
        engine_cfg,
        mass_cfg,
        skin_mat,
        web_mat,
        cap_mat,
        true,
        (rear - front).max(0.0),
        &[],
        None,
        None,
    )
}

/// Analyze the sized wingbox using the frozen reference mass convention.
///
/// The historical parity path omitted the explicitly sized rib mass from
/// analytical inertial relief and modal mass. It remains available solely for
/// replaying the old fixture; product callers should use [`analyze_structure`].
#[allow(clippy::too_many_arguments)] // mirrors upstream's own signature
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
        wsg,
        sizing,
        cfg,
        req,
        engine_cfg,
        mass_cfg,
        skin_mat,
        web_mat,
        cap_mat,
        false,
        1.0,
        &[],
        None,
        None,
    )
}

/// Analyze the *same design load state* used to size a wingbox.
///
/// `integral_fuel_kg_m` is the nonnegative running fuel mass on the sizing
/// stations, kg/m; `wing_mounted_point_masses` contains only the modelled
/// semi-wing's `(station m, mass kg)` pairs. Unlike [`analyze_structure`],
/// this entry point does not silently remove the design state's fuel relief.
/// Use identical requirements, fuel and point masses for sizing and analysis.
/// Invalid arrays produce nonfinite results for a fail-closed assessment.
#[allow(clippy::too_many_arguments)]
pub fn analyze_structure_with_wing_carried_mass(
    wsg: &WingStructureGeometry,
    sizing: &WingboxSizing,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    engine_cfg: &EngineConfig,
    mass_cfg: &MassModelConfig,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
    integral_fuel_kg_m: &[f64],
    wing_mounted_point_masses: &[(f64, f64)],
) -> StructuralAnalysisReport {
    analyze_structure_with_running_mass(
        wsg,
        sizing,
        cfg,
        req,
        engine_cfg,
        mass_cfg,
        skin_mat,
        web_mat,
        cap_mat,
        integral_fuel_kg_m,
        wing_mounted_point_masses,
        None,
    )
}

/// Internal product path: an explicit structural running mass keeps swept
/// cap/web length in both the mass budget and every inertial/modal response.
#[allow(clippy::too_many_arguments)]
pub(crate) fn analyze_structure_with_running_mass(
    wsg: &WingStructureGeometry,
    sizing: &WingboxSizing,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    engine_cfg: &EngineConfig,
    mass_cfg: &MassModelConfig,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
    integral_fuel_kg_m: &[f64],
    wing_mounted_point_masses: &[(f64, f64)],
    structural_running_mass_kg_m: Option<&[f64]>,
) -> StructuralAnalysisReport {
    let count = sizing.y_stations.len();
    let arrays_valid = count >= 2
        && sizing.chord.len() == count
        && !sizing.spars.is_empty()
        && sizing.spars.iter().all(|spar| {
            [
                &spar.h,
                &spar.w_cap,
                &spar.t_cap,
                &spar.a_cap,
                &spar.frac_moment,
            ]
            .iter()
            .all(|values| values.len() == count)
        });
    if !arrays_valid
        || !wsg.semi_span.is_finite()
        || wsg.semi_span <= 0.0
        || integral_fuel_kg_m.len() != count
        || structural_running_mass_kg_m.is_some_and(|values| {
            values.len() != count || values.iter().any(|m| !m.is_finite() || *m < 0.0)
        })
        || wing_mounted_point_masses.iter().any(|(y, mass)| {
            !y.is_finite() || *y < 0.0 || *y > wsg.semi_span || !mass.is_finite() || *mass < 0.0
        })
    {
        return StructuralAnalysisReport {
            y: sizing.y_stations.clone(),
            ei_nm2: vec![f64::NAN; count],
            load_cases: Vec::new(),
            modal: ModalResult {
                frequencies_hz: Vec::new(),
                mode_shapes: Vec::new(),
            },
        };
    }
    let (front, rear) = crate::sizing::box_chord_band(wsg);
    analyze_structure_with_rib_mass(
        wsg,
        sizing,
        cfg,
        req,
        engine_cfg,
        mass_cfg,
        skin_mat,
        web_mat,
        cap_mat,
        true,
        rear - front,
        integral_fuel_kg_m,
        Some(wing_mounted_point_masses),
        structural_running_mass_kg_m,
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
    skin_cover_fraction: f64,
    integral_fuel_kg_m: &[f64],
    wing_mounted_point_masses: Option<&[(f64, f64)]>,
    structural_running_mass_kg_m: Option<&[f64]>,
) -> StructuralAnalysisReport {
    let y = &sizing.y_stations;
    let n = y.len();
    let semi_span = wsg.semi_span;
    let g = req.gravity_m_s2;

    let ei = ei_curve(sizing, cap_mat, skin_mat);
    let mut m_y = structural_running_mass_kg_m.map_or_else(
        || {
            mass_per_length(
                sizing,
                cap_mat.rho_kg_m3,
                web_mat.rho_kg_m3,
                skin_mat.rho_kg_m3,
                include_ribs,
                skin_cover_fraction,
            )
        },
        <[f64]>::to_vec,
    );
    if !integral_fuel_kg_m.is_empty() {
        if integral_fuel_kg_m.len() == n
            && integral_fuel_kg_m
                .iter()
                .all(|m| m.is_finite() && *m >= 0.0)
        {
            for (mass, fuel) in m_y.iter_mut().zip(integral_fuel_kg_m) {
                *mass += fuel;
            }
        } else {
            m_y.fill(f64::NAN);
        }
    }
    let engine_loads = wing_mounted_point_masses.map_or_else(
        || loads::engine_point_loads_n(engine_cfg, mass_cfg, req),
        <[(f64, f64)]>::to_vec,
    );

    let m_bar_tip: Vec<f64> = y.iter().map(|&yj| semi_span - yj).collect();

    let mut load_cases: Vec<LoadCaseResult> = Vec::new();
    for case in loads::load_cases(req, cfg.additional_safety_factor) {
        let sign = if case.total_force_n >= 0.0 { 1.0 } else { -1.0 };
        let l_total = case.total_force_n.abs();
        let n_factor = case.load_factor.abs();

        // Magnitudes throughout: `q_aero`, `n_factor` and `m` are all positive
        // here and `sign` is restored below, which is the sign convention
        // `loads::net_distributed_load` documents.
        let q_aero = loads::elliptic_distributed_load(y, semi_span, l_total);
        let q_net = loads::net_distributed_load(&q_aero, n_factor, g, &m_y);
        let (v, mut m) = loads::cantilever_shear_moment(y, &q_net);
        loads::apply_point_mass_relief(y, &mut m, n_factor, g, &engine_loads);

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
    fn an_empty_station_grid_has_zero_frequencies_rather_than_a_panic() {
        let (frequencies, shapes) = rayleigh_frequencies(&[], &[], &[], 2);
        assert_eq!(frequencies, [0.0, 0.0]);
        assert!(shapes.iter().all(Vec::is_empty));
    }

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
            composite_declaration: None,
        };

        let without_ribs = mass_per_length(&sizing, 1.0, 1.0, 1.0, false, 1.0);
        let with_ribs = mass_per_length(&sizing, 1.0, 1.0, 1.0, true, 1.0);
        for (&with, &without) in with_ribs.iter().zip(&without_ribs) {
            assert!((with - without - 2.0).abs() < 1e-12);
        }
    }
}
