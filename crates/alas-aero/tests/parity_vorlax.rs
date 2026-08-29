// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-aero::vorlax` against SUAVE's `VLM`, via
//! `golden/aero/vorlax.json`.
//!
//! Two tiers. Everything discrete is `exact`: the analysis settings, which
//! are copied constants and which decide which branches exist at all; the
//! panel and strip counts; the break indices; and the leading- and
//! trailing-edge masks. Everything numeric is `f32`, the tier
//! `docs/PORTING.md` assigns this row, because the influence kernel runs in
//! single precision upstream and the panelization is stored in it.
//!
//! Four checks in order of what they would tell you. The settings first,
//! because a fixture generated against different ones is not evidence about
//! this port. Then the panelization, because a panel laid a millimetre out of
//! place still integrates to a plausible lift and there would be nothing in a
//! coefficient to say so. Then the eight single-condition solves, with the
//! circulation field and the panel pressures and not only the eight totals.
//! Then the training grid, which is the call the mission actually depends on
//! and the input `alas-aero::lift_surrogate` is a spline of.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use alas_aero::vorlax::{run, VlmCondition, VlmSettings};
use alas_testkit::{Comparison, Tier};
use support::vorlax::{as_flags, as_i64, widen, Fixture};

fn fixture() -> Fixture {
    alas_testkit::load("aero", "vorlax")
}

/// The settings the analysis held have to be the ones this port assumes.
///
/// Three of them are the row's scope written down. `model_fuselage` and
/// `model_nacelle` are what make the fuselage panelizer a no-op,
/// `discretize_control_surfaces` is what keeps the main wing's three control
/// surfaces from becoming six more lifting surfaces, and
/// `use_VORLAX_matrix_calculation` selects the boundary condition. A port
/// written against the wrong value of any of them agrees with itself and
/// with nothing else, which is the failure `alas-aero::drag_buildup`'s row
/// records having had.
#[test]
fn the_analysis_settings_are_what_the_port_assumes() {
    let fixture = fixture();
    let settings = VlmSettings::default();
    let expected = &fixture.settings;

    let mut c = Comparison::new("alas-aero::vorlax settings", Tier::Exact);
    c.exact(
        "number_spanwise_vortices",
        &settings.number_spanwise_vortices,
        &expected.number_spanwise_vortices,
    );
    c.exact(
        "number_chordwise_vortices",
        &settings.number_chordwise_vortices,
        &expected.number_chordwise_vortices,
    );
    c.exact(
        "spanwise_cosine_spacing",
        &settings.spanwise_cosine_spacing,
        &expected.spanwise_cosine_spacing,
    );
    c.exact(
        "leading_edge_suction_multiplier",
        &settings.leading_edge_suction_multiplier,
        &expected.leading_edge_suction_multiplier,
    );

    // The four the port has no field for, because it translates only the
    // branch each of them selects.
    c.exact("model_fuselage is off", &expected.model_fuselage, &false);
    c.exact("model_nacelle is off", &expected.model_nacelle, &false);
    c.exact(
        "discretize_control_surfaces is off",
        &expected.discretize_control_surfaces,
        &false,
    );
    c.exact(
        "propeller_wake_model is off",
        &expected.propeller_wake_model,
        &false,
    );
    c.exact(
        "the SUAVE boundary condition is selected",
        &expected.use_vorlax_matrix_calculation,
        &false,
    );
    c.exact(
        "the kernel runs in single precision",
        &expected.floating_point_precision.as_str(),
        &"float32",
    );
    c.finish();
}

