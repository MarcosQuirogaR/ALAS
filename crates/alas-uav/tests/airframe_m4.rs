// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! UAV-PLAN M4 item 1-4 checks: lattice neutral point, aggregate result, spar
//! from DB-style laminate rows.  Hand references are stated per test.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_uav::airframe::analysis::{analyse_airframe, AnalysisInputs, CruiseState, MarginVerdict};
use alas_uav::airframe::geometry::{AirframeGeometry, Panel, Planform, TailRole, TailSurfaceGeom};
use alas_uav::airframe::spar::{SparDesign, SparGeometry};
use alas_uav::airframe::stability::{
    LongitudinalOptions, NeutralPointMethod, SimpleLiftingSurface,
};
use alas_uav::airframe::trim::TrimSetup;
use alas_uav::airframe::vlm_np::VlmNeutralPoint;
use alas_uav::spec::topology::TailKind;
use alas_uav_db::parts::LaminateSpec;
use alas_uav_db::{Method, Num, SourceId};

const FT: f64 = 0.3048;
const IN: f64 = 0.0254;

fn rect(span: f64, chord: f64, x_le: f64, z: f64, vertical: bool) -> Planform {
    Planform {
        panels: vec![Panel {
            span_m: if vertical { span } else { 0.5 * span },
            chord_root_m: chord,
            chord_tip_m: chord,
            root_le_m: [x_le, 0.0, z],
            sweep_le_rad: 0.0,
            dihedral_rad: 0.0,
        }],
        mirror: !vertical,
        vertical,
    }
}

/// Raymer DR-1 (the stability tests' reference): wing 118 ft^2, c 56 in;
/// horizontal tail 25.5 ft^2, c 0.5 m, tail AC 206 in from the datum.
fn dr1() -> AirframeGeometry {
    let s = 118.0 * FT * FT;
    let c = 56.0 * IN;
    let wing = rect(s / c, c, 62.0 * IN - 0.25 * c, 0.0, false);
    let tail_c = 0.5;
    let s_t = 25.5 * FT * FT;
    let tail = rect(s_t / tail_c, tail_c, 206.0 * IN - 0.25 * tail_c, 0.0, false);
    AirframeGeometry {
        wing,
        horizontal: Some(TailSurfaceGeom {
            role: TailRole::Horizontal,
            planform: tail,
            v_dihedral_rad: 0.0,
            incidence_rad: 0.0,
            count: 1,
        }),
        vertical: None,
        tail_kind: TailKind::Conventional,
    }
}

fn wing_only() -> AirframeGeometry {
    let mut g = dr1();
    g.horizontal = None;
    g
}

fn lattice() -> VlmNeutralPoint {
    VlmNeutralPoint::default()
}

#[test]
fn wing_alone_neutral_point_is_the_quarter_chord_of_the_mac() {
    // Inviscid thin wing: the neutral point of a wing without a tail is its
    // aerodynamic centre at c/4 (any aspect ratio, thin-airfoil theory).
    let g = wing_only();
    let np = lattice()
        .neutral_point(&g, &LongitudinalOptions::default())
        .unwrap();
    let x_ac = g.wing.aerodynamic_centre_m()[0];
    assert!(
        ((np.x_np_m - x_ac) / np.mac_m).abs() < 0.02,
        "x_np {} vs AC {} (MAC {})",
        np.x_np_m,
        x_ac,
        np.mac_m
    );
    assert!(np.cl_alpha_total() > 0.0);
}

#[test]
fn wing_plus_tail_neutral_point_agrees_with_the_closed_form_within_five_percent_mac() {
    // Hand reference: the closed-form lifting-surface estimate of the same
    // geometry (Raymer 16.9 with Eq. 12.6 and the elliptic downwash gradient).
    let g = dr1();
    let o = LongitudinalOptions::default();
    let closed = SimpleLiftingSurface.neutral_point(&g, &o).unwrap();
    let vlm = lattice().neutral_point(&g, &o).unwrap();
    let diff = (vlm.x_np_m - closed.x_np_m) / closed.mac_m;
    assert!(
        diff.abs() < 0.05,
        "VLM x_np {:.4} m vs closed {:.4} m: {:+.3} MAC",
        vlm.x_np_m,
        closed.x_np_m,
        diff
    );
    // The tail adds a positive lift slope and moves the neutral point aft of
    // the wing aerodynamic centre.
    assert!(vlm.tail_term > 0.0);
    assert!(vlm.x_np_m > vlm.x_ac_wing_m);
    assert_eq!(vlm.method, "vlm-inviscid-lattice");
}

#[test]
fn tail_aft_layout_is_statically_stable_at_a_forward_cg() {
    let g = dr1();
    let o = LongitudinalOptions::default();
    let np = lattice().neutral_point(&g, &o).unwrap();
    let x_cg = np.x_np_m - 0.10 * np.mac_m;
    assert!((np.static_margin(x_cg) - 0.10).abs() < 1e-12);
    assert!(np.cm_alpha_per_rad(x_cg) < 0.0);
}

fn trim_setup() -> TrimSetup {
    TrimSetup {
        cm0: 0.0,
        tail_incidence_rad: 0.0,
        tau: 0.6,
        max_down_rad: 25.0_f64.to_radians(),
        max_up_rad: 25.0_f64.to_radians(),
    }
}

