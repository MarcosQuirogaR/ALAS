// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A test asserts on values it built here, so a failed unwrap is the assertion
// failing rather than a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::mdo::ResidualRole;

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
        ResidualRole::Constraint,
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
    field_residuals_use_design_masses(alas_config::MtowSizing::SizedByMission);
}

#[test]
fn hard_mtow_field_residuals_use_the_declared_design_masses() {
    field_residuals_use_design_masses(alas_config::MtowSizing::FixedRequirement);
}

fn field_residuals_use_design_masses(mode: alas_config::MtowSizing) {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "optimizer": {
            "design_space": {"mode": "reference_adaptation"},
            "objective": {"mtow_sizing": mode}
        }
    }))
    .unwrap();
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    let outcome = run_candidate(&config, &design.to_array()).unwrap();
    let sized = &outcome.sized;
    // A short route dispatches well below the declared MTOW and MLW.
    if mode == alas_config::MtowSizing::SizedByMission {
        assert!(sized.takeoff_mass_kg < sized.design_gross_mass_kg);
    } else {
        let load = sized.takeoff_loading.unwrap();
        assert_eq!(
            sized.takeoff_mass_kg,
            load.zero_fuel_mass_kg + load.carried_usable_fuel_kg
        );
        assert!(sized.takeoff_mass_kg <= config.requirements.mtow_kg);
        assert_eq!(sized.design_gross_mass_kg, config.requirements.mtow_kg);
    }
    assert!(sized.dispatch.destination_landing_mass_kg < sized.design_landing_mass_kg);

    let table = performance_residuals(&outcome, &config, ResidualRole::Constraint);
    let find = |id: &str| table.iter().find(|r| r.id == id).unwrap();
    let s_ref = outcome.plane.s_ref;
    let thrust_n = outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0;
    let tw_mtow = thrust_n / (sized.design_gross_mass_kg * config.requirements.gravity_m_s2);
    assert!((find("takeoff_field").actual / tw_mtow - 1.0).abs() < 1e-12);
    let ws_mlw = sized.design_landing_mass_kg * config.requirements.gravity_m_s2 / s_ref;
    assert!((find("landing_field").actual / ws_mlw - 1.0).abs() < 1e-12);

    // The optimizer limit is the factored definition shared with the delivery
    // gate: at the limiting wing loading the unfactored distance
    // K (W/S)/(sigma CLmax) divided by the 0.6 dry share equals the LDA.
    if let Some(arrival) = outcome.arrival {
        let perf = &config.performance;
        let sigma = density_ratio(arrival.elevation_m, arrival.isa_deviation_c);
        let limit = find("landing_field").limit;
        let lfl_at_limit = perf.k_land * limit
            / (sigma * perf.cl_max_land)
            / alas_perf::performance::JET_DRY_LANDING_SHARE;
        assert!((lfl_at_limit / arrival.lda_m - 1.0).abs() < 1e-12);
    }
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
        let expected = drag.cd_airport_base_at_atmosphere(cl_v2, mach, field_m, 0.0);
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
    let mut config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    // The flat-lapse form (the legacy basis): bit-exact against the closed form.
    config.performance.cruise_thrust_from_deck = false;
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
        let residuals = performance_residuals(&outcome, &config, ResidualRole::Constraint);
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
fn the_oei_requirement_uses_the_v2_lift_coefficient_and_the_shared_evidence() {
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
    // The speed schedule the residual uses carries the computed VMC (CS
    // 25.107(b)); the closed form below rebuilds V2, CL and CD from it.
    let table = outcome.sized.fuel_artifacts.drag.table().unwrap();
    let v2_point = |config: &AlasConfig| {
        let vmc = crate::mdo::residuals_vmc::evaluate_candidate(&outcome, config);
        let resolved = crate::mdo::residuals_vmc::performance_with_vmc(config, &vmc);
        let performance = &resolved.performance;
        let speeds = compute_v_speeds_at_masses(
            outcome.sized.takeoff_mass_kg,
            outcome.sized.takeoff_mass_kg,
            outcome.plane.s_ref,
            &DEPARTURE,
            performance.cl_max_to,
            performance.cl_max_land,
            performance,
        );
        let cl = oei_cl_at_v2(performance.cl_max_to, speeds.v2_ms / speeds.v_stall_to_ms).unwrap();
        let atmosphere =
            alas_atmo::us1976_compute_values(DEPARTURE.elevation_m, DEPARTURE.isa_deviation_c);
        let mach = speeds.v2_ms / atmosphere.speed_of_sound_m_s;
        let reynolds_per_m =
            atmosphere.density_kg_m3 * speeds.v2_ms / atmosphere.dynamic_viscosity_pa_s;
        let cd = table.cd0_at_reynolds_per_m(mach, reynolds_per_m)
            + table.induced_cd(cl)
            + table.wave_cd(cl, mach);
        (cl, cd)
    };
    let (cl, _) = v2_point(&config);
    assert!((cl - config.performance.oei_climb_cl).abs() > 0.5);
    let n = outcome.n_engines as f64;
    let gradient = far25_oei_gradient(outcome.n_engines).unwrap();

    // Every evidence term configured: the closed form of the second segment
    // at the V2 lift coefficient, on the SLS axis through the given ratio.
    config.performance.oei_condition_to_sls_thrust_ratio = Some(0.8);
    config.performance.oei_asymmetric_trim_cd = Some(0.003);
    config.performance.oei_windmilling_cd = Some(0.002);
    let (cl, cd) = v2_point(&config);
    let expected = n / (n - 1.0)
        * (gradient + (cd + config.performance.oei_climb_delta_cd + 0.003 + 0.002) / cl)
        / 0.8;
    let residuals = performance_residuals(&outcome, &config, ResidualRole::Constraint);
    let requirement = residuals
        .iter()
        .find(|residual| residual.id == "oei_second_segment")
        .unwrap();
    assert_eq!(requirement.role, ResidualRole::Constraint);
    assert!((requirement.limit - expected).abs() < 1e-12);
    assert!(!residuals
        .iter()
        .any(|residual| residual.id == "oei_second_segment_evidence_gap"));

    // Nothing configured: the deck ratio and the handbook drag terms are
    // derived, the requirement is still a hard constraint and no evidence
    // gap is raised.
    config.performance.oei_condition_to_sls_thrust_ratio = None;
    config.performance.oei_asymmetric_trim_cd = None;
    config.performance.oei_windmilling_cd = None;
    let (cl, cd) = v2_point(&config);
    let derived = performance_residuals(&outcome, &config, ResidualRole::Constraint);
    let requirement = derived
        .iter()
        .find(|residual| residual.id == "oei_second_segment")
        .unwrap();
    assert_eq!(requirement.role, ResidualRole::Constraint);
    assert!(!derived
        .iter()
        .any(|residual| residual.id == "oei_second_segment_evidence_gap"));
    // The deck lapse puts the V2 thrust below the static rating, so the
    // SLS requirement exceeds the in-flight one by 1/r with r < 1.
    let inflight = n / (n - 1.0) * (gradient + (cd + config.performance.oei_climb_delta_cd) / cl);
    assert!(requirement.limit > inflight);
}

