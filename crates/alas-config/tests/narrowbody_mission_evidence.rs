// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Keeps the two narrowbody mission evidence gaps explicit.
//!
//! These tests are intentionally about provenance, not aircraft performance.
//! A manufacturer capability range is not a design mission unless the source
//! also selects payload, defines the flown profile, and states the required
//! reserve for the exact preset variant.

use alas_config::{
    presets, DesignMissionEvidence, MissingDesignMissionDatum, MissionEvidenceApplicability,
    PartialMissionEvidenceKind, PublishedMissionLoadCase, PublishedRange,
};

#[test]
fn the_a320_wv017_chart_does_not_define_a_complete_design_mission() {
    let Ok(preset) = presets::get("A320-200") else {
        panic!("the A320-200 preset must be registered");
    };

    assert_eq!(
        preset.reference.design_mission_evidence,
        DesignMissionEvidence::Unverified
    );
    let [record] = preset.reference.partial_design_mission_evidence.as_slice() else {
        panic!("the A320-200 evidence audit must have one chart record");
    };
    assert_eq!(record.kind, PartialMissionEvidenceKind::PayloadRangeChart);
    assert_eq!(record.range, None);
    assert_eq!(record.payload_kg, None);
    assert_eq!(
        record.load_case,
        Some(PublishedMissionLoadCase::TakeoffMassesKg(vec![
            73_500.0, 78_000.0,
        ]))
    );
    assert_eq!(record.profile_assumptions, Some("ISA conditions only"));
    assert_eq!(record.reserve_assumptions, None);
    assert_eq!(record.reserve_contract, None);
    assert_eq!(
        record.configuration_applicability,
        MissionEvidenceApplicability::ModelOnly
    );
    assert_eq!(
        record.missing,
        vec![
            MissingDesignMissionDatum::Range,
            MissingDesignMissionDatum::Payload,
            MissingDesignMissionDatum::Profile,
            MissingDesignMissionDatum::ReserveFuel,
        ]
    );
    assert!(record.applicability.contains("informational"));
    assert!(record.applicability.contains("no WV017 design point"));
}

#[test]
fn the_legacy_a220_evidence_does_not_define_a_complete_design_mission() {
    let Ok(preset) = presets::get("A220-300") else {
        panic!("the A220-300 preset must be registered");
    };

    assert_eq!(
        preset.reference.design_mission_evidence,
        DesignMissionEvidence::Unverified
    );
    let records = preset.reference.partial_design_mission_evidence.as_slice();
    assert_eq!(records.len(), 2);

    let advertised = &records[0];
    assert_eq!(advertised.kind, PartialMissionEvidenceKind::AdvertisedRange);
    assert_eq!(
        advertised.range,
        Some(PublishedRange::NauticalMiles(3_400.0))
    );
    assert_eq!(advertised.payload_kg, None);
    assert_eq!(
        advertised.load_case,
        Some(PublishedMissionLoadCase::TakeoffMassesKg(vec![70_900.0]))
    );
    assert_eq!(advertised.profile_assumptions, None);
    assert_eq!(advertised.reserve_assumptions, None);
    assert_eq!(advertised.reserve_contract, None);
    assert_eq!(
        advertised.configuration_applicability,
        MissionEvidenceApplicability::DifferentWeightVariant
    );
    assert_eq!(
        advertised.missing,
        vec![
            MissingDesignMissionDatum::Payload,
            MissingDesignMissionDatum::Profile,
            MissingDesignMissionDatum::ReserveFuel,
        ]
    );
    assert!(advertised.applicability.contains("67,585 kg"));

    let chart = &records[1];
    assert_eq!(chart.kind, PartialMissionEvidenceKind::PayloadRangeChart);
    assert_eq!(chart.range, None);
    assert_eq!(chart.payload_kg, None);
    assert_eq!(
        chart.load_case,
        Some(PublishedMissionLoadCase::ZeroFuelWeightRangeEnvelope)
    );
    assert_eq!(chart.profile_assumptions, Some("ISA conditions only"));
    assert_eq!(chart.reserve_assumptions, None);
    assert_eq!(chart.reserve_contract, None);
    assert_eq!(
        chart.configuration_applicability,
        MissionEvidenceApplicability::ExactPreset
    );
    assert_eq!(
        chart.missing,
        vec![
            MissingDesignMissionDatum::Range,
            MissingDesignMissionDatum::Payload,
            MissingDesignMissionDatum::Profile,
            MissingDesignMissionDatum::ReserveFuel,
        ]
    );
    assert!(chart.applicability.contains("superseded"));
    assert!(chart
        .applicability
        .contains("no legacy-weight design point"));
}
