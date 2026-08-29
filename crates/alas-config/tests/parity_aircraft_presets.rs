// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares the aircraft preset registry against the one the reference
//! implementation registers.
//!
//! Every preset field and registration order is checked at `exact`. An
//! explicit two-sided ledger pins independently audited corrections while all
//! other translated fields compare directly with Python. Numbers compare by
//! value so an upstream integer and an equivalent Rust float agree.

// This file is itself a test binary, so an unwrap that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::{
    presets, AircraftPreset, DesignMissionEvidence, MissingDesignMissionDatum,
    PartialMissionEvidenceKind, PublishedRange,
};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

const SOURCE_CORRECTION_COUNT: usize = 51;
const DC_10_UPSTREAM_DISPLAY_NAME: &str = "McDonnell Douglas DC-10";
const DC_10_CORRECTED_DISPLAY_NAME: &str = "McDonnell Douglas DC-10-30 (572k option)";

struct SourceCorrection {
    upstream: Value,
    corrected: Value,
}

#[derive(Deserialize)]
struct Fixture {
    presets: Vec<Value>,
    available: Vec<String>,
    display_names: std::collections::BTreeMap<String, String>,
}

fn fixture() -> Fixture {
    alas_testkit::load("config", "aircraft_presets")
}

#[test]
fn every_aircraft_preset_matches_the_reference() {
    let fixture = fixture();
    let mut comparison = Comparison::new("alas-config::presets", Tier::Exact);
    let mut source_corrections = source_corrections();
    comparison.exact(
        "source-correction ledger size",
        &source_corrections.len(),
        &SOURCE_CORRECTION_COUNT,
    );

    let names: Vec<&str> = presets::available();
    let expected_names: Vec<&str> = fixture.available.iter().map(String::as_str).collect();
    comparison.exact("registration order", &names, &expected_names);
    if names != expected_names {
        comparison.finish();
        return;
    }

    for (preset, expected) in presets::registry().iter().zip(&fixture.presets) {
        compare_values(
            &mut comparison,
            &mut source_corrections,
            preset.name,
            &as_value(preset),
            expected,
        );
    }
    let unvisited_corrections: Vec<String> = source_corrections.keys().cloned().collect();
    comparison.exact(
        "source-correction ledger paths not present in the fixture",
        &unvisited_corrections,
        &Vec::<String>::new(),
    );
    comparison.finish();
}

#[test]
fn every_product_preset_has_one_continuous_leading_edge_sweep() {
    for preset in presets::registry() {
        let planform = preset
            .geometry
            .wing
            .transport_planform(&preset.design_vector)
            .unwrap_or_else(|error| panic!("{}: {error}", preset.name));
        for panel in planform.panels() {
            assert!(
                (panel.leading_edge_sweep_deg - preset.design_vector.sweep_deg).abs() < 1.0e-10,
                "{} {:?}->{:?}: {} deg instead of {} deg",
                preset.name,
                panel.inboard.kind,
                panel.outboard.kind,
                panel.leading_edge_sweep_deg,
                preset.design_vector.sweep_deg
            );
        }
    }
}

#[test]
fn the_dropdown_labels_match_the_reference() {
    let fixture = fixture();
    let mut comparison = Comparison::new("alas-config::presets display names", Tier::Exact);
    for (name, display_name) in presets::display_names() {
        let upstream = fixture.display_names.get(name).map(String::as_str);
        if name == "DC-10" {
            comparison.exact(
                "DC-10 frozen dropdown label",
                &upstream,
                &Some(DC_10_UPSTREAM_DISPLAY_NAME),
            );
            comparison.exact(
                "DC-10 corrected dropdown label",
                &display_name,
                &DC_10_CORRECTED_DISPLAY_NAME,
            );
        } else {
            comparison.exact(name, &Some(display_name), &upstream);
        }
    }
    comparison.exact(
        "count",
        &presets::display_names().len(),
        &fixture.display_names.len(),
    );
    comparison.finish();
}

