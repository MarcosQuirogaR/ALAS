// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Aerodynamic coefficients and operating-point provenance in exports.

use crate::full_analysis::AnalysisReport;
use alas_config::AlasConfig;
use std::f64::consts::PI;

/// Lift coefficients of the two-term line across its window: fixed, so the
/// result does not move with the polar's point count.
const CLEAN_POLAR_NODES: usize = 7;

/// Mach of the low-speed form of the clean polar, the condition the published
/// clean polars are stated at: below compressibility effects on the wing.
const LOW_SPEED_MACH: f64 = 0.5;

/// `CD = cd0 + k CL^2` with `e = 1 / (pi AR k)`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TwoTermPolar {
    cd0: f64,
    k: f64,
    oswald_e: f64,
}

/// The straight line of drag against `CL^2` through the wave-free drag over the
/// lift window: the two-term definition of CD0 and the Oswald factor
/// (Torenbeek, Advanced Aircraft Design, sec. 4.3; Obert, Aerodynamic Design
/// of Transport Aircraft, ch. 40). CD0 from this line is usually below the true
/// zero-lift drag, and it differs from the three-term coefficients in
/// [`crate::full_analysis::PolarFit`]; handbook and fleet-fit values are on
/// this line, so this is what is compared with them.
fn two_term_polar(
    drag: impl Fn(f64) -> f64,
    window: (f64, f64),
    aspect_ratio: f64,
) -> Option<TwoTermPolar> {
    let (mut sum_x, mut sum_y, mut sum_xx, mut sum_xy) = (0.0, 0.0, 0.0, 0.0);
    for node in 0..CLEAN_POLAR_NODES {
        let cl = window.0 + (window.1 - window.0) * node as f64 / (CLEAN_POLAR_NODES - 1) as f64;
        let (x, y) = (cl * cl, drag(cl));
        sum_x += x;
        sum_y += y;
        sum_xx += x * x;
        sum_xy += x * y;
    }
    let count = CLEAN_POLAR_NODES as f64;
    let k = (count * sum_xy - sum_x * sum_y) / (count * sum_xx - sum_x * sum_x);
    let cd0 = (sum_y - k * sum_x) / count;
    (k.is_finite() && cd0.is_finite() && k > 0.0).then(|| TwoTermPolar {
        cd0,
        k,
        oswald_e: 1.0 / (PI * aspect_ratio * k),
    })
}

/// The clean polar of the report's trimmed drag table at `mach`, wave drag out
/// and trim in, when the table has been built.
fn table_clean_polar(
    report: &AnalysisReport,
    config: &AlasConfig,
    mach: f64,
) -> Option<TwoTermPolar> {
    let artifacts = report.fuel.built_artifacts()?;
    let table = artifacts.drag.table()?;
    let altitude_m = table.reference_altitude_m();
    two_term_polar(
        |cl| table.cd(cl, mach, altitude_m) - table.wave_cd(cl, mach),
        (
            config.analysis.polar_fit_cl_min,
            config.analysis.polar_fit_cl_max,
        ),
        report.polar_fit.aspect_ratio,
    )
}

/// Wave drag and critical Mach of the trimmed drag table at the design point,
/// when the table has been built.
fn table_wave_at_design_point(report: &AnalysisReport, mach: f64) -> Option<serde_json::Value> {
    let artifacts = report.fuel.built_artifacts()?;
    let table = artifacts.drag.table()?;
    let cl = report.design_point.cl;
    Some(serde_json::json!({
        "cl": cl,
        "mach": mach,
        "cd": table.wave_cd(cl, mach),
        "critical_mach": table.critical_mach(cl),
    }))
}

/// Lift coefficients of the exported cruise polar: 0.30 to 0.70 in 0.05.
const CRUISE_POLAR_CL: [f64; 9] = [0.30, 0.35, 0.40, 0.45, 0.50, 0.55, 0.60, 0.65, 0.70];

