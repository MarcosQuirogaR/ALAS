// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! An opt-in payload override for the mission stage only.
//!
//! A passenger preset's payload is always the layout's certifiable cabin
//! capacity (see `alas_pipeline::full_analysis::cabin_sync`):
//! `requirements.num_passengers`, `optimize_passenger_capacity` and
//! `requirements.max_structural_payload_kg` are all superseded by that
//! second mass pass before a report is produced, so none of them can
//! express a real operator's "board fewer than the certifiable maximum"
//! dispatch decision. [`DesignPipeline::with_mission_payload_override_kg`]
//! is the payload-limited-dispatch lever that decision needs: when set,
//! `evaluate_active_mission` flies the mission against a zero-fuel mass
//! built from this payload instead of the report's own, with the freed or
//! consumed mass moved 1:1 into the mission's fuel closure (operating empty
//! mass and MTOW are untouched). It affects only the mission/dispatch
//! stage; every other report field (aerodynamics, structures, the
//! geometry-derived payload layout) still describes the full-capacity
//! aircraft.

use crate::full_analysis::AnalysisReport;

use super::DesignPipeline;

/// Return `report` with its payload replaced by `payload_kg`, moving the
/// difference 1:1 into the fuel closure so operating empty mass and MTOW are
/// unaffected: `Fuel_new = Fuel_old + (Payload_old - payload_kg)`.
///
/// This is the payload/fuel trade a real payload-limited dispatch decision
/// makes (offload payload, carry more fuel under the same certified takeoff
/// mass), applied to the one report field the mission-and-dispatch stage
/// actually reads (`report.component_masses`, via
/// `feasibility::plan_fuel_loading`). Every other report field is left
/// exactly as the full-capacity analysis produced it; this is deliberately
/// not a re-run of the mass/geometry/aerodynamics analysis at a lighter
/// payload.
pub(super) fn report_with_payload_override(
    report: &AnalysisReport,
    payload_kg: f64,
) -> AnalysisReport {
    let mut report = report.clone();
    let old_payload_kg = report
        .component_masses
        .get("Payload")
        .copied()
        .unwrap_or(0.0);
    let old_fuel_kg = report.component_masses.get("Fuel").copied().unwrap_or(0.0);
    report
        .component_masses
        .insert("Payload".to_owned(), payload_kg);
    report.component_masses.insert(
        "Fuel".to_owned(),
        old_fuel_kg + (old_payload_kg - payload_kg),
    );
    report
}

impl DesignPipeline {
    /// Fly the mission stage at `payload_kg` instead of the report's own
    /// certifiable-capacity payload, holding operating empty mass and MTOW
    /// fixed; see this module's documentation for why this exists and what
    /// it does and does not change.
    #[must_use]
    pub fn with_mission_payload_override_kg(mut self, payload_kg: f64) -> Self {
        self.mission_payload_override_kg = Some(payload_kg);
        self
    }
}
