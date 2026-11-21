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
    takeoff_field::correlation_residual(
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
    // A jet is judged on the CS 25.113 field length against the TODA, in
    // metres, at the design gross mass; the T/W correlation is the replay's.
    let field = find("takeoff_field");
    assert_eq!(field.unit, "m");
    let departure = outcome.departure.unwrap();
    assert_eq!(field.actual, departure.toda_m);
    assert!(
        field.limit > 0.3 * departure.toda_m && field.limit < 3.0 * departure.toda_m,
        "{field:?} toda {}",
        departure.toda_m
    );
    assert!(tw_mtow > 0.0);
    let ws_mlw = sized.design_landing_mass_kg * config.requirements.gravity_m_s2 / s_ref;
    assert!((find("landing_field").actual / ws_mlw - 1.0).abs() < 1e-12);

    // The optimizer limit is the factored definition shared with the delivery
    // gate: at the limiting wing loading the unfactored segment-model distance
    // at Vref = 1.23 VS1g divided by the 0.6 dry share equals the LDA.
    if let Some(arrival) = outcome.arrival {
        let perf = &config.performance;
        let sigma = density_ratio(arrival.elevation_m, arrival.isa_deviation_c);
        let limit = find("landing_field").limit;
        let lfl_at_limit = landing_distance_at_limit(perf, limit, sigma)
            / alas_perf::performance::JET_DRY_LANDING_SHARE;
        assert!((lfl_at_limit / arrival.lda_m - 1.0).abs() < 1e-9);
    }
}

/// Segment-model landing distance at a limiting wing loading [Pa].
fn landing_distance_at_limit(perf: &PerformanceConfig, ws_pa: f64, sigma: f64) -> f64 {
    let v_app = alas_perf::performance::VREF_OVER_VS1G
        * (2.0 * ws_pa / (1.225 * sigma * perf.cl_max_land)).sqrt();
    alas_perf::performance::LandingSegmentModel::jet(perf.main_gear_truck_wheels).distance_m(v_app)
}

/// The engine-out departure polar.
mod departure_polar {
    use super::super::*;
    use crate::assess_candidate;
    use crate::objective::DesignObjective;
    use alas_config::design_variables::DesignVector;
    use alas_config::ObjectiveKind;

    /// The gear-down first segment reads the table's own clean drag at the
    /// departure lift coefficient, field altitude and Mach, at the field
    /// Reynolds number: it is not the cruise-altitude, cruise-CL tangent. (The
    /// gear-up second segment reads the `V2` climb-out locus instead, see
    /// `net_path::v2_polar`.)
    #[test]
    fn the_departure_table_is_read_at_the_departure_point_not_the_cruise_tangent() {
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
        let shift = crate::mdo::drag_table::high_lift_lift_shift(
            config.performance.cl_max_to,
            config.performance.cl_max_clean,
        );
        assert!(shift > 0.0);
        let expected = drag.cd_airport_base_at_atmosphere(cl_v2, mach, field_m, 0.0, shift);
        assert!(expected.is_finite() && expected > 0.0);

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
    let aspect_ratio =
        super::net_path::climb_out_aspect_ratio(&config, outcome.plane.b_ref, outcome.plane.s_ref);
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
        // The V2 climb-out locus (Torenbeek 1982, Eq. 5-29) of a slatted wing:
        // C0 0.018, E 0.70, lowered 4 % for a failed wing-mounted engine
        // unless the asymmetric-trim drag is stated in the configuration.
        let stated = performance.oei_asymmetric_trim_cd.is_some();
        let e = if stated { 0.70 } else { 0.70 * 0.96 };
        let cd = 0.018 + cl * cl / (std::f64::consts::PI * aspect_ratio * e);
        (cl, cd)
    };
    let (cl, _) = v2_point(&config);
    assert!((cl - config.performance.oei_climb_cl).abs() > 0.5);
    let n = outcome.n_engines as f64;
    let gradient = far25_oei_gradient(outcome.n_engines).unwrap();

