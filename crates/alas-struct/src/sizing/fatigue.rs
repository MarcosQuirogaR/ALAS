// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lower-cover fatigue area.
//!
//! The lower box cover carries tension and is damage critical, so its stress
//! in 1 g level flight is limited by the design service life rather than by
//! the ultimate strength of the material (Torenbeek, *Advanced Aircraft
//! Design*, 2013, section 11.7.1, book p. 350: 75 MPa for riveted short-range
//! panels, 110 MPa for integrally machined long-range ones). Only the lower
//! cover is raised; the upper, compression cover keeps its strength and
//! buckling sizing.
//!
//! The strength-sized section is symmetric. Extra area `lambda` on the lower
//! flanges moves the neutral axis down and raises the section inertia; the
//! 1 g tensile stress at the lower fibre follows from the transformed
//! (modulus-weighted) section,
//!
//! ```text
//! EA' = EA + dEA,   z = S / EA',   EI' = EI + Q - S^2 / EA'
//! sigma = E M (h/2 - z) / EI'
//! ```
//!
//! with `dEA`, `S` and `Q` the zeroth, first and second moments of the added
//! area about the mid-plane (measured downwards) and `EA`, `EI` those of the
//! symmetric section. Moving the neutral axis towards the lower cover lowers
//! the compression stress of the upper cover less than the inertia gain does,
//! so the upper cover and the webs stay within the strengths they were sized
//! to, and the strength margins of the symmetric section remain valid (they are
//! conservative for the section with the added area). The added area is
//! distributed over the spars in proportion to their depth.

use super::section::station_ei;
use super::WingboxSizing;
use alas_config::materials::MaterialSpec;

/// The 1 g tensile demand of the lower cover, for the fatigue limit.
pub(super) struct FatigueDemand<'a> {
    /// Relieved 1 g level-flight bending moment at each station, N m.
    pub(super) moment_1g_nm: &'a [f64],
    /// Largest 1 g tensile stress of the lower cover, Pa.
    pub(super) limit_pa: f64,
}

/// Axial stiffness `sum(E A)` of the symmetric section at a station, N.
#[cfg(test)]
pub(super) fn station_ea_for_tests(
    sizing: &WingboxSizing,
    station: usize,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) -> f64 {
    station_ea(sizing, station, skin, web, cap)
}

/// Axial stiffness `sum(E A)` of the symmetric section at a station, N.
fn station_ea(
    sizing: &WingboxSizing,
    station: usize,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) -> f64 {
    let active: Vec<_> = sizing
        .spars
        .iter()
        .filter(|spar| spar.h[station] > 0.0)
        .collect();
    let caps: f64 = active
        .iter()
        .map(|spar| 2.0 * cap.e_pa * spar.a_cap[station])
        .sum();
    let webs: f64 = active
        .iter()
        .map(|spar| web.e_pa * spar.t_web[station] * spar.h[station])
        .sum();
    let covers: f64 = active
        .windows(2)
        .map(|pair| {
            let width = (pair[1].chord_fraction - pair[0].chord_fraction) * sizing.chord[station];
            2.0 * skin.e_pa * sizing.t_skin * width
        })
        .sum();
    caps + webs + covers
}

/// 1 g tensile stress of the lower fibre of the deepest spar with `lambda`
/// square metres of added lower-flange area, Pa.
fn lower_fibre_stress_pa(
    sizing: &WingboxSizing,
    station: usize,
    moment_nm: f64,
    lambda_m2: f64,
    symmetric: (f64, f64),
    modulus_pa: f64,
    cap: &MaterialSpec,
) -> f64 {
    let (ea, ei) = symmetric;
    let depth_sum: f64 = sizing.spars.iter().map(|s| s.h[station]).sum();
    let (mut s_first, mut q_second) = (0.0, 0.0);
    for spar in sizing.spars.iter().filter(|spar| spar.h[station] > 0.0) {
        let added = lambda_m2 * spar.h[station] / depth_sum * cap.e_pa;
        let arm = 0.5 * (spar.h[station] - spar.t_cap[station]);
        s_first += added * arm;
        q_second += added * arm * arm;
    }
    let added_ea = lambda_m2 * cap.e_pa;
    let total_ea = ea + added_ea;
    let shift = s_first / total_ea;
    let inertia = ei + q_second - s_first * s_first / total_ea;
    let half_depth = sizing
        .spars
        .iter()
        .map(|spar| 0.5 * spar.h[station])
        .fold(0.0_f64, f64::max);
    modulus_pa * moment_nm.abs() * (half_depth - shift) / inertia
}

