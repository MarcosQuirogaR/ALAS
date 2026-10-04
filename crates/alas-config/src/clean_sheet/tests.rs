// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::json;

use super::*;
use crate::presets::registry;
use crate::GeometryConfig;

/// A narrowbody brief no preset describes: 72.5 t, 168 seats, M 0.785 at
/// 11,278 m on a 3.96 m body.
fn narrowbody_brief() -> serde_json::Value {
    json!({
        "requirements": {
            "mtow_kg": 72_500.0, "num_passengers": 168,
            "cruise_mach": 0.785, "cruise_altitude_m": 11_277.6
        },
        "geometry": {"fuselage": {"diameter_m": 3.96}}
    })
}

fn load(value: &serde_json::Value) -> AlasConfig {
    AlasConfig::from_value(value).unwrap()
}

fn relative(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs()
}

#[test]
fn the_fleet_relations_are_measured_and_the_windows_contain_the_fleet() {
    let stats = fleet_statistics().unwrap();
    for preset in registry() {
        if preset.geometry.engine.propulsion_technology != crate::PropulsionTechnology::Turbofan {
            continue;
        }
        let wing = WingPlanformSummary::of(&preset.geometry.wing, &preset.design_vector).unwrap();
        let r = &preset.requirements;
        let q = cruise_dynamic_pressure_pa(r.cruise_mach, r.cruise_altitude_m).unwrap();
        let cl = r.mtow_kg * r.gravity_m_s2 / (q * wing.area_m2);
        let cl_ratio = cl / stats.cruise_lift_coefficient_at_mtow;
        let ar_ratio = wing.span_m.powi(2) / wing.area_m2 / stats.aspect_ratio;
        assert!(
            (LIFT_COEFFICIENT_WINDOW.0..=LIFT_COEFFICIENT_WINDOW.1).contains(&cl_ratio),
            "{}: CL ratio {cl_ratio}",
            preset.name
        );
        assert!(
            (ASPECT_RATIO_WINDOW.0..=ASPECT_RATIO_WINDOW.1).contains(&ar_ratio),
            "{}: AR ratio {ar_ratio}",
            preset.name
        );
        // At the start root chord, the chord windows reach every registered
        // taper and the sweep window every registered sweep at its Mach.
        let dv = &preset.design_vector;
        let tip_ratio = dv.tip_chord_m / dv.root_chord_m / stats.tip_root_chord_ratio;
        let break_ratio = dv.break_chord_m / dv.root_chord_m / stats.break_root_chord_ratio;
        assert!(
            (TIP_CHORD_WINDOW.0..=TIP_CHORD_WINDOW.1).contains(&tip_ratio),
            "{}: tip ratio {tip_ratio}",
            preset.name
        );
        assert!(
            (INBOARD_CHORD_WINDOW.0..=INBOARD_CHORD_WINDOW.1).contains(&break_ratio),
            "{}: break ratio {break_ratio}",
            preset.name
        );
        let sweep_miss = dv.sweep_deg - sweep_for_mach_deg(r.cruise_mach, stats.normal_mach);
        assert!(
            sweep_miss.abs() <= SWEEP_HALF_WINDOW_DEG,
            "{}: sweep miss {sweep_miss}",
            preset.name
        );
    }
    // Physical ranges, not values: a transport cruise CL, a cantilever aspect
    // ratio, a subsonic normal Mach number, tapering chords.
    assert!((0.4..0.9).contains(&stats.cruise_lift_coefficient_at_mtow));
    assert!((6.0..13.0).contains(&stats.aspect_ratio));
    assert!((0.6..0.8).contains(&stats.normal_mach));
    assert!(stats.tip_root_chord_ratio < stats.break_root_chord_ratio);
    assert!(stats.break_root_chord_ratio < 1.0);
    assert!((0.3..0.6).contains(&stats.quarter_mac_station_per_length));
}

#[test]
fn only_a_preset_less_brief_that_departs_from_the_reference_is_derived() {
    assert!(!AlasConfig::default().derives_clean_sheet_start());
    assert!(!load(&json!({})).derives_clean_sheet_start());
    assert!(load(&narrowbody_brief()).derives_clean_sheet_start());
    for preset in registry() {
        let config = load(&json!({"preset": preset.name}));
        assert!(!config.derives_clean_sheet_start(), "{}", preset.name);
        assert_eq!(config.configured_nominal_design(), preset.design_vector);
        assert_eq!(
            config.geometry,
            crate::preset_policy::reference_geometry(preset)
        );
        let envelope = config.design_envelope(&preset.design_vector);
        let unchanged = config
            .optimizer
            .design_space
            .envelope(&preset.design_vector);
        for (with, without) in envelope.iter().zip(&unchanged) {
            if with.name != "span_m" {
                assert_eq!(with, without, "{} {}", preset.name, with.name);
            }
        }
    }
    assert_eq!(
        AlasConfig::default().configured_nominal_design(),
        DesignVector::default()
    );
}

