// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `alas-mission::vehicle` against `golden/mission/vehicle.json`.
//!
//! Each case names a preset whose full-analysis report the fixture recorded.
//! The parity test rebuilds `AlasConfig::from_value({"preset": name})` and
//! applies the engine spec the way `AircraftBuilder::build` does before the
//! request is assembled: the reference's report was produced by building the
//! aircraft, which mutates `config.geometry.engine` to the selected engine's
//! cycle in place, then feeds the recorded `ReportView` and compares the
//! request `build_vehicle_request` assembles.
//!
//! The comparison walks the two documents in parallel: strings at `exact`, and
//! every number: the derived cruise thrust, the mass, the whole geometry
//! configuration and the engine and requirements blocks, at `closed`. The
//! three report passthroughs are echoed unchanged, so they compare trivially;
//! the check that earns the fixture is the cruise-thrust derivation and the
//! geometry serialization.
//!
//! The independent preset audit supersedes the A320 engine and A340
//! model/weight variant. A two-sided propagation ledger pins the old fixture
//! and corrected request at those leaves; all unrelated request fields retain
//! their original comparison tier.

use std::collections::BTreeMap;

use alas_config::AlasConfig;
use alas_mission::{build_vehicle_request_reference_compatibility, ReportView};
use alas_testkit::{load_json, Comparison, Tier};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    preset: String,
    report_view: ReportFields,
    request: Value,
}

#[derive(Deserialize)]
struct ReportFields {
    trimmed_l_over_d: Option<f64>,
    plain_l_over_d: Option<f64>,
    design_vector: Value,
    geometry_summary: Value,
    component_masses: Value,
}

struct RequestCorrection {
    upstream: Value,
    corrected: Value,
}

/// Walk two request documents in parallel, routing each leaf to its tier.
fn compare(
    path: &str,
    actual: &Value,
    expected: &Value,
    numbers: &mut Comparison,
    strings: &mut Comparison,
    corrections: &mut BTreeMap<&'static str, RequestCorrection>,
) {
    if let Some(correction) = corrections.remove(path) {
        match (
            actual.as_f64(),
            expected.as_f64(),
            correction.upstream.as_f64(),
            correction.corrected.as_f64(),
        ) {
            (Some(actual), Some(expected), Some(upstream), Some(corrected)) => {
                numbers.scalar(
                    &format!("{path}: frozen Python request"),
                    expected,
                    upstream,
                );
                numbers.scalar(
                    &format!("{path}: source-corrected request"),
                    actual,
                    corrected,
                );
            }
            _ => {
                strings.exact(
                    &format!("{path}: frozen Python request"),
                    &expected.to_string(),
                    &correction.upstream.to_string(),
                );
                strings.exact(
                    &format!("{path}: source-corrected request"),
                    &actual.to_string(),
                    &correction.corrected.to_string(),
                );
            }
        }
        return;
    }
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => {
            for (key, expected_value) in e {
                let child = join(path, key);
                match a.get(key) {
                    Some(actual_value) => compare(
                        &child,
                        actual_value,
                        expected_value,
                        numbers,
                        strings,
                        corrections,
                    ),
                    None => {
                        strings.exact(&child, &"<absent>".to_owned(), &"<present>".to_owned());
                    }
                }
            }
            for key in a.keys() {
                if !e.contains_key(key) {
                    // The typed engine payload is a native addition the flat
                    // legacy keys are derived from; the binding unit tests
                    // validate it and the flat keys are compared here.
                    if path.ends_with(".geometry_config.engine")
                        && matches!(key.as_str(), "turbofan" | "turboprop")
                    {
                        continue;
                    }
                    // The declared airfoil class is a native wing input the
                    // frozen request predates; alas-config's parity ledgers
                    // and preset declarations check it.
                    if path.ends_with(".geometry_config.wing") && key == "airfoil_class" {
                        continue;
                    }
                    let child = join(path, key);
                    // A leaf the frozen request never carried is admitted
                    // only through an explicit two-sided correction.
                    if let Some(correction) = corrections.remove(child.as_str()) {
                        let upstream = correction
                            .upstream
                            .as_str()
                            .map_or_else(|| correction.upstream.to_string(), str::to_owned);
                        strings.exact(
                            &format!("{child}: frozen Python request"),
                            &"<absent>".to_owned(),
                            &upstream,
                        );
                        compare(
                            &format!("{child}: source-corrected request"),
                            &a[key],
                            &correction.corrected,
                            numbers,
                            strings,
                            corrections,
                        );
                        continue;
                    }
                    strings.exact(&child, &"<present>".to_owned(), &"<absent>".to_owned());
                }
            }
        }
        (Value::Array(a), Value::Array(e)) => {
            if a.len() != e.len() {
                strings.exact(
                    &format!("{path}.len"),
                    &a.len().to_string(),
                    &e.len().to_string(),
                );
                return;
            }
            for (index, (av, ev)) in a.iter().zip(e).enumerate() {
                compare(
                    &format!("{path}[{index}]"),
                    av,
                    ev,
                    numbers,
                    strings,
                    corrections,
                );
            }
        }
        (Value::String(a), Value::String(e)) => {
            strings.exact(path, a, e);
        }
        _ => {
            // Numbers may be JSON ints on one side and floats on the other, so
            // both are read through `as_f64`. `null` (a `None` cruise thrust)
            // reads as neither, and compares exactly.
            match (actual.as_f64(), expected.as_f64()) {
                (Some(a), Some(e)) => {
                    numbers.scalar(path, a, e);
                }
                _ => {
                    strings.exact(path, &actual.to_string(), &expected.to_string());
                }
            }
        }
    }
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