#[test]
fn source_corrected_presets_name_the_revision_locked_evidence() {
    let a340 = assert_source_record(
        "A340-300",
        [
            "A340-312",
            "WV029",
            "CFM56-5C3/F",
            "public WV029 planning baseline",
            "three tanks",
        ],
        &[
            "EASA.A.015 Issue 28, 2026-01-15, pp.32-36",
            "Airbus A340-200/-300 Aircraft Characteristics Rev 33, 2025-12-01, section 2-1-1",
        ],
    );
    assert_eq!(a340.reference.mtow_kg, Some(260_000.0));
    assert_eq!(a340.reference.mtow_kg, Some(a340.requirements.mtow_kg));

    let a380 = assert_source_record(
        "A380-800",
        [
            "A380-841",
            "WV000",
            "Trent 970-84",
            "WV000 public planning baseline",
            "323,546 L tanks + 793 L usable system inventory",
        ],
        &[
            "EASA.A.110 Issue 17, 2026-08-05, pp.10-15",
            "Airbus A380 Aircraft Characteristics Rev 20, 2025-12-01, section 2-1-1",
            "Airbus A380 Facts and Figures, February 2022, p.3",
        ],
    );
    assert_eq!(a380.reference.reference_wing_area_m2, Some(845.0));
    assert_eq!(
        a380.reference.reference_wing_area_m2,
        Some(a380.requirements.max_wing_area_m2)
    );
    assert!((preset_projected_area(a380) - a380.requirements.max_wing_area_m2).abs() < 1.0e-9);

    let b787 = assert_source_record(
        "B787-9",
        [
            "787-9",
            "legacy 561,500 lb MTOW",
            "GEnx-1B74/75 P2 family",
            "Boeing Rev Q legacy-weight planning baseline",
            "standard 33,399 US gal usable system",
        ],
        &[
            "Boeing 787 ACAP D6-58333 Rev Q, October 2025, section 2",
            "NASA/TP-20210023843, December 2022, Table I",
            "Boeing 787 ACAP D6-58333 Rev L, December 2015, p.2-3 (typical OEW only)",
        ],
    );
    assert_eq!(b787.reference.mtow_kg, Some(254_692.0));
    assert_eq!(b787.reference.mtow_kg, Some(b787.requirements.mtow_kg));
    assert_eq!(
        b787.requirements.max_structural_payload_kg,
        b787.reference.mzfw_kg.unwrap() - b787.reference.oew_kg.unwrap()
    );

    let a320 = assert_source_record(
        "A320-200",
        [
            "A320-214",
            "WV017",
            "CFM56-5B4/3",
            "MOD160500 sharklets; MOD37147 Tech Insertion",
            "three tanks; MOD37331 + MOD160001",
        ],
        &[
            "Airbus A320 Aircraft Characteristics Rev 46, 2026-07-01, section 2-1-1 p.2",
            "EASA.A.064 Issue 62, pp.37-48",
            "EASA.E.003 Issue 06, pp.10-11",
        ],
    );
    assert_eq!(a320.engine_name, a320.identity.engine_model);
    assert_eq!(a320.geometry.engine.engine_name, a320.engine_name);

    let a220 = assert_source_record(
        "A220-300",
        [
            "BD-500-1A11",
            "legacy 149,000 lb MTOW",
            "PW1521G-3",
            "S/N 55001-59999 planning configuration",
            "standard integral tanks",
        ],
        &[
            "Airbus A220 Aircraft Recovery Publication BD500-3AB48-10400-00, May 2026, J06-20-01 p.14 and J08-41-03-01 p.2",
            "Airbus A220 ARP J07-40-00-06AAA-030A-A, 2019-10-22 p.2",
        ],
    );
    assert_eq!(a220.reference.mtow_kg, Some(67_585.0));
    assert_eq!(a220.reference.mtow_kg, Some(a220.requirements.mtow_kg));
    assert_eq!(
        a220.requirements.max_structural_payload_kg,
        a220.reference.mzfw_kg.unwrap() - a220.reference.oew_kg.unwrap()
    );

    let dc_10 = assert_source_record(
        "DC-10",
        [
            "DC-10-30 passenger",
            "ACAP 572,000 lb option",
            "CF6-50C family",
            "DAC-67803A Rev A footnoted 572k planning option",
            "36,652 US gal with center-wing auxiliary tank",
        ],
        &[
            "Boeing DC/MD-10 ACAP DAC-67803A Rev A, Figure 2.1",
            "FAA TCDS A22WE Rev 13, 2018-04-30",
            "NASA CR-3119, April 1979",
        ],
    );
    assert_eq!(dc_10.reference.mtow_kg, Some(259_454.0));
    assert_eq!(dc_10.reference.mtow_kg, Some(dc_10.requirements.mtow_kg));
    assert_eq!(
        dc_10.requirements.max_structural_payload_kg,
        dc_10.reference.mzfw_kg.unwrap() - dc_10.reference.oew_kg.unwrap()
    );
    assert_eq!(
        dc_10.reference.reference_wing_area_m2,
        Some(dc_10.requirements.max_wing_area_m2)
    );
    assert!((preset_projected_area(dc_10) - dc_10.requirements.max_wing_area_m2).abs() < 1.0e-9);
}

