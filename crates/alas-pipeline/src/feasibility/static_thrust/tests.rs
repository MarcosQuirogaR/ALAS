// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Shipped presets are test preconditions: a missing one is the failure being
// reported, not a recoverable library condition.
#![allow(clippy::expect_used)]

use super::*;

/// A representative sea-level lift-off speed, so the tests exercise the
/// contract rather than an airport.
const LIFT_OFF_M_S: f64 = 70.0;

fn preset_config(name: &str) -> AlasConfig {
    let preset = alas_config::presets::get(name).expect("a shipped preset");
    let mut config = AlasConfig {
        preset: preset.name.to_owned(),
        geometry: preset.geometry.clone(),
        requirements: preset.requirements.clone(),
        ..AlasConfig::default()
    };
    config.geometry.engine.apply_engine_spec();
    config
}

#[test]
fn cruise_thrust_requirement_is_the_shared_table_drag_over_weight() {
    for name in ["ATR72-600", "AVE"] {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name}))
            .expect("registered preset config");
        let design = alas_config::presets::get(name)
            .expect("registered preset")
            .design_vector;
        let mut report = crate::full_analysis::FullAnalysis::new(config.clone())
            .run(&design, true)
            .expect("full analysis");
        let artifacts = report
            .fuel
            .artifacts(&config, &report.design)
            .expect("drag source");
        let atmosphere = alas_atmo::Atmosphere::new(config.requirements.cruise_altitude_m);
        let velocity_m_s = config.requirements.cruise_mach * atmosphere.speed_of_sound();
        let dynamic_pressure_pa = 0.5 * atmosphere.density() * velocity_m_s * velocity_m_s;
        // The fit is a report approximation; poisoning it proves that the
        // cruise margin reads the candidate table, including trim/wave.
        report.polar_fit.cd0 = f64::NAN;
        report.polar_fit.k = f64::NAN;
        report.polar_fit.c1 = f64::NAN;
        for cl in [0.3, 0.6, 0.9] {
            let expected = artifacts.drag.cd(
                cl,
                config.requirements.cruise_mach,
                config.requirements.cruise_altitude_m,
            ) / cl;
            let actual = cruise_required_tw(&config, &report, cl * dynamic_pressure_pa)
                .expect("shared cruise demand")
                * config.performance.thrust_lapse;
            assert!((actual - expected).abs() < 1.0e-14, "{name} CL {cl}");
        }
    }
}

#[test]
fn sized_candidate_cruise_demand_matches_optimizer_after_lift_coefficient_changes() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .expect("registered preset config");
    let design = alas_config::presets::get("A320-200")
        .expect("registered preset")
        .design_vector;
    let assessment = alas_opt::assess_candidate(
        &alas_opt::DesignObjective::new(config.clone()),
        &design.to_array(),
    )
    .expect("candidate assessment");
    let sized = &assessment.sized;
    let mut report = crate::full_analysis::FullAnalysis::new(config.clone())
        .run_sized_candidate(&assessment.resolved.design, sized)
        .expect("sized candidate report");
    let artifacts = report
        .fuel
        .artifacts(&config, &report.design)
        .expect("carried candidate artifacts");
    let table = artifacts.drag.table().expect("native candidate table");
    let sized_table = sized
        .fuel_artifacts
        .drag
        .table()
        .expect("optimizer candidate table");
    assert!(std::sync::Arc::ptr_eq(table, sized_table));

    let atmosphere = alas_atmo::Atmosphere::new(config.requirements.cruise_altitude_m);
    let velocity_m_s = config.requirements.cruise_mach * atmosphere.speed_of_sound();
    let dynamic_pressure_pa = 0.5 * atmosphere.density() * velocity_m_s * velocity_m_s;
    let wing_loading_pa =
        sized.takeoff_mass_kg * config.requirements.gravity_m_s2 / report.airplane.s_ref;
    let sized_cl = wing_loading_pa / dynamic_pressure_pa;
    assert!(sized.takeoff_mass_kg < sized.design_gross_mass_kg);
    assert!(
        (sized_cl - table.design_cl()).abs() > 1.0e-6,
        "fixture must move cruise CL: sized {sized_cl}, table {}",
        table.design_cl()
    );

    // These are independently evaluated consumers of the same carried
    // table. A report fit cannot replace the final-mass cruise demand.
    report.polar_fit.cd0 = f64::NAN;
    report.polar_fit.c1 = f64::NAN;
    report.polar_fit.k = f64::NAN;
    let report_demand =
        cruise_required_tw(&config, &report, wing_loading_pa).expect("report cruise demand");
    let optimizer_demand = assessment
        .residuals
        .iter()
        .find(|residual| residual.id == "cruise_thrust")
        .expect("optimizer cruise residual")
        .limit;
    assert!(
        (report_demand - optimizer_demand).abs() < 1.0e-14,
        "report {report_demand}, optimizer {optimizer_demand}"
    );
}

