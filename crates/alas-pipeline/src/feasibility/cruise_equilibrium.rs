// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cruise force-balance telemetry derived from solved mission states.
//!
//! Scalar lift and drag views are useful diagnostics, but only the inertial
//! resultant retains the solved body and aerodynamic angles. Keeping both
//! prevents a nominal `L=W` display from being mistaken for an exact trim
//! residual.

use alas_mission::segments::SegmentKind;
use alas_mission::MissionResult;

/// Force-balance evidence from the converged constant-altitude mission legs.
///
/// The scalar `T-D` and `L-W` columns retain the conventional level-flight
/// view. The inertial residual is the governing check because it includes the
/// solved body and aerodynamic angles instead of assuming both vectors are
/// exactly aligned with the flight path.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CruiseEquilibriumAssessment {
    /// Number of solved constant-altitude control points inspected.
    pub control_points: usize,
    /// True when every inspected cruise segment reports solver convergence.
    pub all_segments_converged: bool,
    /// Largest magnitude of `T-D`, N.
    pub max_abs_thrust_minus_drag_n: Option<f64>,
    /// Largest magnitude of `L-W`, N.
    pub max_abs_lift_minus_weight_n: Option<f64>,
    /// Largest inertial longitudinal force residual, N.
    pub max_abs_longitudinal_residual_n: Option<f64>,
    /// Largest inertial vertical force residual, N.
    pub max_abs_vertical_residual_n: Option<f64>,
    /// Largest inertial residual magnitude per unit mass, m/s^2.
    pub max_abs_residual_acceleration_m_s2: Option<f64>,
    /// Lowest analyzed cruise altitude, m.
    pub minimum_altitude_m: Option<f64>,
    /// Highest analyzed cruise altitude, m.
    pub maximum_altitude_m: Option<f64>,
    /// Lowest analyzed true airspeed, m/s.
    pub minimum_true_airspeed_m_s: Option<f64>,
    /// Highest analyzed true airspeed, m/s.
    pub maximum_true_airspeed_m_s: Option<f64>,
}

impl CruiseEquilibriumAssessment {
    /// Whether every reported value is finite and a cruise point was present.
    pub fn is_finite(&self) -> bool {
        self.control_points > 0
            && [
                self.max_abs_thrust_minus_drag_n,
                self.max_abs_lift_minus_weight_n,
                self.max_abs_longitudinal_residual_n,
                self.max_abs_vertical_residual_n,
                self.max_abs_residual_acceleration_m_s2,
                self.minimum_altitude_m,
                self.maximum_altitude_m,
                self.minimum_true_airspeed_m_s,
                self.maximum_true_airspeed_m_s,
            ]
            .into_iter()
            .all(|value| value.is_some_and(f64::is_finite))
    }
}

/// Format cruise force-balance evidence for product reports.
pub(crate) fn format(assessment: Option<&CruiseEquilibriumAssessment>) -> String {
    let Some(assessment) = assessment else {
        return "Cruise force balance : NOT EVALUATED (mission unavailable or disabled)".to_owned();
    };
    if !assessment.is_finite() {
        return "Cruise force balance : INVALID force record".to_owned();
    }
    let convergence = if assessment.all_segments_converged {
        "SOLVER CONVERGED"
    } else {
        "SOLVER NOT CONVERGED"
    };
    format!(
        "Cruise force balance : {convergence}; {} point(s), |T-D| max {:.1} N, |L-W| max {:.1} N, inertial |Fx|/|Fz| max {:.1}/{:.1} N, |a| max {:.3e} m/s^2, altitude {:.0}-{:.0} m, TAS {:.1}-{:.1} m/s",
        assessment.control_points,
        assessment.max_abs_thrust_minus_drag_n.unwrap_or(f64::NAN),
        assessment.max_abs_lift_minus_weight_n.unwrap_or(f64::NAN),
        assessment.max_abs_longitudinal_residual_n.unwrap_or(f64::NAN),
        assessment.max_abs_vertical_residual_n.unwrap_or(f64::NAN),
        assessment.max_abs_residual_acceleration_m_s2.unwrap_or(f64::NAN),
        assessment.minimum_altitude_m.unwrap_or(f64::NAN),
        assessment.maximum_altitude_m.unwrap_or(f64::NAN),
        assessment.minimum_true_airspeed_m_s.unwrap_or(f64::NAN),
        assessment.maximum_true_airspeed_m_s.unwrap_or(f64::NAN),
    )
}

/// Assess scalar and inertial equilibrium from the mission's actual vectors.
pub(crate) fn assess(mission: &MissionResult) -> CruiseEquilibriumAssessment {
    let mut assessment = CruiseEquilibriumAssessment {
        all_segments_converged: true,
        ..CruiseEquilibriumAssessment::default()
    };

    for segment in &mission.segments {
        if !matches!(segment.spec.kind, SegmentKind::Cruise { .. }) {
            continue;
        }
        assessment.all_segments_converged &= segment.numerics.converged == Some(true);
        for point in 0..segment.conditions.len() {
            let thrust = segment.conditions.thrust_force_vector_n[point][0];
            let drag = -segment.conditions.wind_drag_force_vector_n[point][0];
            let lift = -segment.conditions.wind_lift_force_vector_n[point][2];
            let weight = segment.conditions.gravity_force_vector_n[point][2];
            let force = segment.conditions.total_force_vector_n[point];
            let mass = segment.conditions.total_mass_kg[point];
            let velocity = segment.conditions.velocity_vector_m_s[point];
            let true_airspeed =
                (velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2])
                    .sqrt();

            update_max(
                &mut assessment.max_abs_thrust_minus_drag_n,
                (thrust - drag).abs(),
            );
            update_max(
                &mut assessment.max_abs_lift_minus_weight_n,
                (lift - weight).abs(),
            );
            update_max(
                &mut assessment.max_abs_longitudinal_residual_n,
                force[0].abs(),
            );
            update_max(&mut assessment.max_abs_vertical_residual_n, force[2].abs());
            update_max(
                &mut assessment.max_abs_residual_acceleration_m_s2,
                (force[0] * force[0] + force[1] * force[1] + force[2] * force[2]).sqrt() / mass,
            );
            update_min(
                &mut assessment.minimum_altitude_m,
                segment.conditions.altitude_m[point],
            );
            update_max(
                &mut assessment.maximum_altitude_m,
                segment.conditions.altitude_m[point],
            );
            update_min(&mut assessment.minimum_true_airspeed_m_s, true_airspeed);
            update_max(&mut assessment.maximum_true_airspeed_m_s, true_airspeed);
            assessment.control_points += 1;
        }
    }
    if assessment.control_points == 0 {
        assessment.all_segments_converged = false;
    }
    assessment
}

fn update_max(value: &mut Option<f64>, candidate: f64) {
    *value = Some(value.map_or(candidate, |current| current.max(candidate)));
}

fn update_min(value: &mut Option<f64>, candidate: f64) {
    *value = Some(value.map_or(candidate, |current| current.min(candidate)));
}
