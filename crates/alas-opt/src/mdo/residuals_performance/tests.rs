// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A test asserts on values it built here, so a failed unwrap is the assertion
// failing rather than a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::mdo::sizing::run_candidate;

const G: f64 = 9.806_65;

/// Raw takeoff-field residual (positive is a miss) at `mass_kg` on a runway
/// of `toda_m`, for a 240 kN two-engine airliner on a 122.6 m^2 wing.
fn takeoff_miss(mass_kg: f64, toda_m: f64) -> f64 {
    let runway = Airport::custom("Test", 0.0, toda_m, toda_m, 0.0, 0.0, 0.0);
    takeoff_field_residual(
        mass_kg,
        240_000.0,
        122.6,
        G,
        &runway,
        2.1,
        ConstraintPolicy::Hard,
    )
    .raw_residual
}

#[test]
fn a_runway_cleared_at_the_city_pair_mass_is_missed_at_the_maximum_takeoff_mass() {
    // Find the runway that the aircraft just clears at a mass between the two.
    let (mission_kg, mtow_kg) = (60_000.0, 78_000.0);
    let (mut short_m, mut long_m) = (300.0, 8_000.0);
    for _ in 0..80 {
        let middle_m = 0.5 * (short_m + long_m);
        if takeoff_miss(0.5 * (mission_kg + mtow_kg), middle_m) > 0.0 {
            short_m = middle_m;
        } else {
            long_m = middle_m;
        }
    }
    let toda_m = 0.5 * (short_m + long_m);
    assert!(takeoff_miss(mission_kg, toda_m) < 0.0);
    assert!(takeoff_miss(mtow_kg, toda_m) > 0.0);
}

#[test]
fn the_field_residuals_are_evaluated_at_the_design_masses_not_the_dispatch_mass() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    let outcome = run_candidate(&config, &design.to_array()).unwrap();
    let sized = &outcome.sized;
    // A short route dispatches well below the declared MTOW and MLW.
    assert!(sized.takeoff_mass_kg < sized.design_gross_mass_kg);
    assert!(sized.dispatch.destination_landing_mass_kg < sized.design_landing_mass_kg);

    let table = performance_residuals(&outcome, &config, ConstraintPolicy::Hard);
    let find = |id: &str| table.iter().find(|r| r.id == id).unwrap();
    let s_ref = outcome.plane.s_ref;
    let thrust_n = outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0;
    let tw_mtow = thrust_n / (sized.design_gross_mass_kg * config.requirements.gravity_m_s2);
    assert!((find("takeoff_field").actual / tw_mtow - 1.0).abs() < 1e-12);
    let ws_mlw = sized.design_landing_mass_kg * config.requirements.gravity_m_s2 / s_ref;
    assert!((find("landing_field").actual / ws_mlw - 1.0).abs() < 1e-12);
}

/// The engine-out departure polar.
mod departure_polar {
    use super::super::*;
    use crate::assess_candidate;
    use crate::objective::DesignObjective;
    use alas_config::design_variables::DesignVector;
    use alas_config::ObjectiveKind;

    /// The departure polar reproduces the table's own clean drag at the V2
    /// lift coefficient, field altitude and V2 Mach, and carries the field
    /// Reynolds number: it is not the cruise-altitude, cruise-CL tangent.
    #[test]
    fn the_oei_polar_is_the_table_at_the_departure_point_not_the_cruise_tangent() {
        let mut config = AlasConfig::default();
        config.optimizer.objective.kind = ObjectiveKind::BlockFuel;
        let assessment = assess_candidate(
            &DesignObjective::new(config.clone()),
            &DesignVector::default().to_array(),
        )
        .unwrap_or_else(|reason| panic!("{reason}"));
        let drag = &assessment.sized.fuel_artifacts.drag;
        let table = drag.table().expect("native candidate carries a table");

        let cl_v2 = oei_cl_at_v2(config.performance.cl_max_to, 1.13).unwrap();
        let (mach, field_m) = (0.22, 1_500.0);
        let (cd0, k) = departure_polar(drag, cl_v2, mach, field_m);
        assert!(k >= 0.0 && cd0 > 0.0);
        let expected = drag.cd(cl_v2, mach, field_m);
        assert!((cd0 + k * cl_v2 * cl_v2 - expected).abs() < 1e-12 * expected);

        // Skin friction falls with Reynolds number and the denser field air
        // gives a higher Re at the same Mach, so field CD0 is the lower.
        let cruise_m = table.reference_altitude_m();
        assert!(table.cd0(mach, field_m) < table.cd0(mach, cruise_m));
        let (tangent_cd0, tangent_k) = drag.parabolic_equivalent(mach);
        let tangent_cd = tangent_cd0 + tangent_k * cl_v2 * cl_v2;
        assert!(
            (tangent_cd - expected).abs() > 1e-6,
            "the cruise tangent must not coincide with the departure drag"
        );
    }
}
