// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares the loading path against the reference implementation's: what
//! configuration a saved file turns into, and which files are refused.
//!
//! `parity_config.rs` already checks what a freshly constructed aggregate
//! holds. What is checked here is the step that turns a file into a run,
//! applying a named preset, then laying the file's own keys over it, and it
//! is checked by comparing the whole resulting configuration rather than the
//! fields each file names. A port that applied the two in the other order, or
//! that skipped a preset's own mass-model and high-lift calibrations because
//! the graphical front end also applies them, would agree on every field the
//! file mentioned and disagree on the ones that decide the aircraft's weight
//! and its takeoff speeds.
//!
//! Independently sourced aircraft corrections are deliberately different from
//! the frozen Python fixture. Each such leaf is pinned on both sides below;
//! every other loaded value remains an exact parity comparison. The active
//! transport planform is also a two-sided correction: the frozen files have
//! no side-of-body/kink fields, while native product loading sets their
//! explicitly audited defaults.
//! `optimize_passenger_capacity` is a native load-case switch with no Python
//! field; requirements and preset unit tests pin it instead.
//! Native field-method inputs are covered by sourced physical tests. Published
//! landing inputs are checked against their source records after file loading,
//! while the frozen landing coefficients and speed factors remain checked here.
//!
//! Compared at `exact`: nothing on this path computes anything. A value is
//! copied from a default, from a preset, or from the file.
//!
//! The rejections are compared as rejections and not as messages. Upstream
//! raises a `KeyError` carrying a sentence it composed; here the deserializer
//! composes its own, and reproducing the exact wording of a Python exception
//! would be reproducing a string rather than a behaviour. What has to agree is
//! that the same files are refused and that the message names the key that
//! caused it, which is what sends the author to the right line.

// This file is itself a test binary, so an unwrap that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::BTreeMap;

use alas_config::{AlasConfig, AnalysisConfig, StructuresConfig};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use serde_json::Value;

#[path = "support/product_corrections.rs"]
mod product_corrections;

struct SourceCorrection {
    upstream: Value,
    corrected: Value,
}

/// One saved-file case: what it holds, and either the configuration it
/// produces or the message it was rejected with.
#[derive(Deserialize)]
struct Case {
    name: String,
    input: Value,
    result: Option<Value>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
    from_preset: BTreeMap<String, Value>,
}

fn fixture() -> Fixture {
    alas_testkit::load("config", "settings")
}

#[test]
fn every_saved_file_loads_into_the_configuration_the_reference_produces() {
    let fixture = fixture();
    let mut comparison = Comparison::new("alas-config::settings", Tier::Exact);
    let mut corrections = saved_file_source_corrections();

    for case in &fixture.cases {
        match (AlasConfig::from_value(&case.input), &case.result) {
            (Ok(config), Some(expected)) => {
                compare_values(
                    &mut comparison,
                    &mut corrections,
                    &case.name,
                    &as_value(&config),
                    expected,
                );
            }
            (Ok(_), None) => {
                comparison.exact(&format!("{}: accepted", case.name), &true, &false);
            }
            (Err(error), Some(_)) => {
                comparison.exact(
                    &format!("{}: rejected with {error}", case.name),
                    &false,
                    &true,
                );
            }
            (Err(error), None) => {
                // Both refuse it. What has to agree beyond that is that the
                // message names the key responsible, since that is what the
                // author of the file has to be sent to.
                let key = offending_key(case.error.as_deref().unwrap_or_default());
                comparison.exact(
                    &format!("{}: the message names `{key}`", case.name),
                    &format!("{error}").contains(&key),
                    &true,
                );
            }
        }
    }
    comparison.exact(
        "unvisited saved-file source corrections",
        &corrections.keys().cloned().collect::<Vec<_>>(),
        &Vec::<String>::new(),
    );
    comparison.finish();
}

#[test]
fn every_aircraft_preset_survives_the_loading_path() {
    let fixture = fixture();
    let mut comparison = Comparison::new("alas-config::settings preset loading", Tier::Exact);
    let mut corrections = preset_source_corrections();

    for (name, expected) in &fixture.from_preset {
        let loaded = AlasConfig::from_value(&serde_json::json!({"preset": name}));
        match loaded {
            Ok(config) => compare_values(
                &mut comparison,
                &mut corrections,
                name,
                &as_value(&config),
                expected,
            ),
            Err(error) => {
                comparison.exact(&format!("{name}: {error}"), &false, &true);
            }
        }
    }
    comparison.exact(
        "preset count",
        &alas_config::presets::available()
            .iter()
            .filter(|name| fixture.from_preset.contains_key(**name))
            .count(),
        &fixture.from_preset.len(),
    );
    comparison.exact(
        "unvisited preset source corrections",
        &corrections.keys().cloned().collect::<Vec<_>>(),
        &Vec::<String>::new(),
    );
    comparison.finish();
}

fn as_value(config: &AlasConfig) -> Value {
    serde_json::to_value(config).expect("a configuration serializes")
}

/// The key named between single quotes in the reference's rejection message.
///
/// Its wording is Python's; what is portable about it is the identifier it
/// quotes, and that is what both implementations have to point at.
fn offending_key(message: &str) -> String {
    message
        .split('\'')
        .nth(1)
        .unwrap_or("<no quoted key>")
        .to_owned()
}

