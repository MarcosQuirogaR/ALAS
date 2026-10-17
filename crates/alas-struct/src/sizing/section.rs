// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compatible-strain, symmetric first-order wing-box sections.
//!
//! Each cover is represented by straight panels between active spars, centred
//! on a common horizontal mid-depth plane. Camber, sweep coupling, shear lag,
//! cover local buckling and torsion require the shell model. This is the standard
//! transformed-section/boom approximation: Megson, *Aircraft Structures for
//! Engineering Students*, 4th ed., 2007, ch. 20, and Bruhn, *Analysis and
//! Design of Flight Vehicle Structures*, 1973, section A6. Covers are bending
//! material; modulus-weighted EI and sigma = E*kappa*z enforce one strain.

use super::{SparSizing, WingboxSizing};
use alas_config::materials::MaterialSpec;

/// Cap pair inertia about the section mid-plane, m^4. The caps' centroids
/// lie half a thickness inside the upper/lower spar surface, as in the deck.
pub(crate) fn cap_inertia(height: f64, width: f64, thickness: f64) -> f64 {
    if height <= 0.0 || width <= 0.0 || thickness <= 0.0 {
        return 0.0;
    }
    let offset = 0.5 * (height - thickness);
    2.0 * width * thickness * (offset * offset + thickness * thickness / 12.0)
}

pub(crate) fn cap_ei(spar: &SparSizing, station: usize, cap: &MaterialSpec) -> f64 {
    cap.e_pa * cap_inertia(spar.h[station], spar.w_cap[station], spar.t_cap[station])
}

/// Stiffness of the lower-cover fatigue area, N m^2. It is real flange material
/// on the lower cap (the deck puts it on the lower bar); the response section
/// previously left it out as "conservative for deflection". Credited in the
/// response stiffness only ([`station_ei_spanwise`]): [`station_ei`] stays the
/// symmetric section the fatigue sizing starts from (it adds this area itself,
/// with its neutral-axis shift). Centroid at the lower spar surface; the small
/// neutral-axis shift is not modelled here.
pub(crate) fn lower_extra_ei(sizing: &WingboxSizing, station: usize, cap: &MaterialSpec) -> f64 {
    sizing
        .spars
        .iter()
        .map(|spar| {
            let extra = spar.lower_extra_area.get(station).copied().unwrap_or(0.0);
            let h = spar.h[station];
            if h > 0.0 && extra > 0.0 {
                cap.e_pa * extra * 0.25 * h * h
            } else {
                0.0
            }
        })
        .sum()
}

/// Two straight cover panels: integrate 2*t*(h(x)/2)^2 across each bay.
/// The polynomial integral is exact for linearly varying panel depth.
pub(crate) fn non_cap_ei(
    sizing: &WingboxSizing,
    station: usize,
    skin: &MaterialSpec,
    web: &MaterialSpec,
) -> f64 {
    let (covers, webs) = cover_and_web_ei(sizing, station, skin, web);
    covers * sizing.axis_cos_at(station) + webs
}

/// Cover and web contributions to the streamwise-cut stiffness, separately,
/// because a normal-to-axis cut sees the covers `cos L` as wide.
pub(crate) fn cover_and_web_ei(
    sizing: &WingboxSizing,
    station: usize,
    skin: &MaterialSpec,
    web: &MaterialSpec,
) -> (f64, f64) {
    // Consecutive active spars bound each cover bay. This runs in the inner
    // loop of every cap and web search, so it pairs them in place rather than
    // collecting them; the terms and their summation order are unchanged.
    let active = || sizing.spars.iter().filter(|spar| spar.h[station] > 0.0);
    let covers: f64 = active()
        .zip(active().skip(1))
        .map(|(inboard, outboard)| {
            let width = (outboard.chord_fraction - inboard.chord_fraction) * sizing.chord[station];
            let a = inboard.h[station];
            let b = outboard.h[station];
            skin.e_pa * sizing.t_skin * width * (a * a + a * b + b * b) / 6.0
        })
        .sum();
    (
        covers,
        active()
            .map(|spar| web.e_pa * spar.t_web[station] * spar.h[station].powi(3) / 12.0)
            .sum::<f64>(),
    )
}

