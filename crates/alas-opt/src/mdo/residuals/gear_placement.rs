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
    configured_stations_suffice, configured_translation_m, place_main_gear, LedgerLoadingBasis,
    MainGearPlacement, PlacementLedgers,
};

use super::super::mission_model::SizingBudget;
use super::super::sizing::clmax_geometry;
use super::super::sizing::{run_candidate_cancellable, run_candidate_reusing, SizingOutcome};
use super::super::speedups;
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
    alas_config::presets::get(&config.preset).map_or(true, |registered| {
        !design.same_geometry(&registered.design_vector)
    })
}

/// The main-gear translation that carries candidate `x`'s wing-mounted legs
/// with its wing ([`crate::envelope::wing_follow_translation_m`]), against
/// the registered aircraft built under the same configuration; `None` for
/// an aircraft without a registered design, a fuselage-mounted gear or a
/// wing that did not move under the legs.
fn wing_following(
    config: &AlasConfig,
    x: &[f64],
    preserve_explicit_fuselage_length: bool,
) -> Option<DerivedMainGearStation> {
    if super::super::wing_position::layout_as_given() {
        return None;
    }
    let build = |config: &AlasConfig, x: &[f64]| {
        super::super::build::build_geometry_with_fuselage_policy(
            config,
            x,
            preserve_explicit_fuselage_length,
        )
        .ok()
    };
    // The registered aircraft is the same for every candidate of one
    // configuration: built once per configuration and fuselage policy.
    let cache = &REGISTERED_PLANE[usize::from(preserve_explicit_fuselage_length)];
    let registered_plane = cache.get_or_resolve(config, |config| {
        let registered = alas_config::presets::get(&config.preset).ok()?;
        build(
            config,
            &config.band_design(registered.design_vector).to_array(),
        )
        .map(|(_, _, plane)| plane)
    })?;
    let (candidate_config, _, candidate_plane) = build(config, x)?;
    let translation_m = crate::envelope::wing_follow_translation_m(
        &registered_plane,
        &candidate_plane,
        &candidate_config,
    )?;
    (translation_m.abs() > WING_FOLLOW_TOLERANCE_M)
        .then_some(DerivedMainGearStation { translation_m })
}

/// The registered aircraft as built, per configuration, without and with the
/// explicit fuselage length preserved.
static REGISTERED_PLANE: [super::super::nominal_cache::NominalCache<
    alas_geom::aircraft::airplane::Airplane,
>; 2] = [
    super::super::nominal_cache::NominalCache::new(),
    super::super::nominal_cache::NominalCache::new(),
];

/// A move of the wing under the legs below this, m, leaves the published
/// stations: a millimetre, far below the chart reading of a gear station.
const WING_FOLLOW_TOLERANCE_M: f64 = 1.0e-3;

/// The placement rule's verdict on one sized candidate.
///
/// # Errors
///
/// The ledger or placement failure, as a description.
pub(in crate::mdo) fn solve(
    outcome: &SizingOutcome,
    config: &AlasConfig,
) -> Result<MainGearPlacement, String> {
    let _profile = alas_math::stage_profile::scope(alas_math::stage_profile::Stage::GearPlacement);
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
    let probe = probe_loading_basis(outcome, config)?;
    place_main_gear(
        &outcome.plane,
        PlacementLedgers {
            configured,
            probe,
            probe_translation_m: PROBE_TRANSLATION_M,
            neutral_point_x_m: outcome.x_np,
            mac_m: outcome.mac,
        },
        config,
    )
}

/// The item ledger of `outcome` with its main group translated
/// [`PROBE_TRANSLATION_M`] beyond `config`'s stations: the gear items stand
/// at the probe stations (the ledger's component stations read them from the
/// translated configuration), and nothing else moves. The payload is laid out
/// against the empty-aircraft centre of gravity of the closure's own mass
/// coordinates, the one `outcome.masses.payload` was closed on. Laid out
/// against the translated empty aircraft, the centre-of-gravity-targeted hold
/// load re-packs its containers and its gross mass changes by their tare (one
/// 82 kg container on the three-leg DC-10 fixture), so the statement cannot
/// close on the closure's payload and the placement is never posed. Moving
/// the gear moves no payload; the re-sized closure lays its payload out at
/// its own stations and is re-checked there.
fn probe_loading_basis(
    outcome: &SizingOutcome,
    config: &AlasConfig,
) -> Result<LedgerLoadingBasis, String> {
    balance_ledger::loading_basis(outcome, &translated(config, PROBE_TRANSLATION_M))
}

/// `config` with its main group translated `further_m` beyond the stations
/// it carries.
fn translated(config: &AlasConfig, further_m: f64) -> AlasConfig {
    let mut moved = config.clone();
    moved.landing_gear.derived_main_gear = Some(DerivedMainGearStation {
        translation_m: configured_translation_m(config) + further_m,
    });
    moved
}

/// How far the gear centroid of `outcome`'s mass coordinates moves, m (x
/// aft), when its main group goes from the stations of `from` to those of
/// `to`; zero when the mass model places no geometric component stations.
fn gear_centroid_shift_m(
    outcome: &SizingOutcome,
    from: &AlasConfig,
    to: &AlasConfig,
) -> Result<f64, String> {
    if !from.mass_model.geometric_component_stations {
        return Ok(0.0);
    }
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
    Ok(centroid_x(to)? - centroid_x(from)?)
}