/// The trimmed drag table at the cruise Mach and its reference altitude, wave
/// drag in, on a fixed lift grid: `[CL, CD, L/D, CD_wave]` rows (the
/// wave-free L/D is `CL / (CD - CD_wave)`). Flight and handbook
/// L/D at a stated CL and Mach (e.g. Obert, Aerodynamic Design of Transport
/// Aircraft, Fig. 24.131, CL 0.50 at M 0.82) are read off this, not off the
/// mission design point, whose CL is the model's own. `max_l_over_d` is the
/// lift coefficient of maximum trimmed L/D on the same table (searched over
/// [`MAX_L_OVER_D_CL_BOUNDS`]; null when the maximum lies on a bound).
fn table_cruise_polar(report: &AnalysisReport, mach: f64) -> Option<serde_json::Value> {
    let artifacts = report.fuel.built_artifacts()?;
    let table = artifacts.drag.table()?;
    let altitude_m = table.reference_altitude_m();
    let points: Vec<[f64; 4]> = CRUISE_POLAR_CL
        .iter()
        .map(|&cl| {
            let cd = table.cd(cl, mach, altitude_m);
            [cl, cd, cl / cd, table.wave_cd(cl, mach)]
        })
        .collect();
    let max_l_over_d = max_l_over_d_point(|cl| cl / table.cd(cl, mach, altitude_m)).map(
        |(cl, l_over_d)| {
            serde_json::json!({
                "cl": cl,
                "l_over_d": l_over_d,
                "condition": "lift coefficient of maximum trimmed L/D at the cruise Mach and the table reference altitude (fixed-Mach specific-range optimum, a and TSFC constant with altitude)",
            })
        },
    );
    Some(serde_json::json!({
        "mach": mach,
        "altitude_m": altitude_m,
        "basis": "trimmed_table_with_wave",
        "points": points,
        "max_l_over_d": max_l_over_d,
    }))
}

/// Search bounds of [`max_l_over_d_point`], lift coefficient.
const MAX_L_OVER_D_CL_BOUNDS: (f64, f64) = (0.15, 1.0);

/// Lift coefficient and value of the maximum of `l_over_d(cl)` inside
/// [`MAX_L_OVER_D_CL_BOUNDS`] by golden-section search (the trimmed polar's
/// L/D is unimodal in CL); `None` when the maximum sits on a bound, i.e.
/// outside the searched range, or the curve is not finite there.
fn max_l_over_d_point(l_over_d: impl Fn(f64) -> f64) -> Option<(f64, f64)> {
    const INV_PHI: f64 = 0.618_033_988_749_894_8;
    const TOLERANCE_CL: f64 = 1.0e-5;
    let (mut lo, mut hi) = MAX_L_OVER_D_CL_BOUNDS;
    let mut x1 = hi - INV_PHI * (hi - lo);
    let mut x2 = lo + INV_PHI * (hi - lo);
    let (mut f1, mut f2) = (l_over_d(x1), l_over_d(x2));
    while hi - lo > TOLERANCE_CL {
        if !(f1.is_finite() && f2.is_finite()) {
            return None;
        }
        if f1 < f2 {
            lo = x1;
            x1 = x2;
            f1 = f2;
            x2 = lo + INV_PHI * (hi - lo);
            f2 = l_over_d(x2);
        } else {
            hi = x2;
            x2 = x1;
            f2 = f1;
            x1 = hi - INV_PHI * (hi - lo);
            f1 = l_over_d(x1);
        }
    }
    let cl = 0.5 * (lo + hi);
    let edge = 10.0 * TOLERANCE_CL;
    let value = l_over_d(cl);
    (value.is_finite()
        && cl > MAX_L_OVER_D_CL_BOUNDS.0 + edge
        && cl < MAX_L_OVER_D_CL_BOUNDS.1 - edge)
        .then_some((cl, value))
}

/// Basis string of [`table_profile_cd0`].
const PROFILE_CD0_BASIS: &str = "zero_lift_profile_build_up";

/// The zero-lift profile drag of the component build-up, `sum(Cf FF Q
/// Swet/S)` with its drag areas and the viscous margin, at `mach` and the
/// trimmed table's reference altitude (its Reynolds number), when the table
/// has been built: no lift-dependent term and no wave drag. Poll and
/// Schumann's tabulated `Cd0` (Aeronautical Journal 2021, Part 2, Eqs. 4, 7
/// and 23: `psi0 CF`, `CF = 0.0269 / Re^0.14` at M 0.5) is this quantity, a
/// profile build-up, not the intercept of a flight polar, so cells sourced
/// from it are compared with this and not with `clean_polar_low_speed`.
fn table_profile_cd0(report: &AnalysisReport, mach: f64) -> Option<serde_json::Value> {
    let artifacts = report.fuel.built_artifacts()?;
    let table = artifacts.drag.table()?;
    let altitude_m = table.reference_altitude_m();
    Some(serde_json::json!({
        "mach": mach,
        "altitude_m": altitude_m,
        "cd0": table.cd0(mach, altitude_m),
        "basis": PROFILE_CD0_BASIS,
    }))
}

/// Basis of `cd0_cruise`, `k_factor` and `oswald_efficiency`. They are the
/// two-term line of the trimmed table or nothing: without the table they are
/// null, never the three-term coefficients of a different definition, which
/// stay under `shifted_polar_fit`.
fn clean_polar_basis(table_built: bool) -> &'static str {
    if table_built {
        "trimmed_table_wave_free_two_term"
    } else {
        "unavailable_drag_table_not_built"
    }
}

