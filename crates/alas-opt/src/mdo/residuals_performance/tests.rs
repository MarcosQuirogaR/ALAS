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
        let (cd0, k) = departure_polar(drag, cl_v2, mach, field_m, 0.0);
        assert!(k >= 0.0 && cd0 > 0.0);
        let expected = drag.cd_at_atmosphere(cl_v2, mach, field_m, 0.0);
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

#[test]
fn cruise_thrust_follows_mass_changes_without_rebuilding_the_table() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    let mut outcome = run_candidate(&config, &design.to_array()).unwrap();
    let table = outcome.sized.fuel_artifacts.drag.table().unwrap().clone();
    let original_mass_kg = outcome.sized.takeoff_mass_kg;
    let atmosphere = alas_atmo::Atmosphere::new(config.requirements.cruise_altitude_m);
    let speed_m_s = config.requirements.cruise_mach * atmosphere.speed_of_sound();
    let q_pa = 0.5 * atmosphere.density() * speed_m_s * speed_m_s;
    let (tangent_cd0, tangent_k) = table.parabolic_equivalent(config.requirements.cruise_mach);
    for mass_ratio in [0.8, 1.2] {
        // A retained CG-compatible table remains a function of lift, even
        // when the closure's mass changes without another trim solve.
        outcome.sized.takeoff_mass_kg = original_mass_kg * mass_ratio;
        let cl = outcome.sized.takeoff_mass_kg * config.requirements.gravity_m_s2
            / outcome.plane.s_ref
            / q_pa;
        assert!((cl - table.design_cl()).abs() > 0.05);
        let expected = table.cd(
            cl,
            config.requirements.cruise_mach,
            config.requirements.cruise_altitude_m,
        ) / cl
            / config.performance.thrust_lapse;
        let residuals = performance_residuals(&outcome, &config, ConstraintPolicy::Hard);
        let actual = residuals
            .iter()
            .find(|residual| residual.id == "cruise_thrust")
            .unwrap()
            .limit;
        assert_eq!(actual.to_bits(), expected.to_bits());
        let former_requirement =
            (tangent_cd0 + tangent_k * cl * cl) / cl / config.performance.thrust_lapse;
        assert!(
            (former_requirement - expected).abs() > 1e-8,
            "mass ratio {mass_ratio} must expose the design-CL tangent error"
        );
        assert!(std::sync::Arc::ptr_eq(
            &table,
            outcome.sized.fuel_artifacts.drag.table().unwrap()
        ));
    }
}

#[test]
fn each_oei_evidence_gap_uses_the_v2_lift_coefficient() {
    let mut config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    let mut outcome = run_candidate(&config, &design.to_array()).unwrap();
    static DEPARTURE: std::sync::LazyLock<Airport> = std::sync::LazyLock::new(|| {
        Airport::custom("OEI hot field", 2_100.0, 4_000.0, 4_000.0, 25.0, 0.0, 0.0)
    });
    outcome.departure = Some(&DEPARTURE);
    config.performance.oei_climb_cl = 0.4;
    let speeds = compute_v_speeds_at_masses(
        outcome.sized.takeoff_mass_kg,
        outcome.sized.takeoff_mass_kg,
        outcome.plane.s_ref,
        &DEPARTURE,
        config.performance.cl_max_to,
        config.performance.cl_max_land,
        &config.performance,
    );
    let cl = oei_cl_at_v2(
        config.performance.cl_max_to,
        speeds.v2_ms / speeds.v_stall_to_ms,
    )
    .unwrap();
    assert!((cl - config.performance.oei_climb_cl).abs() > 0.5);
    let atmosphere =
        alas_atmo::us1976_compute_values(DEPARTURE.elevation_m, DEPARTURE.isa_deviation_c);
    let mach = speeds.v2_ms / atmosphere.speed_of_sound_m_s;
    let table = outcome.sized.fuel_artifacts.drag.table().unwrap();
    let reynolds_per_m =
        atmosphere.density_kg_m3 * speeds.v2_ms / atmosphere.dynamic_viscosity_pa_s;
    let cd = table.cd0_at_reynolds_per_m(mach, reynolds_per_m)
        + table.induced_cd(cl)
        + table.wave_cd(cl, mach);
    let n = outcome.n_engines as f64;
    let expected = n / (n - 1.0)
        * (far25_oei_gradient(outcome.n_engines).unwrap()
            + (cd + config.performance.oei_climb_delta_cd) / cl);
    for missing in 0..3 {
        config.performance.oei_condition_to_sls_thrust_ratio = (missing != 0).then_some(0.8);
        config.performance.oei_asymmetric_trim_cd = (missing != 1).then_some(0.003);
        config.performance.oei_windmilling_cd = (missing != 2).then_some(0.002);
        let residuals = performance_residuals(&outcome, &config, ConstraintPolicy::Hard);
        let requirement = residuals
            .iter()
            .find(|residual| residual.id == "oei_second_segment")
            .unwrap();
        assert_eq!(requirement.policy, ConstraintPolicy::Soft);
        assert!(
            (requirement.limit - expected).abs() < 1e-14,
            "gap {missing}"
        );
        assert!(residuals.iter().any(|residual| {
            residual.id == "oei_second_segment_evidence_gap"
                && residual.policy == ConstraintPolicy::Diagnostic
        }));
    }
}

#[test]
fn an_external_polar_has_no_unmeasured_reynolds_correction() {
    let polar = crate::mdo::mission_model::ParabolicPolar::new(0.02, 0.04, 0.003, 0.82);
    let drag = CandidateDrag::External(std::sync::Arc::new(polar));
    for deviation_c in [-20.0, 0.0, 25.0] {
        let (cd, k) = departure_polar(&drag, 1.1, 0.22, 2_100.0, deviation_c);
        assert_eq!(k, 0.0);
        assert_eq!(cd.to_bits(), drag.cd(1.1, 0.22, 2_100.0).to_bits());
    }
}
