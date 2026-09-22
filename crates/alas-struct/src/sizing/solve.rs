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

use super::law::{
    cap_taper, gradient_unit, linspace, rib_count_from_max_spacing, root_cap_dimensions,
    station_cap_dimensions, trapezoid, SizingLaw,
};
use super::types::{CompositeProxyDeclaration, MassBreakdown, SparSizing, WingboxSizing};

/// One load case's net running load and cantilever bending moment, relieved by
/// the mass the wing carries.
///
/// Signed throughout: a push-down case has a negative aerodynamic load and a
/// negative load factor, so the relief term `-n g m` adds back and reduces the
/// magnitude, which is the same direction it acts in on the pull-up case.
fn relieved_case_loads(
    y: &[f64],
    semi_span: f64,
    case: &LoadCase,
    gravity_m_s2: f64,
    relief: &WingInertiaRelief,
) -> (Vec<f64>, Vec<f64>) {
    let q_aero = loads::elliptic_distributed_load(y, semi_span, case.total_force_n);
    if relief.is_empty() {
        let (_, moment) = loads::cantilever_shear_moment(y, &q_aero);
        return (q_aero, moment);
    }
    let q_net = loads::net_distributed_load(
        &q_aero,
        case.load_factor,
        gravity_m_s2,
        &relief.running_mass_kg_m,
    );
    let (_, mut moment) = loads::cantilever_shear_moment(y, &q_net);
    loads::apply_point_mass_relief(
        y,
        &mut moment,
        case.load_factor,
        gravity_m_s2,
        &relief.point_masses_kg,
    );
    (q_net, moment)
}

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
) -> WingboxSizing {
    let n = cfg.spanwise_stations.max(0) as usize;
    let y = linspace(0.0, wsg.semi_span, n);
    let eta: Vec<f64> = y.iter().map(|&yi| yi / wsg.semi_span).collect();
    let chord: Vec<f64> = eta.iter().map(|&e| wsg.local_chord(e)).collect();

    let cases = loads::load_cases(req, cfg.additional_safety_factor);
    // Sized by whichever case has the larger |root moment| once the inertia
    // the wing carries itself has been taken off it. Comparing relieved
    // moments rather than total forces is what keeps the choice honest: relief
    // is common to every case, so it can reorder them.
    let mut worst_idx = 0usize;
    let mut worst_m0 = -1.0;
    let mut case_moments: Vec<(Vec<f64>, Vec<f64>)> = Vec::with_capacity(cases.len());
    for (idx, case) in cases.iter().enumerate() {
        let (q, m) = relieved_case_loads(&y, wsg.semi_span, case, req.gravity_m_s2, relief);
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
        let (w_cap0, t_cap0) = match law {
            SizingLaw::Product => root_cap_dimensions(a_cap0, chord[0], h_i[0]),
            SizingLaw::Frozen => {
                let w_cap0 = (0.5 * chord[0]).min(h_i[0] * 0.6).max(1e-6);
                (w_cap0, (a_cap0 / w_cap0).min(h_i[0] * 0.20))
            }
        };

        // Outboard section.
        //
        // The product law sizes every station the way the root is sized: the
        // local bending demand over the local effective depth, turned into a
        // flange by [`root_cap_dimensions`], and floored at the minimum
        // practical cover gauge. The frozen law instead ramps the root flange
        // down by the taper and only raises a station that falls short.
        //
        // The distinction matters, and it is the taper that is wrong as a
        // mass law. `cap_taper_eta_lock` exists to hold stiffness inboard
        // (its own help text says so) and `cap_taper_tip_fraction` leaves a
        // fifth of the root flange at the tip: on a large transport that is a
        // 20 mm flange carrying a moment a tenth of what it can. Averaged over
        // the span the tapered section is about 1.4 times the section strength
        // demands, and a stiffness ramp charged as a strength floor is mass
        // the aircraft does not have. A real cover does have an outboard
        // floor, but it is an absolute manufacturing gauge, not a fraction of
        // a root flange that grows with the aeroplane; `t_skin_min_m` is the
        // practical cover gauge this configuration already declares for the
        // class, so it is what the caps are floored at too.
        let min_cap_thickness = cfg.t_skin_min_m.max(0.0);
        let mut t_cap = Vec::with_capacity(n);
        let mut w_cap = Vec::with_capacity(n);
        for j in 0..n {
            let demand = (frac_m[j] * m_sizing[j]).abs();
            let (w, t) = match law {
                SizingLaw::Product => {
                    let a_req = if demand > 1.0 {
                        demand / (cap_mat.f_allow_pa * h_eff[j].max(1e-6))
                    } else {
                        0.0
                    };
                    station_cap_dimensions(a_req, chord[j], h_i[j], min_cap_thickness)
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
            source: "Open source gap: the real wing box is composite, but no source has been \
                     recorded that states it. Assigned as an effective isotropic proxy, not a \
                     verified material.",
            applicability: "Effective isotropic proxy for a laminate wing box. f_allow is a single \
                            strength-based design allowable, not a laminate allowable; no ply schedule, \
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
        composite_declaration,
    }
}
