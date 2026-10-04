// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The candidate main-gear placement step of the mission-sized evaluation.
//!
//! A redesigned candidate is sized once at its configured gear stations; the
//! placement rule ([`crate::envelope::place_main_gear`]) then reads that
//! closure's item-level loading states and, when the configured stations
//! miss a ground mechanism that a translation of the main group can meet,
//! the candidate is sized again with the translation written into its
//! configuration. The second sizing is a complete closure, so the centre of
//! gravity, the trim and every mass station the residuals read describe the
//! aircraft with its gear where the rule put it. The registered aircraft's
//! own design vector keeps its published stations, as does every
//! fixed-aircraft (baseline sandbox) evaluation.

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, DerivedMainGearStation, DesignMode};

use crate::cancellation::EvaluationCancellation;
use crate::envelope::{
    configured_stations_suffice, configured_translation_m, place_main_gear, MainGearPlacement,
    PlacementLedgers,
};

use super::super::mission_model::SizingBudget;
use super::super::sizing::{run_candidate_cancellable, run_candidate_reusing, SizingOutcome};
use super::super::types::{CandidateFailure, ExternalPolar, SizingControls, SizingWork};
use super::balance_ledger;

/// The translation the ledger is rebuilt at to measure each state's moment
/// derivative, m. Any finite nonzero value measures the same derivative of an
/// affine ledger; one metre keeps the probe inside ordinary round-off.
const PROBE_TRANSLATION_M: f64 = 1.0;

/// Whether `x` is a redesigned candidate whose gear the rule places.
///
/// The registered preset's own design vector is the real aircraft with its
/// published gear; a baseline sandbox analyses a fixed aircraft.
fn places_gear(config: &AlasConfig, x: &[f64]) -> bool {
    if config.optimizer.design_space.mode == DesignMode::BaselineSandbox {
        return false;
    }
    let Ok(design) = DesignVector::from_array(x) else {
        return false;
    };
    alas_config::presets::get(&config.preset)
        .map_or(true, |registered| registered.design_vector != design)
}

/// The placement rule's verdict on one sized candidate.
///
/// # Errors
///
/// The ledger or placement failure, as a description.
pub(in crate::mdo) fn solve(
    outcome: &mut SizingOutcome,
    config: &AlasConfig,
) -> Result<MainGearPlacement, String> {
    let configured = balance_ledger::loading_basis(outcome, config)?;
    if configured_stations_suffice(
        &outcome.plane,
        configured,
        outcome.x_np,
        outcome.mac,
        config,
    )? {
        return Ok(MainGearPlacement::Unchanged);
    }
    let mut probe_config = config.clone();
    probe_config.landing_gear.derived_main_gear = Some(DerivedMainGearStation {
        translation_m: configured_translation_m(config) + PROBE_TRANSLATION_M,
    });
    // The closure's mass coordinates carry the gear centroid at the
    // configured stations; the probe ledger reads the empty-aircraft centre
    // of gravity from them, so they move with the probe exactly as a
    // re-sized closure's would.
    let gear_shift_m = if config.mass_model.geometric_component_stations {
        let centroid_x = |candidate: &AlasConfig| {
            alas_mass::stations::component_stations_with_gear(
                &outcome.plane,
                &candidate.geometry,
                &candidate.requirements,
                &candidate.mass_model,
                &candidate.structures,
                &candidate.landing_gear,
            )
            .map(|stations| stations.gear_centroid_m()[0])
            .map_err(|error| format!("gear placement stations: {error}"))
        };
        centroid_x(&probe_config)? - centroid_x(config)?
    } else {
        0.0
    };
    let configured_coords = outcome.coords;
    outcome.coords.gear[0] += gear_shift_m;
    let probe = balance_ledger::loading_basis(outcome, &probe_config);
    outcome.coords = configured_coords;
    place_main_gear(
        &outcome.plane,
        PlacementLedgers {
            configured,
            probe: probe?,
            probe_translation_m: PROBE_TRANSLATION_M,
            neutral_point_x_m: outcome.x_np,
            mac_m: outcome.mac,
        },
        config,
    )
}