#[test]
fn the_certification_climb_checks_enter_the_performance_table() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    let outcome = run_candidate(&config, &design.to_array()).unwrap();
    let table = performance_residuals(&outcome, &config, ResidualRole::Constraint);
    for id in [
        "oei_second_segment",
        "oei_final_segment",
        "oei_approach_climb",
        "landing_climb",
    ] {
        let residual = table.iter().find(|residual| residual.id == id).unwrap();
        assert_eq!(residual.role, ResidualRole::Constraint, "{id}");
        assert!(residual.limit.is_finite() && residual.limit > 0.0, "{id}");
    }
    let net = table
        .iter()
        .find(|residual| residual.id == "takeoff_net_path_gradient")
        .unwrap();
    assert_eq!(net.role, ResidualRole::Diagnostic);
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

#[test]
fn a_wet_landing_runway_cuts_the_landing_wing_loading_limit_by_the_cat_pol_a_235_factor() {
    let mut config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    let outcome = run_candidate(&config, &design.to_array()).unwrap();
    let limit = |config: &AlasConfig| {
        performance_residuals(&outcome, config, ResidualRole::Constraint)
            .iter()
            .find(|residual| residual.id == "landing_field")
            .unwrap()
            .limit
    };
    let dry = limit(&config);
    config.performance.wet_landing_runway = true;
    let wet = limit(&config);
    assert!((dry / wet - 1.15).abs() < 1e-12);
    config.performance.wet_landing_runway = false;
    assert_eq!(limit(&config).to_bits(), dry.to_bits());
}