/// Stiffness of the section normal to the box axis, `EI_n`, N m^2: caps and
/// webs as cut, covers `cos L` as wide (see [`super::swept`]). Equal to the
/// streamwise-cut stiffness when no sweep is recorded.
pub(crate) fn station_ei(
    sizing: &WingboxSizing,
    station: usize,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) -> f64 {
    non_cap_ei(sizing, station, skin, web)
        + sizing
            .spars
            .iter()
            .map(|spar| cap_ei(spar, station, cap))
            .sum::<f64>()
}

/// Effective bending stiffness in the spanwise frame, `EI_n cos^3 L`: the
/// curvature `w_yy` under the streamwise moment `M_x` is `M_x / EI_eff`.
pub(crate) fn station_ei_spanwise(
    sizing: &WingboxSizing,
    station: usize,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) -> f64 {
    (station_ei(sizing, station, skin, web, cap) + lower_extra_ei(sizing, station, cap))
        * sizing.axis_cos_at(station).powi(3)
}

/// Per-spar cap stress and the maximum utilization of that cap, its web,
/// and the covers. Cover utilization belongs to every active spar because
/// a failed shared panel cannot be hidden behind a passing cap.
#[allow(clippy::too_many_arguments)] // Station, spar and two resultants with three independently configured materials.
pub(crate) fn stress_utilization(
    sizing: &WingboxSizing,
    station: usize,
    spar_index: usize,
    moment: f64,
    shear: f64,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) -> (f64, f64) {
    let spar = &sizing.spars[spar_index];
    if spar.h[station] <= 0.0 || (moment.abs() <= 1.0 && shear.abs() <= 1.0) {
        return (0.0, 0.0);
    }
    let curvature = moment.abs() / station_ei(sizing, station, skin, web, cap);
    // The maximum fibre, rather than cap centroid, controls normal stress.
    let distance = 0.5 * spar.h[station];
    let cap_stress = cap.e_pa * curvature * distance;
    let cover_distance = sizing
        .spars
        .iter()
        .map(|spar| 0.5 * spar.h[station])
        .fold(0.0_f64, f64::max);
    // The covers and the spar flanges are one stiffened skin-stringer panel:
    // its compressive allowable is the lesser of the material allowable and
    // the structural-index curve of the installed smeared thickness.
    let panel = super::panel::CoverPanel::new(sizing, station);
    let flange = super::panel::flange_area_m2(sizing, station);
    let utilization = (cap_stress / panel.allowable_pa(cap, flange))
        .max(super::web::utilization(
            sizing, station, spar_index, curvature, shear, web,
        ))
        .max(skin.e_pa * curvature * cover_distance / panel.allowable_pa(skin, flange));
    (cap_stress, utilization)
}