#[test]
fn the_start_closes_the_class_wing_area_at_the_fleet_lift_coefficient() {
    let config = load(&narrowbody_brief());
    let stats = fleet_statistics().unwrap();
    let design = config.clean_sheet_design().unwrap();
    let wing = WingPlanformSummary::of(&config.geometry.wing, &design).unwrap();
    let r = &config.requirements;
    let q = cruise_dynamic_pressure_pa(r.cruise_mach, r.cruise_altitude_m).unwrap();
    let expected_area = r.mtow_kg * r.gravity_m_s2 / (q * stats.cruise_lift_coefficient_at_mtow);
    assert!(relative(wing.area_m2, expected_area) < 1e-6);
    assert!(relative(design.span_m.powi(2) / wing.area_m2, stats.aspect_ratio) < 1e-6);
    assert!(
        relative(
            design.break_chord_m / design.root_chord_m,
            stats.break_root_chord_ratio
        ) < 1e-12
    );
    assert!(
        relative(
            design.tip_chord_m / design.root_chord_m,
            stats.tip_root_chord_ratio
        ) < 1e-12
    );
    assert!(
        relative(
            r.cruise_mach * design.sweep_deg.to_radians().cos(),
            stats.normal_mach
        ) < 1e-12
    );
    // The wing sits at the fleet quarter-MAC station of its own fuselage.
    let station =
        config.geometry.wing.root_datum_x_m + design.wing_x_shift_m + wing.quarter_mac_x_m;
    assert!(
        relative(
            station / design.fuselage_length_m,
            stats.quarter_mac_station_per_length
        ) < 1e-9
    );
}

#[test]
fn the_wing_area_scales_with_weight_and_inversely_with_dynamic_pressure() {
    let base = load(&narrowbody_brief()).class_wing().unwrap();
    let mut heavier = narrowbody_brief();
    heavier["requirements"]["mtow_kg"] = json!(145_000.0);
    let heavier = load(&heavier).class_wing().unwrap();
    assert!(relative(heavier.area_m2, 2.0 * base.area_m2) < 1e-9);
    let mut lower = narrowbody_brief();
    lower["requirements"]["cruise_altitude_m"] = json!(9_000.0);
    let lower = load(&lower).class_wing().unwrap();
    assert!(lower.area_m2 < base.area_m2);
}

#[test]
fn simple_sweep_theory_leaves_a_slow_wing_straight_and_sweeps_a_fast_one_more() {
    assert_eq!(sweep_for_mach_deg(0.45, 0.7), 0.0);
    assert_eq!(sweep_for_mach_deg(0.7, 0.7), 0.0);
    let (a, b) = (sweep_for_mach_deg(0.78, 0.7), sweep_for_mach_deg(0.85, 0.7));
    assert!(0.0 < a && a < b && b <= 45.0);
    assert!(sweep_for_mach_deg(5.0, 0.7) <= 45.0);
}

#[test]
fn an_aerodrome_code_holds_the_start_and_the_box_under_its_span_limit() {
    let mut value = narrowbody_brief();
    value["requirements"]["mtow_kg"] = json!(110_000.0);
    value["optimizer"] = json!({"objective": {"aerodrome_reference_code": "C"}});
    let config = load(&value);
    let limit = config.max_design_span_m().unwrap();
    let design = config.configured_nominal_design();
    assert!(design.span_m <= limit);
    let span = config.design_envelope(&design).into_iter().next().unwrap();
    assert!(span.upper <= limit && span.lower <= design.span_m);
}

#[test]
fn the_derived_box_contains_the_start_and_keeps_the_cabin_sized_fuselage_fixed() {
    let config = load(&narrowbody_brief());
    let design = config.configured_nominal_design();
    let envelope = config.design_envelope(&design);
    for variable in &envelope {
        assert!(variable.lower <= variable.nominal && variable.nominal <= variable.upper);
        assert!(variable.lower.is_finite() && variable.upper.is_finite());
    }
    let span = &envelope[0];
    assert!(
        span.upper < 60.0,
        "a narrowbody box is not the widebody box"
    );
    let fuselage = envelope
        .iter()
        .find(|v| v.name == "fuselage_length_m")
        .unwrap();
    assert!(fuselage.fixed);
    let (lower, upper) = config.fuselage_sizing_interval_m();
    let f = &config.geometry.fuselage;
    assert!(
        relative(
            lower,
            (f.cabin_start_x_m + f.tailcone_length_m + f.diameter_m).max(20.0)
        ) < 1e-12
    );
    assert!(lower < design.fuselage_length_m && design.fuselage_length_m < upper);
}