/// Compare two trees, reporting each disagreeing key by its path rather than
/// dumping both. Numbers are compared by value, since an integer upstream and
/// a float here denote the same quantity.
fn compare_values(
    comparison: &mut Comparison,
    corrections: &mut BTreeMap<String, SourceCorrection>,
    path: &str,
    actual: &Value,
    expected: &Value,
) {
    if let Some((frozen, sourced)) = product_corrections::published_landing_change(path) {
        compare_correction_value(
            comparison,
            &format!("{path}: frozen landing input"),
            expected,
            &frozen,
        );
        compare_correction_value(
            comparison,
            &format!("{path}: published landing input"),
            actual,
            &sourced,
        );
        return;
    }
    if let Some((frozen, declared)) = declared_control_surface_run(path) {
        compare_correction_value(
            comparison,
            &format!("{path}: frozen class run"),
            expected,
            &frozen,
        );
        compare_correction_value(
            comparison,
            &format!("{path}: declared control-surface run"),
            actual,
            &declared,
        );
        return;
    }
    if let Some((old, new)) = product_corrections::dimensions(path) {
        compare_correction_value(
            comparison,
            &format!("{path}: frozen dimension"),
            expected,
            &old,
        );
        compare_correction_value(
            comparison,
            &format!("{path}: published dimension"),
            actual,
            &new,
        );
        return;
    }
    if let Some((frozen, product)) = product_corrections::cargo_deck_uld(path) {
        corrections.remove(path);
        compare_correction_value(
            comparison,
            &format!("{path}: frozen cargo deck ULD"),
            expected,
            &frozen,
        );
        compare_correction_value(
            comparison,
            &format!("{path}: product cargo deck ULD"),
            actual,
            &product,
        );
        return;
    }
    if let Some(new) = product_corrections::engine_copy(path)
        .or_else(|| product_corrections::operational(path))
        .or_else(|| product_corrections::design_dive_speed(path))
    {
        corrections.remove(path);
        compare_correction_value(
            comparison,
            &format!("{path}: product binding"),
            actual,
            &new,
        );
        return;
    }
    // The widebody default profile's configuration heights, only where a
    // file inherits the frozen default (a preset schedule sets its own).
    if let Some((upstream, corrected)) =
        mission_profile_default_correction(path).filter(|(upstream, _)| {
            expected.as_f64().is_some() && expected.as_f64() == upstream.as_f64()
        })
    {
        compare_correction_value(
            comparison,
            &format!("{path}: frozen Python value"),
            expected,
            &upstream,
        );
        compare_correction_value(
            comparison,
            &format!("{path}: source-corrected Rust value"),
            actual,
            &corrected,
        );
        return;
    }
    if let Some((upstream, corrected)) = vibration_performance_default_correction(path) {
        compare_correction_value(
            comparison,
            &format!("{path}: frozen Python value"),
            expected,
            &upstream,
        );
        compare_correction_value(
            comparison,
            &format!("{path}: source-corrected Rust value"),
            actual,
            &corrected,
        );
        return;
    }
    if let Some(correction) = corrections.remove(path) {
        compare_correction_value(
            comparison,
            &format!("{path}: frozen Python value"),
            expected,
            &correction.upstream,
        );
        compare_correction_value(
            comparison,
            &format!("{path}: source-corrected Rust value"),
            actual,
            &correction.corrected,
        );
        return;
    }
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            for (key, expected_value) in expected {
                if product_corrections::retired_reference_key(path, key) {
                    continue;
                }
                // Historical Python fixtures include runtime paths that the
                // native mission deliberately no longer serializes.
                if (path.ends_with("MissionConfig") || path.ends_with(".mission"))
                    && (key == "suave_venv_dir" || key == "suave_runner_dir")
                {
                    continue;
                }
                let child = format!("{path}.{key}");
                match actual.get(key) {
                    Some(actual_value) => {
                        compare_values(
                            comparison,
                            corrections,
                            &child,
                            actual_value,
                            expected_value,
                        );
                    }
                    None if is_retired_solver_field(path, key) => {}
                    None => {
                        comparison.exact(&child, &Value::Null, expected_value);
                    }
                }
            }
            for key in actual.keys() {
                if product_corrections::native_field(path, key) {
                    continue;
                }
                if (path.ends_with("MissionConfig") || path.ends_with(".mission"))
                    && (key == "suave_venv_dir" || key == "suave_runner_dir")
                {
                    continue;
                }
                if key == "optimize_passenger_capacity" && path.ends_with(".requirements") {
                    continue;
                }
                if !expected.contains_key(key) {
                    let child = format!("{path}.{key}");
                    if let Some(new) = product_corrections::engine_copy(&child)
                        .or_else(|| product_corrections::added_planform(&child))
                        .or_else(|| product_corrections::declared_airfoil_class(&child))
                        .or_else(|| product_corrections::declared_lower_holds(&child))
                    {
                        corrections.remove(&child);
                        compare_correction_value(comparison, &child, &actual[key], &new);
                        continue;
                    }
                    if path.ends_with(".geometry.engine")
                        && matches!(
                            key.as_str(),
                            "part_power_fuel_flow_ratios" | "part_power_source"
                        )
                    {
                        let default =
                            serde_json::to_value(alas_config::EngineConfig::default()).unwrap();
                        compare_correction_value(comparison, &child, &actual[key], &default[key]);
                        continue;
                    }
                    if let Some(correction) = corrections.remove(child.as_str()) {
                        compare_correction_value(
                            comparison,
                            &format!("{child}: frozen Python value"),
                            &Value::String("absent upstream".to_owned()),
                            &correction.upstream,
                        );
                        compare_correction_value(
                            comparison,
                            &format!("{child}: source-corrected Rust value"),
                            actual.get(key).unwrap_or(&Value::Null),
                            &correction.corrected,
                        );
                    } else {
                        comparison.exact(
                            &child,
                            &"present".to_owned(),
                            &"absent upstream".to_owned(),
                        );
                    }
                }
            }
        }
        (Value::Array(actual), Value::Array(expected)) => {
            if actual.len() != expected.len() {
                comparison.exact(&format!("{path}.len"), &actual.len(), &expected.len());
                return;
            }
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                compare_values(
                    comparison,
                    corrections,
                    &format!("{path}[{index}]"),
                    actual,
                    expected,
                );
            }
        }
        (Value::Number(actual), Value::Number(expected)) => {
            comparison.exact(path, &actual.as_f64(), &expected.as_f64());
        }
        (actual, expected) => {
            comparison.exact(path, actual, expected);
        }
    }
}

/// See the matching corrections in `parity_config`: force-PSD remains the
/// default, the old extended 500 Hz sweep no longer is, and the vortex-lattice
/// mesh no longer meshes a cambered section as a flat plate.
///
/// Every loaded preset inherits the analysis defaults, so the three mesh
/// fields appear here once per registered aircraft. Both sides stay pinned,
/// exactly as they are for a directly constructed configuration.
///
/// Each tail surface is meshed with 24 spanwise panels instead of 6:
/// on the conventional-tail presets Cn_beta falls 8-9 % and
/// Cy_beta 5-6 % from 6 to 48 panels, and 24 is within 3 % of 96 (A320
/// Cn_beta 0.1976 at 6, 0.1830 at 24, 0.1789 at 96 per rad). The
/// reference-compatibility mesh keeps 6.
fn vibration_performance_default_correction(path: &str) -> Option<(Value, Value)> {
    if path.ends_with(".structures.freq_sweep_max_hz") {
        Some((serde_json::json!(500.0), serde_json::json!(60.0)))
    } else if path.ends_with(".structures.n_modes") {
        Some((serde_json::json!(30), serde_json::json!(16)))
    } else if path.ends_with(".analysis.chordwise_resolution") {
        Some((serde_json::json!(1), serde_json::json!(8)))
    } else if path.ends_with(".analysis.fine_chordwise_resolution") {
        Some((serde_json::json!(8), serde_json::json!(16)))
    } else if path.ends_with(".analysis.fine_spanwise_resolution") {
        Some((serde_json::json!(2), serde_json::json!(1)))
    } else if path.ends_with(".geometry.wing.n_subdivisions") {
        Some((serde_json::json!(8), serde_json::json!(24)))
    } else if path.ends_with(".geometry.empennage.n_subdivisions") {
        Some((serde_json::json!(6), serde_json::json!(24)))
    } else {
        None
    }
}

