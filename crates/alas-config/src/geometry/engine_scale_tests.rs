// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A test asserts on values it built from fixtures it controls, so a failed
// unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn engine(name: &str) -> EngineConfig {
    let mut engine = EngineConfig {
        engine_name: name.to_owned(),
        ..Default::default()
    };
    engine.try_apply_engine_spec().unwrap();
    engine
}

fn relative(a: f64, b: f64) -> f64 {
    (a / b - 1.0).abs()
}

#[test]
fn a_factor_of_one_is_the_same_engine_bit_for_bit() {
    let engine = engine("CFM56-5B4/3");
    assert_eq!(engine.scaled_by(1.0).unwrap(), engine);
    let config = AlasConfig::default();
    assert_eq!(config.with_engine_scaled(1.0).unwrap(), config);
}

#[test]
fn a_turbofan_scales_thrust_flow_and_dimensions_by_the_sourced_exponents() {
    let base = engine("CFM56-5B4/3");
    let x: f64 = 1.15;
    let scaled = base.scaled_by(x).unwrap();
    let (Ok(ActiveEngineModel::Turbofan(b)), Ok(ActiveEngineModel::Turbofan(s))) =
        (base.active_model(), scaled.active_model())
    else {
        panic!("turbofan binding");
    };
    assert!(relative(s.rated_thrust_kn, b.rated_thrust_kn * x) < 1e-12);
    assert!(
        relative(
            s.off_design.cruise_reference_thrust_n,
            b.off_design.cruise_reference_thrust_n * x
        ) < 1e-12
    );
    assert!(relative(s.takeoff_fuel_flow_kg_s, b.takeoff_fuel_flow_kg_s * x) < 1e-12);
    // Specific quantities are the baseline's: fuel flow per unit thrust is
    // unchanged (Jenkinson 9.10), so is every cycle number.
    assert_eq!(s.cruise_tsfc_kg_kgf_hr, b.cruise_tsfc_kg_kgf_hr);
    assert_eq!(s.bypass_ratio, b.bypass_ratio);
    assert_eq!(s.part_power_fuel_flow_ratios, b.part_power_fuel_flow_ratios);
    assert_eq!(scaled.cruise_tsfc_kg_kgf_hr, base.cruise_tsfc_kg_kgf_hr);
    // Raymer Eqs. (10.1) and (10.2).
    assert!(relative(scaled.fan_diameter_m, base.fan_diameter_m * x.sqrt()) < 1e-12);
    assert!(relative(scaled.radius_scale_m, base.radius_scale_m * x.sqrt()) < 1e-12);
    assert!(
        relative(
            scaled.nacelle_length_m(),
            base.nacelle_length_m() * x.powf(0.4)
        ) < 1e-12
    );
    // The outline keeps its radius fractions.
    for (a, b) in scaled.nacelle_profile.iter().zip(&base.nacelle_profile) {
        assert_eq!(a.1, b.1);
    }
}

#[test]
fn scaling_composes_multiplicatively() {
    let base = engine("Trent 900");
    let twice = base.scaled_by(1.1).unwrap().scaled_by(0.95).unwrap();
    let once = base.scaled_by(1.1 * 0.95).unwrap();
    assert!(relative(twice.thrust_kn(), once.thrust_kn()) < 1e-12);
    assert!(relative(twice.fan_diameter_m, once.fan_diameter_m) < 1e-12);
    assert!(relative(twice.nacelle_length_m(), once.nacelle_length_m()) < 1e-12);
}

#[test]
fn the_catalogue_scale_is_measured_from_the_selected_row() {
    let base = engine("CFM56-5B4/3");
    assert!((base.catalogue_scale().unwrap() - 1.0).abs() < 1e-12);
    let up = base.scaled_by(1.1).unwrap().scaled_by(1.05).unwrap();
    assert!((up.catalogue_scale().unwrap() - 1.155).abs() < 1e-12);
    // A name that is not in the catalogue has no baseline.
    let mut unknown = base;
    unknown.engine_name = "not an engine".to_owned();
    assert!(unknown.catalogue_scale().is_none());
}

