// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The V-n envelope checks of the report, as search residuals.
//!
//! The report rejects a design whose manoeuvre envelope is not ordered
//! `VS < VA <= VC < VD` or whose positive limit load factor is below the
//! 14 CFR 25.337(b) minimum (`alas_pipeline::feasibility`, finding
//! `invalid_envelope_speed_order`). The search had no such row, so it could
//! deliver a winner the report then refused (E195-E2, payload-adjusted: VA
//! 275.4 kt against VC 272.1 kt EAS, "rejected at reporting fidelity").
//!
//! The rows are built from the report's own function and state:
//! [`alas_perf::performance::build_vn_diagram`] on the candidate's reference
//! area, at the structural design mass of the configuration closed at the
//! candidate's sized takeoff mass (`AlasConfig::at_sized_closure_mass`, the
//! configuration the report's `design_mass_config` builds), with the clean
//! maximum lift coefficient and the design dive speed of that configuration.
//! `VA = sqrt(2 n W / (rho0 S CLmax,clean))` rises with wing loading and
//! VC (the CS 25.335(b) inverse of VD) is fixed by the configured dive speed, so the row binds a
//! candidate that shrinks its wing or closes at a higher mass.
//!
//! Speeds are equivalent airspeeds in knots, load factors in g.

use alas_config::AlasConfig;
use alas_perf::performance::build_vn_diagram;
use alas_perf::performance::envelope::Far25PositiveLoadFactorStatus;

use crate::mdo::ResidualRole;

use super::super::types::ConstraintFamily::Structure;
use super::super::types::ConstraintResidual;

/// A hard row that is violated when `violated`, with the report's strict
/// comparison (no numerical slack: a candidate the report would reject must
/// not pass here).
fn bool_row(id: &'static str, violated: bool) -> ConstraintResidual {
    let value = f64::from(violated);
    ConstraintResidual::direct(
        id,
        Structure,
        value,
        0.0,
        "bool",
        value,
        value,
        ResidualRole::Constraint,
    )
}

/// The V-n rows of a candidate with reference area `s_ref_m2`, for
/// `design`, the configuration closed at the candidate's sized takeoff mass.
pub(super) fn residuals(design: &AlasConfig, s_ref_m2: f64) -> Vec<ConstraintResidual> {
    let mut requirements = design.requirements.clone();
    requirements.mtow_kg = crate::mdo::structural_feasibility::structural_design_mass_kg(design);
    let envelope = build_vn_diagram(
        s_ref_m2,
        &requirements,
        &design.performance,
        design.requirements.cruise_altitude_m,
    );
    let (v_a, v_c) = (envelope.v_a_kt, envelope.v_c_kt);
    let speed_excess = if v_a.is_finite() && v_c.is_finite() && v_c > 0.0 {
        ((v_a - v_c) / v_c).max(0.0)
    } else {
        f64::INFINITY
    };
    let mut rows = vec![ConstraintResidual::direct(
        "maneuver_speed_within_design_cruise_speed",
        Structure,
        v_a,
        v_c,
        "kt EAS",
        v_a - v_c,
        speed_excess,
        ResidualRole::Constraint,
    )
    .with_detail(format!(
        "V-n envelope at the structural design mass {:.0} kg: VS {:.1}, VA {:.1}, VC {:.1}, VD \
         {:.1} kt EAS",
        requirements.mtow_kg, envelope.v_s_kt, v_a, v_c, envelope.v_d_kt
    ))];
    match (
        envelope.far25_positive_load_factor_status,
        envelope.far25_positive_limit_load_factor_min,
    ) {
        (Far25PositiveLoadFactorStatus::Uncheckable, _) | (_, None) => {
            rows.push(bool_row("positive_limit_load_factor_uncheckable", true));
        }
        (status, Some(required)) => {
            let below = status == Far25PositiveLoadFactorStatus::BelowMinimum;
            let shortfall = if below {
                ((required - envelope.n_lim_pos) / required).max(f64::MIN_POSITIVE)
            } else {
                0.0
            };
            rows.push(ConstraintResidual::direct(
                "positive_limit_load_factor",
                Structure,
                envelope.n_lim_pos,
                required,
                "g",
                required - envelope.n_lim_pos,
                shortfall,
                ResidualRole::Constraint,
            ));
        }
    }
    // Anything else the report's validation refuses (a non-finite speed,
    // VS >= VA, VC >= VD) and the two rows above do not already show.
    let shown = rows.iter().any(ConstraintResidual::violated);
    rows.push(bool_row(
        "vn_envelope_invalid",
        envelope.validate_speed_order().is_err() && !shown,
    ));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(preset: &str) -> AlasConfig {
        AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
            .unwrap_or_else(|error| panic!("{preset}: {error}"))
    }

    fn reference_area_m2(config: &AlasConfig) -> f64 {
        let design = alas_config::presets::get(&config.preset)
            .unwrap_or_else(|error| panic!("{error}"))
            .design_vector;
        alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .unwrap_or_else(|error| panic!("{error:?}"))
            .s_ref
    }

    /// The rows agree with the report's validation: violated exactly when
    /// `validate_speed_order` refuses the envelope, on a registered aircraft
    /// and on the same aircraft with a wing too small for its dive speed.
    #[test]
    fn the_rows_are_violated_exactly_when_the_report_refuses_the_envelope() {
        let config = config("E195-E2");
        let design = config.at_sized_closure_mass(config.requirements.mtow_kg);
        let area = reference_area_m2(&config);
        for scale in [1.0, 0.9, 0.8, 0.7, 0.6, 0.5] {
            let s_ref = area * scale;
            let mut requirements = design.requirements.clone();
            requirements.mtow_kg =
                crate::mdo::structural_feasibility::structural_design_mass_kg(&design);
            let refused = build_vn_diagram(
                s_ref,
                &requirements,
                &design.performance,
                design.requirements.cruise_altitude_m,
            )
            .validate_speed_order()
            .is_err();
            let rows = residuals(&design, s_ref);
            assert_eq!(
                rows.iter().any(ConstraintResidual::violated),
                refused,
                "area scale {scale}"
            );
        }
        // The sweep must cross the boundary, or the test proves nothing.
        assert!(!residuals(&design, area)
            .iter()
            .any(ConstraintResidual::violated));
        assert!(residuals(&design, 0.5 * area)
            .iter()
            .any(|row| row.id == "maneuver_speed_within_design_cruise_speed" && row.violated()));
    }
}