/// The default profile ends the takeoff configuration at 1,500 ft (ICAO
/// PANS-OPS noise-abatement departure) instead of 10,000 ft and starts the
/// landing configuration at 3,000 ft (glide-path interception) instead of
/// 6,500 ft.
fn mission_profile_default_correction(path: &str) -> Option<(Value, Value)> {
    if path.ends_with(".mission.profile.takeoff_altitude_gain_m") {
        Some((
            serde_json::json!(3048.0),
            serde_json::json!(1_500.0 * 0.3048),
        ))
    } else if path.ends_with(".mission.profile.descent_4_altitude_ft") {
        Some((serde_json::json!(6500.0), serde_json::json!(3000.0)))
    } else if path.ends_with(".mission.profile.descent_1_rate_m_s") {
        Some((
            serde_json::json!(4.5),
            serde_json::json!(1_850.0 * (0.3048 / 60.0)),
        ))
    } else if path.ends_with(".mission.profile.descent_2_rate_m_s") {
        Some((
            serde_json::json!(5.0),
            serde_json::json!(1_700.0 * (0.3048 / 60.0)),
        ))
    } else if path.ends_with(".mission.profile.descent_3_rate_m_s") {
        Some((
            serde_json::json!(5.0),
            serde_json::json!(1_600.0 * (0.3048 / 60.0)),
        ))
    } else if path.ends_with(".mission.profile.descent_4_rate_m_s") {
        Some((
            serde_json::json!(5.0),
            serde_json::json!(1_450.0 * (0.3048 / 60.0)),
        ))
    } else {
        None
    }
}

fn compare_correction_value(
    comparison: &mut Comparison,
    path: &str,
    actual: &Value,
    expected: &Value,
) {
    match (actual, expected) {
        (Value::Number(actual), Value::Number(expected)) => {
            comparison.exact(path, &actual.as_f64(), &expected.as_f64());
        }
        _ => {
            comparison.exact(path, actual, expected);
        }
    }
}

fn correction(
    path: impl Into<String>,
    upstream: impl Into<Value>,
    corrected: impl Into<Value>,
) -> (String, SourceCorrection) {
    (
        path.into(),
        SourceCorrection {
            upstream: upstream.into(),
            corrected: corrected.into(),
        },
    )
}