/// Moment fractions from the section's actual stiffness, for reporting.
pub(crate) fn update_moment_fractions(
    sizing: &mut WingboxSizing,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) {
    for station in 0..sizing.y_stations.len() {
        let ei = station_ei(sizing, station, skin, web, cap);
        let mut cover_ei = vec![0.0; sizing.spars.len()];
        let active: Vec<_> = sizing
            .spars
            .iter()
            .enumerate()
            .filter(|(_, spar)| spar.h[station] > 0.0)
            .map(|(index, _)| index)
            .collect();
        for pair in active.windows(2) {
            let left = &sizing.spars[pair[0]];
            let right = &sizing.spars[pair[1]];
            let width = (right.chord_fraction - left.chord_fraction) * sizing.chord[station];
            let a = left.h[station];
            let b = right.h[station];
            // Megson boom areas t*b/6*(2 + sigma_neighbor/sigma),
            // with compatible stress proportional to depth, for two covers.
            let scale = skin.e_pa * sizing.t_skin * width / 12.0;
            cover_ei[pair[0]] += scale * a * (2.0 * a + b);
            cover_ei[pair[1]] += scale * b * (2.0 * b + a);
        }
        let contributions: Vec<_> = sizing
            .spars
            .iter()
            .enumerate()
            .map(|(index, spar)| {
                cap_ei(spar, station, cap)
                    + web.e_pa * spar.t_web[station] * spar.h[station].powi(3) / 12.0
                    + cover_ei[index]
            })
            .collect();
        for (spar, contribution) in sizing.spars.iter_mut().zip(contributions) {
            spar.frac_moment[station] = if ei > 0.0 { contribution / ei } else { 0.0 };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sizing::MassBreakdown;
    use alas_config::materials;

    fn rectangular_box() -> WingboxSizing {
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

    #[test]
    fn flange_inertia_includes_its_own_rectangle_and_physical_centroid() {
        let h = 1.0_f64;
        let b = 0.2;
        let t = 0.1;
        let expected = b * (h * h * h - (h - 2.0 * t).powi(3)) / 12.0;
        assert!((cap_inertia(h, b, t) - expected).abs() < 1.0e-15);
    }

    #[test]
    fn rectangular_box_credits_caps_covers_and_webs_once() {
        let sizing = rectangular_box();
        let material = materials::get("Al 7075-T6").expect("registered material");
        let expected_inertia =
            2.0 * cap_inertia(1.0, 0.2, 0.01) + 2.0 * 0.003 / 12.0 + 2.0 * 2.0 * 0.005 * 0.25;
        let actual = station_ei(&sizing, 0, material, material, material);
        assert!((actual / (material.e_pa * expected_inertia) - 1.0).abs() < 1.0e-14);
    }

    #[test]
    fn a_longer_rib_bay_lowers_the_cover_allowable_and_raises_its_utilization() {
        let material = materials::get("Al 7075-T6").expect("registered material");
        let mut short = rectangular_box();
        short.num_ribs = 11; // 1 m bays over the 10 m station span
        let mut long = rectangular_box();
        long.num_ribs = 3; // 5 m bays
        let moment = station_ei(&short, 0, material, material, material) * 0.002;
        let (_, ratio_short) =
            stress_utilization(&short, 0, 0, moment, 0.0, material, material, material);
        let (_, ratio_long) =
            stress_utilization(&long, 0, 0, moment, 0.0, material, material, material);
        assert!(ratio_long > ratio_short, "{ratio_long} vs {ratio_short}");
        // Two spars with 0.2 m x 10 mm flanges: 0.004 m^2 per cover.
        assert!((super::super::panel::flange_area_m2(&short, 0) - 0.004).abs() < 1.0e-15);
    }

    #[test]
    fn partial_zero_depth_spar_does_not_reduce_the_cover_stiffness() {
        let mut sizing = rectangular_box();
        let material = materials::get("Al 7075-T6").expect("registered material");
        let before = station_ei(&sizing, 0, material, material, material);
        let mut partial = sizing.spars[0].clone();
        partial.chord_fraction = 0.5;
        partial.h.fill(0.0);
        partial.w_cap.fill(0.0);
        partial.t_cap.fill(0.0);
        partial.a_cap.fill(0.0);
        sizing.spars.insert(1, partial);
        assert_eq!(station_ei(&sizing, 0, material, material, material), before);
    }

    #[test]
    fn unequal_depth_cover_boom_shares_recover_the_exact_integrated_ei() {
        let mut sizing = rectangular_box();
        sizing.spars[1].h.fill(0.5);
        let material = materials::get("Al 7075-T6").expect("registered material");
        update_moment_fractions(&mut sizing, material, material, material);
        let sum: f64 = sizing.spars.iter().map(|spar| spar.frac_moment[0]).sum();
        assert!((sum - 1.0).abs() < 1.0e-14);
        assert!(sizing.spars[0].frac_moment[0] > sizing.spars[1].frac_moment[0]);
    }

    #[test]
    fn shared_cover_strength_and_combined_web_stress_cannot_hide_behind_caps() {
        let sizing = rectangular_box();
        let metal = materials::get("Al 7075-T6").expect("registered material");
        let carbon = materials::get("CFRP UD").expect("registered material");
        let stiffness = station_ei(&sizing, 0, metal, metal, carbon);
        let curvature = 0.002;
        let moment = stiffness * curvature;
        let (stress, utilization) =
            stress_utilization(&sizing, 0, 0, moment, 0.0, metal, metal, carbon);
        assert!((stress - carbon.e_pa * curvature * 0.5).abs() < 1.0e-6);
        let mut weak_cover = metal.clone();
        weak_cover.f_allow_pa = 10.0e6;
        let moment = station_ei(&sizing, 0, &weak_cover, metal, carbon) * curvature;
        assert!(stress_utilization(&sizing, 0, 0, moment, 0.0, &weak_cover, metal, carbon).1 > 1.0);
        assert!(
            stress_utilization(&sizing, 0, 0, moment, 2.0e6, metal, metal, carbon).1 > utilization
        );
    }
}