    // Every evidence term configured: the closed form of the second segment
    // at the V2 lift coefficient, on the SLS axis through the given ratio. The
    // flaps are in the locus, so no flap increment is added; the stated trim
    // drag is, on the unreduced E, with the stated failed-engine drag.
    config.performance.oei_condition_to_sls_thrust_ratio = Some(0.8);
    config.performance.oei_asymmetric_trim_cd = Some(0.003);
    config.performance.oei_windmilling_cd = Some(0.002);
    let (cl, cd) = v2_point(&config);
    let expected = n / (n - 1.0) * (gradient + (cd + 0.003 + 0.002) / cl) / 0.8;
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
    let inflight = n / (n - 1.0) * (gradient + cd / cl);
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
    assert_eq!(net.role, ResidualRole::Constraint);
    assert!(net.actual > 0.0, "the net take-off path keeps climbing");
}

/// An engine-out climb that cannot be evaluated, or whose engine count has no
/// certification schedule, is a failed hard check and never a skipped one.
#[test]
fn an_undemonstrable_engine_out_climb_is_a_hard_failure() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    let mut outcome = run_candidate(&config, &design.to_array()).unwrap();
    let flag = |outcome: &SizingOutcome, id: &str| {
        performance_residuals(outcome, &config, ResidualRole::Constraint)
            .into_iter()
            .find(|residual| residual.id == id)
    };
    // The registered case evaluates every climb check.
    assert!(flag(&outcome, "oei_second_segment_evidence_gap").is_none());
    outcome.departure = None;
    let gap = flag(&outcome, "oei_second_segment_evidence_gap").unwrap();
    assert_eq!(gap.role, ResidualRole::Constraint);
    assert!(gap.normalized_violation > 0.0);
    let mut five = run_candidate(&config, &design.to_array()).unwrap();
    five.n_engines = 5;
    let unsupported = flag(&five, "oei_part25_engine_count_unsupported").unwrap();
    assert_eq!(unsupported.role, ResidualRole::Constraint);
    assert!(unsupported.normalized_violation > 0.0);
}

/// The first segment's lift-off speed is the rotation speed, delayed with V2
/// by the ratio the field length is charged for when the improved climb
/// raises V2, and never above V2.
#[test]
fn the_first_segment_lift_off_is_delayed_with_an_improved_climb_v2() {
    let mut perf = alas_config::PerformanceConfig::default();
    let schedule = perf.schedule_takeoff_speed_ratios();
    let v_s = 70.0;
    let speeds = |perf: &alas_config::PerformanceConfig| {
        let ratios = perf.takeoff_speed_ratios();
        alas_perf::performance::VSpeeds {
            v_stall_to_ms: v_s,
            v_stall_land_ms: 60.0,
            v_mc_ms: ratios.vmc * v_s,
            v1_ms: ratios.v1 * v_s,
            v_r_ms: ratios.vr * v_s,
            v2_ms: ratios.v2 * v_s,
            v_app_ms: 78.0,
            v_td_ms: 69.0,
        }
    };
    // No improved climb: the schedule rotation speed, as before.
    let plain = speeds(&perf);
    assert_eq!(
        net_path::lift_off_speed_m_s(&perf, &plain).to_bits(),
        plain.v_r_ms.to_bits()
    );
    // V2 raised to 1.30 VSR: VR moves by 1.30 over the schedule V2 ratio.
    perf.improved_climb_v2_over_vsr = Some(1.30);
    let improved = speeds(&perf);
    assert_eq!(improved.v_r_ms.to_bits(), plain.v_r_ms.to_bits());
    let delayed = net_path::lift_off_speed_m_s(&perf, &improved);
    let expected = schedule.vr * v_s * 1.30 / schedule.v2;
    assert!((delayed - expected).abs() < 1e-9, "{delayed} vs {expected}");
    assert!(delayed > plain.v_r_ms && delayed < improved.v2_ms);
    // The spread from lift-off to V2 keeps its schedule ratio.
    assert!((improved.v2_ms / delayed - schedule.v2 / schedule.vr).abs() < 1e-12);
}

