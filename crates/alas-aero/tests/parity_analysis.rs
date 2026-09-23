// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-aero::analysis` against `alas/physics/aerodynamics.py`, via
//! `golden/generators/gen_aero_analysis.py`.
//!
//! # Two tiers, and why they are not one
//!
//! `docs/PORTING.md` names `linalg` for this row, and the three entry points
//! that run the vortex lattice are compared there: their numbers come out of
//! a dense AIC solve, which is the construction that tier describes.
//!
//! The empirical half does not. `swept_pg_beta`,
//! `compressible_report_alpha`, `parasite_drag`, `wave_drag` and
//! `drag_components` evaluate closed-form `f64` arithmetic over a geometry
//! and an atmosphere, with no factorization anywhere in them, and they are
//! compared at `Tier::Closed`, which is a *tighter* bound than the row
//! names, not a looser one. Comparing them at `linalg` would let three orders
//! of magnitude of drift through on a formula whose two implementations
//! evaluate the same products in the same order. This is the split
//! `parity_layout.rs` and `parity_route.rs` already run, applied to the
//! boundary that matters here.
//!
//! # The aircraft
//!
//! Unlike `parity_asb_vlm.rs`, which stands a small probe airplane in for the
//! real geometry, this test runs on the frozen-reference aircraft built by
//! `alas-geom::builder::new_reference_compatibility`: the historical
//! geometry pinned by `golden/geom/builder.json`. The product builder owns a
//! newer transport-planform default and is tested on its own path. Here,
//! `parasite_drag` reads the fuselage's end stations, the nacelle count, every
//! wing's area and the morphed root section's real thickness, and none of
//! those exists on a probe. The fixture also records the aircraft's own
//! reference dimensions, and this test checks them before anything else: if
//! the two sides have stopped meaning the same aeroplane, every comparison
//! below is answering a different question and should say so in one line
//! rather than in forty.
//!
//! # The wave-drag product correction
//!
//! `alas-aero::analysis::AeroAnalysis::wave_drag` applies the published
//! Lock/Korn law, `CD_w = 20 (M - M_crit)^4` with `M_crit` the offset
//! critical Mach, not the drag-divergence Mach `M_dd` the Korn equation
//! itself yields (physics review v1.2, finding A3). The frozen `gen_aero_analysis.py`
//! generator that produced `golden/aero/analysis.json` predates the
//! correction and still records `20 (M - M_dd)^4`, so every `cd_wave` and
//! `cd_total` fixture value at or above `M_dd` disagrees with the corrected
//! product by design. Rather than weaken this comparison, `corrected_wave_drag`
//! below recomputes the same closed-form `M_dd` the fixture and the product
//! still agree on (the Korn equation itself is unchanged) and applies the
//! corrected law, so every other digit of the closed-form arithmetic stays
//! pinned at `Tier::Closed`.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use alas_aero::analysis::TrimPoint;
use alas_config::analysis::AnalysisConfig;
use alas_testkit::{Comparison, Tier};

use support::{aero, analysis_config, build, names, or_nan, reference_mesh, Fixture};

/// The corrected Lock/Korn law's expectation, for a fixture generated
/// against the superseded `20 (M - M_dd)^4` form. See the module-level "wave
/// drag product correction" note.
fn corrected_wave_drag(kappa: f64, sweep_deg: f64, thickness: f64, mach: f64, cl: f64) -> f64 {
    let cos_sweep = sweep_deg.to_radians().cos();
    let mach_dd =
        kappa / cos_sweep - thickness / cos_sweep.powf(2.0) - cl / (10.0 * cos_sweep.powf(3.0));
    let mach_crit = mach_dd - (0.1_f64 / 80.0).cbrt();
    if mach > mach_crit {
        20.0 * (mach - mach_crit).powf(4.0)
    } else {
        0.0
    }
}

/// The superseded law the fixture's generator used (`20 (M - M_dd)^4`),
/// needed to back out a corrected total where the fixture reports only `cd`
/// / `l_over_d` and does not break the wave term out (the `quick` and
/// `trimmed` fixtures). See the module-level "wave drag product correction"
/// note.
fn superseded_wave_drag(kappa: f64, sweep_deg: f64, thickness: f64, mach: f64, cl: f64) -> f64 {
    let cos_sweep = sweep_deg.to_radians().cos();
    let mach_dd =
        kappa / cos_sweep - thickness / cos_sweep.powf(2.0) - cl / (10.0 * cos_sweep.powf(3.0));
    if mach > mach_dd {
        20.0 * (mach - mach_dd).powf(4.0)
    } else {
        0.0
    }
}