fn preset_source_corrections() -> BTreeMap<String, SourceCorrection> {
    let mut corrections = [
        correction("A220-300.requirements.cabin_preset", "Ryanair", "Custom"),
        // The registered cabin seeds are the manufacturers' typical-cabin
        // seat counts, the basis of each reference operating-empty mass; see
        // `AircraftReferenceData::planning_seats`.
        correction("A220-300.requirements.num_passengers", 130, 140),
        correction("A340-300.requirements.num_passengers", 290, 335),
        correction("A380-800.requirements.num_passengers", 525, 555),
        correction("DC-10.requirements.num_passengers", 250, 255),
        // Wing-box material families assigned per preset from the airport
        // planning documents (`alas_config::preset_structures`); the frozen
        // files carried the database default for every type.
        correction("A220-300.structures.skin_material", "Al 7075-T6", "CFRP QI"),
        correction(
            "A220-300.structures.spar_cap_material",
            "CFRP UD",
            "CFRP 60/30/10",
        ),
        correction(
            "A220-300.structures.spar_web_material",
            "Al 7075-T6",
            "CFRP QI",
        ),
        correction(
            "A320-200.structures.spar_cap_material",
            "CFRP UD",
            "Al 7075-T6",
        ),
        correction(
            "A340-300.structures.spar_cap_material",
            "CFRP UD",
            "Al 7075-T6",
        ),
        correction(
            "A380-800.structures.spar_cap_material",
            "CFRP UD",
            "Al 7075-T6",
        ),
        correction("B787-9.structures.skin_material", "Al 7075-T6", "CFRP QI"),
        correction(
            "B787-9.structures.spar_cap_material",
            "CFRP UD",
            "CFRP 60/30/10",
        ),
        correction(
            "B787-9.structures.spar_web_material",
            "Al 7075-T6",
            "CFRP QI",
        ),
        correction(
            "DC-10.structures.spar_cap_material",
            "CFRP UD",
            "Al 7075-T6",
        ),
        // The A320's economy seat pitch follows its cabin source (28 in).
        correction("A320-200.cabin.passenger.economy.pitch_m", 0.79, 0.7112),
        correction("A320-200.requirements.cabin_preset", "Ryanair", "Custom"),
        correction("A340-300.requirements.cabin_preset", "Ryanair", "Custom"),
        correction("A380-800.requirements.cabin_preset", "Ryanair", "Custom"),
        correction("B787-9.requirements.cabin_preset", "Ryanair", "Custom"),
        correction("DC-10.requirements.cabin_preset", "Ryanair", "Custom"),
        correction("A220-300.landing_gear.n_mlg_struts", 0, 2),
        correction("A220-300.landing_gear.n_nlg_wheels", 0, 2),
        correction(
            "A220-300.landing_gear.track_diameter_factor",
            1.85,
            6.731 / 3.50,
        ),
        correction("A220-300.landing_gear.wheels_per_mlg_strut", 0, 2),
        correction(
            "A220-300.requirements.max_structural_payload_kg",
            18_700.0,
            18_643.0,
        ),
        correction("A220-300.requirements.mtow_kg", 70_900.0, 67_585.0),
        correction(
            "A320-200.geometry.engine.engine_name",
            "LEAP-1A",
            "CFM56-5B4/3",
        ),
        correction("A320-200.landing_gear.n_mlg_struts", 0, 2),
        correction("A320-200.landing_gear.n_nlg_wheels", 0, 2),
        correction(
            "A320-200.landing_gear.track_diameter_factor",
            1.85,
            7.59 / 3.95,
        ),
        correction("A320-200.landing_gear.wheels_per_mlg_strut", 0, 2),
        correction(
            "A340-300.geometry.engine.engine_name",
            "CFM56-5C",
            "CFM56-5C3/F",
        ),
        correction("A340-300.landing_gear.n_mlg_struts", 0, 3),
        correction("A340-300.landing_gear.n_nlg_wheels", 0, 2),
        correction(
            "A340-300.landing_gear.track_diameter_factor",
            1.85,
            10.684 / 5.64,
        ),
        correction("A340-300.requirements.mtow_kg", 275_000.0, 260_000.0),
        correction(
            "A380-800.geometry.engine.engine_name",
            "Trent 900",
            "Trent 970-84",
        ),
        correction("A380-800.landing_gear.n_mlg_struts", 0, 4),
        correction("A380-800.landing_gear.n_nlg_wheels", 0, 2),
        correction(
            "A380-800.landing_gear.track_diameter_factor",
            1.85,
            14.34 / 7.14,
        ),
        correction("A380-800.requirements.max_wing_area_m2", 855.0, 845.0),
        // NASA Common Research Model camber distribution; see the matching
        // entries in `parity_aircraft_presets`.
        correction("A380-800.geometry.wing.root_airfoil", "SC2-0714", "sc20414"),
        correction("A380-800.geometry.wing.tip_airfoil", "sc20410", "sc20610"),
        correction("B787-9.landing_gear.n_mlg_struts", 0, 2),
        correction("B787-9.landing_gear.n_nlg_wheels", 0, 2),
        // D6-58333 Rev Q section 2.2.2: 9.80 m track over the 18 ft 11 in
        // (5.77 m) body width.
        correction(
            "B787-9.landing_gear.track_diameter_factor",
            1.85,
            9.8 / 5.77,
        ),
        correction("B787-9.landing_gear.wheels_per_mlg_strut", 0, 4),
        correction(
            "B787-9.requirements.max_structural_payload_kg",
            52_600.0,
            52_586.0,
        ),
        correction("B787-9.requirements.mtow_kg", 254_000.0, 254_692.0),
        correction("DC-10.landing_gear.n_mlg_struts", 0, 3),
        correction("DC-10.landing_gear.n_nlg_wheels", 0, 2),
        correction("DC-10.landing_gear.track_diameter_factor", 1.85, 1.77),
        correction(
            "DC-10.requirements.max_structural_payload_kg",
            48_000.0,
            46_008.0,
        ),
        correction("DC-10.requirements.max_wing_area_m2", 338.8, 338.84),
        correction("DC-10.requirements.mtow_kg", 259_450.0, 259_454.0),
        // A220-300.requirements.dive_speed_m_s is deliberately not pinned: the
        // preset's 212.208 m/s is VD = 1.25 x VMO 330 KIAS (EASA
        // TCDS IM.A.570 Issue 25, Section 2 item 10), a model convention, since
        // VD is not published. The check stays failing.
        //
        // A220 ACP Issue 013 Tables 6 and 7 (ACN at the most forward CG):
        // 13,923 lb on the nose gear at 157,000 lb gross.
        correction(
            "A220-300.landing_gear.nose_gear_max_static_load_kg",
            "absent upstream",
            13_923.0 * 0.453_592_37,
        ),
    ]
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    add_transport_planform_corrections(
        &mut corrections,
        &[
            "A220-300", "A320-200", "A340-300", "A380-800", "AVE", "B787-9", "DC-10",
        ],
    );
    // DAC-67803A Rev A Figure 2.2: fuselage half width 3.01 m over the
    // 25.195 m semispan; the class 0.10 sat inside the body.
    corrections.insert(
        "DC-10.geometry.wing.side_of_body_span_fraction".to_owned(),
        SourceCorrection {
            upstream: Value::String("absent upstream".to_owned()),
            corrected: Value::from(0.1195),
        },
    );
    add_declared_flap_setting_corrections(
        &mut corrections,
        &["A320-200", "A340-300", "B787-9", "DC-10"],
    );
    add_planning_cabin_corrections(&mut corrections, &["A220-300", "A320-200", "A340-300"]);
    add_b787_9_typical_cabin_corrections(&mut corrections, &["B787-9"]);
    add_ave_777_9_cabin_corrections(&mut corrections);
    // A340-200/-300 AC 2-2-0 plan view: trailing-edge kink at 9.25 m of the
    // 28.95 m semispan. A220 ACP Issue 013 Figure 1 plan view:
    // drawn trailing-edge corner at y 6.32 m. DAC-67803A Rev A
    // Figure 2.2: Yehudi trailing-edge kink at y 9.55 m.
    for (case, kink_fraction) in [
        ("A340-300", 9.25 / 28.95),
        ("A380-800", 0.359_236_516_064_625_5),
        ("AVE", 0.35),
        ("B787-9", 0.3696),
        ("A320-200", 0.377_380_002_280_241_9),
        ("A220-300", 0.36),
        ("DC-10", 0.379),
    ] {
        corrections.insert(
            format!("{case}.geometry.wing.kink_span_fraction"),
            SourceCorrection {
                upstream: Value::String("absent upstream".to_owned()),
                corrected: Value::from(kink_fraction),
            },
        );
    }
    add_transport_objective_weight_corrections(
        &mut corrections,
        &[
            "A220-300", "A320-200", "A340-300", "A380-800", "AVE", "B787-9", "DC-10",
        ],
    );
    add_optimizer_method_corrections(
        &mut corrections,
        &[
            "A220-300", "A320-200", "A340-300", "A380-800", "AVE", "B787-9", "DC-10",
        ],
    );
    add_lower_cover_fatigue_corrections(
        &mut corrections,
        &[
            ("A320-200", 75.0e6),
            ("A340-300", 110.0e6),
            ("A380-800", 110.0e6),
            ("DC-10", 110.0e6),
        ],
    );
    add_random_force_psd_corrections(
        &mut corrections,
        &[
            "A220-300", "A320-200", "A340-300", "A380-800", "AVE", "B787-9", "DC-10",
        ],
    );
    add_native_worker_corrections(
        &mut corrections,
        &[
            "A220-300", "A320-200", "A340-300", "A380-800", "AVE", "B787-9", "DC-10",
        ],
    );
    add_native_solver_limit_corrections(
        &mut corrections,
        &[
            "A220-300", "A320-200", "A340-300", "A380-800", "AVE", "B787-9", "DC-10",
        ],
    );
    add_certified_landing_mass_ratio_corrections(&mut corrections);
    add_default_landing_ratio_corrections(&mut corrections, &["AVE"]);
    corrections
}

