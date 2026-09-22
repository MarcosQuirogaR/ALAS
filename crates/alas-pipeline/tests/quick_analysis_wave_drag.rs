// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]
// The printed coefficients are the numerical evidence this test records.
#![allow(clippy::print_stderr)]

//! Quick Analysis wave-drag ownership on the reduced default aircraft: the
//! full-analysis polar fit carries the Korn wave term at the requested
//! cruise Mach, the cruise solve's base polar does not, and the solve adds
//! the wave term exactly once. Covers a wave-active cruise Mach and the
//! zero-wave case (`wave_drag_coefficient = 0`).

use alas_aero::analysis::AeroAnalysis;
use alas_config::{AlasConfig, DesignVector};
use alas_pipeline::full_analysis::{AnalysisReport, FullAnalysis};
use alas_pipeline::quick_analysis::{reduced_config, CruiseSolve};

/// A cruise Mach past the default aircraft's drag-divergence Mach inside
/// the polar fit window, so the sweep's wave term is non-zero there.
const WAVE_ACTIVE_CRUISE_MACH: f64 = 0.86;

fn reduced_report(cruise_mach: f64, wave_drag_coefficient: f64) -> (AlasConfig, AnalysisReport) {
    let mut config = AlasConfig::default();
    config.requirements.cruise_mach = cruise_mach;
    config.drag_model.wave_drag_coefficient = wave_drag_coefficient;
    let reduced = reduced_config(&config);
    let report = FullAnalysis::new(reduced.clone())
        .run(&DesignVector::default(), true)
        .expect("reduced full analysis runs on the default aircraft");
    (reduced, report)
}

/// The wave terms of the sweep points the polar fit selected: the configured
/// window, or the documented fallback window when it holds fewer than three.
fn window_wave_terms(config: &AlasConfig, report: &AnalysisReport) -> Vec<f64> {
    let cfg = &config.analysis;
    let select = |lo: f64, hi: f64| -> Vec<f64> {
        report
            .polar
            .cl
            .iter()
            .zip(&report.polar.cd_wave)
            .filter(|(&cl, _)| cl > lo && cl < hi)
            .map(|(_, &wave)| wave)
            .collect()
    };
    let primary = select(cfg.polar_fit_cl_min, cfg.polar_fit_cl_max);
    if primary.len() >= 3 {
        primary
    } else {
        select(cfg.polar_fit_cl_min_fallback, cfg.polar_fit_cl_max_fallback)
    }
}

