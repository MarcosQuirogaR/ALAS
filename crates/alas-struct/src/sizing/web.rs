// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Uniform-gauge web construction from combined strength and elastic shear
//! buckling of the clear panel between cap flanges and consecutive ribs.

use super::{section::station_ei, SparSizing, WingboxSizing};
use crate::allowables::bending_allowable_pa;
use alas_config::materials::MaterialSpec;

/// Clear web depth between the inward faces of the two cap flanges, m.
/// The deck's overlapping web/cap junction remains included in mass and EI;
/// strength recovery here covers the unreinforced panel. The reinforced
/// cap/web junction needs joint shear-flow and connection-strength analysis;
/// its capacity is not proved by treating it as an isolated bare web edge.
pub(super) fn clear_height(spar: &SparSizing, station: usize) -> f64 {
    (spar.h[station] - 2.0 * spar.t_cap[station]).max(0.0)
}

/// Simply supported rectangular plate shear coefficient. Bruhn, *Analysis
/// and Design of Flight Vehicle Structures*, 1973, shear-web stability, and
/// Niu, *Airframe Structural Design*, 1988, shear panels. Isotropic plate
/// theory is an explicit proxy for a laminate web; no postbuckling is credited.
/// This checks pure shear buckling; compression/shear buckling interaction
/// and orthotropic panel stiffness require a laminate panel or shell analysis.
pub(super) fn buckling_coefficient(height: f64, pitch: f64) -> f64 {
    let aspect = pitch / height;
    if aspect >= 1.0 {
        5.34 + 4.0 / aspect.powi(2)
    } else {
        4.0 + 5.34 / aspect.powi(2)
    }
}

fn buckling_scale(height: f64, pitch: f64, material: &MaterialSpec) -> f64 {
    buckling_coefficient(height, pitch) * std::f64::consts::PI.powi(2) * material.e_pa
        / (12.0 * (1.0 - material.nu.powi(2)))
}

pub(super) fn utilization(
    sizing: &WingboxSizing,
    station: usize,
    spar_index: usize,
    curvature: f64,
    shear: f64,
    material: &MaterialSpec,
) -> f64 {
    let spar = &sizing.spars[spar_index];
    let height = clear_height(spar, station);
    if height <= 0.0 {
        return 0.0;
    }
    let area: f64 = sizing
        .spars
        .iter()
        .map(|spar| spar.t_web * clear_height(spar, station))
        .sum();
    let tau = shear.abs() / area;
    let sigma = material.e_pa * curvature.abs() * height / 2.0;
    // Uniform thin-web shear is the first-order box shear-flow assumption.
    // Normal and shear act at the same clear-web edge, with a von-Mises
    // interaction. A peak shear-flow analysis remains a shell-model check.
    let strength = sigma.hypot(3.0_f64.sqrt() * tau) / bending_allowable_pa(material);
    let critical = buckling_scale(height, sizing.installed_rib_spacing_m(), material)
        * (spar.t_web / height).powi(2);
    strength.max(tau / critical)
}

/// Enlarge a common uniform web gauge to satisfy all current clear panels.
/// The normal stress is recovered from current EI, and caps can be resized
/// next. t_strength follows sigma^2+3*tau^2=f^2. Combining tau=V/(t*sum(h))
/// with tau_cr=K*(t/h)^2 gives t_buckling^3=V*h^2/(K*sum(h)).
pub(super) fn ensure_thickness(
    sizing: &mut WingboxSizing,
    moments: &[f64],
    shears: &[f64],
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) -> bool {
    let current = sizing
        .spars
        .iter()
        .map(|spar| spar.t_web)
        .fold(0.0_f64, f64::max);
    let allowable = bending_allowable_pa(web);
    let mut required = current;
    for (station, moment) in moments.iter().enumerate() {
        let height_sum: f64 = sizing
            .spars
            .iter()
            .map(|spar| clear_height(spar, station))
            .sum();
        if height_sum <= 0.0 || shears[station].abs() <= 1.0 {
            continue;
        }
        let shear_per_gauge = shears[station].abs() / height_sum;
        let curvature = moment.abs() / station_ei(sizing, station, skin, web, cap);
        for spar in &sizing.spars {
            let height = clear_height(spar, station);
            if height <= 0.0 {
                continue;
            }
            let normal = web.e_pa * curvature * height / 2.0;
            let residual = (allowable - normal) * (allowable + normal);
            let strength_gauge = if residual > 0.0 {
                3.0_f64.sqrt() * shear_per_gauge / residual.sqrt()
            } else {
                f64::INFINITY
            };
            let buckling_gauge = (shear_per_gauge * height.powi(2)
                / buckling_scale(height, sizing.installed_rib_spacing_m(), web))
            .cbrt();
            required = required.max(strength_gauge).max(buckling_gauge);
        }
    }
    if !required.is_finite() || required <= current * (1.0 + super::MARGIN_NUMERICAL_ZERO) {
        return false;
    }
    // Choose the feasible side of the arithmetic boundary; this is floating
    // point closure, not a changed physical or acceptance tolerance.
    let installed = required * (1.0 + super::MARGIN_NUMERICAL_ZERO);
    for spar in &mut sizing.spars {
        spar.t_web = installed;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_panel_and_long_panel_recover_standard_shear_coefficients() {
        assert!((buckling_coefficient(1.0, 1.0) - 9.34).abs() < 1.0e-14);
        assert!((buckling_coefficient(1.0, 1.0e6) - 5.34).abs() < 5.0e-12);
        // Swap the plate dimensions: k/h^2 must preserve the same critical
        // stress, although the conventional k uses a different short edge.
        let a = buckling_coefficient(1.0, 0.5);
        let b = buckling_coefficient(0.5, 1.0) / 0.5_f64.powi(2);
        assert!((a - b).abs() < 1.0e-14);
    }
}