fn spar_design() -> SparDesign {
    let t = 0.125e-3;
    let cap = LaminateSpec {
        ply_thickness_m: Some(num(t, "m")),
        areal_mass_kg_m2: Some(num(0.2, "kg/m2")),
        ply_e1_pa: Some(num(181.0e9, "Pa")),
        ply_e2_pa: Some(num(10.3e9, "Pa")),
        ply_g12_pa: Some(num(7.17e9, "Pa")),
        ply_nu12: Some(num(0.28, "1")),
        ply_xt_pa: Some(num(1.5e9, "Pa")),
        ply_xc_pa: Some(num(1.2e9, "Pa")),
        ply_yt_pa: Some(num(4.0e7, "Pa")),
        ply_yc_pa: Some(num(1.5e8, "Pa")),
        ply_s12_pa: Some(num(6.0e7, "Pa")),
        knockdown: Some(num(0.8, "1")),
        ..LaminateSpec::default()
    };
    let web = cap.clone();
    SparDesign::from_db(
        SparGeometry {
            semispan_m: 1.4,
            root_depth_m: 0.05,
            tip_depth_m: 0.02,
            root_cap_width_m: 0.02,
            tip_cap_width_m: 0.01,
        },
        &cap,
        &[0.0, 0.0, 0.0, 0.0],
        &web,
        &[45.0, -45.0],
        1.5,
        400,
    )
    .unwrap()
}

fn num(value: f64, unit: &str) -> Num {
    Num::new(
        value,
        unit,
        SourceId::new("test-src").unwrap(),
        None,
        Method::User,
    )
}

#[test]
fn aggregate_result_reports_margin_tail_volume_trim_drag_and_spar() {
    let g = dr1();
    let o = LongitudinalOptions::default();
    let setup = trim_setup();
    let design = spar_design();
    let plane_s = g.wing.projected_area_m2();
    let c_mac = g.wing.mac_m();
    let cruise = CruiseState {
        speed_m_s: 25.0,
        density_kg_m3: 1.10,
        mass_kg: 6.0,
    };
    let x_cg = {
        let np = lattice().neutral_point(&g, &o).unwrap();
        np.x_np_m - 0.10 * np.mac_m
    };
    let out = analyse_airframe(
        &g,
        &lattice(),
        &AnalysisInputs {
            options: &o,
            x_cg_m: x_cg,
            cruise: Some(cruise),
            trim_setup: Some(&setup),
            spar: Some((&design, 6.0 * 9.806_65 * 2.5)),
        },
    )
    .unwrap();

    // Static margin at the loaded CG (10 % MAC by construction), in the band.
    assert!((out.static_margin - 0.10).abs() < 1e-9);
    assert_eq!(out.static_margin_verdict, MarginVerdict::Within);
    assert_eq!(out.np_method, "vlm-inviscid-lattice");

    // Horizontal tail volume from the same geometry: S_h l_h /(S c), with the
    // arm between the wing and tail aerodynamic centres.
    let s_h = g
        .horizontal
        .as_ref()
        .unwrap()
        .effective_horizontal_area_m2();
    let arm = g
        .horizontal
        .as_ref()
        .unwrap()
        .planform
        .aerodynamic_centre_m()[0]
        - g.wing.aerodynamic_centre_m()[0];
    let v_h_hand = s_h * arm / (plane_s * c_mac);
    let v_h = out.tail_volume_horizontal.expect("tail volume");
    assert!(
        ((v_h - v_h_hand) / v_h_hand).abs() < 1e-9,
        "V_H {v_h} vs {v_h_hand}"
    );
    assert!(out.tail_volume_vertical.is_none());

    // Cruise trim: finite elevator, tail lift and positive trim drag.
    let trim = out.cruise_trim.as_ref().expect("cruise trim");
    assert!(trim.cl_required > 0.0);
    assert!(trim.trim_drag_n.is_finite() && trim.trim_drag_n >= 0.0);
    assert!(trim.trim_drag_n > 0.0 || trim.tail_lift_n.abs() < 1e-12);

    // Spar: finite, positive mass, deflection, bending and shear margins.
    let spar = out.spar.as_ref().expect("spar");
    assert!(spar.spar_mass_kg > 0.0);
    assert!(spar.tip_deflection_m > 0.0);
    assert!(spar.bending_margin_of_safety.is_finite());
    assert!(spar.shear_margin_of_safety.is_finite());
    assert!(out.estimate);
}

#[test]
fn spar_from_db_rows_reports_the_documented_margin_definition() {
    let design = spar_design();
    let r = alas_uav::airframe::spar::analyse_spar(&design, 60.0).unwrap();
    // Bending margin is allowable/(SF sigma) - 1 at the critical ply, so a
    // margin of exactly zero is reached when SF sigma equals the knocked-down
    // strength: check the relation with the stated stress.
    let allow = 0.8 * 1.5e9;
    let sigma = r.root_cap_stress_limit_pa;
    assert!(sigma > 0.0);
    assert!(r.bending_margin_of_safety <= allow / (1.5 * sigma) - 1.0 + 1e-9);
}