#[test]
fn invalid_factors_are_refused_without_a_result() {
    let base = engine("GE9X");
    for factor in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            base.scaled_by(factor),
            Err(EngineScaleError::Factor(_))
        ));
    }
}

#[test]
fn a_turboprop_scales_power_flow_propeller_and_keeps_the_tip_speed() {
    let base = engine("PW127M");
    let x: f64 = 0.9;
    let scaled = base.scaled_by(x).unwrap();
    let (Ok(ActiveEngineModel::Turboprop(b)), Ok(ActiveEngineModel::Turboprop(s))) =
        (base.active_model(), scaled.active_model())
    else {
        panic!("turboprop binding");
    };
    for (new, old) in [
        (s.takeoff_shaft_power_kw, b.takeoff_shaft_power_kw),
        (
            s.maximum_reserve_shaft_power_kw,
            b.maximum_reserve_shaft_power_kw,
        ),
        (
            s.maximum_continuous_shaft_power_kw,
            b.maximum_continuous_shaft_power_kw,
        ),
        (
            s.maximum_climb_shaft_power_kw,
            b.maximum_climb_shaft_power_kw,
        ),
        (
            s.maximum_cruise_shaft_power_kw,
            b.maximum_cruise_shaft_power_kw,
        ),
        (
            s.maximum_cruise_fuel_flow_kg_h,
            b.maximum_cruise_fuel_flow_kg_h,
        ),
    ] {
        assert!(relative(new, old * x) < 1e-12);
    }
    // Raymer Eq. (10.23): D proportional to P^(1/4).
    assert!(
        relative(
            s.propeller_diameter_m,
            b.propeller_diameter_m * x.powf(0.25)
        ) < 1e-12
    );
    // Same tip speed (n D) and the same fuel burnt per unit power.
    let tip =
        |spec: &TurbopropEngineSpec| spec.propeller_diameter_m * spec.governed_propeller_speed_rpm;
    assert!(relative(tip(s), tip(b)) < 1e-12);
    assert!(
        relative(
            s.maximum_cruise_fuel_flow_kg_h / s.maximum_cruise_shaft_power_kw,
            b.maximum_cruise_fuel_flow_kg_h / b.maximum_cruise_shaft_power_kw
        ) < 1e-12
    );
    assert_eq!(s.propeller_blade_count, b.propeller_blade_count);
}

#[test]
fn a_centre_pod_scales_with_the_nacelle_and_keeps_its_tail_end() {
    let mut base = engine("CFM56-5B4/3");
    base.centre_engine = Some(CentreEngineInstallation {
        axis_z_m: 4.0,
        axis_pitch_deg: 3.0,
        width_m: 2.0,
        height_m: 2.2,
        profile: vec![(0.0, 0.5), (1.0, 1.0), (4.0, 1.0), (6.0, 0.4)],
        nozzle_aft_of_fuselage_m: 0.0,
        struts: Vec::new(),
        fin_aft_fairing: None,
    });
    let scaled = base.scaled_by(1.2).unwrap();
    let (pod, scaled_pod) = (
        base.centre_engine.as_ref().unwrap(),
        scaled.centre_engine.as_ref().unwrap(),
    );
    assert!(relative(scaled_pod.width_m, pod.width_m * 1.2f64.sqrt()) < 1e-12);
    assert!(relative(scaled_pod.height_m, pod.height_m * 1.2f64.sqrt()) < 1e-12);
    assert!(relative(scaled_pod.length_m(), pod.length_m() * 1.2f64.powf(0.4)) < 1e-12);
    assert!(scaled_pod.is_valid());
}

