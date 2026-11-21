// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Learjet 60 OEW reference record.
//!
//! Jane's prints a basic operating weight for the Learjet 60 without stating
//! what it includes, so the record is a visible conditional comparison, never a
//! validation metric.

use super::sources::unknown;
use super::{OewApplicability, OewReference, OewReferenceConfiguration, OewSource, OewSourceTier};

const JANES_LEARJET_60: OewSource = OewSource {
    document: "Jane's All the World's Aircraft 2008-2009, Learjet 60",
    publisher: "IHS Jane's",
    revision: "2008-2009 edition",
    date: "2008",
    locator: "pp 825-826, weights and loadings, column A (Learjet 60): weight empty 6,364 kg, basic operating weight empty 6,700 kg, max payload 1,011 kg",
    url: "",
    local_path: "",
    retrieved: "2026-11-05",
    quote: "Basic operating weight empty: A 6,700 kg (14,772 lb)",
    tier: OewSourceTier::Aggregator,
};

/// The Learjet 60 basic operating weight as Jane's prints it.
pub(super) const LJ60_RECORD: OewReference = OewReference {
    preset: "LJ60",
    applicability: OewApplicability::ConditionalMismatch,
    reference_oew_kg: Some(6_700.0),
    definition_label: "Basic operating weight empty printed by Jane's (inclusion list not stated)",
    reference_configuration: OewReferenceConfiguration {
        model: "Learjet Model 60",
        weight_variant: "23,500 lb MTOW, 17,000 lb MZFW",
        mtow_kg: Some(10_659.0),
        engine: "PW305A",
        modification_state: "Learjet 60 as listed by Jane's; modification state not stated",
        cabin: "cabin furnishing not stated",
    },
    differences_from_preset: &[
        "the inclusion list (crew, catering, water, unusable fuel, oil) is not stated",
        "the value is a compiled handbook figure, not a weighed aircraft or a Bombardier planning document",
    ],
    inclusion: unknown(),
    source: Some(JANES_LEARJET_60),
    uncertainty_kg: Some(150.0),
    case_anchor: None,
    other_published_values: &[],
    structural_payload_basis_oew_kg: Some(6_700.0),
    notes: "MZFW 7,711 kg less this value gives the 1,011 kg maximum payload the same table prints.",
};