#[test]
fn explicit_start_and_bounds_win_and_are_validated() {
    let mut value = narrowbody_brief();
    value["optimizer"] = json!({"design_space": {
        "initial_design": {"span_m": 35.8, "sweep_deg": 25.0},
        "bounds": {"span_m": [33.0, 35.9], "root_chord_m": [5.0, 8.0], "fuselage_length_m": [30.0, 50.0]}
    }});
    let config = load(&value);
    assert!(config.optimizer.design_space.validate().is_ok());
    let design = config.configured_nominal_design();
    assert_eq!((design.span_m, design.sweep_deg), (35.8, 25.0));
    let envelope = config.design_envelope(&design);
    assert_eq!((envelope[0].lower, envelope[0].upper), (33.0, 35.9));
    assert_eq!((envelope[1].lower, envelope[1].upper), (5.0, 8.0));
    assert_eq!(config.fuselage_sizing_interval_m(), (30.0, 50.0));

    for bad in [
        json!({"initial_design": {"span": 30.0}}),
        json!({"bounds": {"span_m": [40.0, 30.0]}}),
        json!({"initial_design": {"span_m": 50.0}, "bounds": {"span_m": [30.0, 40.0]}}),
    ] {
        let mut value = narrowbody_brief();
        value["optimizer"] = json!({"design_space": bad});
        assert!(
            load(&value).optimizer.design_space.validate().is_err(),
            "{bad}"
        );
    }
}

#[test]
fn dependent_geometry_follows_the_diameter_and_explicit_keys_win() {
    let stats = fleet_statistics().unwrap();
    let config = load(&narrowbody_brief());
    let f = &config.geometry.fuselage;
    assert!(relative(f.cabin_start_x_m, stats.nose_length_per_diameter * 3.96) < 1e-12);
    assert!(
        relative(
            f.tailcone_length_m,
            stats.tailcone_length_per_diameter * 3.96
        ) < 1e-12
    );
    let semispan = 0.5 * config.class_wing().unwrap().span_m;
    let side_of_body = config.geometry.wing.side_of_body_span_fraction.unwrap();
    assert!(relative(side_of_body * semispan, 0.5 * 3.96) < 1e-12);
    let engines = &config.geometry.engine.spanwise_positions_m;
    assert!(relative(engines[0], stats.engine_semispan_fraction * semispan) < 1e-12);
    assert_eq!(engines[0], -engines[1]);
    assert!(
        config.geometry.empennage.hstab_root_chord_m
            < GeometryConfig::default().empennage.hstab_root_chord_m
    );

    let mut value = narrowbody_brief();
    value["geometry"]["fuselage"]["cabin_start_x_m"] = json!(5.0);
    value["geometry"]["empennage"] = json!({"hstab_root_chord_m": 4.0});
    let explicit = load(&value);
    assert_eq!(explicit.geometry.fuselage.cabin_start_x_m, 5.0);
    assert_eq!(explicit.geometry.empennage.hstab_root_chord_m, 4.0);
    assert_eq!(
        explicit.geometry.fuselage.tailcone_length_m,
        f.tailcone_length_m
    );
}

#[test]
fn the_reference_empennage_is_its_own_volume_scaled_image() {
    let mut geometry = GeometryConfig::default();
    let wing = WingPlanformSummary::of(&geometry.wing, &DesignVector::default()).unwrap();
    geometry::scale_empennage(
        &mut geometry,
        &json!({}),
        wing,
        DesignVector::default().fuselage_length_m,
    );
    let (scaled, reference) = (&geometry.empennage, &GeometryConfig::default().empennage);
    assert!(relative(scaled.hstab_root_chord_m, reference.hstab_root_chord_m) < 1e-12);
    assert!(relative(scaled.vstab_tip_le_m.2, reference.vstab_tip_le_m.2) < 1e-12);
}

#[test]
fn a_preset_less_engine_name_selects_its_catalogue_entry() {
    let mut value = narrowbody_brief();
    value["geometry"]["engine"] = json!({"engine_name": "LEAP-1A"});
    let config = load(&value);
    let catalogue = crate::engines::get("LEAP-1A").unwrap();
    assert_eq!(
        config.geometry.engine.radius_scale_m,
        catalogue.nacelle_max_radius_m
    );
    assert_eq!(config.geometry.engine.bypass_ratio, catalogue.bypass_ratio);
    // A cycle field the file sets still wins over the catalogue.
    value["geometry"]["engine"]["bypass_ratio"] = json!(11.0);
    assert_eq!(load(&value).geometry.engine.bypass_ratio, 11.0);
}