// The test builds inputs it recorded and asserts on them, so a failed expect is
// a broken fixture rather than a library invariant.
#[allow(clippy::expect_used)]
#[test]
fn vehicle_request_matches_the_reference() {
    let fixture: Fixture =
        serde_json::from_value(load_json("mission", "vehicle")).expect("fixture shape");

    let mut numbers = Comparison::new("vehicle request numbers", Tier::Closed);
    let mut strings = Comparison::new("vehicle request strings", Tier::Exact);
    let mut corrections = request_corrections();

    for case in &fixture.cases {
        let mut config =
            AlasConfig::from_value(&json!({ "preset": case.preset })).expect("the preset loads");
        // The reference's report came from building the aircraft, which applies
        // the selected engine's cycle onto `config.geometry.engine` in place.
        config.geometry.engine.apply_engine_spec();

        let view = ReportView {
            trimmed_l_over_d: case.report_view.trimmed_l_over_d,
            plain_l_over_d: case.report_view.plain_l_over_d,
            design_vector: case.report_view.design_vector.clone(),
            geometry_summary: case.report_view.geometry_summary.clone(),
            component_masses: case.report_view.component_masses.clone(),
        };

        let request = build_vehicle_request_reference_compatibility(&view, &config);
        let actual = serde_json::to_value(&request).expect("the request serializes");
        compare(
            &case.preset,
            &actual,
            &case.request,
            &mut numbers,
            &mut strings,
            &mut corrections,
        );
    }

    strings.exact(
        "unvisited preset-correction request leaves",
        &corrections.keys().copied().collect::<Vec<_>>(),
        &Vec::<&str>::new(),
    );

    numbers.finish();
    strings.finish();
}

