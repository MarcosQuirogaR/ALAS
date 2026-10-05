// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The E195-E2 OEW reference record.
//!
//! Embraer publishes no operating empty weight for the E195-E2 in the
//! documents retained here. Its definition of maximum payload (E-Jets E2
//! Airport Planning Manual APM 5824 Rev 8, section 2.1.1: "the difference
//! between the MZFW and the BOW") gives a basic operating weight from two
//! published numbers, and the same manual's E190-E2 table confirms the
//! identity: MZFW 46,700 kg - maximum payload 13,700 kg = BOW 33,000 kg.

use super::sources::unknown;
use super::{
    OewApplicability, OewReference, OewReferenceConfiguration, OewSource, OewSourceTier,
    PublishedOewValue,
};

const E195_E2_SPEC_SHEET: OewSource = OewSource {
    document: "E195-E2 specification sheet",
    publisher: "Embraer",
    revision: "April 2025",
    date: "2025-04",
    locator: "page 1, weights: maximum takeoff 62,500 kg, maximum landing 54,000 kg, maximum payload 16,150 kg, maximum usable fuel 13,690 kg",
    url: "https://www.embraer.com/media/ue1bdfnq/e195-e2-spec-1.pdf",
    local_path: "",
    retrieved: "2026-10-04",
    quote: "Maximum Payload 16,150 kg 35,604 lb",
    tier: OewSourceTier::ManufacturerPlanningDocument,
};

const ERJ_190_400_TCDS: OewSource = OewSource {
    document: "TCDS EASA.IM.A.071 Embraer ERJ-190, Section 5 (ERJ 190-400)",
    publisher: "EASA",
    revision: "Issue 28",
    date: "2026-09-03",
    locator: "Section 5 item III.13 Maximum Certified Weights, p.40",
    url: "https://www.easa.europa.eu/en/downloads/7382/en",
    local_path: "",
    retrieved: "2026-10-04",
    quote: "Zero Fuel 114309 lb 51850 kg",
    tier: OewSourceTier::CertificationDocument,
};

/// MZFW 51,850 kg less the 16,150 kg maximum payload.
pub(super) const E195_E2_RECORD: OewReference = OewReference {
    preset: "E195-E2",
    applicability: OewApplicability::ConditionalMismatch,
    reference_oew_kg: Some(35_700.0),
    definition_label: "Basic operating weight implied by Embraer's maximum-payload definition: MZFW (EASA TCDS) less maximum payload (Embraer specification sheet)",
    reference_configuration: OewReferenceConfiguration {
        model: "ERJ 190-400 (E195-E2)",
        weight_variant: "62,500 kg MTOW; MLW 54,000 kg; MZFW 51,850 kg",
        mtow_kg: Some(62_500.0),
        engine: "PW1921G / PW1923G / PW1923G-A (TCDS); registered as PW1923G",
        modification_state: "TCDS Issue 28 planning configuration",
        cabin: "typical standard configuration, layout not stated",
    },
    differences_from_preset: &[
        "the value is derived from two documents, not printed in either; the E190-E2 table of the same planning manual obeys the identity exactly (46,700 - 13,700 = 33,000 kg)",
        "the BOW definition is read from the E190-E2 APM (structure, power plant, systems, furnishings, unusable fuel, oil, hydraulic and toilet fluid, potable water, crew and baggage, catering and removable galley equipment; usable fuel and payload excluded) and is not restated for the E195-E2",
        "the preset's planning cabin is 132 single-class seats at 31 in; the cabin behind the Embraer BOW is not stated",
    ],
    inclusion: unknown(),
    source: Some(E195_E2_SPEC_SHEET),
    uncertainty_kg: Some(500.0),
    case_anchor: None,
    other_published_values: &[PublishedOewValue {
        label: "Maximum zero-fuel weight (not an OEW; one half of the derivation)",
        value_kg: 51_850.0,
        is_operating_empty: false,
        source: ERJ_190_400_TCDS,
        note: "the certified MZFW from which the 16,150 kg maximum payload is subtracted",
    }],
    structural_payload_basis_oew_kg: Some(35_700.0),
    notes: "MZFW source: EASA.IM.A.071 Issue 28 Section 5 III.13 (51,850 kg), retrieved 2026-10-04; maximum payload source: Embraer E195-E2 specification sheet April 2025. Excluded from validation metrics because the inclusion list is not stated for the E195-E2.",
};