#[test]
fn the_two_implementations_are_analysing_the_same_aeroplane() {
    let fixture: Fixture = alas_testkit::load("aero", "analysis");
    let plane = build(true);

    let mut discrete = Comparison::new("analysis.airplane (discrete)", Tier::Exact);
    discrete.exact(
        "wing_names",
        &plane
            .wings
            .iter()
            .map(|w| w.name.clone())
            .collect::<Vec<_>>(),
        &fixture.airplane.wing_names,
    );
    discrete.exact(
        "fuselage_names",
        &plane
            .fuselages
            .iter()
            .map(|f| f.name.clone())
            .collect::<Vec<_>>(),
        &fixture.airplane.fuselage_names,
    );
    discrete.finish();

    let mut numeric = Comparison::new("analysis.airplane", Tier::Closed);
    numeric.scalar("s_ref", plane.s_ref, fixture.airplane.s_ref);
    numeric.scalar("c_ref", plane.c_ref, fixture.airplane.c_ref);
    numeric.scalar("b_ref", plane.b_ref, fixture.airplane.b_ref);
    numeric.slice("xyz_ref", &plane.xyz_ref, &fixture.airplane.xyz_ref);
    let analysis = aero(&plane, &fixture, reference_mesh());
    numeric.scalar(
        "section_thickness",
        analysis.section_thickness(),
        fixture.airplane.section_thickness,
    );
    numeric.finish();
}

#[test]
fn the_empirical_drag_buildup_matches_python() {
    let fixture: Fixture = alas_testkit::load("aero", "analysis");
    let plane = build(true);
    let plane_no_engines = build(false);

    let mut comparison = Comparison::new("analysis (empirical)", Tier::Closed);

    for name in names(&fixture.pg_beta) {
        let case = &fixture.pg_beta[name];
        comparison.scalar(
            &format!("pg_beta.{name}"),
            alas_aero::analysis::swept_pg_beta(case.inputs.mach, case.inputs.sweep_deg),
            case.beta,
        );
    }

    for name in names(&fixture.report_alpha) {
        let case = &fixture.report_alpha[name];
        comparison.scalar(
            &format!("report_alpha.{name}"),
            alas_aero::analysis::compressible_report_alpha(
                case.inputs.alpha_inc_deg,
                case.inputs.alpha_zero_lift_deg,
                case.inputs.mach,
                case.inputs.sweep_deg,
            ),
            case.alpha_deg,
        );
    }

    for name in names(&fixture.parasite) {
        let case = &fixture.parasite[name];
        let target = if case.inputs.include_engines {
            &plane
        } else {
            &plane_no_engines
        };
        let analysis = aero(target, &fixture, reference_mesh());
        comparison.scalar(
            &format!("parasite.{name}"),
            analysis.parasite_drag(
                case.inputs.mach,
                case.inputs.altitude,
                case.inputs.cl,
                None,
                None,
            ),
            case.cd_parasite,
        );
    }

    let analysis = aero(&plane, &fixture, reference_mesh());
    let kappa = analysis.drag.korn_technology_factor;
    let thickness = analysis.section_thickness();
    for name in names(&fixture.wave) {
        let case = &fixture.wave[name];
        comparison.scalar(
            &format!("wave.{name}"),
            analysis.wave_drag(case.inputs.mach, case.inputs.cl, None),
            corrected_wave_drag(
                kappa,
                fixture.sweep_deg,
                thickness,
                case.inputs.mach,
                case.inputs.cl,
            ),
        );
    }

    for name in names(&fixture.components) {
        let case = &fixture.components[name];
        let components = analysis.drag_components(
            case.inputs.mach,
            case.inputs.altitude,
            case.inputs.cl,
            case.inputs.cd_induced,
            None,
        );
        comparison.scalar(
            &format!("components.{name}.cd_parasite"),
            components.cd_parasite,
            case.cd_parasite,
        );
        comparison.scalar(
            &format!("components.{name}.cd_induced"),
            components.cd_induced,
            case.cd_induced,
        );
        let expected_wave = corrected_wave_drag(
            kappa,
            fixture.sweep_deg,
            thickness,
            case.inputs.mach,
            case.inputs.cl,
        );
        comparison.scalar(
            &format!("components.{name}.cd_wave"),
            components.cd_wave,
            expected_wave,
        );
        comparison.scalar(
            &format!("components.{name}.cd_total"),
            components.cd_total(),
            // The fixture's own cd_total was summed with the superseded
            // cd_wave; substitute the corrected term into the same sum
            // rather than compare against the stale total.
            case.cd_total - case.cd_wave + expected_wave,
        );
    }

    comparison.finish();
}

