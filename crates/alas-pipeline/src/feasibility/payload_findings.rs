// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Findings about the payload the layout placed: seats, baggage holds and
//! net cargo.

use alas_payload::layout::LayoutSummary;

use super::{error, warning, FindingCode, PhysicalFinding};
use crate::full_analysis::AnalysisReport;

/// Append the payload-layout findings of `report`.
///
/// - Passengers left without seats are an error.
/// - Baggage above every modelled hold compartment is a warning carrying the
///   overload mass, kg: the excess stays in the payload mass, so it is a
///   modelling limit of the hold, not a dropped load.
/// - Net cargo below the request is an error.
pub(super) fn append_payload_findings(
    report: &AnalysisReport,
    findings: &mut Vec<PhysicalFinding>,
) {
    match report.payload_layout.as_ref().map(|layout| &layout.summary) {
        Some(LayoutSummary::Passenger(summary)) => {
            if summary.unseated_pax > 0 {
                findings.push(error(
                    FindingCode::PassengerCapacityShortfall,
                    format!(
                        "passenger payload leaves {} requested passengers without seats",
                        summary.unseated_pax
                    ),
                    Some(summary.seated_pax as f64),
                    Some(summary.total_pax as f64),
                    "passengers",
                ));
            }
            if summary.overload_kg.is_finite() && summary.overload_kg > 0.0 {
                findings.push(warning(
                    FindingCode::BaggageOverload,
                    "Baggage exceeds the modelled hold compartments",
                    Some(summary.overload_kg),
                    Some(0.0),
                    "kg",
                ));
            }
        }
        Some(LayoutSummary::Cargo(summary)) => {
            let requested_net_kg = summary.requested_net_payload_t * 1_000.0;
            let loaded_net_kg = summary.loaded_net_payload_t * 1_000.0;
            if requested_net_kg.is_finite()
                && loaded_net_kg.is_finite()
                && loaded_net_kg + 1.0e-6 < requested_net_kg
            {
                findings.push(error(
                    FindingCode::CargoCapacityShortfall,
                    format!(
                        "cargo layout delivers {loaded_net_kg:.1} kg net against \
                         {requested_net_kg:.1} kg requested"
                    ),
                    Some(loaded_net_kg),
                    Some(requested_net_kg),
                    "kg net",
                ));
            }
        }
        _ => {}
    }
}