#[test]
fn an_unphysical_brief_derives_nothing_rather_than_a_non_finite_start() {
    for (key, bad) in [
        ("mtow_kg", -1.0),
        ("cruise_mach", 0.0),
        ("cruise_altitude_m", 1.0e6),
    ] {
        let mut value = narrowbody_brief();
        value["requirements"][key] = json!(bad);
        let config = load(&value);
        assert!(config.clean_sheet_design().is_none(), "{key}");
        let design = config.configured_nominal_design();
        assert!(design.to_array().iter().all(|v| v.is_finite()), "{key}");
    }
}

#[test]
fn the_clean_sheet_mark_and_auto_code_survive_a_save_and_reload() {
    let config = load(&narrowbody_brief());
    assert_eq!(config.optimizer.design_space.clean_sheet_brief, Some(true));
    assert_eq!(
        config.optimizer.objective.aerodrome_reference_code,
        AerodromeReferenceCode::Auto
    );
    assert_eq!(config.aerodrome_reference_code(), AerodromeReferenceCode::C);
    let saved = serde_json::to_value(&config).unwrap();
    assert_eq!(
        saved["optimizer"]["design_space"]["clean_sheet_brief"],
        json!(true)
    );
    assert_eq!(load(&saved), config);
}

#[test]
fn an_explicit_false_mark_and_other_design_modes_derive_nothing() {
    let reference = load(&json!({}));
    for optimizer in [
        json!({"design_space": {"clean_sheet_brief": false}}),
        json!({"design_space": {"mode": "reference_adaptation"}}),
        json!({"design_space": {"mode": "baseline_sandbox"}}),
    ] {
        let mut value = narrowbody_brief();
        value["optimizer"] = optimizer.clone();
        let config = load(&value);
        assert!(!config.derives_clean_sheet_start(), "{optimizer}");
        assert_eq!(config.geometry.wing, reference.geometry.wing, "{optimizer}");
        assert!(config.clean_sheet_design().is_none(), "{optimizer}");
    }
    let kept = load(&json!({"optimizer": {"design_space": {"clean_sheet_brief": false}}}));
    let saved = serde_json::to_value(&kept).unwrap();
    assert_eq!(
        saved["optimizer"]["design_space"]["clean_sheet_brief"],
        json!(false)
    );
}

#[test]
fn an_explicit_letter_is_kept_and_auto_follows_the_brief() {
    let mut value = narrowbody_brief();
    value["optimizer"] = json!({"objective": {"aerodrome_reference_code": "F"}});
    assert_eq!(
        load(&value).aerodrome_reference_code(),
        AerodromeReferenceCode::F
    );
    // A heavier brief implies a longer span, and Auto follows it.
    let mut heavier = narrowbody_brief();
    heavier["requirements"]["mtow_kg"] = json!(200_000.0);
    assert_ne!(
        load(&heavier).aerodrome_reference_code(),
        AerodromeReferenceCode::C
    );
}

#[test]
fn explicit_bounds_outside_the_guardrails_are_rejected() {
    let mut value = narrowbody_brief();
    value["optimizer"] = json!({"design_space": {"bounds": {"span_m": [10.0, 95.0]}}});
    assert!(load(&value).optimizer.design_space.validate().is_err());
}

#[test]
fn the_derived_box_respects_the_guardrails_and_contains_the_start() {
    let config = load(&narrowbody_brief());
    let design = config.configured_nominal_design();
    for (variable, spec) in config.design_envelope(&design).iter().zip(SPECS) {
        assert!(variable.lower <= variable.nominal && variable.nominal <= variable.upper);
        let inside = variable.lower >= spec.preset_lower - 1e-9
            && variable.upper <= spec.preset_upper + 1e-9;
        let start_outside =
            variable.nominal < spec.preset_lower || variable.nominal > spec.preset_upper;
        assert!(inside || start_outside, "{}", spec.name);
    }
}

#[test]
fn a_reference_brief_does_not_bind_a_named_engine() {
    let config = load(&json!({"geometry": {"engine": {"engine_name": "LEAP-1A"}}}));
    assert!(!config.derives_clean_sheet_start());
    assert_eq!(
        config.geometry.engine.radius_scale_m,
        GeometryConfig::default().engine.radius_scale_m
    );
}