#[test]
fn partial_mission_evidence_never_promotes_a_capability_claim_to_a_design_mission() {
    let expected_sources: [(&str, &[&str]); 7] = [
        ("AVE", &[]),
        (
            "A340-300",
            &["Airbus A340-200/-300 Aircraft Characteristics Rev 33, 2025-12-01, section 3-2-1 p.4, Figure 3-2-1-991-013-A01"],
        ),
        (
            "A380-800",
            &["Airbus A380 Aircraft Characteristics Rev 20, 2025-12-01, section 3-2-1 p.2, Figure 3-2-1-991-001-A01"],
        ),
        (
            "B787-9",
            &["Boeing 787 ACAP D6-58333 Rev Q, October 2025, section 3.2.2 p.3-3"],
        ),
        (
            "A320-200",
            &["Airbus A320 Aircraft Characteristics Rev 46, 2026-07-01, section 3-2-1 p.3, Figure 3-2-1-991-017-A01"],
        ),
        (
            "A220-300",
            &[
                "Airbus A220 Digital Pamphlet FAI V5.2, July 2022, p.1",
                "Airbus A220-300 APP Issue 031, 2023-10-19, data module BD500-A-J00-00-00-13AAB-030A-A pp.2-3, Figure 1",
            ],
        ),
        ("DC-10", &[]),
    ];

    for (name, expected) in expected_sources {
        let preset = presets::get(name).unwrap();
        assert_eq!(
            preset.reference.design_mission_evidence,
            DesignMissionEvidence::Unverified,
            "{name} must remain unverified"
        );
        assert_eq!(
            preset
                .reference
                .partial_design_mission_evidence
                .iter()
                .map(|record| record.source)
                .collect::<Vec<_>>(),
            expected,
            "{name} partial evidence sources"
        );
        for record in &preset.reference.partial_design_mission_evidence {
            assert_ne!(
                record.kind,
                PartialMissionEvidenceKind::ActualDesignMission,
                "{name} has no public actual-design-mission record"
            );
            assert_ne!(
                record.kind,
                PartialMissionEvidenceKind::CertificationDemonstration,
                "{name} has no public certification-demonstration mission"
            );
            assert!(
                record.payload_kg.is_none(),
                "{name} has no published mission payload"
            );
            assert!(
                record.missing.contains(&MissingDesignMissionDatum::Payload),
                "{name} must name its missing payload"
            );
            assert!(
                record.missing.contains(&MissingDesignMissionDatum::Profile),
                "{name} must name its incomplete profile"
            );
            assert!(
                record
                    .missing
                    .contains(&MissingDesignMissionDatum::ReserveFuel),
                "{name} must name its missing reserve mass"
            );
        }
    }

    let a220 = presets::get("A220-300").unwrap();
    assert_eq!(
        a220.reference.partial_design_mission_evidence[0].range,
        Some(PublishedRange::NauticalMiles(3_400.0))
    );
    assert_eq!(
        a220.reference.partial_design_mission_evidence[0].kind,
        PartialMissionEvidenceKind::AdvertisedRange
    );
    assert_eq!(
        a220.reference.partial_design_mission_evidence[1].kind,
        PartialMissionEvidenceKind::PayloadRangeChart
    );

    let a380 = presets::get("A380-800").unwrap();
    let chart = &a380.reference.partial_design_mission_evidence[0];
    assert_eq!(chart.kind, PartialMissionEvidenceKind::PayloadRangeChart);
    assert_eq!(
        chart.reserve_assumptions,
        Some("200 nm diversion; 5% trip fuel allowance; 30 min holding")
    );
    assert!(chart.missing.contains(&MissingDesignMissionDatum::Range));
    assert!(chart
        .missing
        .contains(&MissingDesignMissionDatum::ReserveFuel));
}

fn preset_projected_area(preset: &AircraftPreset) -> f64 {
    let semi_span = preset.design_vector.span_m / 2.0;
    let inner_span = semi_span * preset.geometry.wing.break_span_fraction;
    let outer_span = semi_span - inner_span;
    2.0 * (inner_span * (preset.design_vector.root_chord_m + preset.design_vector.break_chord_m)
        / 2.0
        + outer_span * (preset.design_vector.break_chord_m + preset.design_vector.tip_chord_m)
            / 2.0)
}