/// The vehicle both sides panel has to be the same aeroplane, and it has to
/// be one whose wings are unsegmented trapezoids: that is the only form
/// `make_VLM_wings` reduces to two span breaks, and it is the whole of what
/// this port translates.
#[test]
fn the_fixture_describes_the_vehicle_the_port_panels() {
    let fixture = fixture();
    let mut c = Comparison::new("alas-aero::vorlax vehicle", Tier::Exact);

    c.exact(
        "wing tags",
        &fixture
            .geometry
            .wings
            .iter()
            .map(|wing| wing.tag.as_str())
            .collect::<Vec<_>>(),
        &vec!["main_wing", "horizontal_stabilizer", "vertical_stabilizer"],
    );
    for wing in &fixture.geometry.wings {
        c.exact(
            &format!("{} has no Segments", wing.tag),
            &wing.n_segments,
            &0,
        );
        c.exact(
            &format!("{} has no leading-edge sweep of its own", wing.tag),
            &wing.sweep_leading_edge_rad.is_none(),
            &true,
        );
        c.exact(
            &format!("{} has no vortex lift", wing.tag),
            &wing.vortex_lift,
            &false,
        );
    }
    // The main wing carries three control surfaces and none of them is
    // discretized, which is the case worth naming: a vehicle with no control
    // surfaces at all would agree with a port that had translated the
    // discretization wrongly.
    c.exact(
        "the main wing carries control surfaces that are not discretized",
        &(fixture.geometry.wings[0].n_control_surfaces > 0),
        &true,
    );
    c.finish();
}