#[test]
fn the_cruise_solve_applies_the_wave_term_once_where_the_fitted_polar_already_has_it() {
    let default_wave = AlasConfig::default().drag_model.wave_drag_coefficient;
    let (with_wave, report_wave) = reduced_report(WAVE_ACTIVE_CRUISE_MACH, default_wave);
    let (zero_wave, report_zero) = reduced_report(WAVE_ACTIVE_CRUISE_MACH, 0.0);
    let design = DesignVector::default();
    let mass_kg = with_wave.requirements.mtow_kg;

    // Precondition: the sweep carries wave drag inside the fit window.
    let window = window_wave_terms(&with_wave, &report_wave);
    assert!(
        window.iter().any(|&wave| wave > 0.0),
        "no wave drag inside the fit window at Mach {WAVE_ACTIVE_CRUISE_MACH}: {window:?}"
    );
    assert!(window_wave_terms(&zero_wave, &report_zero)
        .iter()
        .all(|&wave| wave == 0.0));

    // Reproduction of the finding: the full-analysis fit differs with and
    // without the wave term, so it contains it.
    let total_wave = report_wave.polar_fit;
    let total_zero = report_zero.polar_fit;
    assert!(
        (total_wave.cd0 - total_zero.cd0).abs() > 1e-6
            || (total_wave.k - total_zero.k).abs() > 1e-6,
        "the fitted total polar should carry the wave term: {total_wave:?} vs {total_zero:?}"
    );

    // Correction: the cruise solve's base polar is identical with and without
    // the wave term, because it is fitted to parasite plus induced drag only.
    let solve_wave = CruiseSolve::new(
        &with_wave,
        &report_wave.airplane,
        &design,
        &report_wave,
        mass_kg,
    )
    .expect("cruise solve binds with wave drag");
    let solve_zero = CruiseSolve::new(
        &zero_wave,
        &report_zero.airplane,
        &design,
        &report_zero,
        mass_kg,
    )
    .expect("cruise solve binds without wave drag");
    let base_wave = solve_wave.base_polar();
    let base_zero = solve_zero.base_polar();
    assert!(
        (base_wave.cd0 - base_zero.cd0).abs() < 1e-12,
        "{base_wave:?} vs {base_zero:?}"
    );
    assert!(
        (base_wave.k - base_zero.k).abs() < 1e-12,
        "{base_wave:?} vs {base_zero:?}"
    );
    assert_eq!(base_wave.status, base_zero.status);

    // Applied once: at the requested cruise point the solve's wave term is one
    // Korn evaluation at the flight Mach and lift coefficient, and the total
    // exceeds the zero-wave total by exactly that term.
    let mach = with_wave.requirements.cruise_mach;
    let altitude_m = with_wave.requirements.cruise_altitude_m;
    let drag_wave = solve_wave
        .drag_coefficients(mach, altitude_m)
        .expect("usable q");
    let drag_zero = solve_zero
        .drag_coefficients(mach, altitude_m)
        .expect("usable q");
    let aero = AeroAnalysis::new(
        &report_wave.airplane,
        design.sweep_deg,
        Some(with_wave.geometry.clone()),
        Some(with_wave.drag_model.clone()),
        Some(with_wave.analysis.clone()),
    );
    let korn = aero.wave_drag(mach, drag_wave.cl, None);
    assert!(
        korn > 0.0,
        "the requested cruise point should sit past drag divergence"
    );
    assert_eq!(drag_wave.cd_wave.to_bits(), korn.to_bits());
    assert_eq!(drag_zero.cd_wave.to_bits(), 0.0f64.to_bits());
    assert!((drag_wave.cl - drag_zero.cl).abs() < 1e-15);
    assert!((drag_wave.cd_base - drag_zero.cd_base).abs() < 1e-12);
    let once = drag_wave.cd_total() - drag_zero.cd_total();
    assert!(
        (once - korn).abs() < 1e-12,
        "total difference {once} should be one wave term {korn}"
    );

    // The superseded formulation (total fit plus a second Korn term) sat above
    // the corrected total by the fit-window wave content; record the size.
    let cl = drag_wave.cl;
    let superseded = total_wave.cd0 + total_wave.k * cl * cl + korn;
    let duplicate = superseded - drag_wave.cd_total();
    assert!(
        duplicate > 0.0,
        "the superseded total {superseded} should exceed {}",
        drag_wave.cd_total()
    );
    eprintln!(
        "wave-active Mach {mach}: window cd_wave {window:?}; total fit (cd0 {:.6}, k {:.6}) vs base (cd0 {:.6}, k {:.6}); cl {cl:.4}; korn {korn:.6}; corrected cd {:.6}; superseded cd {superseded:.6}; duplicate {duplicate:.6}",
        total_wave.cd0, total_wave.k, base_wave.cd0, base_wave.k, drag_wave.cd_total()
    );
}

#[test]
fn a_zero_wave_aircraft_keeps_the_fitted_polar_as_its_whole_drag() {
    let (zero_wave, report) = reduced_report(WAVE_ACTIVE_CRUISE_MACH, 0.0);
    let design = DesignVector::default();
    let solve = CruiseSolve::new(
        &zero_wave,
        &report.airplane,
        &design,
        &report,
        zero_wave.requirements.mtow_kg,
    )
    .expect("cruise solve binds");
    let base = solve.base_polar();
    let total = report.polar_fit;
    assert!(
        (base.cd0 - total.cd0).abs() < 1e-12,
        "{base:?} vs {total:?}"
    );
    assert!((base.k - total.k).abs() < 1e-12, "{base:?} vs {total:?}");
    for mach in [0.5, 0.7, WAVE_ACTIVE_CRUISE_MACH] {
        let drag = solve
            .drag_coefficients(mach, zero_wave.requirements.cruise_altitude_m)
            .expect("usable q");
        assert_eq!(drag.cd_wave.to_bits(), 0.0f64.to_bits());
        let polar = total.cd0 + total.k * drag.cl * drag.cl;
        assert!(
            (drag.cd_total() - polar).abs() < 1e-12,
            "Mach {mach}: {} vs {polar}",
            drag.cd_total()
        );
    }
}

