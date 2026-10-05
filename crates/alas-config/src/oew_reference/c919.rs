// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The C919 OEW reference record.
//!
//! COMAC publishes no weight-and-balance or airport-planning document that
//! this project could retrieve, and the CAAC type-certificate data sheet is
//! not public. The only operating empty weight in circulation is the
//! 45,700 kg printed in the Wikipedia C919 specification table (English and
//! Chinese editions), whose weight rows cite COMAC's 2023-01-11 delivery
//! release. It is a secondary value with no stated inclusion list, so the
//! record is a visible conditional comparison, never a validation metric.

use super::sources::unknown;
use super::{OewApplicability, OewReference, OewReferenceConfiguration, OewSource, OewSourceTier};

const C919_SPECIFICATION_TABLE: OewSource = OewSource {
    document: "Comac C919, Specifications table (C919-100 STD and ER columns)",
    publisher: "Wikipedia (secondary compilation citing COMAC)",
    revision: "live page, English and Chinese editions",
    date: "2026-10-05",
    locator: "Specifications: OEW 45,700 kg, maximum payload 18,900 kg, MTOW 75,100 kg (STD) / 78,900 kg (ER), MLW 67,800 kg, maximum fuel 19,560 kg",
    url: "https://en.wikipedia.org/wiki/Comac_C919",
    local_path: "",
    retrieved: "2026-10-05",
    quote: "OEW 45,700 kg",
    tier: OewSourceTier::Aggregator,
};

/// The secondary 45,700 kg operating empty weight of the C919-100 STD.
pub(super) const C919_RECORD: OewReference = OewReference {
    preset: "C919",
    applicability: OewApplicability::ConditionalMismatch,
    reference_oew_kg: Some(45_700.0),
    definition_label: "Operating empty weight printed in the Wikipedia C919 specification table (secondary; inclusion list not stated)",
    reference_configuration: OewReferenceConfiguration {
        model: "COMAC C919-100 STD",
        weight_variant: "75,100 kg MTOW; MLW 67,800 kg",
        mtow_kg: Some(75_100.0),
        engine: "LEAP-1C28",
        modification_state: "production standard-range aircraft; modification state not stated",
        cabin: "158 seats, 8 business + 150 economy (the table's lower seat figure); layout and pitch not stated",
    },
    differences_from_preset: &[
        "the value has no primary document: COMAC and CAAC publish no weight statement retained here, and the cited COMAC release is a delivery news item",
        "the inclusion list (crew, catering, water, unusable fuel, oil) is not stated, so the model's operating-items definition may differ",
        "the 45,700 kg is stated for both the STD and ER columns of the table although their MTOWs differ, and it cannot be cross-checked against a second source",
    ],
    inclusion: unknown(),
    source: Some(C919_SPECIFICATION_TABLE),
    uncertainty_kg: Some(2_000.0),
    case_anchor: None,
    other_published_values: &[],
    structural_payload_basis_oew_kg: Some(45_700.0),
    notes: "Secondary source only (Wikipedia, retrieved 2026-10-05). Maximum payload 18,900 kg from the same table gives MZFW 64,600 kg by the manufacturer-style definition MZFW = OEW + maximum payload; that MZFW is derived, not published. Excluded from validation metrics because the inclusion list is not stated.",
};