/// The panelization, panel by panel.
///
/// This is the check that a coefficient cannot make. A strip snapped to the
/// wrong station, a twist rotation about the wrong pivot or a mirrored side
/// whose bound vortex runs the wrong way all still produce a lift curve.
#[test]
fn the_vortex_distribution_matches_panel_for_panel() {
    let fixture = fixture();
    let results = run(
        &fixture.vlm_geometry(),
        &fixture.vlm_settings(),
        &fixture.conditions(),
    )
    .expect("the fixture's vehicle panels and solves");
    let vd = &results.distribution;
    let expected = &fixture.vortex_distribution;

    let mut discrete = Comparison::new("alas-aero::vorlax panelization", Tier::Exact);
    discrete.exact("n_w", &vd.n_w, &expected.n_w);
    discrete.exact("n_cp", &vd.n_cp, &expected.n_cp);
    discrete.exact("n_sw", &vd.n_sw, &expected.n_sw);
    discrete.exact("n_cw", &vd.n_cw, &expected.n_cw);
    discrete.exact(
        "chordwise_breaks",
        &vd.chordwise_breaks,
        &expected.chordwise_breaks,
    );
    discrete.exact(
        "spanwise_breaks",
        &vd.spanwise_breaks,
        &expected.spanwise_breaks,
    );
    discrete.exact(
        "symmetric_wings",
        &as_flags(&vd.symmetric_wings),
        &expected.symmetric_wings,
    );
    discrete.exact(
        "leading_edge_indices",
        &as_flags(&vd.leading_edge_indices),
        &expected.leading_edge_indices,
    );
    discrete.exact(
        "trailing_edge_indices",
        &as_flags(&vd.trailing_edge_indices),
        &expected.trailing_edge_indices,
    );
    discrete.exact(
        "panels_per_strip",
        &as_i64(&vd.panels_per_strip),
        &as_i64(&expected.panels_per_strip),
    );
    discrete.exact(
        "chordwise_panel_number",
        &as_i64(&vd.chordwise_panel_number),
        &as_i64(&expected.chordwise_panel_number),
    );
    discrete.exact(
        "exposed_leading_edge_flag",
        &vd.exposed_leading_edge_flag
            .iter()
            .map(|&v| i64::from(v))
            .collect::<Vec<_>>(),
        &expected.exposed_leading_edge_flag,
    );
    discrete.exact("vortex_lift", &vd.vortex_lift, &expected.vortex_lift);
    discrete.finish();

    let mut c = Comparison::new("alas-aero::vorlax panel coordinates", Tier::F32);
    let p = &vd.panels;
    let e = &expected.panels;
    for (name, actual, want) in [
        ("XAH", &p.xah, &e.xah),
        ("YAH", &p.yah, &e.yah),
        ("ZAH", &p.zah, &e.zah),
        ("XBH", &p.xbh, &e.xbh),
        ("YBH", &p.ybh, &e.ybh),
        ("ZBH", &p.zbh, &e.zbh),
        ("XCH", &p.xch, &e.xch),
        ("YCH", &p.ych, &e.ych),
        ("ZCH", &p.zch, &e.zch),
        ("XA1", &p.xa1, &e.xa1),
        ("YA1", &p.ya1, &e.ya1),
        ("ZA1", &p.za1, &e.za1),
        ("XA2", &p.xa2, &e.xa2),
        ("YA2", &p.ya2, &e.ya2),
        ("ZA2", &p.za2, &e.za2),
        ("XB1", &p.xb1, &e.xb1),
        ("YB1", &p.yb1, &e.yb1),
        ("ZB1", &p.zb1, &e.zb1),
        ("XB2", &p.xb2, &e.xb2),
        ("YB2", &p.yb2, &e.yb2),
        ("ZB2", &p.zb2, &e.zb2),
        ("XAC", &p.xac, &e.xac),
        ("YAC", &p.yac, &e.yac),
        ("ZAC", &p.zac, &e.zac),
        ("XBC", &p.xbc, &e.xbc),
        ("YBC", &p.ybc, &e.ybc),
        ("ZBC", &p.zbc, &e.zbc),
        ("XC", &p.xc, &e.xc),
        ("YC", &p.yc, &e.yc),
        ("ZC", &p.zc, &e.zc),
        ("XA_TE", &p.xa_te, &e.xa_te),
        ("YA_TE", &p.ya_te, &e.ya_te),
        ("ZA_TE", &p.za_te, &e.za_te),
        ("XB_TE", &p.xb_te, &e.xb_te),
        ("YB_TE", &p.yb_te, &e.yb_te),
        ("ZB_TE", &p.zb_te, &e.zb_te),
    ] {
        c.slice(name, &widen(actual), want);
    }
    c.slice(
        "wing_areas",
        &widen(&vd.wing_areas_m2),
        &expected.wing_areas_m2,
    );
    c.slice(
        "chord_lengths",
        &widen(&vd.chord_lengths_m),
        &expected.chord_lengths_m,
    );
    c.slice(
        "tangent_incidence_angle",
        &widen(&vd.tangent_incidence_angle),
        &expected.tangent_incidence_angle,
    );
    c.slice(
        "panel_areas",
        &widen(&vd.panel_areas_m2),
        &expected.panel_areas_m2,
    );
    for axis in 0..3 {
        c.slice(
            &format!("normals[{axis}]"),
            &vd.normals
                .iter()
                .map(|n| f64::from(n[axis]))
                .collect::<Vec<_>>(),
            &expected.normals.iter().map(|n| n[axis]).collect::<Vec<_>>(),
        );
    }
    c.slice("SLOPE", &widen(&vd.slope), &expected.slope);
    c.slice("SLE", &widen(&vd.sle), &expected.sle);
    c.slice("D", &widen(&vd.d), &expected.d);
    c.finish();
}

