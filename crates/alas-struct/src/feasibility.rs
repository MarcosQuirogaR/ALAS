// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Structural feasibility inside the supported linear beam model.
//!
//! This is a preliminary sizing and model-domain check, not an airworthiness
//! claim. Strength, panel spacing and numerical validity are independent of
//! the chosen objective. Deformation is assessed by the actual small-slope
//! approximation, not by an arbitrary metre limit or a desired wing shape.
//!
//! For a graph z(y), exact curvature is z''/(1+z'^2)^(3/2). The relative
//! difference introduced by the linear approximation z'' is therefore
//! (1+z'^2)^(3/2)-1. See the small-angle beam derivation in MIT OCW,
//! <https://ocw.mit.edu/courses/8-01sc-classical-mechanics-fall-2016/mit8_01scs22_chapter26.pdf>,
//! section 26.5. A 5% budget below is an explicit engineering choice for
//! model applicability; it is neither a certified deflection limit nor a
//! bound on total model error (sweep, torsion and aeroelastic loads remain
//! unrepresented by the spanwise Euler-Bernoulli model).
//!
//! The budget gates the 1 g flight shape only. That is the case whose
//! deflection the aircraft model uses; at the ultimate manoeuvre cases the
//! quantities the sizing consumes are bending moments and stresses, which the
//! beam model forms on the undeformed geometry and which therefore do not
//! contain the small-slope approximation at all (engineering statement: the
//! geometric nonlinearity it omits, lift following the rotated surface and
//! span foreshortening, lowers the root moment, so the linear stress is the
//! conservative one). Neither is ultimate deflection a certification quantity:
//! EASA CS-25, CS 25.305(a) limits deformation at *limit* load to what does
//! not interfere with safe operation, and CS 25.305(b) asks only that ultimate
//! load be carried for three seconds without failure. Real certified wings
//! leave the 5 % budget there by a wide margin: the Boeing 787 static-test
//! wing flexed about 7.6 m (25 ft) upward on a 30 m semispan at ultimate load
//! (Boeing news release, "787 Dreamliner completes ultimate-load wing test",
//! 28 March 2010), a tip slope near 0.34 rad and a curvature error near 18 %.
//! Gating the ultimate cases made [`crate::sizing::size_for_linear_model`]
//! buy cap area to fit the aeroplane to the analysis rather than to a load,
//! doubling the primary box on the A320-200. The ultimate-case error is still
//! published, in
//! [`StructuralFeasibility::manoeuvre_curvature_relative_error`], as the
//! error bound on the ultimate deflection the report shows.

use crate::analytical::StructuralAnalysisReport;
use crate::sizing::WingboxSizing;

/// The load case whose deflection the linear-model budget gates.
pub const FLIGHT_SHAPE_CASE: &str = "level";

/// Numerical applicability budget for the linear beam calculation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearModelLimits {
    /// Permitted relative error from dropping the exact curvature denominator.
    pub max_curvature_relative_error: f64,
}

impl Default for LinearModelLimits {
    fn default() -> Self {
        Self {
            max_curvature_relative_error: 0.05,
        }
    }
}

/// Dimensionless, graded checks; values remain visible when a candidate fails.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StructuralFeasibility {
    /// Primary structural mass of the full symmetric wing, kg (2 semiwings).
    /// Its difference from an empirical complete-wing estimate is diagnostic.
    pub primary_mass_kg: f64,
    /// Actual product FE shell/bar mass of the full wing, kg, excluding
    /// concentrated fuel/engine masses. Native beam-only assessment leaves
    /// this absent; the coupled candidate stage builds and checks the mesh.
    pub mesh_primary_mass_kg: Option<f64>,
    /// All required arrays, sections, loads and numerical results were valid.
    pub input_valid: bool,
    /// Largest demand/allowable ratio over the strength-sized spars.
    pub max_strength_utilization: f64,
    /// Installed rib spacing divided by its panel-buckling maximum.
    pub rib_spacing_ratio: f64,
    /// Largest cap width/available width ratio (adjacent caps and chord edges).
    pub cap_packaging_ratio: f64,
    /// Relative curvature approximation error of the 1 g flight shape: the
    /// quantity the linear-model budget gates (see the module documentation).
    pub max_linear_curvature_relative_error: f64,
    /// Largest relative curvature approximation error over the ultimate
    /// manoeuvre cases. Reported as the error bound on the ultimate
    /// deflection; it does not gate the section, whose ultimate stresses do
    /// not depend on the small-slope approximation.
    pub manoeuvre_curvature_relative_error: f64,
    /// Largest absolute tip displacement divided by modelled semispan, over
    /// every load case.
    pub max_tip_deflection_ratio: f64,
    /// Largest absolute integral of M/EI of the 1 g flight shape,
    /// dimensionless dz/dy.
    pub max_abs_slope: f64,
    /// Case the gated curvature error belongs to: the 1 g level case.
    pub governing_load_case: &'static str,
    /// The signed load factor of that case.
    pub governing_load_factor: f64,
    /// The explicit error budget used for this assessment.
    pub limits: LinearModelLimits,
}

