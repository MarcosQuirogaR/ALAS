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

use alas_config::AlasConfig;
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
    if let Some(new) =
        product_corrections::engine_copy(path).or_else(|| product_corrections::operational(path))
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
        // Wing-box material families assigned per preset from the airport
        // planning documents (`alas_config::preset_structures`); the frozen
        // files carried the database default for every type.
        correction("A220-300.structures.skin_material", "Al 7075-T6", "CFRP QI"),
        correction(
            "A220-300.structures.spar_cap_material",
            "CFRP UD",
            "CFRP QI",
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
        correction("B787-9.structures.spar_cap_material", "CFRP UD", "CFRP QI"),
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
        correction("B787-9.landing_gear.n_mlg_struts", 0, 2),
        correction("B787-9.landing_gear.n_nlg_wheels", 0, 2),
        correction(
            "B787-9.landing_gear.track_diameter_factor",
            1.85,
            9.8 / 5.94,
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
    ]
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    add_transport_planform_corrections(
        &mut corrections,
        &[
            "A220-300", "A320-200", "A340-300", "A380-800", "AVE", "B787-9", "DC-10",
        ],
    );
    add_planning_cabin_corrections(&mut corrections, &["A220-300", "A320-200", "A340-300"]);
    for (case, kink_fraction) in [
        ("A340-300", 0.362_094_754_983_253_8),
        ("A380-800", 0.359_236_516_064_625_5),
        ("AVE", 0.35),
        ("B787-9", 0.353_771_245_388_011_8),
        ("A320-200", 0.377_380_002_280_241_9),
        ("A220-300", 0.382_736_255_076_680_5),
        ("DC-10", 0.35),
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
    add_certified_landing_mass_ratio_corrections(&mut corrections);
    corrections
}

/// A registered aircraft now carries its own certified landing-to-takeoff
/// mass ratio instead of the frozen study fraction.
///
/// FLOPS equation 63 sizes the main gear on `WLDG^0.95`, so the generic 0.92
/// put 515 t of landing weight into the A380-800's gear equation against its
/// certified 386 t. Both masses are already in the registry with their
/// airport-planning and type-certificate provenance, so the ratio is read
/// from them. AVE declares no certified pair and keeps 0.92, which is why it
/// is absent here.
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
        // The saved-file cases load the A220-300 and B787-9 presets, whose
        // wing-box materials are now assigned per type; see the preset table.
        correction(
            "preset_only.structures.skin_material",
            "Al 7075-T6",
            "CFRP QI",
        ),
        correction(
            "preset_only.structures.spar_cap_material",
            "CFRP UD",
            "CFRP QI",
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
            "CFRP QI",
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
            9.8 / 5.94,
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
    corrections.insert(
        "preset_then_field.geometry.wing.kink_span_fraction".to_owned(),
        SourceCorrection {
            upstream: Value::String("absent upstream".to_owned()),
            corrected: Value::from(0.353_771_245_388_011_8),
        },
    );
    corrections.insert(
        "preset_only.geometry.wing.kink_span_fraction".to_owned(),
        SourceCorrection {
            upstream: Value::String("absent upstream".to_owned()),
            corrected: Value::from(0.382_736_255_076_680_5),
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
    corrections
}

/// The product presets now load an explicit, physically representable generic
/// planning cabin instead of inheriting the old widebody business block. The
/// saved Python fixture remains frozen; these leaves document the deliberate
/// source correction rather than making the parity test silently accept drift.
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

/// The native worker count.
///
/// `workers` moved from the frozen literal `1` to `0`, meaning "resolve
/// against this machine": the staged MADS search evaluates a poll block in
/// parallel at that count without changing which points it evaluates or which
/// one it returns. Differential evolution is deliberately excluded from the
/// automatic setting - its generation loop batches only on an explicit
/// request, because a batched generation defers the population update and is a
/// different algorithm - so the frozen replay keeps the reference
/// interleaving.
fn add_native_worker_corrections(
    corrections: &mut BTreeMap<String, SourceCorrection>,
    cases: &[&str],
) {
    for case in cases {
        corrections.insert(
            format!("{case}.optimizer.solver.workers"),
            SourceCorrection {
                upstream: Value::from(1.0),
                corrected: Value::from(0.0),
            },
        );
    }
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
        ("geometric_body_alpha_min_deg", Value::from(2.0)),
        ("geometric_body_alpha_max_deg", Value::from(4.0)),
        ("geometric_body_alpha_penalty_scale", Value::from(200.0)),
        ("min_root_wingbox_depth_m", Value::from(1.20)),
        ("min_break_wingbox_depth_m", Value::from(0.65)),
        ("min_break_wingbox_width_m", Value::from(2.50)),
        ("wingbox_packaging_penalty_scale", Value::from(250.0)),
        ("min_flap_area_fraction", Value::from(0.11)),
        ("flap_area_penalty_scale", Value::from(250.0)),
        ("max_root_bending_box_slenderness", Value::from(700.0)),
        ("bending_slenderness_penalty_scale", Value::from(150.0)),
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