/// Size candidate `x`, placing its main gear when the rule applies.
///
/// Returns the closure the residuals must be built on and, when the gear was
/// translated, the configuration that carries the translation; the caller
/// builds the residual table and the resolved state from that configuration.
/// A placement that is infeasible or cannot be posed leaves the configured
/// stations, so the unchanged envelope reports the ground mechanisms the
/// candidate misses.
///
/// # Errors
///
/// The sizing failure of either closure.
pub(in crate::mdo) fn size_with_main_gear_placement(
    config: &AlasConfig,
    x: &[f64],
    polar: Option<&ExternalPolar>,
    preserve_explicit_fuselage_length: bool,
    cancellation: Option<EvaluationCancellation>,
    controls: SizingControls,
) -> Result<(SizingOutcome, Option<AlasConfig>), CandidateFailure> {
    if !places_gear(config, x) {
        let outcome = run_candidate_cancellable(
            config,
            x,
            polar,
            preserve_explicit_fuselage_length,
            cancellation,
            controls,
        )?;
        return Ok((outcome, None));
    }
    // The rule places the gear from the published stations: a translation
    // carried in by a replayed configuration is not a base to place from.
    let published;
    let base = if config.landing_gear.derived_main_gear.is_some() {
        let mut cleared = config.clone();
        cleared.landing_gear.derived_main_gear = None;
        published = cleared;
        &published
    } else {
        config
    };
    let mut outcome = run_candidate_cancellable(
        base,
        x,
        polar,
        preserve_explicit_fuselage_length,
        cancellation.clone(),
        controls,
    )?;
    let mut work = outcome.sized.work;
    let mut placed: Option<AlasConfig> = None;
    // The first solve places the gear; each re-sized closure is re-checked
    // at its own station and, when the secant missed a mechanism, re-solved
    // from that closure once. The unchanged envelope is the verdict either
    // way. A re-sizing starts from the previous closure's trim, which the
    // closure's re-trim rule keeps or replaces (`sizing::TrimReuse`).
    for _ in 0..=MAX_RESOLVES {
        let current = placed.as_ref().unwrap_or(base);
        let further_m = match solve(&mut outcome, current) {
            Ok(MainGearPlacement::Translated(further_m)) => further_m,
            Ok(MainGearPlacement::Unchanged | MainGearPlacement::Infeasible) => break,
            Err(reason) => {
                tracing::debug!(%reason, "main-gear placement unavailable; stations kept");
                break;
            }
        };
        let mut next = base.clone();
        next.landing_gear.derived_main_gear = Some(DerivedMainGearStation {
            translation_m: configured_translation_m(current) + further_m,
        });
        let remaining = SizingControls {
            budget: controls.budget.map(|budget| remaining_budget(budget, work)),
            ..controls
        };
        let reuse = outcome.trim_reuse.take();
        outcome = run_candidate_reusing(
            &next,
            x,
            polar,
            preserve_explicit_fuselage_length,
            cancellation.clone(),
            remaining,
            reuse.as_ref(),
        )?;
        work = summed(work, outcome.sized.work);
        placed = Some(next);
    }
    // Every closure is this candidate's work.
    outcome.sized.work = work;
    Ok((outcome, placed))
}

/// Re-solves after the first placement: the re-sized closure is checked at
/// the solved station and placed again at most this many times.
const MAX_RESOLVES: usize = 1;

fn summed(left: SizingWork, right: SizingWork) -> SizingWork {
    SizingWork {
        plan_freezes: left.plan_freezes.saturating_add(right.plan_freezes),
        trip_flights: left.trip_flights.saturating_add(right.trip_flights),
        deck_evals: left.deck_evals.saturating_add(right.deck_evals),
    }
}

/// What remains of `budget` after `spent`, so every closure of one
/// candidate shares its one work cap.
fn remaining_budget(budget: SizingBudget, spent: SizingWork) -> SizingBudget {
    SizingBudget {
        max_trip_flights: budget
            .max_trip_flights
            .saturating_sub(u32::try_from(spent.trip_flights).unwrap_or(u32::MAX)),
        max_deck_evals: budget.max_deck_evals.saturating_sub(spent.deck_evals),
        ..budget
    }
}

#[cfg(test)]
#[path = "gear_placement_tests.rs"]
mod tests;
