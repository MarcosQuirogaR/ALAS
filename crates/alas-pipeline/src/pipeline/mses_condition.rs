// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Map the aircraft state to the existing exposed-root MSES proxy.

use alas_config::AlasConfig;

use crate::full_analysis::AnalysisReport;

/// Freestream Reynolds number, normal Mach and downwash-adjusted section
/// incidence. This mean whole-aircraft wake angle is only a section proxy;
/// a resolved spanwise viscous analysis remains outside this mapping.
#[derive(Debug, Clone, Copy)]
pub(super) struct MsesSectionCondition {
    pub(super) mach: f64,
    pub(super) reynolds: f64,
    pub(super) alpha_deg: f64,
}

pub(super) fn mses_section_condition(
    config: &AlasConfig,
    report: &AnalysisReport,
) -> MsesSectionCondition {
    let freestream_mach = config.requirements.cruise_mach;
    let atmo = alas_atmo::Atmosphere::new(config.requirements.cruise_altitude_m);
    let velocity = freestream_mach * atmo.speed_of_sound();
    let inboard_section = config
        .geometry
        .wing
        .inboard_aerodynamic_station(&report.design)
        .ok();
    let section_chord_m =
        inboard_section.map_or(report.design.root_chord_m, |section| section.chord_m);
    let section_twist_deg = inboard_section
        .map_or(config.geometry.wing.root_twist_deg, |section| {
            section.twist_deg
        });
    let reynolds =
        (atmo.density() * velocity * section_chord_m) / atmo.dynamic_viscosity().max(1e-9);
    // Section Mach normal to the built wing's mean quarter-chord line.
    let quarter_chord_sweep_deg = alas_aero::analysis::AeroAnalysis::quarter_chord_sweep_deg(
        &report.airplane,
        report.design.sweep_deg,
    );
    let mach = freestream_mach * quarter_chord_sweep_deg.to_radians().cos();
    let body_alpha_deg = report
        .trimmed_design_point
        .map_or(report.design_point.alpha_deg, |trim| {
            trim.geometric_body_alpha_deg
        });
    let cl = report
        .trimmed_design_point
        .map_or(report.design_point.cl, |trim| trim.cl);
    let induced_cd = report
        .fuel
        .artifacts(config, &report.design)
        .ok()
        .map(|artifacts| match &artifacts.drag {
            alas_opt::mdo::CandidateDrag::Table(table) => table.induced_cd(cl),
            alas_opt::mdo::CandidateDrag::External(polar) => polar.induced_factor_k * cl * cl,
        })
        .filter(|value| value.is_finite() && *value >= 0.0)
        // Reports without candidate artifacts retain their measured wake
        // samples as an explicit fallback, never the total fitted curvature.
        .or_else(|| polar_induced_cd(report, cl));
    let induced_angle_deg = mean_induced_angle_deg(cl, induced_cd.unwrap_or(f64::NAN));
    let alpha_deg = body_alpha_deg + section_twist_deg - induced_angle_deg;

    MsesSectionCondition {
        mach,
        reynolds,
        alpha_deg,
    }
}

fn polar_induced_cd(report: &AnalysisReport, cl: f64) -> Option<f64> {
    report
        .polar
        .cl
        .windows(2)
        .zip(report.polar.cd_induced.windows(2))
        .find_map(|(lift, drag)| {
            if !lift.iter().chain(drag).all(|value| value.is_finite())
                || drag.iter().any(|value| *value < 0.0)
                || lift[0] == lift[1]
                || (cl - lift[0]) * (cl - lift[1]) > 0.0
            {
                return None;
            }
            Some(drag[0] + (cl - lift[0]) / (lift[1] - lift[0]) * (drag[1] - drag[0]))
        })
}

/// Wake-power mean downwash angle: `tan(alpha_i) = CDi / CL`. Parasite and
/// wave drag do not produce this induced angle. Missing or malformed wake
/// data retains the existing neutral section-proxy fallback.
pub(super) fn mean_induced_angle_deg(cl: f64, induced_cd: f64) -> f64 {
    if !cl.is_finite() || cl.abs() <= f64::EPSILON || !induced_cd.is_finite() || induced_cd < 0.0 {
        return 0.0;
    }
    (induced_cd / cl).atan().to_degrees()
}
