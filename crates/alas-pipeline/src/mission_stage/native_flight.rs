// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The standard mission flown on a built aircraft without a pipeline run.

use std::sync::atomic::AtomicBool;

use alas_config::airports::Airport;
use alas_config::AlasConfig;
use alas_mission::MissionResult;

use super::dispatch::SelectedLoadCase;
use super::sequence::Phase;
use super::standard_flight::{evaluate_cancellable, NativeSupplement, ReachNote};
use crate::full_analysis::AnalysisReport;

impl NativeSupplement {
    /// Every phase of the flight in order: taxi-out, the trip as flown, taxi-in and the reserve
    /// program as flown.
    #[must_use]
    pub fn phases(&self) -> Vec<Phase> {
        let mut phases = vec![self.taxi_out.clone()];
        phases.extend(self.trip_phases.iter().cloned());
        phases.push(self.taxi_in.clone());
        if let Some(reserves) = &self.reserves {
            phases.extend(reserves.phases.iter().cloned());
        }
        phases
    }
}

/// The standard mission as the run flies it on a built aircraft, without a pipeline run: what the
/// Mission page draws and the sandbox prices.
#[derive(Debug, Clone)]
pub struct NativeFlight {
    /// The ground time, the trip and the reserve program as flown, in order.
    pub phases: Vec<Phase>,
    /// Why the top level is below the configured one, when it is.
    pub reach_note: Option<ReachNote>,
    /// Mass at touchdown at the destination, kg.
    pub landing_mass_kg: f64,
    /// The load case the flight was sized to (takeoff mass, fuel plan).
    pub load_case: SelectedLoadCase,
    /// The flown mission result, which the feasibility assessment of this same flight reads beside
    /// the load case (the pipeline hands both to it).
    pub mission: Option<MissionResult>,
}

/// Fly the standard mission of `config` on `report` exactly as the pipeline's mission stage does
/// (same dispatch, same sizing passes) and return what was flown.
///
/// # Errors
///
/// The airports or route do not resolve, the mission stage fails, the flight does not complete
/// (the refusal is the error), or `cancel` was set.
pub fn fly_standard_mission(
    config: &AlasConfig,
    report: &AnalysisReport,
    origin: &Airport,
    destination: &Airport,
    route_distance_m: f64,
    cancel: &AtomicBool,
) -> Result<NativeFlight, String> {
    let (result, load_case) = evaluate_cancellable(
        config,
        report,
        origin,
        destination,
        route_distance_m,
        cancel,
    )?;
    let Some(supplement) = load_case.native_supplement.as_deref() else {
        return Err(load_case
            .native_error
            .clone()
            .or_else(|| result.and_then(|result| result.completion_refusal()))
            .unwrap_or_else(|| "the flight did not complete".to_owned()));
    };
    let landing_mass_kg = result
        .as_ref()
        .and_then(|result| result.completed_summary())
        .map_or(f64::NAN, |summary| summary.landing_mass_kg);
    Ok(NativeFlight {
        landing_mass_kg,
        phases: supplement.phases(),
        reach_note: supplement.reach_note,
        load_case,
        mission: result,
    })
}
