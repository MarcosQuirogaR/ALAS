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

/// Freestream state shared by every spanwise section of one report.
struct FlightProxy {
    atmosphere: alas_atmo::Atmosphere,
    velocity_m_s: f64,
    /// Section Mach normal to the built wing's mean quarter-chord line.
    normal_mach: f64,
    body_alpha_deg: f64,
    induced_angle_deg: f64,
}

impl FlightProxy {
    fn new(config: &AlasConfig, report: &AnalysisReport) -> Self {
        let freestream_mach = config.requirements.cruise_mach;
        let atmosphere = alas_atmo::Atmosphere::new(config.requirements.cruise_altitude_m);
        let velocity_m_s = freestream_mach * atmosphere.speed_of_sound();
        let quarter_chord_sweep_deg = alas_aero::analysis::AeroAnalysis::quarter_chord_sweep_deg(
            &report.airplane,
            report.design.sweep_deg,
        );
        let normal_mach = freestream_mach * quarter_chord_sweep_deg.to_radians().cos();
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
        Self {
            atmosphere,
            velocity_m_s,
            normal_mach,
            body_alpha_deg,
            induced_angle_deg,
        }
    }

    fn reynolds(&self, chord_m: f64) -> f64 {
        (self.atmosphere.density() * self.velocity_m_s * chord_m)
            / self.atmosphere.dynamic_viscosity().max(1e-9)
    }

    fn condition(&self, chord_m: f64, twist_deg: f64) -> MsesSectionCondition {
        MsesSectionCondition {
            mach: self.normal_mach,
            reynolds: self.reynolds(chord_m),
            alpha_deg: self.body_alpha_deg + twist_deg - self.induced_angle_deg,
        }
    }
}

pub(super) fn mses_section_condition(
    config: &AlasConfig,
    report: &AnalysisReport,
) -> MsesSectionCondition {
    let proxy = FlightProxy::new(config, report);
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
    proxy.condition(section_chord_m, section_twist_deg)
}

/// A built wing cross-section outboard of the root that MSES also solves.
pub(super) struct MsesSpanSectionRequest<'a> {
    pub(super) label: &'static str,
    pub(super) y_m: f64,
    pub(super) chord_m: f64,
    pub(super) airfoil: &'a alas_geom::aircraft::airfoil::Airfoil,
    pub(super) condition: MsesSectionCondition,
}

/// The kink and tip cross-sections of the main wing, each at its own chord
/// (Reynolds number) and twist, with the same normal Mach and mean downwash
/// as the root proxy. The kink is the built cross-section nearest the
/// planform's kink station; the tip is the outermost one. A section that
/// coincides with the root or with an earlier request is not repeated, so a
/// single-panel wing yields the tip only.
pub(super) fn mses_span_section_requests<'a>(
    config: &AlasConfig,
    report: &'a AnalysisReport,
) -> Vec<MsesSpanSectionRequest<'a>> {
    let Some(wing) = report.airplane.wings.first() else {
        return Vec::new();
    };
    if wing.xsecs.len() < 2 {
        return Vec::new();
    }
    let proxy = FlightProxy::new(config, report);
    let y_of = |index: usize| wing.xsecs[index].xyz_le[1].abs();
    let tip_index = (1..wing.xsecs.len())
        .max_by(|a, b| y_of(*a).total_cmp(&y_of(*b)))
        .unwrap_or(wing.xsecs.len() - 1);
    let kink_y_m = config
        .geometry
        .wing
        .transport_planform(&report.design)
        .ok()
        .map(|planform| planform.kink.y_m);
    let kink_index = kink_y_m
        .and_then(|target| {
            (1..wing.xsecs.len()).min_by(|a, b| {
                (y_of(*a) - target)
                    .abs()
                    .total_cmp(&(y_of(*b) - target).abs())
            })
        })
        .filter(|index| *index != tip_index);
    let mut requests = Vec::new();
    for (label, index) in [("kink", kink_index), ("tip", Some(tip_index))] {
        let Some(index) = index else { continue };
        if requests
            .iter()
            .any(|request: &MsesSpanSectionRequest<'_>| request.y_m == y_of(index))
        {
            continue;
        }
        let xsec = &wing.xsecs[index];
        requests.push(MsesSpanSectionRequest {
            label,
            y_m: y_of(index),
            chord_m: xsec.chord,
            airfoil: &xsec.airfoil,
            condition: proxy.condition(xsec.chord, xsec.twist),
        });
    }
    requests
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
