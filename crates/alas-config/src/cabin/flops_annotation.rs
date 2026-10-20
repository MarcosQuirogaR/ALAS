// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Provenance bookkeeping for a FLOPS transport whose class counts were
//! resolved from the canonical cabin.

use super::ResolvedPassengerCounts;
use crate::{FlopsInputEvidence, MassModelConfig};

/// Mark a FLOPS transport clone when its class-count mirror has been resolved
/// from the canonical cabin rather than copied from the serialized seed.
///
/// The transport node retains the source document and revision for the seed,
/// but a changed numeric split must not continue to look source-backed in an
/// evaluated report. There is no fourth Derived evidence enum in the saved
/// schema, so the existing auditable UserDeclared category is used together
/// with an explicit applicability/uncertainty note. The comparison is made
/// before the caller writes the new counts; calling this helper twice is
/// idempotent.
pub fn annotate_flops_cabin_resolution(
    mass_model: &mut MassModelConfig,
    counts: ResolvedPassengerCounts,
) {
    let current = [
        mass_model
            .flops_transport
            .first_class_passenger_count
            .and_then(|value| i64::try_from(value).ok()),
        mass_model
            .flops_transport
            .business_class_passenger_count
            .and_then(|value| i64::try_from(value).ok()),
        mass_model
            .flops_transport
            .tourist_class_passenger_count
            .and_then(|value| i64::try_from(value).ok()),
    ];
    let resolved = [
        Some(counts.first),
        Some(counts.business),
        Some(counts.tourist),
    ];
    if current == resolved {
        return;
    }

    let note = format!(
        "canonical cabin resolution for this evaluation uses FLOPS class counts First={}, Business={}, Tourist={}",
        counts.first, counts.business, counts.tourist
    );
    let provenance = &mut mass_model.flops_transport.provenance.cabin;
    provenance.applicability = replace_provenance_note(
        &provenance.applicability,
        "canonical cabin resolution for this evaluation uses FLOPS class counts",
        &note,
    );
    if !provenance
        .uncertainty
        .contains("serialized FLOPS class-count seed was superseded")
    {
        provenance.uncertainty = append_provenance_note(
            &provenance.uncertainty,
            "the serialized FLOPS class-count seed was superseded by the canonical cabin for this load case",
        );
    }
    provenance.evidence = FlopsInputEvidence::UserDeclared;
}

fn replace_provenance_note(existing: &str, marker: &str, note: &str) -> String {
    let existing = existing.trim().trim_end_matches('.');
    if let Some(index) = existing.find(marker) {
        let prefix = existing[..index].trim().trim_end_matches(';').trim();
        if prefix.is_empty() {
            return note.to_owned();
        }
        return format!("{prefix}; {note}");
    }
    append_provenance_note(existing, note)
}

fn append_provenance_note(existing: &str, note: &str) -> String {
    let existing = existing.trim().trim_end_matches('.');
    if existing.is_empty() {
        note.to_owned()
    } else {
        format!("{existing}; {note}")
    }
}