#[test]
fn an_aircraft_with_no_wing_falls_back_to_pythons_own_section_thickness() {
    let fixture: Fixture = alas_testkit::load("aero", "analysis");
    let mut plane = build(true);
    plane.wings.clear();
    let analysis = aero(&plane, &fixture, reference_mesh());

    let mut comparison = Comparison::new("analysis.no_wings", Tier::Closed);
    comparison.scalar(
        "section_thickness",
        analysis.section_thickness(),
        fixture.no_wings.section_thickness,
    );
    // The fallback is only observable through a consumer that is not itself a
    // sum over wings, which is what makes the wave-drag term the check here.
    // The corrected Lock/Korn law applies here too; see the module-level
    // "wave drag product correction" note.
    comparison.scalar(
        "cd_wave",
        analysis.wave_drag(0.86, 0.9, None),
        corrected_wave_drag(
            analysis.drag.korn_technology_factor,
            fixture.sweep_deg,
            analysis.section_thickness(),
            0.86,
            0.9,
        ),
    );
    comparison.scalar(
        "cd_parasite",
        analysis.parasite_drag(0.82, 11000.0, 0.5, None, None),
        fixture.no_wings.cd_parasite,
    );
    comparison.finish();
}

#[test]
fn the_vortex_lattice_fed_estimates_match_python() {
    let fixture: Fixture = alas_testkit::load("aero", "analysis");
    let plane = build(true);

    let mut comparison = Comparison::new("analysis (vlm-fed)", Tier::Linalg);

    for name in names(&fixture.quick) {
        let case = &fixture.quick[name];
        let analysis = aero(
            &plane,
            &fixture,
            analysis_config(case.inputs.spanwise, case.inputs.chordwise),
        );
        let kappa = analysis.drag.korn_technology_factor;
        let thickness = analysis.section_thickness();
        let quick = analysis
            .quick_performance(
                case.inputs.cl_target,
                case.inputs.mach,
                case.inputs.altitude,
            )
            .expect("the nominal aircraft meshes and solves");
        // Corrected Lock/Korn law: back out the fixture's superseded wave
        // term (evaluated at the exact input CL, no solve uncertainty) and
        // substitute the corrected one. See the module-level note.
        let old_wave = superseded_wave_drag(
            kappa,
            fixture.sweep_deg,
            thickness,
            case.inputs.mach,
            case.inputs.cl_target,
        );
        let new_wave = corrected_wave_drag(
            kappa,
            fixture.sweep_deg,
            thickness,
            case.inputs.mach,
            case.inputs.cl_target,
        );
        let expected_cd = case.cd - old_wave + new_wave;
        comparison.scalar(
            &format!("quick.{name}.l_over_d"),
            quick.l_over_d,
            case.inputs.cl_target / expected_cd,
        );
        comparison.scalar(&format!("quick.{name}.alpha"), quick.alpha_deg, case.alpha);
        comparison.scalar(&format!("quick.{name}.cd"), quick.cd, expected_cd);
        comparison.scalar(&format!("quick.{name}.cl"), quick.cl, case.cl);
    }

    let analysis = aero(&plane, &fixture, reference_mesh());
    for name in names(&fixture.trimmed) {
        let case = &fixture.trimmed[name];
        let trim = TrimPoint {
            trim_alpha_deg: case.inputs.trim_alpha_deg,
            trim_ih_deg: or_nan(case.inputs.trim_ih_deg),
            cl_alpha: case.inputs.cl_alpha,
        };
        let trimmed = analysis
            .trimmed_performance(&trim, case.inputs.mach, case.inputs.altitude)
            .expect("the nominal aircraft meshes and solves");
        // Corrected Lock/Korn law: back out the fixture's superseded wave
        // term, evaluated at the trim solve's own settled CL (already
        // pinned within Linalg tolerance by the `cl` comparison below), and
        // substitute the corrected one. See the module-level note.
        let kappa = analysis.drag.korn_technology_factor;
        let thickness = analysis.section_thickness();
        let old_wave = superseded_wave_drag(
            kappa,
            fixture.sweep_deg,
            thickness,
            case.inputs.mach,
            trimmed.cl,
        );
        let new_wave = corrected_wave_drag(
            kappa,
            fixture.sweep_deg,
            thickness,
            case.inputs.mach,
            trimmed.cl,
        );
        let expected_cd = case.cd - old_wave + new_wave;
        comparison.scalar(
            &format!("trimmed.{name}.l_over_d"),
            trimmed.l_over_d,
            trimmed.cl / expected_cd,
        );
        comparison.scalar(
            &format!("trimmed.{name}.alpha"),
            trimmed.alpha_deg,
            case.alpha,
        );
        comparison.scalar(
            &format!("trimmed.{name}.i_h"),
            trimmed.incidence_deg,
            or_nan(case.i_h),
        );
        comparison.scalar(&format!("trimmed.{name}.cd"), trimmed.cd, expected_cd);
        comparison.scalar(&format!("trimmed.{name}.cl"), trimmed.cl, case.cl);
        comparison.scalar(
            &format!("trimmed.{name}.cm_residual"),
            trimmed.cm_residual,
            case.cm_residual,
        );
    }

    for name in names(&fixture.sweep) {
        let case = &fixture.sweep[name];
        let config = AnalysisConfig {
            sweep_n_points: case.inputs.n_points,
            sweep_alpha_min_deg: case.inputs.alpha_min,
            sweep_alpha_max_deg: case.inputs.alpha_max,
            ..reference_mesh()
        };
        let sweep_analysis = aero(&plane, &fixture, config);
        let kappa = sweep_analysis.drag.korn_technology_factor;
        let thickness = sweep_analysis.section_thickness();
        let polar = sweep_analysis
            .run_sweep(case.inputs.mach, case.inputs.altitude)
            .expect("the nominal aircraft meshes and solves");
        comparison.slice(
            &format!("sweep.{name}.alpha"),
            &polar.alpha_deg,
            &case.alpha,
        );
        comparison.slice(&format!("sweep.{name}.cl"), &polar.cl, &case.cl);
        // The corrected Lock/Korn law changes cd_wave and everything summed
        // from it (cd, l_over_d); see the module-level "wave drag product
        // correction" note. Recompute the corrected wave term from the
        // sweep's own solved CL (rather than the fixture's) so the only
        // remaining source of difference between the two sides is the
        // parasite/induced VLM residual this row already tolerates, not a
        // CL mismatch amplified through the quartic term's derivative.
        let corrected_wave: Vec<f64> = polar
            .cl
            .iter()
            .map(|&cl| {
                corrected_wave_drag(kappa, fixture.sweep_deg, thickness, case.inputs.mach, cl)
            })
            .collect();
        let corrected_cd: Vec<f64> = case
            .cd
            .iter()
            .zip(case.cd_wave.iter())
            .zip(corrected_wave.iter())
            .map(|((total, old_wave), new_wave)| total - old_wave + new_wave)
            .collect();
        let corrected_l_over_d: Vec<f64> = case
            .cl
            .iter()
            .zip(corrected_cd.iter())
            .map(|(cl, cd)| cl / cd)
            .collect();
        comparison.slice(&format!("sweep.{name}.cd"), &polar.cd, &corrected_cd);
        comparison.slice(
            &format!("sweep.{name}.cd_induced"),
            &polar.cd_induced,
            &case.cd_induced,
        );
        comparison.slice(
            &format!("sweep.{name}.cd_wave"),
            &polar.cd_wave,
            &corrected_wave,
        );
        comparison.slice(
            &format!("sweep.{name}.cd_parasite"),
            &polar.cd_parasite,
            &case.cd_parasite,
        );
        comparison.slice(&format!("sweep.{name}.cm"), &polar.cm, &case.cm);
        comparison.slice(
            &format!("sweep.{name}.l_over_d"),
            &polar.l_over_d,
            &corrected_l_over_d,
        );
    }

    comparison.finish();
}

#[test]
fn a_trimmed_evaluation_leaves_the_aircraft_it_was_given_unaltered() {
    // Upstream overwrites the stabilizer's twist for the duration of the
    // solve and restores it afterwards; this port copies instead. Either way
    // the aircraft a caller holds must come back unchanged, or every
    // evaluation after the first would run on different geometry, which is
    // the property the generator asserts on its side too.
    let fixture: Fixture = alas_testkit::load("aero", "analysis");
    let plane = build(true);
    let before = plane.clone();
    let analysis = aero(&plane, &fixture, reference_mesh());

    analysis
        .trimmed_performance(
            &TrimPoint {
                trim_alpha_deg: 2.4,
                trim_ih_deg: -7.5,
                cl_alpha: 0.11,
            },
            0.82,
            11000.0,
        )
        .expect("the nominal aircraft meshes and solves");

    assert_eq!(plane.wings, before.wings);
}