/// A registered aircraft now carries its own certified landing-to-takeoff
/// mass ratio instead of the frozen study fraction.
///
/// FLOPS equation 63 sizes the main gear on `WLDG^0.95`, so the generic 0.92
/// put 515 t of landing weight into the A380-800's gear equation against its
/// certified 386 t. Both masses are already in the registry with their
/// airport-planning and type-certificate provenance, so the ratio is read
/// from them. AVE declares no certified pair and keeps the default ratio; see
/// [`add_default_landing_ratio_corrections`].
fn add_certified_landing_mass_ratio_corrections(
    corrections: &mut BTreeMap<String, SourceCorrection>,
) {
    for (preset, mlw_kg, mtow_kg) in [
        ("A220-300", 58_740.0, 67_585.0),
        ("A320-200", 66_000.0, 78_000.0),
        ("A340-300", 188_000.0, 260_000.0),
        ("A380-800", 386_000.0, 560_000.0),
        ("B787-9", 192_776.0, 254_692.0),
        ("DC-10", 190_962.0, 259_454.0),
    ] {
        corrections.insert(
            format!("{preset}.mass_model.mlw_fraction_mtow"),
            SourceCorrection {
                upstream: Value::from(0.92),
                corrected: Value::from(mlw_kg / mtow_kg),
            },
        );
    }
}

fn saved_file_source_corrections() -> BTreeMap<String, SourceCorrection> {
    let mut corrections = [
        correction("preset_only.requirements.cabin_preset", "Ryanair", "Custom"),
        correction("preset_only.requirements.num_passengers", 130, 140),
        // The saved-file cases load the A220-300 and B787-9 presets, whose
        // wing-box materials are assigned per type; see the preset table.
        correction(
            "preset_only.structures.skin_material",
            "Al 7075-T6",
            "CFRP QI",
        ),
        correction(
            "preset_only.structures.spar_cap_material",
            "CFRP UD",
            "CFRP 60/30/10",
        ),
        correction(
            "preset_only.structures.spar_web_material",
            "Al 7075-T6",
            "CFRP QI",
        ),
        correction(
            "preset_then_field.structures.skin_material",
            "Al 7075-T6",
            "CFRP QI",
        ),
        correction(
            "preset_then_field.structures.spar_cap_material",
            "CFRP UD",
            "CFRP 60/30/10",
        ),
        correction(
            "preset_then_field.structures.spar_web_material",
            "Al 7075-T6",
            "CFRP QI",
        ),
        correction(
            "preset_then_field.requirements.cabin_preset",
            "Ryanair",
            "Custom",
        ),
        // The two saved-file cases load the A220-300 and the B787-9, which
        // now carry their own certified MLW/MTOW ratio; see
        // `add_certified_landing_mass_ratio_corrections`.
        correction(
            "preset_only.mass_model.mlw_fraction_mtow",
            0.92,
            58_740.0 / 67_585.0,
        ),
        correction(
            "preset_then_field.mass_model.mlw_fraction_mtow",
            0.92,
            192_776.0 / 254_692.0,
        ),
        // Wing roots on the manufacturer plan views: the A220 leading-edge
        // line of ACP Issue 013 Figure 1 and the straight leading
        // edge of the 787 ACAP D6-58333 Rev Q 2.2.2; see
        // support/drawing_fit_2026_11.rs.
        correction("preset_only.geometry.wing.root_datum_x_m", 13.3, 12.775),
        correction(
            "preset_then_field.geometry.wing.root_datum_x_m",
            21.0,
            21.314,
        ),
        // B787-9 root section matched to the CRM thickness distribution
        // (see support/product_corrections.rs).
        correction(
            "preset_then_field.geometry.wing.root_airfoil",
            "sc20614",
            "sc20612",
        ),
        // B787-9 engine installation from the D6-58333 Rev Q section 2.2.2
        // plan view (see support/product_corrections.rs).
        correction(
            "preset_then_field.geometry.engine.spanwise_positions_m[0]",
            9.5,
            9.91,
        ),
        correction(
            "preset_then_field.geometry.engine.spanwise_positions_m[1]",
            -9.5,
            -9.91,
        ),
        correction(
            "preset_then_field.geometry.engine.inlet_x_offset_m",
            3.5,
            5.888,
        ),
        correction("preset_only.landing_gear.n_mlg_struts", 0, 2),
        correction("preset_only.landing_gear.n_nlg_wheels", 0, 2),
        correction(
            "preset_only.landing_gear.track_diameter_factor",
            1.85,
            6.731 / 3.50,
        ),
        correction("preset_only.landing_gear.wheels_per_mlg_strut", 0, 2),
        correction(
            "preset_only.requirements.max_structural_payload_kg",
            18_700.0,
            18_643.0,
        ),
        correction("preset_only.requirements.mtow_kg", 70_900.0, 67_585.0),
        correction("preset_then_field.landing_gear.n_mlg_struts", 0, 2),
        correction("preset_then_field.landing_gear.n_nlg_wheels", 0, 2),
        correction(
            "preset_then_field.landing_gear.track_diameter_factor",
            1.85,
            9.8 / 5.77,
        ),
        correction("preset_then_field.landing_gear.wheels_per_mlg_strut", 0, 4),
        correction(
            "preset_then_field.requirements.max_structural_payload_kg",
            52_600.0,
            52_586.0,
        ),
        correction(
            "preset_then_field.requirements.mtow_kg",
            254_000.0,
            254_692.0,
        ),
        // The A220-300 published nose-gear load; see the preset table (its dive
        // speed is not pinned there).
        correction(
            "preset_only.landing_gear.nose_gear_max_static_load_kg",
            "absent upstream",
            13_923.0 * 0.453_592_37,
        ),
    ]
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    add_transport_planform_corrections(
        &mut corrections,
        &[
            "empty",
            "preset_only",
            "preset_then_field",
            "tuple_field_from_a_list",
            "airports",
            "unknown_preset",
            "deep_partial",
        ],
    );
    add_planning_cabin_corrections(&mut corrections, &["preset_only"]);
    add_b787_9_typical_cabin_corrections(&mut corrections, &["preset_then_field"]);
    add_declared_flap_setting_corrections(&mut corrections, &["preset_then_field"]);
    corrections.insert(
        "preset_then_field.geometry.wing.kink_span_fraction".to_owned(),
        SourceCorrection {
            upstream: Value::String("absent upstream".to_owned()),
            corrected: Value::from(0.3696),
        },
    );
    corrections.insert(
        "preset_only.geometry.wing.kink_span_fraction".to_owned(),
        SourceCorrection {
            upstream: Value::String("absent upstream".to_owned()),
            corrected: Value::from(0.36),
        },
    );
    add_transport_objective_weight_corrections(
        &mut corrections,
        &[
            "empty",
            "preset_only",
            "preset_then_field",
            "tuple_field_from_a_list",
            "airports",
            "unknown_preset",
            "deep_partial",
        ],
    );
    add_optimizer_method_corrections(
        &mut corrections,
        &[
            "empty",
            "preset_only",
            "preset_then_field",
            "tuple_field_from_a_list",
            "airports",
            "unknown_preset",
            "deep_partial",
        ],
    );
    add_random_force_psd_corrections(
        &mut corrections,
        &[
            "empty",
            "preset_only",
            "preset_then_field",
            "tuple_field_from_a_list",
            "airports",
            "unknown_preset",
            "deep_partial",
        ],
    );
    add_native_worker_corrections(
        &mut corrections,
        &[
            "empty",
            "preset_only",
            "preset_then_field",
            "tuple_field_from_a_list",
            "airports",
            "unknown_preset",
            "deep_partial",
        ],
    );
    add_native_solver_limit_corrections(
        &mut corrections,
        &[
            "empty",
            "preset_only",
            "preset_then_field",
            "tuple_field_from_a_list",
            "airports",
            "unknown_preset",
            "deep_partial",
        ],
    );
    // The five cases that load no registered aircraft keep the default
    // landing ratio; the two preset cases carry their own (above).
    add_default_landing_ratio_corrections(
        &mut corrections,
        &[
            "empty",
            "tuple_field_from_a_list",
            "airports",
            "unknown_preset",
            "deep_partial",
        ],
    );
    corrections
}

