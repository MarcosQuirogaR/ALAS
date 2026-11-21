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
/// - A galley or lavatory the section is too narrow to place is a warning, one
///   per monument, carrying the placed and the provisioned count.
/// - Net cargo below the request is an error, unless the deck was held to
///   the structural payload limit: then the offload is a warning.
pub(super) fn append_payload_findings(
    report: &AnalysisReport,
    findings: &mut Vec<PhysicalFinding>,
) {
    if let Some(balance) = report
        .payload_layout
        .as_ref()
        .and_then(|layout| layout.balance.as_ref())
    {
        if balance.saturated || !balance.settled {
            let why = if balance.saturated {
                "the movable payload reached the end of its ranges"
            } else {
                "the auto-balance ran out of passes"
            };
            findings.push(warning(
                FindingCode::PayloadAutoBalanceIncomplete,
                format!(
                    "{why} before the zero-fuel centre of gravity target: aimed at {:.2} m, reached {:.2} m",
                    balance.target_x_m, balance.reached_x_m
                ),
                Some(balance.reached_x_m),
                Some(balance.target_x_m),
                "m",
            ));
        }
    }
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
            for dropped in &summary.dropped_monuments {
                let (placed, kind) = if dropped.starts_with("Galley") {
                    (summary.galleys, "galleys")
                } else {
                    (summary.lavatories, "lavatories")
                };
                let lost = summary
                    .dropped_monuments
                    .iter()
                    .filter(|other| other.starts_with(&dropped[..3]))
                    .count() as f64;
                findings.push(warning(
                    FindingCode::MonumentDropped,
                    dropped.clone(),
                    Some(placed as f64),
                    Some(placed as f64 + lost),
                    kind,
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
            // A deck loaded up to the structural payload limit (maximum
            // zero-fuel weight less modeled OEW, tare included) carries what
            // the aircraft may carry: the request above it is offloaded, as a
            // passenger route is, and said so. The limit is the one the
            // loader itself held the deck to when it lowered the net load to
            // fit; failing that, the effective limit of an unchanged
            // registered preset. Without the first, a design that inherits no
            // published MZFW (every optimised candidate) was held to the
            // declared structural payload by the loader and then reported as
            // a capacity shortfall although its positions hold more than the
            // request (B747-400F: 105.7 t net plus 6.8 t of tare against a
            // 112.6 t limit, 67 positions holding 113.5 t).
            let gross_kg = summary.payload_t * 1_000.0;
            let tare_kg = summary.tare_mass_t * 1_000.0;
            let held_kg = summary
                .held_to_structural_limit_kg
                .filter(|limit| limit.is_finite() && *limit > 0.0);
            let effective_kg = report
                .geometry_summary
                .get("effective_structural_payload_limit_kg")
                .copied()
                .filter(|limit| limit.is_finite() && *limit > 0.0);
            let effective_at_limit = effective_kg.is_some_and(|limit| {
                gross_kg + tare_kg.max(1.0) >= limit && gross_kg <= limit * (1.0 + 1.0e-9)
            });
            let limit_kg = held_kg.or(effective_kg);
            let at_limit = held_kg.is_some() || effective_at_limit;
            if requested_net_kg.is_finite()
                && loaded_net_kg.is_finite()
                && loaded_net_kg + 1.0e-6 < requested_net_kg
                && at_limit
            {
                findings.push(warning(
                    FindingCode::StructuralPayloadLimitViolation,
                    format!(
                        "the requested cargo of {requested_net_kg:.1} kg net, with {tare_kg:.1} kg of unit \
                         load devices, exceeds the structural payload limit (maximum zero-fuel weight \
                         less modeled OEW); the deck carries {loaded_net_kg:.1} kg net, {:.1} kg less",
                        requested_net_kg - loaded_net_kg
                    ),
                    Some(requested_net_kg + tare_kg),
                    limit_kg,
                    "kg",
                ));
            } else if requested_net_kg.is_finite()
                && loaded_net_kg.is_finite()
                && loaded_net_kg + 1.0e-6 < requested_net_kg
            {
                findings.push(error(
                    FindingCode::CargoCapacityShortfall,
                    format!(
                        "cargo layout delivers {loaded_net_kg:.1} kg net against \
                         {requested_net_kg:.1} kg requested: {} positions hold {:.1} kg; lower \
                         requirements.cargo_payload_kg or change the holds or the ULD type \
                         (cabin.cargo)",
                        summary.n_slots,
                        summary.capacity_t * 1_000.0
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
