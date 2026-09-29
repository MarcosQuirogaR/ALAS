// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mass-efficient stationwise compliance allocation for a fixed load snapshot.
//! With cap stiffness locally proportional to running cap mass, stationarity
//! of integral(m dy) + lambda*integral(|M|/EI dy) gives
//! EI = sqrt(lambda*|M|*dEI/dm). Baseline strength sections are lower bounds;
//! actual finite-thickness section geometry is evaluated during the dual
//! search. This is a bounded heuristic guided by the thin-section optimum,
//! not a proof of minimum mass for finite-thickness coupled sections. The
//! outer solve checks changed self-weight loads and allocation convergence.

use super::law::station_cap_dimensions;
use super::WingboxSizing;
use crate::analytical::StructuralAnalysisReport;
use alas_config::{materials::MaterialSpec, StructuresConfig};
use alas_geom::wing_structure::WingStructureGeometry;

fn cap_ei(height: f64, width: f64, thickness: f64, elastic_pa: f64) -> f64 {
    if height <= 2.0 * thickness {
        elastic_pa * width * height.powi(3) / 12.0
    } else {
        elastic_pa * 2.0 * width * thickness * ((height - thickness) / 2.0).powi(2)
    }
}

/// Returns whether a realizable cap area changed; never shrinks below the
/// original strength sizing. SI: EI N m^2, moment N m, running mass kg/m.
pub(super) fn allocate(
    wsg: &WingStructureGeometry,
    sizing: &mut WingboxSizing,
    floor: &WingboxSizing,
    response: &StructuralAnalysisReport,
    cap: &MaterialSpec,
    cfg: &StructuresConfig,
    target_slope: f64,
) -> bool {
    let count = floor.y_stations.len();
    let width_fractions: Vec<f64> = floor
        .spars
        .iter()
        .enumerate()
        .map(|(i, spar)| {
            let mut width = 0.5_f64.min(2.0 * spar.chord_fraction.min(1.0 - spar.chord_fraction));
            if i > 0 {
                width = width.min(spar.chord_fraction - floor.spars[i - 1].chord_fraction);
            }
            if i + 1 < floor.spars.len() {
                width = width.min(floor.spars[i + 1].chord_fraction - spar.chord_fraction);
            }
            width
        })
        .collect();
    let mut moment = vec![0.0_f64; count];
    let mut floor_ei = vec![0.0; count];
    let mut non_cap_ei = vec![0.0; count];
    let mut beta = vec![0.0; count];
    let mut cap_mass = vec![0.0; count];
    let cap_pair_stretch: Vec<_> = (0..floor.spars.len())
        .map(|i| super::arc_mass::spar_stretches(wsg, floor, i).0)
        .collect();
    for j in 0..count {
        moment[j] = response
            .load_cases
            .iter()
            .map(|case| case.moment_nm[j].abs())
            .fold(0.0_f64, f64::max);
        let current_cap_ei: f64 = sizing
            .spars
            .iter()
            .map(|spar| cap_ei(spar.h[j], spar.w_cap[j], spar.t_cap[j], cap.e_pa))
            .sum();
        non_cap_ei[j] = (response.ei_nm2[j] - current_cap_ei).max(0.0);
        floor_ei[j] = non_cap_ei[j]
            + floor
                .spars
                .iter()
                .map(|spar| cap_ei(spar.h[j], spar.w_cap[j], spar.t_cap[j], cap.e_pa))
                .sum::<f64>();
        let area: f64 = floor.spars.iter().map(|spar| spar.a_cap[j]).sum();
        cap_mass[j] = cap.rho_kg_m3
            * floor
                .spars
                .iter()
                .enumerate()
                .map(|(i, spar)| spar.a_cap[j] * cap_pair_stretch[i][j])
                .sum::<f64>();
        if area > 0.0 {
            beta[j] = cap.e_pa
                * floor
                    .spars
                    .iter()
                    .map(|spar| spar.a_cap[j] * spar.h[j].powi(2))
                    .sum::<f64>()
                / (2.0 * cap_mass[j]);
        }
    }
    let dimensions = |i: usize, j: usize, growth: f64| {
        let spar = &floor.spars[i];
        if growth <= 1.0 || spar.h[j] <= 0.0 {
            return (spar.w_cap[j], spar.t_cap[j]);
        }
        let area = spar.a_cap[j] * growth;
        let (width, thickness) =
            station_cap_dimensions(area, floor.chord[j], spar.h[j], cfg.t_skin_min_m);
        let width = width.min(width_fractions[i] * floor.chord[j]);
        let thickness = if width > 0.0 {
            thickness.max((area / width).min(spar.h[j] * 0.20))
        } else {
            0.0
        };
        if width * thickness < spar.a_cap[j] {
            (spar.w_cap[j], spar.t_cap[j])
        } else {
            (width, thickness)
        }
    };
    let scale = |j: usize, lambda: f64| {
        let desired = (lambda * moment[j] * beta[j]).sqrt().max(floor_ei[j]);
        if beta[j] * cap_mass[j] > 0.0 {
            (1.0 + (desired - floor_ei[j]) / (beta[j] * cap_mass[j])).max(1.0)
        } else {
            1.0
        }
    };
    let slope = |lambda: f64| {
        let mut previous = 0.0;
        let mut integral = 0.0;
        for j in 0..count {
            let growth = scale(j, lambda);
            let ei = non_cap_ei[j]
                + floor
                    .spars
                    .iter()
                    .enumerate()
                    .map(|(i, spar)| {
                        let (width, thickness) = dimensions(i, j, growth);
                        cap_ei(spar.h[j], width, thickness, cap.e_pa)
                    })
                    .sum::<f64>();
            let curvature = moment[j] / ei;
            if j > 0 {
                integral +=
                    0.5 * (floor.y_stations[j] - floor.y_stations[j - 1]) * (previous + curvature);
            }
            previous = curvature;
        }
        integral
    };
    let mut lower = 0.0;
    let mut upper = 1.0;
    for _ in 0..64 {
        let current = slope(upper);
        if current <= target_slope {
            break;
        }
        lower = upper;
        upper *= 4.0;
    }
    if slope(upper) <= target_slope {
        for _ in 0..48 {
            let middle = 0.5 * (lower + upper);
            if slope(middle) > target_slope {
                lower = middle;
            } else {
                upper = middle;
            }
        }
    }
    let mut changed = false;
    for (i, spar) in sizing.spars.iter_mut().enumerate() {
        for j in 0..count {
            let (width, thickness) = dimensions(i, j, scale(j, upper));
            let area = width * thickness;
            changed |= (area - spar.a_cap[j]).abs()
                > super::MARGIN_NUMERICAL_ZERO * spar.a_cap[j].max(1.0e-12);
            spar.w_cap[j] = width;
            spar.t_cap[j] = thickness;
            spar.a_cap[j] = area;
        }
    }
    changed
}