#[test]
fn an_external_polar_has_no_unmeasured_reynolds_correction() {
    let polar = crate::mdo::mission_model::ParabolicPolar::new(0.02, 0.04, 0.003, 0.82);
    let drag = crate::mdo::types::CandidateDrag::External(std::sync::Arc::new(polar));
    for deviation_c in [-20.0, 0.0, 25.0] {
        let cd = drag.cd_airport_base_at_atmosphere(1.1, 0.22, 2_100.0, deviation_c, 0.4);
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
    // The limit is the wing loading whose segment-model distance fills the
    // usable runway share, which the wet factor divides by 1.15.
    let arrival = outcome.arrival.as_ref().unwrap();
    let sigma = density_ratio(arrival.elevation_m, arrival.isa_deviation_c);
    let distance_dry = landing_distance_at_limit(&config.performance, dry, sigma);
    let distance_wet = landing_distance_at_limit(&config.performance, wet, sigma);
    assert!((distance_dry / distance_wet - 1.15).abs() < 1e-9);
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
    let mission = crate::mdo::candidate_mission_model(&config, artifacts).unwrap();
    let states = mission.planned_cruise_states(mass).unwrap();
    assert!(!states.is_empty());
    // The planned levels are checked in the state the mission
    // flies, the US Standard Atmosphere 1976 on the mission's temperature
    // deviation and the gravity at the level (`CruiseDay::mission`), so the
    // hand calculation uses the same air and weight, not the standard day
    // and the surface gravity of the design-level rows above.
    let mut planned_hand = 0.0_f64;
    for state in &states {
        let day = alas_atmo::us1976_compute_values(state.altitude_m, mission.isa_deviation_c);
        let g_day = crate::mdo::mission_model::gravity_at_altitude(G, state.altitude_m);
        let mach = state.tas_m_s / day.speed_of_sound_m_s;
        let q = 0.5 * day.density_kg_m3 * state.tas_m_s * state.tas_m_s;
        let (_, r) = model
            .thrust(
                oei_deck::EngineOutCondition {
                    altitude_m: state.altitude_m,
                    isa_deviation_c: mission.isa_deviation_c,
                    tas_m_s: state.tas_m_s,
                },
                oei_deck::EngineOutRating::MaximumCruise,
                false,
            )
            .unwrap();
        let cl_state = state.mass_kg * g_day / (s_ref * q);
        planned_hand = planned_hand.max(
            state.mass_kg / mass * (g_day / G) * drag.cd(cl_state, mach, state.altitude_m)
                / cl_state
                / r,
        );
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
    // Before blockers B work the turboprop row used the flat thrust lapse while
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

/// Every registered preset demonstrates its engine-out climbs: no preset hits
/// the evidence-gap or unsupported-engine-count flags, which are hard
/// failures, and the net take-off path margin is positive.
#[test]
fn every_registered_preset_demonstrates_its_engine_out_climbs() {
    let mut failures = Vec::new();
    let mut evaluated = 0;
    for name in alas_config::presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let Ok(outcome) = run_candidate(&config, &design.to_array()) else {
            failures.push(format!("{name}: the nominal candidate does not evaluate"));
            continue;
        };
        evaluated += 1;
        for residual in performance_residuals(&outcome, &config, ResidualRole::Constraint) {
            if residual.id.ends_with("_evidence_gap")
                || residual.id == "oei_part25_engine_count_unsupported"
                || (residual.id == "takeoff_net_path_gradient" && residual.actual <= 0.0)
            {
                failures.push(format!("{name}: {} ({})", residual.id, residual.actual));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
    assert!(evaluated >= 8, "{evaluated} presets evaluated");
}

/// The three obstacle verdicts on synthetic margins: clears and
/// violated give one constraint row against the margin the data certify; undetermined
/// gives a met constraint row on the violated bound plus a Diagnostic warning that
/// carries the margin, the certified margin and the completeness reason, and never
/// ranks or rejects.
#[test]
fn an_undetermined_obstacle_verdict_is_a_warning_and_a_violated_one_rejects() {
    use alas_perf::performance::obstacle::{ObstacleMargin, ObstacleVerdict};
    let margin = |margin_m: f64, violation_slack_m: f64| ObstacleMargin {
        uncertainty_m: 14.0,
        symmetric_uncertainty_m: 14.0,
        unverified_allowance_m: 0.0,
        margin_best_m: margin_m + 5.0,
        uncertainty_best_m: 14.0,
        violation_slack_m,
        margin_m,
        runway: "36".to_owned(),
        acceleration_height_m: 122.0,
        limiting_distance_m: 1455.0,
        limiting_height_m: 1.9,
    };
    let rows = |m: &ObstacleMargin| {
        super::net_path::margin_rows(
            "ZZZZ",
            m,
            m.verdict(),
            String::new(),
            ResidualRole::Constraint,
        )
    };
    let hard = |rows: &[ConstraintResidual]| {
        rows.iter()
            .filter(|row| row.role == ResidualRole::Constraint && row.violated())
            .count()
    };
    // Clears: the margin reaches the certified margin.
    let clears = margin(130.0, 144.0);
    assert_eq!(clears.verdict(), ObstacleVerdict::Clears);
    let clear_rows = rows(&clears);
    assert_eq!(clear_rows.len(), 1);
    assert_eq!(hard(&clear_rows), 0);
    // Violated: even the most favourable margin lies below minus the symmetric uncertainty.
    let violated = margin(-40.0, -20.0);
    assert_eq!(violated.verdict(), ObstacleVerdict::Violated);
    let violated_rows = rows(&violated);
    assert_eq!(violated_rows.len(), 1);
    assert_eq!(hard(&violated_rows), 1);
    assert!(violated_rows[0].raw_residual > 0.0);
    // Undetermined: a warning, not a rejection.
    let undetermined = margin(5.0, 19.0);
    assert_eq!(undetermined.verdict(), ObstacleVerdict::Undetermined);
    let undetermined_rows = rows(&undetermined);
    assert_eq!(undetermined_rows.len(), 2);
    assert_eq!(hard(&undetermined_rows), 0);
    let bound = &undetermined_rows[0];
    assert_eq!(bound.id, "takeoff_obstacle_clearance");
    assert_eq!(bound.role, ResidualRole::Constraint);
    assert!(bound.raw_residual <= 0.0 && bound.actual == 19.0 && bound.limit == 0.0);
    let warning = &undetermined_rows[1];
    assert_eq!(warning.id, "takeoff_obstacle_clearance_undetermined");
    assert_eq!(warning.role, ResidualRole::Diagnostic);
    assert_eq!(warning.normalized_violation, 0.0);
    assert!((warning.actual, warning.limit) == (5.0, 14.0) && warning.raw_residual > 0.0);
    assert!(warning
        .detail
        .as_deref()
        .is_some_and(|d| d.contains(super::net_path::OBSTACLE_UNDETERMINED_REASON)));
    // An unverified survey adds a Diagnostic beside the verdict and changes nothing else.
    let mut unverified = margin(120.0, 134.0);
    unverified.unverified_allowance_m = 112.0;
    assert_eq!(unverified.verdict(), ObstacleVerdict::Clears);
    let unverified_rows = rows(&unverified);
    assert_eq!(unverified_rows.len(), 2);
    assert_eq!(hard(&unverified_rows), 0);
    let flag = &unverified_rows[1];
    assert_eq!(flag.id, "takeoff_obstacle_survey_unverified");
    assert_eq!(flag.role, ResidualRole::Diagnostic);
    assert!(!flag.violated());
    assert!((flag.limit - 126.0).abs() < 1e-12);
}

/// Every registered preset has a departure survey and gets an obstacle row, never
/// the evidence-gap flag. The margin the requirement needs is the symmetric accuracy
/// of the terrain dataset alone; the unverified-survey allowances are the Diagnostic
/// `takeoff_obstacle_survey_unverified`. The net path ends where the take-off path does
/// (CS 25.111 (a)). With no declared departure track the hard row reads the path up to the first
/// allowed turn (CAT.POL.A.210 (b)(2)); what the straight-out track meets beyond it is the
/// Diagnostic `takeoff_track_assumed_straight_out` (A320 LEMD 36L +35.2 m; C295 LEMD 36L
/// +67.1 m at its certified 21,050 kg; DC-10 +13.5 m). Probe, gear 15 s
/// (registered_preset_obstacle_table): violated AVE EGLL 09L
/// -10.8 m; undetermined (A350-900 EGLL 27R: the verdict changes between 8 s and 15 s) B787-9 RJTT 16L +8.85 m against 10.1 m; all others clear.
#[test]
fn every_registered_preset_reports_its_obstacle_clearance() {
    let mut missing = Vec::new();
    let mut not_clear = Vec::new();
    for name in alas_config::presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let Ok(outcome) = run_candidate(&config, &design.to_array()) else {
            continue;
        };
        let table = performance_residuals(&outcome, &config, ResidualRole::Constraint);
        // Every preset meets the row, the AVE included (+0.0035 with the
        // lift-off delayed to its improved-climb V2).
        assert!(
            table
                .iter()
                .any(|residual| residual.id == "oei_first_segment" && !residual.violated()),
            "{name}: the CS 25.121(a) first-segment row is present and met"
        );
        let undetermined = table
            .iter()
            .find(|residual| residual.id == "takeoff_obstacle_clearance_undetermined");
        if let Some(warning) = undetermined {
            // A reported warning, never a rejection.
            assert_eq!(warning.role, ResidualRole::Diagnostic, "{name}");
            assert!(!warning.violated(), "{name}");
            assert!(
                warning.detail.as_deref().is_some_and(|d| d
                    .contains(super::net_path::OBSTACLE_UNDETERMINED_REASON)
                    || d.contains(super::net_path::GEAR_UNSOURCED_REASON)),
                "{name}"
            );
        }
        match table
            .iter()
            .find(|residual| residual.id == "takeoff_obstacle_clearance")
        {
            Some(row) if row.violated() => {
                assert!(undetermined.is_none(), "{name}: violated and undetermined");
                not_clear.push(format!("{name}: violated"));
            }
            Some(_) if undetermined.is_some() => not_clear.push(format!("{name}: undetermined")),
            Some(_) => {}
            None => missing.push(name.to_string()),
        }
    }
    assert!(missing.is_empty(), "{missing:?}");
    not_clear.sort();
    // Expected states follow from the physics of the take-off path, not from a target:
    // - A320-200: no longer violated. Its flap drag is Raymer Eq. 12.61 at the declared
    //   flap run (0.0125, not the class 0.015), the take-off span efficiency rises with
    //   flap angle (Obert Fig. 27.39) and the 0.70 airplane efficiency factor is gone.
    // - The net path ends with the take-off path (CS 25.111(a)) and the
    //   required margin is the dataset's symmetric survey uncertainty alone (the
    //   unverified-survey allowances are diagnostics), so most presets clear. By probe
    //   (registered_preset_obstacle_table, generic gear retraction time 15 s = 3 s + 12 s,
    //   flagged; the hard row reads the path up to the first allowed turn, CAT.POL.A.210 (b)(2)):
    //   A220-300, A320-200, DC-10 and the other presets clear or hold no obstacle there.
    // - A350-900: undetermined at EGLL 27R: -5.7 m at the generic gear time (15 s, violated) but
    //   +4.2 m at 8 s (cleared); the verdict changes with the unsourced gear retraction time, so it
    //   is undetermined. The cell is a DSM cell 24 m up
    //   at 2.5 km, before the turn point (net path half a wingspan high at 2.7 km), with the
    //   engine sized to the 1.6 % net second segment. The strip-edge fix (read the cell from where
    //   the strip reaches it) is in; the gear time (15 s) is the generic flagged value.
    // - B787-9: undetermined at RJTT 16L, +8.85 m against 10.1 m: the 6.1 m datum offset
    //   corrects the reference (the tile reads the reclaimed runway as sea) and is no longer
    //   added to the required margin (16.2 m before).
    // - AVE: violated at EGLL 09L, -10.8 m, the marginal finding noted above.
    // - C295: its LEMD 36L obstacle (503.6 m ground, 42.7 km straight out) lies far beyond the
    //   first allowed turn (CAT.POL.A.210 (b)(2)), so with no declared departure track it is the
    //   Diagnostic takeoff_track_assumed_straight_out (-57.1 m), not a hard row. The same holds
    //   for every other LEMD, EDDF, OMDB and EVRA case; the hard row there reads the part of the
    //   path up to the turn, which holds no obstacle.
    assert_eq!(
        not_clear,
        [
            "A350-900: undetermined",
            "AVE: violated",
            "B787-9: undetermined",
        ]
    );
}

/// Probe: the obstacle margin and its limiting cell for every registered preset.
#[test]
#[ignore = "diagnostic table of all registered presets; prints"]
// A probe prints its numbers; it asserts nothing.
#[allow(clippy::print_stderr)]
fn registered_preset_obstacle_table() {
    for name in alas_config::presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let Ok(outcome) = run_candidate(&config, &design.to_array()) else {
            continue;
        };
        let table = performance_residuals(&outcome, &config, ResidualRole::Constraint);
        // The clean drag table is fitted on CL 0 to CL_max_clean; V2 at 1.13 VSR and
        // lift-off near 1.05 VSR sit above it (the last quadratic is extended).
        let perf = &config.performance;
        eprintln!(
            "LIFT {name}: CL_max_clean {:.2}, CL_V2 {:.2} ({:.2} x), CL_LOF {:.2} ({:.2} x)",
            perf.cl_max_clean,
            perf.cl_max_to / 1.13_f64.powi(2),
            perf.cl_max_to / 1.13_f64.powi(2) / perf.cl_max_clean,
            perf.cl_max_to / 1.05_f64.powi(2),
            perf.cl_max_to / 1.05_f64.powi(2) / perf.cl_max_clean
        );
        for row in table.iter().filter(|row| {
            row.id == "takeoff_obstacle_clearance"
                || row.id == "oei_first_segment"
                || row.id.starts_with("enroute_obstacle")
        }) {
            eprintln!(
                "OBSTACLE {name} {}: {:.2} {}",
                row.id,
                row.actual,
                row.detail.as_deref().unwrap_or("")
            );
        }
    }
}

/// An aerodrome without an obstacle survey (a hand-entered one) fails the
/// CAT.POL.A.210 check as a hard evidence gap, never a skipped row; a
/// surveyed one gets the clearance row.
#[test]
fn an_aerodrome_without_a_survey_fails_the_obstacle_check() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    let mut outcome = run_candidate(&config, &design.to_array()).unwrap();
    let row = |outcome: &SizingOutcome, id: &str| {
        performance_residuals(outcome, &config, ResidualRole::Constraint)
            .into_iter()
            .find(|residual| residual.id == id)
    };
    assert!(row(&outcome, "takeoff_obstacle_clearance").is_some());
    assert!(row(&outcome, "takeoff_obstacle_clearance_evidence_gap").is_none());
    outcome.departure = Some(Box::leak(Box::new(Airport::custom(
        "Hand entered",
        100.0,
        3000.0,
        3000.0,
        0.0,
        40.0,
        -3.0,
    ))));
    let gap = row(&outcome, "takeoff_obstacle_clearance_evidence_gap").unwrap();
    assert_eq!(gap.role, ResidualRole::Constraint);
    assert!(gap.normalized_violation > 0.0);
    assert!(row(&outcome, "takeoff_obstacle_clearance").is_none());
}

/// The take-off field row against the belly clearance over the jet sample
/// (0.298, 0.35, 0.546 D), declared each time. The clearance reaches the field
/// through two channels of one rotation on one plane: a higher plane raises
/// the tail-strike attitude (a lower geometry-limited `VMU`, CS 25.107(d) and
/// (e)(1)(iv)(B)) and the wing (less ground relief on the drag), so the field
/// need not be monotone in the clearance. What is asserted:
/// - the tail-strike attitude and the static wing height both rise strictly
///   with the clearance (geometry);
/// - the two ends give different fields (the clearance reaches the field);
/// - an undeclared clearance is flown whole at each end of the sample and
///   keeps the longer field: exactly `max(field(0.298 D), field(0.546 D))`,
///   never a mix of the ends;
/// - the nominal 0.35 D field does not exceed that within 0.2 %: the
///   quasi-convexity of the field in the clearance (no interior maximum)
///   that lets the judgement fly the ends only (`takeoff_field::planes`).
///   0.2 % is the resolution allowance of the integration, not a margin.
///
/// The row's `limit` is the field length (`actual` is the available take-off
/// distance, the same at every plane). Prints the lengths.
#[test]
// The finite difference prints its numbers.
#[allow(clippy::print_stderr)]
fn the_takeoff_field_is_one_rotation_per_ground_plane() {
    for name in ["E195-E2", "A320-200"] {
        let base = AlasConfig::from_value(&serde_json::json!({
            "preset": name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let outcome = run_candidate(&base, &design.to_array()).unwrap();
        let declared = |clearance: Option<f64>| {
            let mut config = base.clone();
            config.landing_gear.fuselage_ground_clearance_m =
                clearance.map(|fraction| fraction * base.geometry.fuselage.diameter_m);
            config
        };
        let field = |clearance: Option<f64>| {
            let config = declared(clearance);
            let row = performance_residuals(&outcome, &config, ResidualRole::Constraint)
                .into_iter()
                .find(|row| row.id == "takeoff_field")
                .unwrap_or_else(|| panic!("{name}: no takeoff_field row"));
            assert!(row.limit.is_finite(), "{name}: field length {}", row.limit);
            row.limit
        };
        let lift = |fraction: f64| {
            crate::envelope::liftoff_lift(&outcome.plane, &declared(Some(fraction)))
                .unwrap_or_else(|| panic!("{name}: no lift-off lift at {fraction} D"))
        };
        let fractions = [0.298, 0.35, 0.546];
        let lifts = fractions.map(lift);
        let strike = lifts.map(|l| l.tail_strike_deg);
        let height = lifts.map(|l| l.wing_heights_m(0.0, 0.0).0);
        assert!(
            strike.windows(2).all(|pair| pair[1] > pair[0]),
            "{name}: tail strike {strike:?}"
        );
        assert!(
            height.windows(2).all(|pair| pair[1] > pair[0]),
            "{name}: wing height {height:?}"
        );
        let [low, nominal, high] = fractions.map(|fraction| field(Some(fraction)));
        let undeclared = field(None);
        eprintln!(
            "{name}: take-off field length {low:.1} m (0.298 D), {nominal:.1} m (0.35 D), \
             {high:.1} m (0.546 D), undeclared {undeclared:.1} m; tail strike {strike:?} deg"
        );
        assert!((low - high).abs() > 1e-3, "{name}: ends {low} {high}");
        let longer = low.max(high);
        assert!(
            (undeclared - longer).abs() < 1e-9,
            "{name}: undeclared {undeclared} against the ends {low} {high}"
        );
        assert!(
            nominal <= longer * 1.002,
            "{name}: nominal {nominal} above the ends {low} {high}"
        );
    }
}

/// Probe: the departure/arrival aerodromes and the field rows of every
/// registered preset, for before/after comparisons of the airport table.
#[test]
#[ignore = "diagnostic table of all registered presets; prints"]
// A probe prints its numbers; it asserts nothing.
#[allow(clippy::print_stderr)]
fn registered_preset_field_table() {
    for name in alas_config::presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let Ok(outcome) = run_candidate(&config, &design.to_array()) else {
            continue;
        };
        eprintln!(
            "FIELD {name} departure {} arrival {}",
            outcome.departure.map_or("-", |a| a.icao.as_str()),
            outcome.arrival.map_or("-", |a| a.icao.as_str())
        );
        for row in performance_residuals(&outcome, &config, ResidualRole::Constraint)
            .iter()
            .filter(|row| {
                [
                    "takeoff_field",
                    "landing_field",
                    "oei_second_segment",
                    "approach_speed",
                ]
                .contains(&row.id)
            })
        {
            eprintln!(
                "FIELD {name} {} actual {:.4} limit {:.4}",
                row.id, row.actual, row.limit
            );
        }
    }
}

/// CAT.POL.A.215 applies along the planned route. With no declared departure track the
/// straight-out margin is the Diagnostic `enroute_track_assumed_straight_out` (A320, LEMD 36L:
/// violated) and no hard row exists; with a declared track (here straight on the runway
/// heading for the whole survey) `enroute_obstacle_clearance` is a Constraint in a design mode
/// and a Diagnostic in the fixed baseline. The +-615 m survey strip never covers the 9.3 km
/// corridor, which is its own Diagnostic row and never a pass.
#[test]
fn the_en_route_obstacle_row_needs_a_declared_track_to_be_hard() {
    let heading = alas_config::airport_obstacles::departure_surveys("LEMD")
        .unwrap()
        .iter()
        .find(|survey| survey.runway == "36L")
        .unwrap()
        .heading_deg;
    let rows = |mode: &str, track: bool| {
        let mut value = serde_json::json!({
            "preset": "A320-200",
            "optimizer": {"design_space": {"mode": mode}}
        });
        if track {
            value["requirements"] = serde_json::json!({
                "departure_track": [{"heading_deg": heading, "distance_m": 60_000.0}]
            });
        }
        let config = AlasConfig::from_value(&value).unwrap();
        let design = alas_config::presets::get("A320-200").unwrap().design_vector;
        let outcome = run_candidate(&config, &design.to_array()).unwrap();
        performance_residuals(&outcome, &config, ResidualRole::Constraint)
    };
    let find = |table: &[ConstraintResidual], id: &str| {
        table
            .iter()
            .find(|row| row.id == id)
            .map(|row| (row.role, row.raw_residual > 0.0))
    };
    let assumed = rows("reference_adaptation", false);
    assert!(find(&assumed, "enroute_obstacle_clearance").is_none());
    let (role, violated) = find(&assumed, "enroute_track_assumed_straight_out").unwrap();
    assert_eq!(role, ResidualRole::Diagnostic);
    assert!(violated, "the straight-out margin is negative");
    assert!(find(&assumed, "enroute_obstacle_survey_does_not_cover_corridor").is_some());
    let declared = rows("reference_adaptation", true);
    assert!(find(&declared, "enroute_track_assumed_straight_out").is_none());
    let design = find(&declared, "enroute_obstacle_clearance").unwrap();
    assert_eq!(design.0, ResidualRole::Constraint);
    let baseline = find(
        &rows("baseline_sandbox", true),
        "enroute_obstacle_clearance",
    )
    .unwrap();
    assert_eq!(baseline.0, ResidualRole::Diagnostic);
    assert_eq!(design.1, baseline.1);
}

/// CAT.POL.A.210 (b)(2) fixes the track up to the first allowed turn (net path half a wingspan,
/// not under 50 ft, high); beyond it the straight-out take-off obstacle is hard only along a
/// declared departure track. The C295 at the non-certified 23,200 kg overload (the certified
/// 21,050 kg clears) meets the Guadarrama 42.7 km straight out of LEMD 36L, long after the turn
/// point: hard with a declared track, `takeoff_track_assumed_straight_out` (Diagnostic) without.
#[test]
fn take_off_obstacles_beyond_the_first_turn_need_a_declared_track_to_be_hard() {
    let heading = alas_config::airport_obstacles::departure_surveys("LEMD")
        .unwrap()
        .iter()
        .find(|survey| survey.runway == "36L")
        .unwrap()
        .heading_deg;
    let table = |track: bool| {
        let mut value = serde_json::json!({
            "preset": "C295",
            "optimizer": {"design_space": {"mode": "reference_adaptation"}},
            "requirements": {"mtow_kg": 23_200.0}
        });
        if track {
            value["requirements"] = serde_json::json!({
                "mtow_kg": 23_200.0,
                "departure_track": [{"heading_deg": heading, "distance_m": 60_000.0}]
            });
        }
        let config = AlasConfig::from_value(&value).unwrap();
        let design = alas_config::presets::get("C295").unwrap().design_vector;
        let outcome = run_candidate(&config, &design.to_array()).unwrap();
        performance_residuals(&outcome, &config, ResidualRole::Constraint)
    };
    let find = |table: &[ConstraintResidual], id: &str| {
        table
            .iter()
            .find(|row| row.id == id)
            .map(|row| (row.role, row.raw_residual > 0.0))
    };
    let assumed = table(false);
    let (role, short) = find(&assumed, "takeoff_track_assumed_straight_out").unwrap();
    assert_eq!(role, ResidualRole::Diagnostic);
    assert!(short, "the straight-out margin is negative");
    assert!(
        !find(&assumed, "takeoff_obstacle_clearance").is_some_and(|(_, violated)| violated),
        "nothing before the first turn violates"
    );
    let declared = table(true);
    assert!(find(&declared, "takeoff_track_assumed_straight_out").is_none());
    let (role, violated) = find(&declared, "takeoff_obstacle_clearance").unwrap();
    assert_eq!(role, ResidualRole::Constraint);
    assert!(violated);
}
