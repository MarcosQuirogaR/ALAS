// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Term-by-term neutral-point decomposition shared by the NP747 probes: the same terms, in the same units (fraction of the
//! configuration's own reference MAC from its own LEMAC), for the RM A58H12
//! model and a preset, so the two can be compared line by line.
//!
//! Every lattice solve is at the stated Mach through the Goethert stretch the
//! product uses (`alas_stab::neutral_point::goethert_stretch`) and the product
//! probe angles (`autobalance_alpha_low_deg` / `_high_deg`), on the fine mesh
//! the caller passes. Terms:
//! - wing alone: the main wing solved with no other surface (`a_w` on S_ref,
//!   lattice a.c.); the wing in presence of the tail (the product's
//!   `x_wing_alone_ac` diagnostic);
//! - wing-body a.c.: wing in presence + Multhopp + `wing share x` Torenbeek D2,
//!   and with the Torenbeek nacelle term;
//! - tail: geometric quarter-MAC arm `l_h`, `V_h = S_h l_h / (S c)`, isolated
//!   tail slope on its own projected area at dynamic-pressure ratio 1,
//!   downwash `1 - a_t,presence / a_t,isolated` (product diagnostic), `eta`;
//! - tail term: product (surfaces NP minus the wing in presence) and the
//!   textbook `eta V_h (a_t / a_w)(1 - de/da)`.
#![allow(dead_code)]

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_config::analysis::AnalysisConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_stab::neutral_point::{
    beta_for_mach, goethert_stretch, neutral_point_conditions, per_surface_contributions,
    NpConditionsInput,
};

/// The decomposed terms; stations as MAC fractions from LEMAC, shifts as MAC
/// fractions.
#[derive(Debug, Clone, Copy)]
pub struct NpTerms {
    pub mach: f64,
    pub wing_xsecs: usize,
    pub spanwise_resolution: i64,
    pub chordwise_resolution: i64,
    pub h_wing_isolated: f64,
    pub h_wing_presence: f64,
    pub fuselage: f64,
    pub nacelles: f64,
    pub h_wb: f64,
    pub s_h_over_s: f64,
    pub l_h_over_c: f64,
    pub v_h: f64,
    pub a_t_own: f64,
    pub deps: f64,
    pub eta: f64,
    pub a_w: f64,
    pub a_total: f64,
    pub tail_term_product: f64,
    pub tail_term_textbook: f64,
    pub h_np: f64,
}

fn slope_and_ac(plane: &Airplane, analysis: &AnalysisConfig, mach: f64) -> (f64, f64) {
    let beta = beta_for_mach(mach);
    let stretched = goethert_stretch(plane, beta);
    let atmosphere = Atmosphere::new(0.0);
    let velocity = mach * atmosphere.speed_of_sound();
    let system = VlmSystem::assemble(
        &stretched,
        analysis.fine_spanwise_resolution.max(1) as usize,
        analysis.fine_chordwise_resolution.max(1) as usize,
    )
    .expect("assemble");
    let op = |a: f64| OperatingPoint::new(atmosphere, velocity, a, 0.0, 0.0, 0.0, 0.0);
    let (a_lo, a_hi) = (
        analysis.autobalance_alpha_low_deg,
        analysis.autobalance_alpha_high_deg,
    );
    let lo = system.solve(&op(a_lo)).expect("solve");
    let hi = system.solve(&op(a_hi)).expect("solve");
    let slope = (hi.cl_lift - lo.cl_lift) / (a_hi - a_lo).to_radians();
    let x_ac = per_surface_contributions(&stretched, &lo, &hi)[0].x_ac * beta;
    (slope, x_ac)
}

