// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The fuselage (F2) and nacelle (F3) neutral-point terms for the physical
//! airplane, split out of `super` to keep each file under the line budget.

use alas_geom::aircraft::airplane::Airplane;

use super::super::{fuselage, nacelles, FuselageTerms, NacelleTerm};
use crate::trim::{hstab, main_wing, AC_CHORD_FRACTION};

/// The F2/F3 fuselage and nacelle terms for `airplane`'s own (physical)
/// geometry, given the Mach-consistent `cl_alpha_total` and the VLM-implied
/// tail downwash `deps_dalpha_vlm`: `(fuselage_shift_m, nacelle_shift_m,
/// FuselageTerms, Vec<NacelleTerm>)`. `0.0`/empty when `cl_alpha_total` is
/// too small a slope to normalise by.
pub(super) fn airframe_terms(
    airplane: &Airplane,
    cl_alpha_total: f64,
    wing_lift_fraction: f64,
    deps_dalpha_vlm: f64,
) -> (f64, f64, FuselageTerms, Vec<NacelleTerm>) {
    if !cl_alpha_total.is_finite() || cl_alpha_total.abs() < 0.1 {
        return zero_airframe_terms(deps_dalpha_vlm);
    }
    let Some(wing) = main_wing(airplane).filter(|wing| !wing.xsecs.is_empty()) else {
        return zero_airframe_terms(deps_dalpha_vlm);
    };
    let (root_le_m, root_chord_m) = wing.design_root();
    let x_le_root = root_le_m[0];
    let x_te_root = x_le_root + root_chord_m;
    let x_wing_ac_root = x_le_root + AC_CHORD_FRACTION * root_chord_m;
    // Torenbeek 1982, Eq. E-41: nacelle arms are measured to 1/4 MAC, the
    // wing's own (lofted) MAC, not the reference frame's, so a published
    // reference chord or LEMAC does not move the term.
    let x_c4_mac = Some(wing.aerodynamic_center(AC_CHORD_FRACTION)[0])
        .filter(|x| x.is_finite())
        .unwrap_or(x_wing_ac_root);
    let x_tail_ac = match hstab(airplane) {
        Some(h) => h.aerodynamic_center(AC_CHORD_FRACTION)[0],
        None => x_te_root + 3.0 * airplane.c_ref.max(0.1),
    };
    // The strip method's "wing root" is the exposed root chord at the wing-fuselage
    // junction (USAF DATCOM 1978, Sec. 4.2.2.1, Sketch (a)), not the centre-line
    // chord the wing is extended to inside the body: on a swept wing the
    // junction section sits aft of the centre-line section. A half width inboard
    // of the side of body falls on the box, not on the plan buried in the body.
    let (x_le_junction, chord_junction) = fuselage::wing_section_at(
        &wing.carried_box(),
        fuselage::half_width_near(airplane, x_wing_ac_root),
    )
    .unwrap_or((x_le_root, root_chord_m));
    let fus = fuselage::fuselage_terms(
        airplane,
        cl_alpha_total,
        cl_alpha_total * wing_lift_fraction.clamp(0.0, 1.0),
        deps_dalpha_vlm,
        x_le_junction,
        x_le_junction + chord_junction,
        x_tail_ac,
        wing.mean_sweep_angle(AC_CHORD_FRACTION).to_radians(),
        wing.taper_ratio(),
    );
    let (nac_shift, nac_terms) =
        nacelles::nacelle_terms(airplane, cl_alpha_total, x_c4_mac, x_te_root);
    (
        // Torenbeek's pair, Synthesis of Subsonic Airplane Design, 1982,
        // App. E, eq. E-36: the empirical body term E-37 (`D1`) with the
        // swept-wing lift-loss term E-38 (`D2`), printed pp. 479-480, the same
        // pair the forward-limit wing-body centre takes. Both shift the
        // wing-body aerodynamic centre; the neutral point moves by that shift
        // weighted with the wing-body share of the total lift-curve slope.
        // `D1` is normalised by the total slope, which is that weighting
        // already (`-1.8/(C_L,alpha)_wf` with `(C_L,alpha)_wf` the share of
        // the total); `D2` is geometric and takes the share here. The DATCOM
        // 4.2.2.1 Multhopp strip shift (`multhopp_shift_m`) is a cross-check:
        // its forebody part equals `D1` to 0.01 MAC on D-3797 and A58H12, and
        // without `D2` it moves both 0.04-0.05 MAC forward of the tunnel
        // neutral points, while with `D2` it is within 0.002 (A58H12), so `D2`
        // is retained; the 747-100 tunnel data (CR-147641, CR-141800) are the
        // one case that argues against it (see the commit notes).
        // The flagged tunnel-data correction (`super::super::tunnel_ac`) is a
        // wing-body aerodynamic-centre shift like `D2`, so it takes the same
        // wing-body share.
        fus.torenbeek_d1_shift_m
            + wing_lift_fraction.clamp(0.0, 1.0)
                * (fus.torenbeek_d2_shift_m + fus.tunnel_correction_shift_m),
        nac_shift,
        fus,
        nac_terms,
    )
}