/// One preset in the shape the fixture records it.
///
/// The two calibrations are `null` upstream when the preset does not carry
/// one, and `None` here; both serialize to the same absence, which is what
/// makes "this type uses the global default" comparable at all.
fn as_value(preset: &AircraftPreset) -> Value {
    serde_json::json!({
        "name": preset.name,
        "display_name": preset.display_name,
        "description": preset.description,
        "engine_name": preset.engine_name,
        "n_engines": preset.n_engines,
        "design_vector": preset.design_vector,
        "geometry": preset.geometry,
        "requirements": preset.requirements,
        "mass_model": preset.mass_model,
        "performance": preset.performance,
        "engine_spanwise_positions": preset.engine_spanwise_positions(),
    })
}

/// The fields for which the source audit supersedes the frozen Python table.
///
/// Pinning both sides prevents this exception from accepting a different Rust
/// value or hiding a later, unrelated fixture change at the same path.
fn source_corrections() -> BTreeMap<String, SourceCorrection> {
    let mut corrections = [
        source_correction("A340-300.requirements.cabin_preset", "Ryanair", "Custom"),
        source_correction("A380-800.requirements.cabin_preset", "Ryanair", "Custom"),
        source_correction("B787-9.requirements.cabin_preset", "Ryanair", "Custom"),
        source_correction("A320-200.requirements.cabin_preset", "Ryanair", "Custom"),
        source_correction("A220-300.requirements.cabin_preset", "Ryanair", "Custom"),
        source_correction("DC-10.requirements.cabin_preset", "Ryanair", "Custom"),
        source_correction(
            "A340-300.description",
            "Long-range quad-engine widebody with CFM56-5C engines.",
            "Airbus A340-312 WV029 with CFM56-5C3-family engines.",
        ),
        source_correction("A340-300.design_vector.fuselage_length_m", 63.69, 63.66),
        source_correction("A340-300.engine_name", "CFM56-5C", "CFM56-5C3/F"),
        source_correction(
            "A340-300.geometry.engine.engine_name",
            "CFM56-5C",
            "CFM56-5C3/F",
        ),
        source_correction("A340-300.requirements.mtow_kg", 275_000.0, 260_000.0),
        source_correction(
            "A380-800.description",
            "Double-deck super-jumbo with Trent 900 engines.",
            "Airbus A380-841 WV000 with Trent 970-84 engines.",
        ),
        source_correction("A380-800.design_vector.fuselage_length_m", 72.72, 72.73),
        source_correction(
            "A380-800.design_vector.root_chord_m",
            23.0,
            22.952_583_900_271_1,
        ),
        source_correction(
            "A380-800.design_vector.break_chord_m",
            11.3,
            11.276_704_264_046_2,
        ),
        source_correction(
            "A380-800.design_vector.tip_chord_m",
            3.5,
            3.492_784_506_562_99,
        ),
        source_correction("A380-800.engine_name", "Trent 900", "Trent 970-84"),
        source_correction(
            "A380-800.geometry.engine.engine_name",
            "Trent 900",
            "Trent 970-84",
        ),
        source_correction("A380-800.requirements.max_wing_area_m2", 855.0, 845.0),
        source_correction(
            "B787-9.requirements.max_structural_payload_kg",
            52_600.0,
            52_586.0,
        ),
        source_correction("B787-9.requirements.mtow_kg", 254_000.0, 254_692.0),
        source_correction(
            "A320-200.description",
            "Short/medium-range narrow-body twin with LEAP-1A engines.",
            "Airbus A320-214 WV017 with CFM56-5B4/3 engines and sharklets.",
        ),
        source_correction("A320-200.engine_name", "LEAP-1A", "CFM56-5B4/3"),
        source_correction(
            "A320-200.geometry.engine.engine_name",
            "LEAP-1A",
            "CFM56-5B4/3",
        ),
        source_correction(
            "A220-300.description",
            "Short/medium-range narrow-body twin with PW1500G geared turbofans.",
            "BD-500-1A11 legacy-weight A220-300 with PW1521G-3 engines.",
        ),
        source_correction(
            "A220-300.requirements.max_structural_payload_kg",
            18_700.0,
            18_643.0,
        ),
        source_correction("A220-300.requirements.mtow_kg", 70_900.0, 67_585.0),
        source_correction(
            "DC-10.description",
            "Classic long-range trijet widebody with underwing and tail-mounted CF6-50 engines.",
            "DC-10-30 ACAP 572,000 lb option with three CF6-50C engines.",
        ),
        source_correction("DC-10.design_vector.fuselage_length_m", 55.35, 55.55),
        source_correction("DC-10.design_vector.span_m", 50.41, 50.39),
        source_correction(
            "DC-10.design_vector.root_chord_m",
            12.8,
            12.798_762_957_481_8,
        ),
        source_correction(
            "DC-10.design_vector.break_chord_m",
            7.8,
            7.799_246_177_215_49,
        ),
        source_correction("DC-10.design_vector.tip_chord_m", 1.8, 1.799_826_040_895_88),
        source_correction(
            "DC-10.display_name",
            DC_10_UPSTREAM_DISPLAY_NAME,
            DC_10_CORRECTED_DISPLAY_NAME,
        ),
        source_correction(
            "DC-10.requirements.max_structural_payload_kg",
            48_000.0,
            46_008.0,
        ),
        source_correction("DC-10.requirements.max_wing_area_m2", 338.8, 338.84),
        source_correction("DC-10.requirements.mtow_kg", 259_450.0, 259_454.0),
    ]
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    add_transport_planform_corrections(&mut corrections);
    corrections
}