#[test]
fn a_turbofan_keeps_the_certificated_rating_unchanged() {
    let config = preset_config("A320-200");
    let resolved = resolve(&config, LIFT_OFF_M_S, 1.0);
    let expected_n = config.geometry.engine.spanwise_positions_m.len() as f64
        * config.geometry.engine.thrust_kn()
        * 1_000.0;
    assert_eq!(resolved.source, StaticThrustSource::CertifiedJetRating);
    assert_eq!(resolved.thrust_n, expected_n);
    assert!(resolved.provenance_note().is_none());
}

#[test]
fn the_turboprop_uses_the_roll_mean_not_the_static_thrust() {
    let config = preset_config("ATR72-600");
    assert_eq!(
        config.geometry.engine.thrust_kn(),
        0.0,
        "the jet rating must stay exactly zero for a shaft-power engine"
    );
    let resolved = resolve(&config, LIFT_OFF_M_S, 1.0);
    let StaticThrustSource::PropellerRollMean {
        static_thrust_n,
        mean_roll_speed_m_s,
        residual_jet_thrust_per_engine_n,
        low_thrust_n,
        high_thrust_n,
        ..
    } = &resolved.source
    else {
        panic!("expected a propeller roll mean, got {:?}", resolved.source);
    };
    // A propeller loses thrust with speed, so the roll mean is strictly
    // below the static value. Taking the static one is the flattering
    // error this replaced.
    assert!(
        resolved.thrust_n < *static_thrust_n,
        "roll mean {} is not below static {static_thrust_n}",
        resolved.thrust_n
    );
    assert!((mean_roll_speed_m_s - LIFT_OFF_M_S / std::f64::consts::SQRT_2).abs() < 1.0e-9);
    assert_eq!(*residual_jet_thrust_per_engine_n, 0.0);
    // The forward-flight band is one-sided by construction: the blade
    // efficiency is declared at the conservative end of the 0.86-0.91
    // range the three independent routes agree on, so the modelled thrust
    // is its own lower bound and the band only opens upward.
    assert!(*low_thrust_n <= resolved.thrust_n && resolved.thrust_n < *high_thrust_n);

    let weight_n = config.requirements.mtow_kg * config.requirements.gravity_m_s2;
    let roll_mean_tw = resolved.thrust_n / weight_n;
    assert!(
        (0.15..=0.45).contains(&roll_mean_tw),
        "ground-roll mean T/W {roll_mean_tw} is outside the physical band for a twin turboprop"
    );
    let note = resolved
        .provenance_note()
        .expect("a propeller result must carry its provenance");
    assert!(note.contains("ground-roll MEAN"));
    assert!(note.contains("stays exactly zero"));
}

#[test]
fn an_aircraft_with_no_installed_engine_is_a_typed_absence() {
    let mut config = preset_config("ATR72-600");
    config.geometry.engine.spanwise_positions_m.clear();
    let resolved = resolve(&config, LIFT_OFF_M_S, 1.0);
    assert!(matches!(
        resolved.source,
        StaticThrustSource::Unavailable { .. }
    ));
    assert_eq!(resolved.thrust_n, 0.0);
}

#[test]
fn an_unusable_lift_off_speed_is_a_typed_absence_rather_than_a_guess() {
    let config = preset_config("ATR72-600");
    let resolved = resolve(&config, f64::NAN, 1.0);
    assert!(matches!(
        resolved.source,
        StaticThrustSource::Unavailable { .. }
    ));
}