#[test]
fn the_deck_cruise_rows_use_the_decks_ratio_and_the_top_of_climb_rate() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    assert!(config.performance.cruise_thrust_from_deck);
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    let outcome = run_candidate(&config, &design.to_array()).unwrap();
    let rows = performance_residuals(&outcome, &config, ResidualRole::Constraint);
    let find = |id: &str| rows.iter().find(|row| row.id == id).unwrap();
    let level = find("cruise_thrust");
    let toc = find("toc_rate_of_climb");
    // The top-of-climb row is a hard row at the initial cruise level the
    // mission plan selected: 300 ft/min at the top-of-climb mass, evaluated by
    // the mission model that chooses the level.
    assert_eq!(toc.role, ResidualRole::Constraint);
    assert!((toc.limit - 300.0 * 0.3048 / 60.0).abs() < 1e-12);
    assert!(toc.actual >= toc.limit - 1e-3, "{toc:?}");
    assert_eq!(level.role, ResidualRole::Constraint);
    assert!(rows.iter().all(|row| row.id != "cruise_deck_evidence_gap"));

    // Independent closed form from the deck and the carried drag.
    let req = &config.requirements;
    let s_ref = outcome.plane.s_ref;
    let mass = outcome.sized.takeoff_mass_kg;
    let model = EngineOutModel::new(
        &config,
        None,
        s_ref,
        Some(outcome.static_thrust_kn * 1_000.0),
    )
    .unwrap();
    let atmosphere = alas_atmo::Atmosphere::new(req.cruise_altitude_m);
    let tas = req.cruise_mach * atmosphere.speed_of_sound();
    let q = 0.5 * atmosphere.density() * tas * tas;
    let (_, ratio) = model
        .thrust(
            oei_deck::EngineOutCondition {
                altitude_m: req.cruise_altitude_m,
                isa_deviation_c: 0.0,
                tas_m_s: tas,
            },
            oei_deck::EngineOutRating::MaximumClimb,
            false,
        )
        .unwrap();
    // Maximum-climb thrust at FL370 / M 0.78 is about a fifth to a quarter
    // of the static rating for a bypass-ratio-5 turbofan.
    assert!((0.18..0.30).contains(&ratio), "{ratio}");
    let drag = &outcome.sized.fuel_artifacts.drag;
    let cl = mass * G / (s_ref * q);
    let cd = |cl: f64| drag.cd(cl, req.cruise_mach, req.cruise_altitude_m);
    // Level flight is evaluated at the mid-cruise mass, on the take-off
    // weight axis.
    let level_mass = outcome.sized.mid_cruise_mass_kg();
    assert!(level_mass < mass, "{level_mass} < {mass}");
    let cl_level = level_mass * G / (s_ref * q);
    let (_, cruise_ratio) = model
        .thrust(
            oei_deck::EngineOutCondition {
                altitude_m: req.cruise_altitude_m,
                isa_deviation_c: 0.0,
                tas_m_s: tas,
            },
            oei_deck::EngineOutRating::MaximumCruise,
            false,
        )
        .unwrap();
    // Maximum cruise is 0.93 of maximum climb (Torenbeek 2013 Sec. 9.2,
    // Jenkinson Figs. 9.19, 9.21).
    assert!((cruise_ratio / ratio - 0.93).abs() < 1e-12);
    // Without a frozen plan the row is the design-level form at the
    // mid-cruise mass (the earlier definition), flagged as such.
    let level_hand = level_mass / mass * cd(cl_level) / cl_level / cruise_ratio;
    let mut unplanned = (*outcome.sized.fuel_artifacts).clone();
    unplanned.frozen_plan = None;
    let fallback = cruise_deck::cruise_rows(
        Some(&model),
        &outcome.sized.fuel_artifacts.drag,
        &config,
        (mass, level_mass),
        (s_ref, level.actual),
        &unplanned,
        ResidualRole::Constraint,
    );
    let fallback = fallback
        .iter()
        .find(|row| row.id == "cruise_thrust")
        .unwrap();
    assert!(
        (fallback.limit - level_hand).abs() < 1e-12 * level_hand,
        "{fallback:?}"
    );
    assert!(fallback.detail.as_deref().unwrap().contains("no plan"));

    // With the plan: the worst of the levels the mission flies, each at the
    // mass, altitude and Mach the mission model reaches it with, written
    // out independently from the deck and the carried drag.
    let artifacts = &outcome.sized.fuel_artifacts;
    let states = crate::mdo::candidate_mission_model(&config, artifacts)
        .unwrap()
        .planned_cruise_states(mass)
        .unwrap();
    assert!(!states.is_empty());
    let mut planned_hand = 0.0_f64;
    for state in &states {
        let atmosphere = alas_atmo::Atmosphere::new(state.altitude_m);
        let mach = state.tas_m_s / atmosphere.speed_of_sound();
        let q = 0.5 * atmosphere.density() * state.tas_m_s * state.tas_m_s;
        let (_, r) = model
            .thrust(
                oei_deck::EngineOutCondition {
                    altitude_m: state.altitude_m,
                    isa_deviation_c: 0.0,
                    tas_m_s: state.tas_m_s,
                },
                oei_deck::EngineOutRating::MaximumCruise,
                false,
            )
            .unwrap();
        let cl_state = state.mass_kg * G / (s_ref * q);
        planned_hand = planned_hand
            .max(state.mass_kg / mass * drag.cd(cl_state, mach, state.altitude_m) / cl_state / r);
    }
    assert!(
        (level.limit - planned_hand).abs() < 1e-12 * planned_hand,
        "{level:?} against {planned_hand}"
    );
    assert!(level.detail.as_deref().unwrap().contains("the plan's"));
    // Every planned level is flown below the design level here, with the
    // aircraft heavier than at mid-cruise: the plan, not the design label,
    // is what binds.
    assert!(states.iter().all(|s| s.mass_kg <= mass));

    // The legacy basis restores the flat lapse and drops the top-of-climb row.
    let mut legacy = config.clone();
    legacy.performance.cruise_thrust_from_deck = false;
    let rows = performance_residuals(&outcome, &legacy, ResidualRole::Constraint);
    assert!(rows.iter().all(|row| row.id != "toc_rate_of_climb"));
    let flat = rows.iter().find(|row| row.id == "cruise_thrust").unwrap();
    assert!(
        (flat.limit - cd(cl) / cl / legacy.performance.thrust_lapse).abs() < 1e-12,
        "{flat:?}"
    );
}

#[test]
fn a_turboprop_cruise_thrust_row_is_the_unit_models_thrust_over_the_levels_the_plan_flies() {
    // Before lane blockers B the turboprop row used the flat thrust lapse while
    // the reporting finding used the unit model at the design point (FL370 for
    // the A400M, half the drag); both now read the plan-level deck definition.
    for name in ["A400M", "ATR72-600"] {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let assessment = crate::assess_product_candidate(&config, &design).unwrap();
        let row = assessment
            .residuals
            .iter()
            .find(|row| row.id == "cruise_thrust")
            .unwrap_or_else(|| panic!("{name}: no cruise_thrust row"));
        let detail = row.detail.as_deref().unwrap_or_default();
        assert!(
            detail.starts_with("level cruise at the plan's"),
            "{name}: {detail}"
        );
        assert!(!row.violated(), "{name}: {} vs {}", row.limit, row.actual);
    }
}
