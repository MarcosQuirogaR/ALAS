// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Keeping the registered aircraft when the search does not beat it.
//!
//! A design search is asked to improve on the aircraft it starts from. Its
//! candidates are ranked on the in-loop models and the delivered one is
//! re-analysed at reporting fidelity, where its objective can come out worse
//! than the registered aircraft's, or no candidate may survive the
//! re-analysis at all. In both cases the honest result of the run is the
//! registered aircraft itself, whose reporting analysis the ladder has
//! already made: it is delivered, verified, with the run saying that nothing
//! beat it.

use alas_config::{AlasConfig, DesignVector, ObjectiveKind};

use super::{AcceptanceRoute, FinalistVerification, ReportingNominal};

/// The reporting analysis's measure of `objective`, when it can be read:
/// mission trip fuel (kg) for the fuel objectives (taxi fuel, passengers and
/// range are the same for every candidate of a run, so trip fuel orders
/// block fuel and fuel per seat-kilometre alike), the sized takeoff mass
/// (kg) or the operating empty mass (kg, the report's OEW groups) for the
/// mass objectives.
fn objective_measure(verification: &FinalistVerification, objective: ObjectiveKind) -> Option<f64> {
    let value = match objective {
        ObjectiveKind::BlockFuel | ObjectiveKind::FuelPerSeatKilometre => verification
            .mission
            .as_ref()
            .and_then(alas_mission::MissionResult::completed_summary)
            .map(|summary| summary.trip_fuel_kg),
        ObjectiveKind::TakeoffMass => verification.report.sized_takeoff_mass_kg(),
        ObjectiveKind::OperatingEmptyMass => Some(
            alas_mass::breakdown::OEW_KEYS
                .iter()
                .filter_map(|key| verification.report.component_masses.get(*key))
                .sum(),
        ),
    };
    value.filter(|value| value.is_finite() && *value > 0.0)
}

impl ReportingNominal {
    /// The run's own baseline `design` at reporting fidelity, in any design
    /// mode: what a run outside a reference adaptation compares its
    /// delivery with, and delivers when nothing beats it. It is never a
    /// relative balance nominal there.
    ///
    /// # Errors
    ///
    /// Only a cancellation, as the replay's own message. An analysis failure
    /// is kept as a baseline that cannot be delivered.
    pub fn of_baseline(
        config: &AlasConfig,
        design: &DesignVector,
        route: Option<&AcceptanceRoute>,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<Self, String> {
        let mass_kg = config.at_band_mass(design.mtow_kg).requirements.mtow_kg;
        match super::reporting_fidelity(config, design, route, cancel, false) {
            Err(error) if error.starts_with("Cancelled safely") => Err(error),
            evaluation => Ok(Self {
                design: *design,
                mass_kg,
                evaluation,
            }),
        }
    }

    /// The registered aircraft's own reporting analysis when it is
    /// deliverable: analysed, with no rejecting finding and no violated hard
    /// residual of its replay.
    pub(crate) fn deliverable(&self) -> Option<&FinalistVerification> {
        self.evaluation
            .as_ref()
            .ok()
            .filter(|(verification, violated)| verification.accepted() && violated.is_empty())
            .map(|(verification, _)| verification)
    }

    /// The registered aircraft's reporting analysis, to deliver in place of
    /// the search's `accepted` candidate: when no candidate was accepted (a
    /// design the search or its replay found infeasible is never delivered;
    /// the registered aircraft is, with its own findings, deliverable or
    /// not), or when the accepted one is
    /// worse than the registered aircraft on the run's `objective` at
    /// reporting fidelity. A worse design is never delivered: the registered
    /// aircraft is then returned even when it is not deliverable itself, and
    /// the run reports it with its own findings rather than a candidate that
    /// lost to it. `None` keeps the search's delivery: the accepted candidate
    /// is at least as good or the two cannot be compared (no completed
    /// mission or no registered analysis).
    pub fn retained_over(
        &self,
        accepted: Option<&FinalistVerification>,
        objective: ObjectiveKind,
    ) -> Option<FinalistVerification> {
        let (registered, _) = self.evaluation.as_ref().ok()?;
        let Some(accepted) = accepted else {
            return Some(registered.clone());
        };
        let beaten = match (
            objective_measure(accepted, objective),
            objective_measure(registered, objective),
        ) {
            (Some(candidate), Some(reference)) => candidate <= reference,
            // Without two measures there is no comparison to overrule the
            // search with.
            _ => true,
        };
        (!beaten).then(|| registered.clone())
    }
}