pub(super) fn database(report: &AnalysisReport, config: &AlasConfig) -> serde_json::Value {
    let req = &config.requirements;
    let fit = &report.polar_fit;
    let cruise = table_clean_polar(report, config, req.cruise_mach);
    let low_speed = table_clean_polar(report, config, LOW_SPEED_MACH.min(req.cruise_mach));
    serde_json::json!({
        "cruise_mach": req.cruise_mach,
        "cruise_altitude_m": req.cruise_altitude_m,
        "cd0_cruise": cruise.map(|polar| polar.cd0),
        "k_factor": cruise.map(|polar| polar.k),
        "oswald_efficiency": cruise.map(|polar| polar.oswald_e),
        "clean_polar_basis": clean_polar_basis(cruise.is_some()),
        "clean_polar_low_speed": low_speed.map(|polar| serde_json::json!({
            "mach": LOW_SPEED_MACH.min(req.cruise_mach),
            "cd0": polar.cd0,
            "k_factor": polar.k,
            "oswald_efficiency": polar.oswald_e,
        })),
        "profile_cd0_low_speed": table_profile_cd0(report, LOW_SPEED_MACH.min(req.cruise_mach)),
        "wave_drag_design_point": table_wave_at_design_point(report, req.cruise_mach),
        "cruise_polar": table_cruise_polar(report, req.cruise_mach),
        "shifted_polar_fit": {
            "cd0": fit.cd0,
            "k_factor": fit.k,
            "oswald_efficiency": fit.oswald_e,
        },
        "linear_lift_drag_coefficient": fit.c1,
        "aspect_ratio": fit.aspect_ratio,
        "polar_fit_status": fit.status.as_str(),
        "design_point": report.design_point,
        // The mission `design_point` and `trimmed_design_point` are the
        // mid-cruise state of (null: the take-off mass, reference path).
        "cruise_point": report.cruise_point_basis().map(|basis| basis.to_json()),
        "static_margin": report.static_margin,
        "trimmed_design_point": report.trimmed_design_point,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_term_line_recovers_a_parabola_and_lies_below_a_shifted_one() {
        let ar = 10.0;
        let k = 1.0 / (PI * ar * 0.8);
        let polar = two_term_polar(|cl| 0.02 + k * cl * cl, (0.3, 0.6), ar)
            .unwrap_or_else(|| panic!("a parabola fits"));
        assert!((polar.cd0 - 0.02).abs() < 1e-12);
        assert!((polar.oswald_e - 0.8).abs() < 1e-12);
        // The minimum at positive lift of a real polar: the two-term intercept
        // lies below the true zero-lift drag and e below the curvature's.
        let shifted = two_term_polar(|cl| 0.02 - 0.004 * cl + k * cl * cl, (0.3, 0.6), ar)
            .unwrap_or_else(|| panic!("a parabola fits"));
        assert!(shifted.cd0 < 0.02);
        assert!(shifted.oswald_e > 0.8);
    }

    #[test]
    fn the_clean_polar_names_no_basis_it_does_not_have() {
        assert_eq!(clean_polar_basis(true), "trimmed_table_wave_free_two_term");
        assert_eq!(clean_polar_basis(false), "unavailable_drag_table_not_built");
    }

    #[test]
    fn a_falling_line_is_no_polar() {
        assert_eq!(
            two_term_polar(|cl| 0.05 - 0.01 * cl, (0.3, 0.6), 10.0),
            None
        );
    }

    #[test]
    fn the_max_l_over_d_point_of_a_parabolic_polar_is_sqrt_cd0_over_k() {
        // CD = CD0 + k CL^2: L/D peaks at CL = sqrt(CD0 / k), value 1 / (2 sqrt(CD0 k)).
        let (cd0, k) = (0.015, 0.055);
        let (cl, l_over_d) = max_l_over_d_point(|cl| cl / (cd0 + k * cl * cl))
            .unwrap_or_else(|| panic!("an interior maximum"));
        assert!((cl - (cd0 / k).sqrt()).abs() < 1.0e-4, "{cl}");
        assert!((l_over_d - 0.5 / (cd0 * k).sqrt()).abs() < 1.0e-6, "{l_over_d}");
        // Maximum outside the searched CL 0.15-1.0: no point, not the bound.
        // Above: sqrt(0.05 / 0.01) = 2.24; below: sqrt(0.001 / 0.1) = 0.10.
        assert_eq!(max_l_over_d_point(|cl| cl / (0.05 + 0.01 * cl * cl)), None);
        assert_eq!(max_l_over_d_point(|cl| cl / (0.001 + 0.1 * cl * cl)), None);
    }
}
