// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The OEW reference record of the registered military transport.

use super::sources::unknown;
use super::{
    OewApplicability, OewReference, OewReferenceConfiguration, OewSource, OewSourceTier,
    PublishedOewValue,
};

const BUNDESWEHR_A400M: OewSource = OewSource {
    document: "A400M transport aircraft (German Air Force operator page)",
    publisher: "Bundeswehr",
    revision: "web page, undated",
    date: "",
    locator: "technical data: empty weight",
    url: "https://www.bundeswehr.de/en/organization/german-air-force/a400m",
    local_path: "",
    retrieved: "2026-10-04",
    quote: "EMPTY WEIGHT: 78,6 t",
    tier: OewSourceTier::OperatorRecord,
};

pub(super) static A400M: OewReference = OewReference {
    preset: "A400M",
    applicability: OewApplicability::ConditionalMismatch,
    reference_oew_kg: Some(78_600.0),
    definition_label: "empty weight, operator page; manufacturer empty weight, basic empty weight and operating empty weight are not distinguished",
    reference_configuration: OewReferenceConfiguration {
        model: "A400M (military standard)",
        weight_variant: "141,000 kg MTOW military (Airbus brochure TMMA0026/01/2025 p024)",
        mtow_kg: Some(141_000.0),
        engine: "TP400-D6",
        modification_state: "series production military standard, role equipment not stated",
        cabin: "military cargo hold, no passenger seats",
    },
    differences_from_preset: &[
        "the operator page does not say whether 78.6 t is a manufacturer empty weight or an operating empty weight, and states no inclusion list",
        "military role equipment (refuelling, defensive aids, loading systems) is not itemised and is not modelled",
        "older secondary sources give 76,500 kg",
    ],
    inclusion: unknown(),
    source: Some(BUNDESWEHR_A400M),
    uncertainty_kg: Some(2_100.0),
    case_anchor: None,
    other_published_values: &[PublishedOewValue {
        label: "EASA TCDS A.169 minimum flight weight (civil WV001)",
        value_kg: 78_000.0,
        is_operating_empty: false,
        source: OewSource {
            document: "EASA Type-Certificate Data Sheet EASA.A.169 Airbus A400M",
            publisher: "EASA",
            revision: "Issue 07",
            date: "2025-11-28",
            locator: "III.13 weight variant WV001, minimum weight",
            url: "https://www.easa.europa.eu/en/downloads/7276/en",
            local_path: "",
            retrieved: "2026-10-04",
            quote: "Minimum weight 78 000 kg",
            tier: OewSourceTier::CertificationDocument,
        },
        note: "a minimum flight weight of the civil type design, not an empty mass",
    }],
    structural_payload_basis_oew_kg: None,
    notes: "Single operator-page value with no stated definition; the uncertainty is the 2,100 kg spread to the older 76,500 kg secondary figure. It is a visible conditional comparison only and enters no validation metric.",
};
