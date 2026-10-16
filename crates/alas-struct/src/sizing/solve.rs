// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one solve both sizing laws run through.
//!
//! [`size_wingbox_with_law`] is the whole strength pass: pick the load case
//! with the largest relieved root moment, split moment and shear across the
//! spars by local section depth, size caps and webs, lay out the ribs and
//! integrate the four mass components. The public entry points in
//! [`crate::sizing`] differ only in the law and the relief they hand it.

use alas_config::materials::MaterialSpec;
use alas_config::{DesignRequirements, StructuresConfig};
use alas_geom::wing_structure::WingStructureGeometry;

use crate::loads::{self, LoadCase, WingInertiaRelief};

use super::envelope::{self, CaseSet};
use super::fatigue::FatigueDemand;

use super::law::{
    cap_taper, gradient_unit, linspace, rib_count_from_max_spacing, station_cap_dimensions,
    trapezoid, SizingLaw,
};
use super::types::{CompositeProxyDeclaration, MassBreakdown, SparSizing, WingboxSizing};

// The public entry points' signature plus the law and relief they differ in.
#[allow(clippy::too_many_arguments)]
pub(super) fn size_wingbox_with_law(
    wsg: &WingStructureGeometry,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
    rib_mat: &MaterialSpec,
    law: SizingLaw,
    relief: &WingInertiaRelief,
    case_set: CaseSet,
) -> WingboxSizing {
    let n = cfg.spanwise_stations.max(0) as usize;
    let y = linspace(0.0, wsg.semi_span, n);
    let eta: Vec<f64> = y.iter().map(|&yi| yi / wsg.semi_span).collect();
    let chord: Vec<f64> = eta.iter().map(|&e| wsg.local_chord(e)).collect();

    // The frozen law keeps the manoeuvre set; the product law sizes to the
    // full envelope (manoeuvre, gust, torque, fatigue limit).
    let set = match law {
        SizingLaw::Product => case_set,
        SizingLaw::Frozen => CaseSet::MANOEUVRE_ONLY,
    };
    let loaded = envelope::case_loads(wsg, cfg, req, set, &y, relief);
    // Sized by whichever case has the larger |root moment| once the inertia
    // the wing carries itself has been taken off it. Comparing relieved
    // moments rather than total forces is what keeps the choice honest: relief
    // is common to every case, so it can reorder them.
    let mut worst_idx = 0usize;
    let mut worst_m0 = -1.0;
    for (idx, case) in loaded.iter().enumerate() {
        if case.moment[0].abs() > worst_m0 {
            worst_m0 = case.moment[0].abs();
            worst_idx = idx;
        }
    }
    let worst_case: &LoadCase = &loaded[worst_idx].case;
    let q_sizing = &loaded[worst_idx].q_net;
    let m_sizing = &loaded[worst_idx].moment;
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

    // Skin fixed at the configured minimum (no torsional shear-flow upsizing,
    // the same fidelity the analytical model uses).
    let t_skin = cfg.t_skin_min_m;

    // Only the frozen law retains its historical root shear reserve. Product
    // webs start at manufacturing gauge and are coupled to bending strength
    // and clear-panel shear buckling in product_strength.
    let tau_allow_web = web_mat.f_allow_pa / (2.0 * 3.0_f64.sqrt());
    let taper = cap_taper(&eta, cfg.cap_taper_eta_lock, cfg.cap_taper_tip_fraction);

    let m0 = m_sizing[0].abs();
    let v0 = v_sizing[0].abs();

    let mut spars: Vec<SparSizing> = Vec::with_capacity(wsg.spar_fracs.len());
    for (i, &frac_c) in wsg.spar_fracs.iter().enumerate() {
        let h_i = &h_all[i];
        let frac_m = &frac_moment_all[i];
        let h_eff: Vec<f64> = h_i.iter().map(|&h| h * 0.85).collect();

        // The frozen law retains its historical effective lever arm. Product
        // caps start at manufacturing gauge and are sized as one compatible
        // section after the covers and webs have been assembled below.
        let h_eff0 = h_eff[0].max(1e-6);
        let a_cap0 = (frac_m[0] * m0) / (cap_mat.f_allow_pa * h_eff0);
        let w_cap0 = (0.5 * chord[0]).min(h_i[0] * 0.6).max(1e-6);
        let t_cap0 = (a_cap0 / w_cap0).min(h_i[0] * 0.20);
        let min_cap_thickness = cfg.t_skin_min_m.max(0.0);
        let mut t_cap = Vec::with_capacity(n);
        let mut w_cap = Vec::with_capacity(n);
        for j in 0..n {
            let (w, t) = match law {
                SizingLaw::Product => {
                    station_cap_dimensions(0.0, chord[j], h_i[j], min_cap_thickness)
                }
                SizingLaw::Frozen => {
                    let t = (t_cap0 * taper[j]).min(h_i[j] / 3.0);
                    let w = (w_cap0 * taper[j]).max(t);
                    (w, t)
                }
            };
            t_cap.push(t);
            w_cap.push(w);
        }
        let a_cap: Vec<f64> = (0..n).map(|j| w_cap[j] * t_cap[j]).collect();

        let t_web = match law {
            SizingLaw::Product => cfg.t_web_min_m,
            SizingLaw::Frozen => cfg
                .t_web_min_m
                .max((frac_m[0] * v0) / (tau_allow_web * h_eff0)),
        };

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
            t_web: vec![t_web; n],
            lower_extra_area: vec![0.0; n],
            frac_moment: frac_m.clone(),
            margin_of_safety,
        });
    }

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
        None => rib_count_from_max_spacing(wsg.semi_span, l_rib)
            .max(10)
            .max(relief.rib_count_floor),
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
            .map(|j| s.t_web[j] * s.h[j] * web_mat.rho_kg_m3 * dy[j])
            .sum();
        m_caps += caps;
        m_webs += webs;
    }
    // The wingbox skin is the upper and lower cover between the outermost two
    // spars. The leading-edge and trailing-edge skins ahead of and behind the
    // box are fixed secondary structure and are enumerated separately by the
    // non-box wing inventory, so charging them here would count them twice;
    // `alas-mass`'s own wingbox centroid integral already uses this same
    // `(rear - front) x chord` cover width. The frozen law keeps the whole
    // chord, which is what its fixtures record.
    let skin_cover_fraction = if law.charges_the_whole_chord() {
        1.0
    } else {
        (frac_max - frac_min).max(0.0)
    };
    let m_skin: f64 = (0..n)
        .map(|j| 2.0 * skin_cover_fraction * chord[j] * t_skin * skin_mat.rho_kg_m3 * dy[j])
        .sum();

    // Rib webs span the box between the same two spars, for the same reason.
    let (rib_chord_start, rib_chord_end) = if law.charges_the_whole_chord() {
        (0.01, 0.99)
    } else {
        (frac_min, frac_max)
    };
    let xc_full = linspace(rib_chord_start, rib_chord_end, 60);
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

    let any_composite = [skin_mat, web_mat, cap_mat, rib_mat]
        .iter()
        .any(|m| m.category == "composite");

    let composite_declaration = if any_composite {
        Some(CompositeProxyDeclaration {
            source: "Open source gap: aircraft-specific ply schedule and measured laminate design \
                     allowables are unavailable. Assigned as an effective isotropic proxy, not a \
                     verified aircraft material.",
            applicability: "Effective isotropic proxy for a laminate wing box. f_allow is a single \
                            axial design allowable, not a full laminate failure envelope; no ply schedule, \
                            stacking sequence, compression-after-impact knockdown, inter-laminar check \
                            or aeroelastic tailoring is modelled. Not a certified laminate analysis. \
                            Gauge is a declared class assumption, not a measured gauge.",
            // Represent uncalibrated relative uncertainty explicitly as unknown (None),
            // never inventing a spurious number or 0.0 per strict audit instructions.
            relative_uncertainty: None,
        })
    } else {
        None
    };

    let mut result = WingboxSizing {
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
        composite_declaration,
    };
    if matches!(law, SizingLaw::Product) {
        let demand = envelope::station_envelope(&loaded);
        let fatigue = envelope::fatigue_limit_pa(cfg, cap_mat, set).map(|limit_pa| FatigueDemand {
            moment_1g_nm: &demand.level_moment_nm,
            limit_pa,
        });
        super::product_strength::size(
            &mut result,
            &demand.moment_nm,
            &demand.shear_n,
            cfg,
            skin_mat,
            web_mat,
            cap_mat,
            fatigue.as_ref(),
        );
    }
    result
}