/// The all-zero [`airframe_terms`] result, for a caller that has disabled
/// the fuselage/nacelle correction (`AnalysisConfig::include_fuselage_stability`)
/// or has too degenerate a lift slope to normalise by.
pub(super) fn zero_airframe_terms(
    deps_dalpha_vlm: f64,
) -> (f64, f64, FuselageTerms, Vec<NacelleTerm>) {
    (
        0.0,
        0.0,
        FuselageTerms {
            multhopp_shift_m: 0.0,
            torenbeek_d2_shift_m: 0.0,
            torenbeek_d1_shift_m: 0.0,
            gilruth_shift_m: 0.0,
            tunnel_correction_shift_m: 0.0,
            deps_dalpha: deps_dalpha_vlm,
        },
        Vec::new(),
    )
}

/// Wing-body lift interference of Torenbeek, Synthesis of Subsonic Airplane
/// Design (1982), App. E, Eq. E-33/E-34 (printed p. 479):
/// `K_I = (1 + 2.15 b_f/b)(S_net/S) + pi b_f^2 / (2 a_w S)` and
/// `(CL_alpha)_wf = K_I (CL_alpha)_w`, `b_f` the body width at the wing,
/// `S_net = S - b_f c_root` the wing area outside the body, `a_w` the wing
/// slope [1/rad] on `S`. Returns `(K_I, x_m)`, `x_m` the station (geometry
/// axes, m) where the extra lift `(K_I - 1) a_w` is applied: the mid-chord of
/// the exposed root chord, where the carry-over concentrates (Torenbeek
/// Fig. E-13). `None` with no fuselage or a degenerate wing.
pub(super) fn wing_body_lift_interference(airplane: &Airplane, a_w: f64) -> Option<(f64, f64)> {
    if !a_w.is_finite() || a_w < 0.1 {
        return None;
    }
    let wing = main_wing(airplane).filter(|wing| !wing.xsecs.is_empty())?;
    airplane.fuselages.first()?;
    let (root_le_m, root_chord_m) = wing.design_root();
    let x_ref = root_le_m[0] + AC_CHORD_FRACTION * root_chord_m;
    let half_width = fuselage::half_width_near(airplane, x_ref);
    let b_f = 2.0 * half_width;
    let (s, b) = (airplane.s_ref, airplane.b_ref);
    if b_f <= 0.0 || s <= 0.0 || b <= 0.0 || b_f / b >= 0.2 {
        return None;
    }
    let (x_le, chord) = fuselage::wing_section_at(&wing.carried_box(), half_width)?;
    let s_net = (s - b_f * chord).max(0.0);
    let k_i =
        (1.0 + 2.15 * b_f / b) * (s_net / s) + std::f64::consts::PI * b_f * b_f / (2.0 * a_w * s);
    k_i.is_finite().then_some((k_i, x_le + 0.5 * chord))
}