/// Configurations with no certified MLW/MTOW pair (AVE and the unconfigured
/// defaults) carry the default landing ratio, which is the AVE's 777-9
/// benchmark (Boeing D6-86073 Rev G Table 2-1) instead of the frozen,
/// unsourced 0.92; see `alas_config::landing_mass_ratio::LONG_HAUL_MLW_FRACTION_MTOW`.
fn add_default_landing_ratio_corrections(
    corrections: &mut BTreeMap<String, SourceCorrection>,
    cases: &[&str],
) {
    for case in cases {
        corrections.insert(
            format!("{case}.mass_model.mlw_fraction_mtow"),
            SourceCorrection {
                upstream: Value::from(0.92),
                corrected: Value::from(
                    alas_config::landing_mass_ratio::LONG_HAUL_MLW_FRACTION_MTOW,
                ),
            },
        );
    }
}

/// Take-off and landing flap settings a preset documents, a field the frozen
/// schema has no counterpart for: the sourced literals of
/// `product_corrections::declared_flap_setting_deg` (A320 15 / 35, A340 22 /
/// 32, B787 15 / 30, DC-10 15 / 50 deg, each with its document there).
fn add_declared_flap_setting_corrections(
    corrections: &mut BTreeMap<String, SourceCorrection>,
    cases: &[&str],
) {
    for case in cases {
        let (takeoff_deg, landing_deg) = product_corrections::declared_flap_setting_deg(case)
            .unwrap_or_else(|| panic!("{case} documents its flap settings"));
        for (field, deg) in [
            ("takeoff_flap_deflection_deg", takeoff_deg),
            ("landing_flap_deflection_deg", landing_deg),
        ] {
            corrections.insert(
                format!("{case}.performance.{field}"),
                SourceCorrection {
                    upstream: Value::String("absent upstream".to_owned()),
                    corrected: Value::from(deg),
                },
            );
        }
    }
}

/// The frozen class runs every reference file carries
/// (`ControlSurfacesConfig::default`): chord fraction, span start, span end.
const FROZEN_CLASS_RUNS: [(&str, (f64, f64, f64)); 6] = [
    ("slat", (0.15, 0.08, 0.95)),
    ("flap", (0.25, 0.10, 0.62)),
    ("aileron", (0.20, 0.66, 0.95)),
    ("spoiler", (0.10, 0.10, 0.64)),
    ("elevator", (0.35, 0.05, 0.95)),
    ("rudder", (0.35, 0.10, 0.90)),
];