/// Carry the closed candidate's gear centroid `shift_m` aft and its centre
/// of gravity with it: the residuals that read `cg_x` (OEI deck, VMC) see the
/// gear group's moment at its new station, nothing else moves.
fn move_gear_rigidly(outcome: &mut SizingOutcome, shift_m: f64) {
    let before = alas_mass::breakdown::calculate_physical_cg(&outcome.masses, &outcome.coords);
    outcome.coords.gear[0] += shift_m;
    let after = alas_mass::breakdown::calculate_physical_cg(&outcome.masses, &outcome.coords);
    outcome.cg_x += after[0] - before[0];
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
        // The configuration the residuals read carries the candidate's
        // geometry-scaled CLmax (AERL-01); `None` keeps the declared ones.
        let scaled = outcome
            .clmax
            .as_ref()
            .map(|set| clmax_geometry::with_clmax(config, set));
        return Ok((outcome, scaled));
    }
    // The rule places the gear from the published stations carried with the
    // wing (`wing_following`): a translation carried in by a replayed
    // configuration is not a base to place from.
    let follows = wing_following(config, x, preserve_explicit_fuselage_length);
    let published;
    let base = if config.landing_gear.derived_main_gear.is_some() || follows.is_some() {
        let mut cleared = config.clone();
        cleared.landing_gear.derived_main_gear = follows;
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
    // A gear carried with the wing is part of the candidate the residuals
    // and the report read, whether or not the rule moves it further.
    let mut placed: Option<AlasConfig> = follows.is_some().then(|| base.clone());
    // The first solve places the gear; each re-sized closure is re-checked
    // at its own station and, when the secant missed a mechanism, re-solved
    // from that closure once. The unchanged envelope is the verdict either
    // way. A re-sizing starts from the previous closure's trim, which the
    // closure's re-trim rule keeps or replaces (`sizing::TrimReuse`).
    for _ in 0..=MAX_RESOLVES {
        let current = placed.as_ref().unwrap_or(base);
        let current_scaled = outcome.config_with_clmax(current);
        let further_m = match solve(&outcome, &current_scaled) {
            Ok(MainGearPlacement::Translated(further_m)) => further_m,
            Ok(MainGearPlacement::Residual(further_m)) => {
                // A move inside the chart-read precision is a rigid
                // translation of the closed candidate's main group: its mass
                // coordinates carry the gear centroid to the new stations and
                // nothing is re-sized (no FLOPS group depends on the gear's
                // longitudinal station).
                let next = translated(current, further_m);
                match gear_centroid_shift_m(&outcome, current, &next) {
                    Ok(shift_m) => {
                        move_gear_rigidly(&mut outcome, shift_m);
                        placed = Some(next);
                    }
                    Err(reason) => {
                        tracing::debug!(%reason, "residual main-gear move unavailable; stations kept");
                    }
                }
                break;
            }
            Ok(MainGearPlacement::Unchanged | MainGearPlacement::Infeasible) => break,
            Err(reason) => {
                tracing::debug!(%reason, "main-gear placement unavailable; stations kept");
                break;
            }
        };
        let next = translated(current, further_m);
        let reuse = outcome.trim_reuse.take();
        // Lever A13: the re-sizing differs from the closure just solved by
        // the gear translation only. It starts from that closure's mass and
        // keeps its trim for a centre-of-gravity shift up to the tolerance
        // below (the shift stays disclosed in `cg_shift_pct_mac`). A closure
        // that did not settle has no fixed point to start from: its re-sizing
        // runs as before.
        let reuse_closure =
            speedups::enabled(speedups::Lever::GearReuse) && outcome.sized.sizing_closed;
        let remaining = SizingControls {
            budget: controls.budget.map(|budget| remaining_budget(budget, work)),
            initial_takeoff_mass_kg: if reuse_closure {
                Some(outcome.sized.takeoff_mass_kg)
            } else {
                controls.initial_takeoff_mass_kg
            },
            ..controls
        };
        let reuse = reuse.map(|trim| {
            if reuse_closure {
                let configured = base.optimizer.objective.retrim_cg_tolerance_pct_mac;
                trim.with_cg_tolerance(configured.max(GEAR_REUSE_CG_TOLERANCE_PCT_MAC))
            } else {
                trim
            }
        });
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
    let placed = match outcome.clmax.as_ref() {
        Some(set) => Some(clmax_geometry::with_clmax(
            placed.as_ref().unwrap_or(base),
            set,
        )),
        None => placed,
    };
    Ok((outcome, placed))
}

/// Centre-of-gravity shift, percent MAC, over which the first closure's trim
/// stands for the re-sized closure (lever A13). Moving the main gear changes
/// no aerodynamic input; the shift only changes the tail load. Trim drag
/// moves of the order of 0.04 % of fuel burn per 1 % MAC here: on 24 sampled
/// ATR72-600 candidates (FixedRequirement, 3 % MAC against the exact re-trim)
/// the block fuel changed by at most 6.9e-4 at a 1.7 % MAC shift, mass and
/// field length unchanged. 1 % MAC stays below the 0.1 %
/// equivalence tolerance; beyond it the trim is solved afresh.
const GEAR_REUSE_CG_TOLERANCE_PCT_MAC: f64 = 1.0;

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