fn source_correction(
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

fn add_transport_planform_corrections(corrections: &mut BTreeMap<String, SourceCorrection>) {
    for (preset, kink_fraction) in [
        ("AVE", 0.35),
        ("A340-300", 0.362_094_754_983_253_8),
        ("A380-800", 0.359_236_516_064_625_5),
        ("B787-9", 0.353_771_245_388_011_8),
        ("A320-200", 0.377_380_002_280_241_9),
        ("A220-300", 0.382_736_255_076_680_5),
        ("DC-10", 0.35),
    ] {
        for (field, value) in [
            ("side_of_body_span_fraction", 0.10),
            ("kink_span_fraction", kink_fraction),
        ] {
            corrections.insert(
                format!("{preset}.geometry.wing.{field}"),
                SourceCorrection {
                    upstream: Value::String("absent upstream".to_owned()),
                    corrected: Value::from(value),
                },
            );
        }
    }
}

fn assert_source_record(
    name: &str,
    identity: [&str; 5],
    sources: &[&str],
) -> &'static AircraftPreset {
    let preset = presets::get(name).unwrap();
    assert_eq!(
        [
            preset.identity.model,
            preset.identity.weight_variant,
            preset.identity.engine_model,
            preset.identity.modification_state,
            preset.identity.tank_configuration,
        ],
        identity,
        "{name} variant identity"
    );
    assert_eq!(
        preset.reference.sources.as_slice(),
        sources,
        "{name} sources"
    );
    preset
}

/// Compare two trees, reporting each disagreeing key by its path rather than
/// dumping both.
///
/// Two numbers are compared by value, so an integer upstream and a float here
/// agree when they denote the same quantity.
fn compare_values(
    comparison: &mut Comparison,
    source_corrections: &mut BTreeMap<String, SourceCorrection>,
    path: &str,
    actual: &Value,
    expected: &Value,
) {
    if let Some(correction) = source_corrections.remove(path) {
        compare_recorded_value(
            comparison,
            &format!("{path} frozen Python value"),
            expected,
            &correction.upstream,
        );
        compare_recorded_value(
            comparison,
            &format!("{path} source-backed Rust value"),
            actual,
            &correction.corrected,
        );
        return;
    }

    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            for (key, expected_value) in expected {
                let child = format!("{path}.{key}");
                match actual.get(key) {
                    Some(actual_value) => {
                        compare_values(
                            comparison,
                            source_corrections,
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
                if key == "optimize_passenger_capacity" && path.ends_with(".requirements") {
                    continue;
                }
                if !expected.contains_key(key) {
                    let child = format!("{path}.{key}");
                    if let Some(correction) = source_corrections.remove(child.as_str()) {
                        compare_recorded_value(
                            comparison,
                            &format!("{child} frozen Python value"),
                            &Value::String("absent upstream".to_owned()),
                            &correction.upstream,
                        );
                        compare_recorded_value(
                            comparison,
                            &format!("{child} source-corrected Rust value"),
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
                    source_corrections,
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

fn compare_recorded_value(
    comparison: &mut Comparison,
    path: &str,
    actual: &Value,
    expected: &Value,
) {
    match (actual, expected) {
        (Value::Number(actual), Value::Number(expected)) => {
            comparison.exact(path, &actual.as_f64(), &expected.as_f64());
        }
        (actual, expected) => {
            comparison.exact(path, actual, expected);
        }
    }
}