/// One preset's six declared control-surface runs, as `DECLARED_RUNS` lists them.
type DeclaredRuns = (&'static str, [(f64, f64, f64); 6]);

/// The control-surface runs each registered aircraft declares
/// (`alas_config::preset_control_surfaces`), in the order of [`FROZEN_CLASS_RUNS`]. Chords: S1, a printed
/// area over the chord integral of the band (Jane's All the World's Aircraft
/// 2008-09 AREAS blocks; DC-10 flaps from Jenkinson, Simpkin and Rhodes, Civil
/// Jet Aircraft Design, 1999, Data A, Table 6, its spoilers from a grade-C
/// secondary listing and held, see [`AWAITING_SOURCE`]); S2, a printed
/// chord ratio (Sadraey, Aircraft Design: A Systems Engineering Approach,
/// Tables 12.17, 12.18, 12.20, the type's or the family's row); E, the S1
/// chord of the class's printed aircraft (A320: slat 0.1452, flap 0.2504,
/// spoiler 0.1096; 747-400: slat 0.1177, flap 0.2602, spoiler 0.1039).
/// Stations from the manufacturer drawings (Airbus AC section 2-3-0 ground
/// clearance figures; NASA CR-4746 Fig. 2.16 DC-10-30 butt lines; A220 ACP
/// general dimensions). The B787-9 stations are estimated (grade E): the 787
/// ACAP D6-58333 Rev Q plan on file draws no surface lines, so each run's ends
/// are placed at the flap-track fairings the plan does draw
/// (`preset_control_surfaces` module notes); they are pinned as those
/// estimates, not as printed values. The AVE declares none and keeps the class
/// runs.
const DECLARED_RUNS: [DeclaredRuns; 6] = [
    (
        "A220-300",
        [
            (0.1452, 0.11, 0.92),
            (0.2504, 0.105, 0.70),
            (0.24, 0.70, 0.877),
            (0.1096, 0.14, 0.69),
            (0.32, 0.07, 0.95),
            (0.28, 0.05, 0.95),
        ],
    ),
    (
        "A320-200",
        [
            (0.1452, 0.169, 0.959),
            (0.2504, 0.115, 0.767),
            (0.2659, 0.810, 0.963),
            (0.1096, 0.154, 0.797),
            (0.32, 0.03, 0.97),
            (0.32, 0.05, 0.97),
        ],
    ),
    (
        "A340-300",
        [
            (0.1177, 0.110, 0.952),
            (0.2602, 0.104, 0.647),
            (0.275, 0.660, 0.942),
            (0.1039, 0.134, 0.646),
            (0.31, 0.10, 0.87),
            (0.32, 0.03, 0.97),
        ],
    ),
    (
        "A380-800",
        [
            (0.1177, 0.214, 0.980),
            (0.2602, 0.113, 0.640),
            (0.275, 0.653, 0.884),
            (0.1039, 0.106, 0.805),
            (0.31, 0.08, 0.93),
            (0.32, 0.05, 0.95),
        ],
    ),
    (
        "B787-9",
        [
            (0.1177, 0.12, 0.93),
            (0.2602, 0.12, 0.73),
            (0.22, 0.75, 0.93),
            (0.1039, 0.20, 0.70),
            (0.32, 0.08, 0.93),
            (0.28, 0.05, 0.95),
        ],
    ),
    (
        "DC-10",
        [
            (0.1177, 0.119, 0.95),
            (0.3153, 0.119, 0.703),
            (0.22, 0.718, 0.902),
            (0.1341, 0.403, 0.703),
            (0.25, 0.05, 0.95),
            (0.38, 0.05, 0.95),
        ],
    ),
];

/// Declared run leaves left left unpinned: the DC-10 spoiler
/// chord 0.1341 is the S1 chord of a 12.7 m^2 spoiler area from a secondary
/// listing (grade C, `preset_control_surfaces::DC10_SPOILER`), not a primary
/// source. Its check stays failing until a primary source replaces it.
const AWAITING_SOURCE: [&str; 1] = ["DC-10.control_surfaces.spoiler_chord_fraction"];

/// `(frozen class value, declared value)` of one control-surface run leaf of
/// a case that loads a registered aircraft.
fn declared_control_surface_run(path: &str) -> Option<(Value, Value)> {
    if AWAITING_SOURCE.contains(&path) {
        return None;
    }
    let (case, field) = path.split_once(".control_surfaces.")?;
    let preset = match case {
        "preset_only" => "A220-300",
        "preset_then_field" => "B787-9",
        name => name,
    };
    let (_, runs) = DECLARED_RUNS.iter().find(|(name, _)| *name == preset)?;
    let (surface, part) = if let Some(surface) = field.strip_suffix("_chord_fraction") {
        (surface, 0)
    } else if let Some(surface) = field.strip_suffix("_span_start_frac") {
        (surface, 1)
    } else {
        (field.strip_suffix("_span_end_frac")?, 2)
    };
    let index = FROZEN_CLASS_RUNS
        .iter()
        .position(|(name, _)| *name == surface)?;
    let pick = |(chord, start, end): (f64, f64, f64)| [chord, start, end][part];
    Some((
        Value::from(pick(FROZEN_CLASS_RUNS[index].1)),
        Value::from(pick(runs[index])),
    ))
}

/// The product presets now load an explicit, physically representable generic
/// planning cabin instead of inheriting the old widebody business block. The
/// saved Python fixture remains frozen; these leaves document the deliberate
/// source correction rather than making the parity test silently accept drift.
/// The 787-9 cabin is seeded with Boeing's typical two-class seat shares
/// (D6-58333 Rev Q section 2.1.2: 28 business, 262 economy) instead of the
/// generic 15/85 mix.
fn add_b787_9_typical_cabin_corrections(
    corrections: &mut BTreeMap<String, SourceCorrection>,
    cases: &[&str],
) {
    for case in cases {
        for (class, upstream, seats) in [("business", 15.0, 28.0), ("economy", 85.0, 262.0)] {
            corrections.insert(
                format!("{case}.cabin.passenger.{class}.share_pct"),
                SourceCorrection {
                    upstream: Value::from(upstream),
                    corrected: Value::from(seats / 290.0 * 100.0),
                },
            );
        }
    }
}

/// AVE flies the 777-9 standard two-class cabin (D6-86073 Rev G Table 2-1
/// and Figures 2-3 and 2-5: 42 business at 85 in, 384 economy at 32 in, 10
/// abreast on 18 in seats) in place of the all-economy airline profile.
fn add_ave_777_9_cabin_corrections(corrections: &mut BTreeMap<String, SourceCorrection>) {
    const INCH_M: f64 = 0.0254;
    let mut insert = |path: &str, upstream: Value, corrected: Value| {
        corrections.insert(
            format!("AVE.{path}"),
            SourceCorrection {
                upstream,
                corrected,
            },
        );
    };
    insert(
        "requirements.cabin_preset",
        Value::from("Ryanair"),
        Value::from("Custom"),
    );
    insert(
        "cabin.passenger.business.share_pct",
        Value::from(15.0),
        Value::from(42.0 / 426.0 * 100.0),
    );
    insert(
        "cabin.passenger.economy.share_pct",
        Value::from(85.0),
        Value::from(384.0 / 426.0 * 100.0),
    );
    insert(
        "cabin.passenger.business.pitch_m",
        Value::from(1.55),
        Value::from(85.0 * INCH_M),
    );
    insert(
        "cabin.passenger.economy.pitch_m",
        Value::from(0.79),
        Value::from(32.0 * INCH_M),
    );
    insert(
        "cabin.passenger.economy.width_m",
        Value::from(0.46),
        Value::from(18.0 * INCH_M),
    );
    insert(
        "cabin.passenger.economy.abreast",
        Value::from(0),
        Value::from(10),
    );
}

fn add_planning_cabin_corrections(
    corrections: &mut BTreeMap<String, SourceCorrection>,
    cases: &[&str],
) {
    for case in cases {
        corrections.insert(
            format!("{case}.cabin.passenger.business.share_pct"),
            SourceCorrection {
                upstream: Value::from(15.0),
                corrected: Value::from(0.0),
            },
        );
        corrections.insert(
            format!("{case}.cabin.passenger.economy.share_pct"),
            SourceCorrection {
                upstream: Value::from(85.0),
                corrected: Value::from(100.0),
            },
        );
    }
}

/// The product search budget: every thread by default (whole-generation
/// batches are worker-count independent), a normalized design-space spread
/// tolerance, and the two stage budgets that replace the retired
/// generation-count keys (see [`is_retired_solver_field`]).
fn add_native_worker_corrections(
    corrections: &mut BTreeMap<String, SourceCorrection>,
    cases: &[&str],
) {
    let product = alas_config::SolverSettings::default();
    for case in cases {
        corrections.insert(
            format!("{case}.optimizer.solver.workers"),
            SourceCorrection {
                upstream: Value::from(1.0),
                corrected: Value::from(0.0),
            },
        );
        // A file that does not state a seed
        // gets the fixed default seed; the frozen default is the clock.
        corrections.insert(
            format!("{case}.optimizer.solver.seed"),
            SourceCorrection {
                upstream: Value::Null,
                corrected: Value::from(alas_config::DEFAULT_SEED),
            },
        );
        corrections.insert(
            format!("{case}.optimizer.solver.tolerance"),
            SourceCorrection {
                upstream: Value::from(0.01),
                corrected: Value::from(product.tolerance),
            },
        );
        for (stage, budget) in [
            ("screening", &product.screening),
            ("refinement", &product.refinement),
        ] {
            corrections.insert(
                format!("{case}.optimizer.solver.{stage}"),
                SourceCorrection {
                    upstream: Value::String("absent upstream".to_owned()),
                    corrected: serde_json::to_value(budget).unwrap(),
                },
            );
        }
    }
}

/// The generation-count budget keys the product retired. A saved file that
/// states them is migrated to the refinement evaluation budget at load time
/// (tested by the load-note unit tests), so the loaded configuration has no
/// such field to compare.
fn is_retired_solver_field(path: &str, key: &str) -> bool {
    matches!(key, "max_iterations" | "population_size") && path.ends_with(".solver")
}

fn add_optimizer_method_corrections(
    corrections: &mut BTreeMap<String, SourceCorrection>,
    cases: &[&str],
) {
    for case in cases {
        corrections.insert(
            format!("{case}.optimizer.solver.method"),
            SourceCorrection {
                upstream: Value::String("absent upstream".to_owned()),
                corrected: Value::String("differential_evolution".to_owned()),
            },
        );
    }
}

/// The external-solver resource limits the Rust application added: the AVL
/// timeout and the NASTRAN memory request. The frozen Python configuration
/// has neither, so a loaded file receives the product defaults.
fn add_native_solver_limit_corrections(
    corrections: &mut BTreeMap<String, SourceCorrection>,
    cases: &[&str],
) {
    for case in cases {
        corrections.insert(
            format!("{case}.analysis.avl_timeout_s"),
            SourceCorrection {
                upstream: Value::String("absent upstream".to_owned()),
                corrected: Value::from(AnalysisConfig::default().avl_timeout_s),
            },
        );
        corrections.insert(
            format!("{case}.structures.nastran_memory_mb"),
            SourceCorrection {
                upstream: Value::String("absent upstream".to_owned()),
                corrected: Value::from(StructuresConfig::default().nastran_memory_mb),
            },
        );
    }
}

fn add_random_force_psd_corrections(
    corrections: &mut BTreeMap<String, SourceCorrection>,
    cases: &[&str],
) {
    for case in cases {
        corrections.insert(
            format!("{case}.structures.random_force_psd_n2_per_hz"),
            SourceCorrection {
                upstream: Value::String("absent upstream".to_owned()),
                corrected: Value::from(1.0),
            },
        );
    }
}

/// The lower-cover fatigue stress limit is a native product addition declared
/// by the metallic civil presets (Torenbeek 2013, section 11.7.1: 75 MPa for
/// the short-haul class, 110 MPa for the long-haul class); the frozen schema
/// has no such field.
fn add_lower_cover_fatigue_corrections(
    corrections: &mut BTreeMap<String, SourceCorrection>,
    cases: &[(&str, f64)],
) {
    for (case, limit_pa) in cases {
        corrections.insert(
            format!("{case}.structures.lower_cover_fatigue_stress_1g_pa"),
            SourceCorrection {
                upstream: Value::String("absent upstream".to_owned()),
                corrected: Value::from(*limit_pa),
            },
        );
    }
}

fn add_transport_planform_corrections(
    corrections: &mut BTreeMap<String, SourceCorrection>,
    cases: &[&str],
) {
    for case in cases {
        for (field, value) in [
            ("side_of_body_span_fraction", 0.10),
            ("kink_span_fraction", 0.37),
        ] {
            corrections.insert(
                format!("{case}.geometry.wing.{field}"),
                SourceCorrection {
                    upstream: Value::String("absent upstream".to_owned()),
                    corrected: Value::from(value),
                },
            );
        }
    }
}

fn add_transport_objective_weight_corrections(
    corrections: &mut BTreeMap<String, SourceCorrection>,
    cases: &[&str],
) {
    let native_fields = [
        ("transport_planform_constraints_enabled", Value::Bool(true)),
        ("transport_shape_priors_enabled", Value::Bool(false)),
        ("geometric_body_alpha_min_deg", Value::from(0.0)),
        ("geometric_body_alpha_max_deg", Value::from(4.0)),
        ("min_root_wingbox_depth_m", Value::from(1.20)),
        ("min_break_wingbox_depth_m", Value::from(0.65)),
        ("min_break_wingbox_width_m", Value::from(2.50)),
        ("min_flap_area_fraction", Value::from(0.11)),
        ("max_root_bending_box_slenderness", Value::from(700.0)),
        ("tankable_span_start_fraction", Value::from(0.10)),
        ("tankable_span_end_fraction", Value::from(0.75)),
        ("min_break_root_chord_ratio", Value::from(0.40)),
        ("min_tip_root_chord_ratio", Value::from(0.16)),
        ("min_inboard_te_sweep_deg", Value::from(0.0)),
        ("max_inboard_te_sweep_deg", Value::from(22.0)),
    ];
    for case in cases {
        for (field, value) in &native_fields {
            corrections.insert(
                format!("{case}.optimizer.weights.{field}"),
                SourceCorrection {
                    upstream: Value::String("absent upstream".to_owned()),
                    corrected: value.clone(),
                },
            );
        }
    }
}