impl StructuralFeasibility {
    /// All available preliminary checks pass, including linear-model domain.
    /// This does not cover gust, aeroelasticity, fatigue or certification.
    pub fn passes(&self) -> bool {
        self.input_valid
            && self.primary_mass_kg.is_finite()
            && self.primary_mass_kg > 0.0
            && self
                .mesh_primary_mass_kg
                .is_none_or(|mass| mass.is_finite() && mass > 0.0)
            && self.max_strength_utilization <= 1.0 + crate::sizing::MARGIN_NUMERICAL_ZERO
            && self.rib_spacing_ratio <= 1.0
            && self.cap_packaging_ratio <= 1.0 + crate::sizing::MARGIN_NUMERICAL_ZERO
            && self.max_linear_curvature_relative_error <= self.limits.max_curvature_relative_error
    }

    fn invalid(limits: LinearModelLimits) -> Self {
        Self {
            primary_mass_kg: f64::INFINITY,
            mesh_primary_mass_kg: None,
            input_valid: false,
            max_strength_utilization: f64::INFINITY,
            rib_spacing_ratio: f64::INFINITY,
            cap_packaging_ratio: f64::INFINITY,
            max_linear_curvature_relative_error: f64::INFINITY,
            manoeuvre_curvature_relative_error: f64::INFINITY,
            max_tip_deflection_ratio: f64::INFINITY,
            max_abs_slope: f64::INFINITY,
            governing_load_case: "unavailable",
            governing_load_factor: f64::NAN,
            limits,
        }
    }
}

fn finite(values: &[f64], count: usize) -> bool {
    values.len() == count && values.iter().all(|v| v.is_finite())
}