/// The eight single-condition solves, with their fields.
///
/// `gamma` and `CP` are compared as well as the eight coefficients, because
/// a wrong influence matrix can still integrate to a coincidentally close
/// total -- the same reasoning `alas-aero::asb_vlm`'s own fixture records for
/// recording its vortex strengths.
#[test]
fn every_single_condition_solve_matches() {
    let fixture = fixture();
    let results = run(
        &fixture.vlm_geometry(),
        &fixture.vlm_settings(),
        &fixture.conditions(),
    )
    .expect("the fixture's vehicle panels and solves");

    let mut c = Comparison::new("alas-aero::vorlax solves", Tier::F32);
    for (case, actual) in fixture.cases.iter().zip(&results.cases) {
        let want = &case.results;
        let tag = &case.tag;
        c.scalar(&format!("{tag}/CL"), actual.cl, want.cl);
        c.scalar(&format!("{tag}/CDi"), actual.cdi, want.cdi);
        c.scalar(&format!("{tag}/CM"), actual.cm, want.cm);
        c.scalar(&format!("{tag}/CYTOT"), actual.cytot, want.cytot);
        c.scalar(&format!("{tag}/CRTOT"), actual.crtot, want.crtot);
        c.scalar(&format!("{tag}/CRMTOT"), actual.crmtot, want.crmtot);
        c.scalar(&format!("{tag}/CNTOT"), actual.cntot, want.cntot);
        c.scalar(&format!("{tag}/CYMTOT"), actual.cymtot, want.cymtot);
        c.slice(&format!("{tag}/CL_wing"), &actual.cl_wing, &want.cl_wing);
        c.slice(&format!("{tag}/CDi_wing"), &actual.cdi_wing, &want.cdi_wing);
        c.slice(&format!("{tag}/cl_y"), &actual.cl_y, &want.cl_y);
        c.slice(&format!("{tag}/cdi_y"), &actual.cdi_y, &want.cdi_y);
        c.slice(&format!("{tag}/CP"), &widen(&actual.cp), &want.cp);
        c.slice(&format!("{tag}/gamma"), &widen(&actual.gamma), &want.gamma);
    }
    c.finish();
}

/// The training grid: the one call the mission depends on.
///
/// `sample_training` builds the outer product of ten angles of attack and
/// eight Mach numbers, at *zero* velocity, and solves all eighty rows at
/// once. The zero is what reaches `VLM`'s 1e-6 substitution, so this is also
/// the only case that exercises it.
#[test]
fn the_surrogate_training_grid_matches() {
    let fixture = fixture();
    let training = &fixture.training;

    let conditions: Vec<VlmCondition> = training
        .angle_of_attack_flat_rad
        .iter()
        .zip(&training.mach_flat)
        .map(|(&alpha, &mach)| VlmCondition {
            angle_of_attack_rad: alpha,
            mach,
            side_slip_angle_rad: 0.0,
            pitch_rate_rad_s: 0.0,
            roll_rate_rad_s: 0.0,
            yaw_rate_rad_s: 0.0,
            velocity_m_s: 0.0,
        })
        .collect();

    let geometry = fixture.vlm_geometry();
    let results =
        run(&geometry, &fixture.vlm_settings(), &conditions).expect("the training grid solves");

    let mut c = Comparison::new("alas-aero::vorlax training grid", Tier::F32);
    c.exact(
        "the grid is subsonic throughout",
        &training.mach.iter().any(|&m| m >= 1.0),
        &false,
    );
    c.slice(
        "CL",
        &results.cases.iter().map(|case| case.cl).collect::<Vec<_>>(),
        &training.cl,
    );
    c.slice(
        "CDi",
        &results
            .cases
            .iter()
            .map(|case| case.cdi)
            .collect::<Vec<_>>(),
        &training.cdi,
    );

    // `calculate_VLM`'s regrouping: a symmetric wing occupies two columns of
    // the per-surface arrays, and they are dimensionalized on the surface
    // areas, summed, and divided by the wing's own reference area.
    let mut surface = 0usize;
    for (index, wing) in geometry.wings.iter().enumerate() {
        let tag = &training.wing_tags[index];
        let count = if wing.symmetric { 2 } else { 1 };
        let (lift, drag): (Vec<f64>, Vec<f64>) = results
            .cases
            .iter()
            .map(|case| {
                let l: f64 = (0..count)
                    .map(|k| {
                        case.cl_wing[surface + k]
                            * f64::from(results.distribution.wing_areas_m2[surface + k])
                    })
                    .sum();
                let d: f64 = (0..count)
                    .map(|k| {
                        case.cdi_wing[surface + k]
                            * f64::from(results.distribution.wing_areas_m2[surface + k])
                    })
                    .sum();
                (l / wing.area_reference_m2, d / wing.area_reference_m2)
            })
            .unzip();
        c.slice(&format!("{tag}/CL"), &lift, &training.wing_cl[tag]);
        c.slice(&format!("{tag}/CDi"), &drag, &training.wing_cdi[tag]);
        surface += count;
    }
    c.finish();
}
