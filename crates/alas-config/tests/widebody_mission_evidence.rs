// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Keeps the four widebody mission-source gaps explicit.
//!
//! Aircraft-characteristics manuals are useful primary evidence for weights,
//! geometry, and capability charts, but a chart is not a selected design
//! mission. This test prevents a future source update from silently turning a
//! range axis, a generic profile caption, or an unqualified reserve note into
//! the complete range/payload/profile/reserve contract.

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use alas_config::{
    presets, DesignMissionEvidence, MissingDesignMissionDatum, MissionEvidenceApplicability,
    PartialMissionEvidenceKind, PublishedRange, PublishedReserveContract,
};

const WIDEBODIES: [&str; 4] = ["A340-300", "A380-800", "B787-9", "DC-10"];

#[test]
fn widebody_sources_remain_unverified_until_all_four_mission_fields_are_published() {
    for name in WIDEBODIES {
        let preset = presets::get(name).expect("widebody preset must be registered");
        assert_eq!(
            preset.reference.design_mission_evidence,
            DesignMissionEvidence::Unverified,
            "{name} must not claim a source-backed design mission"
        );

        for evidence in &preset.reference.partial_design_mission_evidence {
            assert_ne!(
                evidence.kind,
                PartialMissionEvidenceKind::ActualDesignMission,
                "{name} has no actual design mission in the reviewed source"
            );
            assert_ne!(
                evidence.kind,
                PartialMissionEvidenceKind::CertificationDemonstration,
                "{name} has no certification demonstration mission in the reviewed source"
            );
            assert!(
                evidence.range.is_none(),
                "{name} has no source-selected range point"
            );
            assert!(
                evidence.payload_kg.is_none(),
                "{name} has no payload attached to a selected range point"
            );
            for missing in [
                MissingDesignMissionDatum::Range,
                MissingDesignMissionDatum::Payload,
                MissingDesignMissionDatum::Profile,
                MissingDesignMissionDatum::ReserveFuel,
            ] {
                assert!(
                    evidence.missing.contains(&missing),
                    "{name} must retain the missing {missing:?} datum"
                );
            }
        }
    }

    assert!(
        presets::get("DC-10")
            .expect("DC-10 preset must be registered")
            .reference
            .partial_design_mission_evidence
            .is_empty(),
        "the public DC-10 source index does not provide a mission page"
    );
}

#[test]
fn widebody_partial_records_keep_variant_applicability_and_source_identity() {
    let expected = [
        (
            "A340-300",
            "A340-312",
            "WV029",
            "Airbus A340-200/-300 Aircraft Characteristics Rev 33, 2025-12-01, section 3-2-1 p.4, Figure 3-2-1-991-013-A01",
            "A340-300 CFM56-5C3 chart matches the model and engine family but selects no weight-variant design point",
        ),
        (
            "A380-800",
            "A380-841",
            "WV000",
            "Airbus A380 Aircraft Characteristics Rev 20, 2025-12-01, section 3-2-1 p.2, Figure 3-2-1-991-001-A01",
            "A380-800 Trent 900 chart matches the model and engine family but selects no weight-variant design point",
        ),
        (
            "B787-9",
            "787-9",
            "legacy 561,500 lb MTOW",
            "Boeing 787 ACAP D6-58333 Rev Q, October 2025, section 3.2.2 p.3-3",
            "787-9 chart is model-level planning evidence and selects no legacy-weight design point",
        ),
    ];

    for (name, model, weight_variant, source, applicability) in expected {
        let preset = presets::get(name).expect("widebody preset must be registered");
        assert_eq!(preset.identity.model, model);
        assert_eq!(preset.identity.weight_variant, weight_variant);
        let record = preset
            .reference
            .partial_design_mission_evidence
            .first()
            .expect("reviewed chart must remain represented");
        assert_eq!(record.kind, PartialMissionEvidenceKind::PayloadRangeChart);
        assert_eq!(record.source, source);
        assert_eq!(record.applicability, applicability);
    }

    let a340 = presets::get("A340-300").expect("A340-300 preset must be registered");
    assert_eq!(
        a340.reference.partial_design_mission_evidence[0].configuration_applicability,
        MissionEvidenceApplicability::ModelAndEngineFamily
    );

    let a380 = presets::get("A380-800").expect("A380-800 preset must be registered");
    let a380_chart = &a380.reference.partial_design_mission_evidence[0];
    assert_eq!(
        a380_chart.configuration_applicability,
        MissionEvidenceApplicability::ModelAndEngineFamily
    );
    assert_eq!(
        a380_chart.reserve_contract,
        Some(PublishedReserveContract {
            diversion_range: Some(PublishedRange::NauticalMiles(200.0)),
            trip_fuel_allowance_fraction: Some(0.05),
            holding_time_minutes: Some(30.0),
        })
    );
    assert!(
        a380_chart
            .missing
            .contains(&MissingDesignMissionDatum::ReserveFuel),
        "published reserve policy must not be converted into a reserve mass"
    );

    let b787 = presets::get("B787-9").expect("B787-9 preset must be registered");
    assert_eq!(
        b787.reference.partial_design_mission_evidence[0].configuration_applicability,
        MissionEvidenceApplicability::ModelOnly
    );
}