#[test]
fn the_default_cruise_point_reports_its_wave_state() {
    let default = AlasConfig::default();
    let (reduced, report) = reduced_report(
        default.requirements.cruise_mach,
        default.drag_model.wave_drag_coefficient,
    );
    let design = DesignVector::default();
    let solve = CruiseSolve::new(
        &reduced,
        &report.airplane,
        &design,
        &report,
        reduced.requirements.mtow_kg,
    )
    .expect("cruise solve binds");
    let window = window_wave_terms(&reduced, &report);
    let drag = solve
        .drag_coefficients(
            reduced.requirements.cruise_mach,
            reduced.requirements.cruise_altitude_m,
        )
        .expect("usable q");
    let total = report.polar_fit;
    let base = solve.base_polar();
    eprintln!(
        "default Mach {}: window cd_wave {window:?}; total fit (cd0 {:.6}, k {:.6}); base (cd0 {:.6}, k {:.6}); cruise cl {:.4}; cd_wave {:.6}; cd {:.6}",
        reduced.requirements.cruise_mach, total.cd0, total.k, base.cd0, base.k, drag.cl, drag.cd_wave, drag.cd_total()
    );
    // With no wave drag among the fitted points the base and total fits agree
    // exactly. Otherwise the correction removes the fit window's own wave
    // content from the total polar so the single Korn evaluation at the
    // requested cruise point is not counted twice. Physics review v1.2
    // (finding A3) corrected `wave_drag` to the published Lock/Korn law,
    // `CD_w = 20 (M - M_crit)^4` rather than `20 (M - M_dd)^4`: M_crit sits
    // below M_dd by `(0.1/80)^(1/3) ~= 0.108`, so the fit window now carries
    // wave drag starting noticeably before drag divergence instead of being
    // pinned at zero there. At the default aircraft's M0.84/CL 0.68 cruise
    // point this raises the duplicate fit-window content to several dozen
    // drag counts (order 1e-3, ~10% of cd_total here) rather than the < 1
    // count the superseded law left below M_dd. The physically meaningful
    // bound is that the correction stays a minority of total drag, not that
    // it is negligible.
    let shift = (total.cd0 + total.k * drag.cl * drag.cl) - drag.cd_base;
    eprintln!("default cruise cd shift removed by the correction: {shift:.3e}");
    if window.iter().all(|&wave| wave == 0.0) {
        assert!((base.cd0 - total.cd0).abs() < 1e-12);
        assert!((base.k - total.k).abs() < 1e-12);
    } else {
        assert!(
            shift.abs() < 0.3 * drag.cd_total(),
            "shift {shift} should stay a minority of cd_total {}",
            drag.cd_total()
        );
    }
    assert!(drag.cd_total().is_finite() && drag.cd_total() > 0.0);
}

/// The error a cruise solve refuses to bind with; `CruiseSolve` has no
/// `Debug`, so `expect_err` cannot be used.
fn refusal(result: Result<CruiseSolve<'_>, String>, what: &str) -> String {
    match result {
        Ok(_) => panic!("{what} must not bind"),
        Err(error) => error,
    }
}

#[test]
fn a_retained_fallback_polar_is_refused_instead_of_published() {
    let default = AlasConfig::default();
    let (reduced, report) = reduced_report(
        default.requirements.cruise_mach,
        default.drag_model.wave_drag_coefficient,
    );
    let design = DesignVector::default();
    let mass_kg = reduced.requirements.mtow_kg;
    CruiseSolve::new(&reduced, &report.airplane, &design, &report, mass_kg)
        .expect("the computed reduced report binds");

    // Too few sweep points: the fit retains its constants and the solve
    // refuses them, naming the status.
    let mut short = report.clone();
    short.polar.alpha_deg.truncate(1);
    short.polar.geometric_alpha_deg.truncate(1);
    short.polar.cl.truncate(1);
    short.polar.cd.truncate(1);
    short.polar.cd_induced.truncate(1);
    short.polar.cd_wave.truncate(1);
    short.polar.cd_parasite.truncate(1);
    short.polar.cm.truncate(1);
    short.polar.l_over_d.truncate(1);
    let error = refusal(
        CruiseSolve::new(&reduced, &short.airplane, &design, &short, mass_kg),
        "a retained fallback polar",
    );
    assert!(error.contains("fallback_insufficient_points"), "{error}");
    assert!(error.contains("unsupported"), "{error}");

    // A non-finite sweep value: the least-squares fallback is refused too.
    let mut broken = report.clone();
    let mid = broken.polar.cd_parasite.len() / 2;
    broken.polar.cd_parasite[mid] = f64::NAN;
    let error = refusal(
        CruiseSolve::new(&reduced, &broken.airplane, &design, &broken, mass_kg),
        "a least-squares fallback polar",
    );
    assert!(error.contains("fallback_least_squares_failure"), "{error}");
}