fn request_corrections() -> BTreeMap<&'static str, RequestCorrection> {
    [
        // The wing spanwise panel count, which changed meaning rather than
        // fidelity: the frozen value is a per-section multiplier, the product
        // one an absolute panel count across the semispan, and 24 is what the
        // frozen three-section planform already meshed to. The same decision
        // is declared for the configuration default in `alas-config`'s
        // `parity_config::product_default_correction`; it reaches the vehicle
        // request through these two presets, so it is recorded here too rather
        // than left as an undeclared disagreement. See
        // `alas_geom::aircraft::spanwise`.
        correction("A320-200.geometry_config.wing.n_subdivisions", 8.0, 24.0),
        correction("A340-300.geometry_config.wing.n_subdivisions", 8.0, 24.0),
        // Airbus/EASA-sourced A320-214 and A340-312 dimension corrections,
        // pinned two-sidedly by the alas-config preset ledger.
        correction(
            "A320-200.geometry_config.empennage.hstab_root_chord_m",
            4.0,
            3.740,
        ),
        correction(
            "A320-200.geometry_config.empennage.hstab_tip_chord_m",
            1.2,
            1.24,
        ),
        correction(
            "A320-200.geometry_config.empennage.hstab_tip_le_m[0]",
            3.5,
            3.31,
        ),
        correction(
            "A320-200.geometry_config.empennage.hstab_tip_le_m[1]",
            6.0,
            6.225,
        ),
        correction(
            "A320-200.geometry_config.empennage.vstab_root_chord_m",
            5.2,
            5.444,
        ),
        correction(
            "A320-200.geometry_config.empennage.vstab_tip_chord_m",
            1.8,
            1.884,
        ),
        correction(
            "A320-200.geometry_config.empennage.vstab_tip_le_m[0]",
            5.0,
            5.06,
        ),
        correction(
            "A320-200.geometry_config.empennage.vstab_tip_le_m[2]",
            5.8,
            5.87,
        ),
        // Airbus A320 AC Jun 01/24, FIGURE-2-2-0-991-004-A01 sheet 1: fin
        // height from the fuselage top line, so the root sits on the crown.
        correction("A320-200.geometry_config.empennage.vstab_z_m", 1.2, 2.17),
        correction(
            "A320-200.geometry_config.engine.spanwise_positions_m[0]",
            5.5,
            5.755,
        ),
        correction(
            "A320-200.geometry_config.engine.spanwise_positions_m[1]",
            -5.5,
            -5.755,
        ),
        correction(
            "A320-200.geometry_config.wing.break_span_fraction",
            0.37,
            0.379_3,
        ),
        correction("A320-200.geometry_config.wing.root_datum_x_m", 12.9, 11.891),
        // The A320 planar wing and dihedral of the Airbus AC plan view and
        // sharklet ground clearances; pinned by the alas-config preset ledger.
        correction("A320-200.geometry_config.wing.break_z_m", -0.2, -0.62),
        correction("A320-200.geometry_config.wing.tip_z_m", 1.5, 0.33),
        correction("A320-200.geometry_config.engine.z_m", -1.71, -1.50),
        correction(
            "A320-200.geometry_config.engine.inlet_x_offset_m",
            2.5,
            3.646,
        ),
        // Wing root anchored to the Airbus quarter-MAC point (gear-load statics).
        correction("A340-300.geometry_config.wing.root_datum_x_m", 22.0, 22.341),
        correction(
            "A340-300.geometry_config.empennage.hstab_tip_le_m[1]",
            9.0,
            9.7,
        ),
        // Airbus A340-200/-300 AC Rev 33, Figure 2-2-0-991-007-A01 sheets 1-2
        // (engine stations and inlets, fin and tailplane) and Figure
        // 2-3-0-991-005-A01 (wing tip and nacelle ground clearances, aft CG),
        // pinned two-sidedly by the alas-config preset ledger.
        correction(
            "A340-300.geometry_config.empennage.hstab_offset_from_tail_m",
            9.0,
            8.42,
        ),
        correction(
            "A340-300.geometry_config.empennage.hstab_tip_chord_m",
            1.8,
            2.02,
        ),
        correction(
            "A340-300.geometry_config.empennage.hstab_tip_le_m[0]",
            6.0,
            6.43,
        ),
        correction(
            "A340-300.geometry_config.empennage.vstab_offset_from_tail_m",
            10.5,
            11.24,
        ),
        correction(
            "A340-300.geometry_config.empennage.vstab_root_chord_m",
            8.0,
            7.78,
        ),
        correction(
            "A340-300.geometry_config.empennage.vstab_tip_chord_m",
            2.8,
            2.28,
        ),
        correction(
            "A340-300.geometry_config.empennage.vstab_tip_le_m[0]",
            7.5,
            8.14,
        ),
        correction(
            "A340-300.geometry_config.empennage.vstab_tip_le_m[2]",
            8.5,
            8.3,
        ),
        correction("A340-300.geometry_config.empennage.vstab_z_m", 1.8, 3.02),
        correction(
            "A340-300.geometry_config.engine.inlet_x_offset_m",
            3.0,
            4.227,
        ),
        correction(
            "A340-300.geometry_config.engine.spanwise_positions_m[0]",
            7.5,
            9.37,
        ),
        correction(
            "A340-300.geometry_config.engine.spanwise_positions_m[1]",
            -7.5,
            -9.37,
        ),
        correction(
            "A340-300.geometry_config.engine.spanwise_positions_m[2]",
            14.0,
            19.27,
        ),
        correction(
            "A340-300.geometry_config.engine.spanwise_positions_m[3]",
            -14.0,
            -19.27,
        ),
        correction("A340-300.geometry_config.engine.z_m", -1.94, -1.436),
        correction("A340-300.geometry_config.wing.break_z_m", -0.3, -0.922),
        correction("A340-300.geometry_config.wing.tip_z_m", 2.0, 1.315),
        correction(
            "A320-200.geometry_config.fuselage.height_m",
            Value::Null,
            4.14,
        ),
        correction(
            "A320-200.geometry_config.wing.side_of_body_chord_ratio",
            "<absent>",
            0.852_98,
        ),
        correction("A320-200.engine.bypass_ratio", 11.0, 5.5),
        correction("A320-200.engine.fan_pressure_ratio", 1.4, 1.6),
        correction("A320-200.engine.nacelle_length_m", 3.6, 3.3),
        correction("A320-200.engine.nacelle_max_radius_m", 1.15, 1.0),
        correction("A320-200.engine.overall_pressure_ratio", 40.0, 32.6),
        // The typed CFM56-5B4/3 binding carries its identity-matched ICAO
        // LTO rating rather than the published design scalar.
        correction("A320-200.engine.thrust_kn", 120.64, 120.1),
        correction("A320-200.engine.turbine_inlet_temp_k", 1650.0, 1600.0),
        correction("A320-200.geometry_config.engine.bypass_ratio", 11.0, 5.5),
        correction(
            "A320-200.geometry_config.engine.engine_name",
            "LEAP-1A",
            "CFM56-5B4/3",
        ),
        correction(
            "A320-200.geometry_config.engine.cruise_tsfc_kg_kgf_hr",
            0.52,
            0.6,
        ),
        correction(
            "A320-200.geometry_config.engine.fan_diameter_m",
            1.981,
            1.735,
        ),
        correction(
            "A320-200.geometry_config.engine.fan_pressure_ratio",
            1.4,
            1.6,
        ),
        correction(
            "A320-200.geometry_config.engine.nacelle_profile[1][0]",
            0.288,
            0.264,
        ),
        correction(
            "A320-200.geometry_config.engine.nacelle_profile[2][0]",
            0.54,
            0.495,
        ),
        correction(
            "A320-200.geometry_config.engine.nacelle_profile[3][0]",
            1.98,
            1.815,
        ),
        correction(
            "A320-200.geometry_config.engine.nacelle_profile[4][0]",
            2.7,
            2.475,
        ),
        correction(
            "A320-200.geometry_config.engine.nacelle_profile[5][0]",
            3.6,
            3.3,
        ),
        correction(
            "A320-200.geometry_config.engine.overall_pressure_ratio",
            40.0,
            32.6,
        ),
        correction("A320-200.geometry_config.engine.radius_scale_m", 1.15, 1.0),
        correction("A320-200.geometry_config.engine.thrust_kn", 120.64, 120.1),
        correction(
            "A320-200.geometry_config.engine.turbine_inlet_temp_k",
            1650.0,
            1600.0,
        ),
        correction(
            "A340-300.engine.cruise_thrust_kn",
            36.647_778_131_002_63,
            34.648_808_414_766_13,
        ),
        correction("A340-300.engine.thrust_kn", 151.0, 144.56),
        correction(
            "A340-300.geometry_config.engine.engine_name",
            "CFM56-5C",
            "CFM56-5C3/F",
        ),
        correction("A340-300.geometry_config.engine.thrust_kn", 151.0, 144.56),
        correction("A340-300.mtow_kg", 275_000.0, 260_000.0),
        // The registered A340-300 cabin is the published planning cabin of
        // 335 seats [S Airbus A340 ACAP], replacing the 290 seats of the
        // frozen table. The fixture holds only the A320 (150 seats, unchanged)
        // and the A340, so the A380 (555), A220 (140) and DC-10 (255) planning
        // seeds have no leaf here; the alas-config preset ledger pins them.
        correction("A340-300.requirements.num_passengers", 290.0, 335.0),
    ]
    .into_iter()
    .collect()
}

fn correction(
    path: &'static str,
    upstream: impl Into<Value>,
    corrected: impl Into<Value>,
) -> (&'static str, RequestCorrection) {
    (
        path,
        RequestCorrection {
            upstream: upstream.into(),
            corrected: corrected.into(),
        },
    )
}