/// Add lower-flange area at every station whose 1 g tensile stress exceeds the
/// fatigue limit, the smallest area that brings it to the limit.
pub(super) fn size_lower_cover(
    sizing: &mut WingboxSizing,
    demand: &FatigueDemand<'_>,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) {
    let modulus = skin.e_pa.max(cap.e_pa);
    for station in 0..sizing.y_stations.len() {
        for spar in &mut sizing.spars {
            spar.lower_extra_area[station] = 0.0;
        }
        let moment = demand.moment_1g_nm.get(station).copied().unwrap_or(0.0);
        if moment.abs() <= 1.0 {
            continue;
        }
        let symmetric = (
            station_ea(sizing, station, skin, web, cap),
            station_ei(sizing, station, skin, web, cap),
        );
        let stress = |sizing: &WingboxSizing, lambda: f64| {
            lower_fibre_stress_pa(sizing, station, moment, lambda, symmetric, modulus, cap)
        };
        if stress(sizing, 0.0) <= demand.limit_pa {
            continue;
        }
        // Bracket the area by doubling from the existing flange area, then
        // bisect: the stress falls monotonically with the added area.
        let flange: f64 = sizing.spars.iter().map(|spar| spar.a_cap[station]).sum();
        let mut upper = flange.max(1.0e-9);
        let mut lower = 0.0;
        for _ in 0..80 {
            if stress(sizing, upper) <= demand.limit_pa {
                break;
            }
            lower = upper;
            upper *= 2.0;
        }
        for _ in 0..100 {
            if upper - lower <= 1.0e-12 * upper {
                break;
            }
            let middle = 0.5 * (lower + upper);
            if stress(sizing, middle) <= demand.limit_pa {
                upper = middle;
            } else {
                lower = middle;
            }
        }
        let depth_sum: f64 = sizing.spars.iter().map(|spar| spar.h[station]).sum();
        for spar in &mut sizing.spars {
            spar.lower_extra_area[station] = upper * spar.h[station] / depth_sum;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::materials;

    fn rectangular_box() -> WingboxSizing {
        use super::super::{MassBreakdown, SparSizing};
        let spar = |fraction| SparSizing {
            chord_fraction: fraction,
            h: vec![1.0; 2],
            w_cap: vec![0.2; 2],
            t_cap: vec![0.01; 2],
            a_cap: vec![0.002; 2],
            t_web: vec![0.003; 2],
            lower_extra_area: vec![0.0; 2],
            frac_moment: vec![0.5; 2],
            margin_of_safety: vec![0.0; 2],
        };
        WingboxSizing {
            y_stations: vec![0.0, 10.0],
            axis_cos: Vec::new(),
            eta_stations: vec![0.0, 1.0],
            chord: vec![4.0; 2],
            spar_fracs: vec![0.25, 0.75],
            spars: vec![spar(0.25), spar(0.75)],
            t_skin: 0.005,
            num_ribs: 11,
            rib_spacing_m: 1.0,
            mass_breakdown_kg: MassBreakdown {
                spar_caps: 0.0,
                spar_webs: 0.0,
                skin: 0.0,
                ribs: 0.0,
            },
            total_mass_kg: 0.0,
            sizing_load_case: "pull-up",
            composite_declaration: None,
        }
    }

    fn section(extra: f64) -> (WingboxSizing, f64) {
        let mut sizing = rectangular_box();
        for spar in &mut sizing.spars {
            spar.lower_extra_area = vec![extra; 2];
        }
        (sizing, extra)
    }

    #[test]
    fn added_lower_area_lowers_the_lower_fibre_stress_monotonically() {
        let al = materials::get("Al 7075-T6").unwrap();
        let (sizing, _) = section(0.0);
        let symmetric = (
            station_ea(&sizing, 0, al, al, al),
            station_ei(&sizing, 0, al, al, al),
        );
        let mut previous = f64::INFINITY;
        for lambda in [0.0, 0.001, 0.004, 0.016, 0.064] {
            let stress = lower_fibre_stress_pa(&sizing, 0, 1.0e6, lambda, symmetric, al.e_pa, al);
            assert!(stress < previous, "{lambda}: {stress}");
            previous = stress;
        }
    }

    #[test]
    fn without_added_area_the_stress_is_the_symmetric_section_stress() {
        let al = materials::get("Al 7075-T6").unwrap();
        let (sizing, _) = section(0.0);
        let ea = station_ea(&sizing, 0, al, al, al);
        let ei = station_ei(&sizing, 0, al, al, al);
        let stress = lower_fibre_stress_pa(&sizing, 0, 2.0e6, 0.0, (ea, ei), al.e_pa, al);
        assert!((stress - al.e_pa * 2.0e6 * 0.5 / ei).abs() < 1.0e-6 * stress);
    }

    #[test]
    fn the_sized_area_lands_on_the_limit_and_leaves_symmetric_stations_alone() {
        let al = materials::get("Al 7075-T6").unwrap();
        let (mut sizing, _) = section(0.0);
        let ei = station_ei(&sizing, 0, al, al, al);
        let moment = 3.0e6;
        let base = al.e_pa * moment * 0.5 / ei;
        let limit = 0.8 * base;
        let demand = FatigueDemand {
            moment_1g_nm: &[moment, 0.0],
            limit_pa: limit,
        };
        size_lower_cover(&mut sizing, &demand, al, al, al);
        let added: f64 = sizing.spars.iter().map(|s| s.lower_extra_area[0]).sum();
        assert!(added > 0.0);
        assert_eq!(sizing.spars[0].lower_extra_area[1], 0.0);
        let symmetric = (
            station_ea(&sizing, 0, al, al, al),
            station_ei(&sizing, 0, al, al, al),
        );
        let stress = lower_fibre_stress_pa(&sizing, 0, moment, added, symmetric, al.e_pa, al);
        assert!((stress / limit - 1.0).abs() < 1.0e-6, "{stress} {limit}");
        // A limit above the symmetric stress adds nothing.
        let lax = FatigueDemand {
            moment_1g_nm: &[moment, 0.0],
            limit_pa: 2.0 * base,
        };
        size_lower_cover(&mut sizing, &lax, al, al, al);
        assert_eq!(sizing.spars[0].lower_extra_area[0], 0.0);
    }
}