/// Decompose the clean, rigid neutral point of `plane` at `mach`, sea level,
/// in the frame (`x_lemac_m`, `mac_m`). `analysis` carries the fine mesh and
/// the tail efficiency.
pub fn decompose(
    plane: &Airplane,
    analysis: &AnalysisConfig,
    mach: f64,
    x_lemac_m: f64,
    mac_m: f64,
) -> NpTerms {
    let input = NpConditionsInput {
        low_speed_mach: mach,
        low_speed_altitude_m: 0.0,
        cruise_mach: mach,
        cruise_altitude_m: 0.0,
        high_lift: None,
        include_flexibility: false,
        wing_stiffness: None,
        tail_flexibility: None,
        propeller: None,
        jet_inlets: None,
        max_operating: None,
    };
    let np = neutral_point_conditions(plane, analysis, &input).expect("np");
    let d = &np.diagnostics;
    let h = |x: f64| (x - x_lemac_m) / mac_m;
    let wing = plane
        .wings
        .iter()
        .find(|w| w.name == "Main Wing")
        .expect("main wing");
    let tail = plane
        .wings
        .iter()
        .find(|w| w.name == "Horizontal Stabilizer")
        .expect("tail");
    let mut wing_only = plane.clone();
    wing_only.wings = vec![wing.clone()];
    let (a_w, x_w_iso) = slope_and_ac(&wing_only, analysis, mach);
    let mut tail_only = plane.clone();
    let mut t = tail.clone();
    t.dynamic_pressure_ratio = 1.0;
    tail_only.wings = vec![t];
    let s_h = tail.projected_area();
    let (a_t_ref, _) = slope_and_ac(&tail_only, analysis, mach);
    let a_t_own = a_t_ref * plane.s_ref / s_h;
    let l_h = tail.aerodynamic_center(0.25)[0] - wing.aerodynamic_center(0.25)[0];
    let v_h = s_h * l_h / (plane.s_ref * mac_m);
    let eta = analysis.tail_efficiency_or(plane.horizontal_tail_dynamic_pressure_ratio());
    let fuselage = (d.fuselage.torenbeek_d1_shift_m
        + d.wing_lift_fraction
            * (d.fuselage.torenbeek_d2_shift_m + d.fuselage.tunnel_correction_shift_m))
        / mac_m;
    let nacelles = d.nacelle_shift_m / mac_m;
    let h_w = h(d.x_wing_alone_ac);
    NpTerms {
        mach,
        wing_xsecs: wing.xsecs.len(),
        spanwise_resolution: analysis.fine_spanwise_resolution,
        chordwise_resolution: analysis.fine_chordwise_resolution,
        h_wing_isolated: h(x_w_iso),
        h_wing_presence: h_w,
        fuselage,
        nacelles,
        h_wb: h_w + fuselage + nacelles,
        s_h_over_s: s_h / plane.s_ref,
        l_h_over_c: l_h / mac_m,
        v_h,
        a_t_own,
        deps: d.deps_dalpha_vlm,
        eta,
        a_w,
        a_total: d.cl_alpha_total,
        tail_term_product: h(d.x_np_surfaces) - h_w,
        tail_term_textbook: eta * v_h * a_t_own / a_w * (1.0 - d.deps_dalpha_vlm),
        h_np: h(np.clean_low_speed),
    }
}

/// One line of the decomposition table.
pub fn line(label: &str, t: &NpTerms) -> String {
    format!(
        "{label}: M {:.2} wing xsecs {} (fine {}x{}) | wing alone {:.3} in presence {:.3} | fuselage {:+.3} nacelles {:+.3} -> h_wb {:.3} | S_h/S {:.3} l_h/c {:.2} V_h {:.3} | a_t {:.2}/rad (own S) de/da {:.3} eta {:.2} a_w {:.2}/rad a_tot {:.2}/rad | tail term product {:.3} textbook {:.3} | NP {:.3}",
        t.mach,
        t.wing_xsecs,
        t.spanwise_resolution,
        t.chordwise_resolution,
        t.h_wing_isolated,
        t.h_wing_presence,
        t.fuselage,
        t.nacelles,
        t.h_wb,
        t.s_h_over_s,
        t.l_h_over_c,
        t.v_h,
        t.a_t_own,
        t.deps,
        t.eta,
        t.a_w,
        t.a_total,
        t.tail_term_product,
        t.tail_term_textbook,
        t.h_np,
    )
}