#[test]
fn the_config_scale_pins_the_mass_baseline_so_engine_mass_follows_the_rating() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
    let base_thrust = config.geometry.engine.thrust_kn();
    let scaled = config.with_engine_scaled(1.1).unwrap();
    assert_eq!(
        scaled.mass_model.flops_structure.baseline_engine_thrust_kn,
        Some(base_thrust)
    );
    assert!(relative(scaled.geometry.engine.thrust_kn(), 1.1 * base_thrust) < 1e-12);
    // A declared baseline is kept (the certified mass belongs to it).
    let mut declared = config.clone();
    declared
        .mass_model
        .flops_structure
        .baseline_engine_thrust_kn = Some(100.0);
    let scaled = declared.with_engine_scaled(1.1).unwrap();
    assert_eq!(
        scaled.mass_model.flops_structure.baseline_engine_thrust_kn,
        Some(100.0)
    );
}

#[test]
fn applying_a_sized_engine_replaces_and_is_idempotent() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
    let sized = SizedEngine::of(&config.with_engine_scaled(0.93).unwrap());
    let mut target = config.clone();
    target.apply_sized_engine(&sized);
    let once = target.clone();
    target.apply_sized_engine(&sized);
    assert_eq!(target, once);
    assert!(
        (sized.catalogue_scale - 0.93 * config.geometry.engine.catalogue_scale().unwrap()).abs()
            < 1e-12
    );
    assert_eq!(SizedEngine::of(&target), sized);
}

#[test]
fn the_validity_rule_is_jenkinsons_twenty_percent() {
    assert_eq!(ENGINE_SCALE_VALIDITY_FRACTION, 0.20);
    assert!(ENGINE_SCALE_SEARCH_BRACKET.0 < 1.0 - ENGINE_SCALE_VALIDITY_FRACTION);
    assert!(ENGINE_SCALE_SEARCH_BRACKET.1 > 1.0 + ENGINE_SCALE_VALIDITY_FRACTION);
}

#[test]
fn the_catalogue_outline_scale_recognises_only_the_rubber_scaled_nacelle() {
    let base = engine("CFM56-5B4/3");
    assert!((base.catalogue_outline_scale().unwrap() - 1.0).abs() < 1e-12);
    let scaled = base.scaled_by(1.13).unwrap().scaled_by(0.97).unwrap();
    assert!((scaled.catalogue_outline_scale().unwrap() - 1.13 * 0.97).abs() < 1e-9);
    // A radius edited alone, or a station moved, is a different installation.
    let mut wider = base.clone();
    wider.radius_scale_m *= 1.01;
    assert!(wider.catalogue_outline_scale().is_none());
    let mut moved = base.clone();
    moved.nacelle_profile[2].0 += 0.05;
    assert!(moved.catalogue_outline_scale().is_none());
}

#[test]
fn a_re_rating_of_the_same_hardware_keeps_its_mass_basis_and_a_rubber_engine_scales_it() {
    // CF6-50C1 is a re-rating of the CF6-50C hardware (one EEDB test engine):
    // the mass relations read the 224.2 kN -50C rating, not the 230.4 kN one.
    let cf6 = engine("CF6-50");
    assert_eq!(cf6.thrust_kn(), 230.4);
    assert_eq!(cf6.mass_basis_thrust_kn(), Some(224.2));
    let scaled = cf6.scaled_by(1.1).unwrap();
    assert!(relative(scaled.mass_basis_thrust_kn().unwrap(), 1.1 * 224.2) < 1e-12);
    // Without a declared basis the mass relations read the rating itself.
    let cfm = engine("CFM56-5B4/3");
    assert_eq!(cfm.mass_basis_thrust_kn(), Some(cfm.thrust_kn()));
    // The config scale pins the baseline at the mass basis, so a scale
    // factor of x prices the engine at x times its unscaled mass.
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "DC-10"})).unwrap();
    let pinned = config.with_engine_scaled(1.1).unwrap();
    assert_eq!(
        pinned.mass_model.flops_structure.baseline_engine_thrust_kn,
        Some(224.2)
    );
}