/// Evaluate a strength-sized box and its response under the same load state.
///
/// Stations and deflections are metres, stiffness N m^2 and moments N m.
/// `y` runs root to tip; deflection/load signs are preserved. The maximum
/// absolute slope is evaluated for pull-up ultimate, push-down ultimate and
/// 1 g; the 1 g value is gated and the ultimate values are reported.
/// Invalid data fail closed; +infinite *strength margins* at zero demand are
/// the sole intentional nonfinite input permitted.
pub fn assess(
    sizing: &WingboxSizing,
    report: &StructuralAnalysisReport,
    limits: LinearModelLimits,
) -> StructuralFeasibility {
    let invalid = || StructuralFeasibility::invalid(limits);
    let count = report.y.len();
    if count < 2
        || !limits.max_curvature_relative_error.is_finite()
        || limits.max_curvature_relative_error <= 0.0
        || !finite(&report.y, count)
        || report.y[0] != 0.0
        || report.y.windows(2).any(|pair| pair[1] <= pair[0])
        || sizing.y_stations != report.y
        || !finite(&report.ei_nm2, count)
        || report.ei_nm2.iter().any(|ei| *ei <= 0.0)
        || !finite(&sizing.chord, count)
        || sizing.chord.iter().any(|chord| *chord <= 0.0)
        || sizing.spars.is_empty()
        || !sizing.total_mass_kg.is_finite()
        || sizing.total_mass_kg <= 0.0
        || [
            sizing.mass_breakdown_kg.spar_caps,
            sizing.mass_breakdown_kg.spar_webs,
            sizing.mass_breakdown_kg.skin,
            sizing.mass_breakdown_kg.ribs,
        ]
        .iter()
        .any(|mass| !mass.is_finite() || *mass < 0.0)
        || !sizing.t_skin.is_finite()
        || sizing.t_skin <= 0.0
        || report.load_cases.len() != 3
        || !["pull-up", "push-down", "level"].iter().all(|name| {
            report
                .load_cases
                .iter()
                .filter(|case| case.name == *name)
                .count()
                == 1
        })
    {
        return invalid();
    }
    let mut strength_utilization = 0.0_f64;
    let mut cap_packaging_ratio = 0.0_f64;
    for spar in &sizing.spars {
        if !spar.chord_fraction.is_finite()
            || spar.chord_fraction <= 0.0
            || spar.chord_fraction >= 1.0
        {
            return invalid();
        }
        if [
            &spar.h,
            &spar.w_cap,
            &spar.t_cap,
            &spar.a_cap,
            &spar.frac_moment,
        ]
        .iter()
        .any(|values| !finite(values, count) || values.iter().any(|v| *v < 0.0))
            || !spar.t_web.is_finite()
            || spar.t_web <= 0.0
            || spar.margin_of_safety.len() != count
        {
            return invalid();
        }
        for &margin in &spar.margin_of_safety {
            if margin.is_nan() || margin == f64::NEG_INFINITY || margin < -1.0 {
                return invalid();
            }
            let utilization = if margin == f64::INFINITY {
                0.0
            } else {
                1.0 / (1.0 + margin)
            };
            strength_utilization = strength_utilization.max(utilization);
        }
        for (j, width) in spar.w_cap.iter().enumerate() {
            if spar.a_cap[j] > 0.0 {
                let edge_space =
                    2.0 * spar.chord_fraction.min(1.0 - spar.chord_fraction) * sizing.chord[j];
                cap_packaging_ratio = cap_packaging_ratio.max(width / edge_space);
            }
        }
    }
    for pair in sizing.spars.windows(2) {
        let gap = pair[1].chord_fraction - pair[0].chord_fraction;
        if gap <= 0.0 {
            return invalid();
        }
        for j in 0..count {
            if pair[0].a_cap[j] > 0.0 && pair[1].a_cap[j] > 0.0 {
                cap_packaging_ratio = cap_packaging_ratio
                    .max((pair[0].w_cap[j] + pair[1].w_cap[j]) / (2.0 * gap * sizing.chord[j]));
            }
        }
    }
    let rib_ratio = sizing.installed_rib_spacing_m() / sizing.rib_spacing_m;
    if !rib_ratio.is_finite() || rib_ratio <= 0.0 {
        return invalid();
    }
    let semispan = report.y[count - 1];
    let mut assessment = StructuralFeasibility {
        primary_mass_kg: 2.0 * sizing.total_mass_kg,
        mesh_primary_mass_kg: None,
        input_valid: true,
        max_strength_utilization: strength_utilization,
        rib_spacing_ratio: rib_ratio,
        cap_packaging_ratio,
        max_linear_curvature_relative_error: 0.0,
        manoeuvre_curvature_relative_error: 0.0,
        max_tip_deflection_ratio: 0.0,
        max_abs_slope: 0.0,
        governing_load_case: "level",
        governing_load_factor: 1.0,
        limits,
    };
    for case in &report.load_cases {
        if case.y != report.y
            || !case.load_factor.is_finite()
            || (case.name == "pull-up" && case.load_factor <= 1.0)
            || (case.name == "push-down" && case.load_factor >= 0.0)
            || (case.name == "level" && case.load_factor != 1.0)
            || !case.tip_deflection_m.is_finite()
            || [
                &case.moment_nm,
                &case.shear_n,
                &case.q_net,
                &case.deflection_m,
            ]
            .iter()
            .any(|values| !finite(values, count))
            || case.spar_stress.len() != sizing.spars.len()
            || case.spar_stress.iter().any(|spar| {
                !finite(&spar.stress_pa, count)
                    || spar.stress_pa.iter().any(|stress| *stress < 0.0)
                    || spar.margin_of_safety.len() != count
                    || spar
                        .margin_of_safety
                        .iter()
                        .any(|margin| margin.is_nan() || *margin == f64::NEG_INFINITY)
            })
        {
            return invalid();
        }
        for spar in &case.spar_stress {
            for &margin in &spar.margin_of_safety {
                let utilization = if margin == f64::INFINITY {
                    0.0
                } else if margin <= -1.0 {
                    f64::INFINITY
                } else {
                    1.0 / (1.0 + margin)
                };
                assessment.max_strength_utilization =
                    assessment.max_strength_utilization.max(utilization);
            }
        }
        let mut slope = 0.0_f64;
        let mut max_slope = 0.0_f64;
        for j in 1..count {
            slope += 0.5
                * (report.y[j] - report.y[j - 1])
                * (case.moment_nm[j - 1] / report.ei_nm2[j - 1]
                    + case.moment_nm[j] / report.ei_nm2[j]);
            max_slope = max_slope.max(slope.abs());
        }
        // Include the observed secant slope as a consistency safeguard for
        // externally populated reports, not just the moment-derived slope.
        for j in 1..count {
            max_slope = max_slope.max(
                ((case.deflection_m[j] - case.deflection_m[j - 1])
                    / (report.y[j] - report.y[j - 1]))
                    .abs(),
            );
        }
        max_slope = max_slope.max(case.tip_deflection_m.abs() / semispan);
        let error = (1.0 + max_slope * max_slope).powf(1.5) - 1.0;
        if !error.is_finite() {
            return invalid();
        }
        if case.name == FLIGHT_SHAPE_CASE {
            assessment.max_linear_curvature_relative_error = error;
            assessment.max_abs_slope = max_slope;
            assessment.governing_load_case = case.name;
            assessment.governing_load_factor = case.load_factor;
        } else {
            assessment.manoeuvre_curvature_relative_error =
                assessment.manoeuvre_curvature_relative_error.max(error);
        }
        assessment.max_tip_deflection_ratio = assessment
            .max_tip_deflection_ratio
            .max(case.tip_deflection_m.abs() / semispan);
    }
    assessment
}
