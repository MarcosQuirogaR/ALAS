// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wing position of a redesigned registered aircraft, derived from its
//! balance (`AlasConfig::derives_wing_position`).
//!
//! The candidate's wing is moved along the fuselage until the centre of
//! gravity of its first mass pass (the take-off loading the sizing starts
//! from) sits at the same fraction of the mean aerodynamic chord as the
//! registered aircraft's, evaluated by the same pass under the same
//! configuration. The tails are re-sized at every trial position
//! (`tail_sizing`), so the solved aircraft carries the tail volumes of the
//! position it ends at. The balance is measured with the main gear at its
//! published stations, whatever translation the configuration carries
//! (`LandingGearConfig::derived_main_gear`): the gear follows the wing
//! afterwards (`residuals::gear_placement`), and a position that depended on
//! it would differ between the build that places the gear and the one that
//! carries it.
//!
//! Frame: body axes, x aft from the nose tip, m. The fraction is
//! `(x_cg - x_LEMAC) / MAC`, dimensionless.
//!
//! The solve is a secant iteration started from the position the caller
//! gave. A position that already meets the target within
//! [`BALANCE_TOLERANCE`] is kept bit for bit, so a delivered design
//! re-evaluated from its reported vector is the same aircraft, and the
//! registered design is never moved.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, PoisonError};

use alas_config::design_variables::{DesignVector, SPECS};
use alas_config::{AlasConfig, TailSizing, WING_POSITION_VARIABLE};
use alas_geom::aircraft::airplane::Airplane;

use super::build::{first_mass_pass, rebuild_airplane, resolve_before_build};
use super::nominal_cache::{config_key, NominalCache};
use super::types::CandidateFailure;

/// The wing-position trials either side of the target: `(position, residual)`
/// with the residual positive, then negative; `None` until that sign is seen.
type PositionBracket = (Option<(f64, f64)>, Option<(f64, f64)>);

/// Centre-of-gravity station, as a fraction of the mean aerodynamic chord,
/// within which the wing is in place: 0.25 % of the chord. The loading is
/// laid out in whole seats and containers placed against the wing, so the
/// fraction moves in steps of about 0.1 % of the chord as the wing travels
/// (measured on the B747-400 and the C295: 0.09 % across 1.2 mm); a tighter
/// tolerance has no position that meets it. The balance rows resolve 1 %.
const BALANCE_TOLERANCE: f64 = 2.5e-3;
/// Trials allowed after the first.
const MAX_ITERATIONS: usize = 12;
/// Share of the take-off mass that travels with the wing (the wing, its
/// fuel, the engines and gear it carries, and the payload the cabin layout
/// places against it), used only for the first step: the secant replaces it
/// with the measured slope. Measured slopes on the B747-400 and the C295
/// correspond to shares of 0.5 to 0.9.
const FIRST_STEP_WING_GROUP_SHARE: f64 = 0.75;
/// Largest move of one secant step, in mean aerodynamic chords.
const MAX_STEP_CHORDS: f64 = 0.25;
/// Width of a sign-changing interval, m, below which the fraction steps
/// across the target and no position meets the tolerance.
const POSITION_RESOLUTION_M: f64 = 1.0e-3;
/// Largest move of the wing from the position the caller gave, as a fraction
/// of the fuselage length: beyond it the candidate has no balanced layout
/// under this fuselage.
const MAX_TRAVEL_FUSELAGE_FRACTION: f64 = 0.15;

/// A candidate as built: its configuration, design vector and airplane.
type Built = (AlasConfig, DesignVector, Airplane);

/// The registered aircraft's take-off centre of gravity as a fraction of its
/// mean aerodynamic chord, per configuration.
static NOMINAL_BALANCE: NominalCache<f64> = NominalCache::new();

/// Solved positions by configuration and requested vector (bit patterns):
/// one candidate is built several times per evaluation (the gear placement,
/// the closure, the reporting replay) and each solve costs mass passes.
type Solved = HashMap<((String, u64), Vec<u64>), f64>;
static SOLVED: OnceLock<Mutex<Solved>> = OnceLock::new();
/// Entries kept before the memo is emptied.
const SOLVED_CAPACITY: usize = 8192;

fn position_index() -> Option<usize> {
    SPECS
        .iter()
        .position(|spec| spec.name == WING_POSITION_VARIABLE)
}

/// Build `x` as given.
fn build(
    config: &AlasConfig,
    x: &[f64],
    preserve_explicit_fuselage_length: bool,
) -> Result<Built, CandidateFailure> {
    let (mut candidate_config, mut dv, tail): (AlasConfig, DesignVector, TailSizing) =
        resolve_before_build(config, x, preserve_explicit_fuselage_length)?;
    let plane = rebuild_airplane(&mut candidate_config, &mut dv, &tail)?;
    Ok((candidate_config, dv, plane))
}

/// Build `x` and measure its balance fraction.
fn built(
    config: &AlasConfig,
    x: &[f64],
    preserve_explicit_fuselage_length: bool,
) -> Result<(Built, f64), CandidateFailure> {
    let (candidate_config, dv, plane) = build(config, x, preserve_explicit_fuselage_length)?;
    let (_, _, cg, _, _, _) = first_mass_pass(&candidate_config, &dv, &plane)?;
    let frame = plane
        .mac_frame()
        .ok_or(CandidateFailure::new("geometry_build"))?;
    let fraction = (cg[0] - frame.x_lemac_m) / frame.chord_m;
    if !fraction.is_finite() {
        return Err(CandidateFailure::new("geometry_build"));
    }
    Ok(((candidate_config, dv, plane), fraction))
}

/// The registered aircraft's balance fraction under `config`.
fn nominal(config: &AlasConfig, preserve_explicit_fuselage_length: bool) -> Option<f64> {
    NOMINAL_BALANCE.get_or_resolve(config, |config| {
        let registered = alas_config::presets::get(&config.preset).ok()?;
        let x = config.band_design(registered.design_vector).to_array();
        built(config, &x, preserve_explicit_fuselage_length)
            .ok()
            .map(|(_, fraction)| fraction)
    })
}

#[cfg(test)]
thread_local! {
    /// Set by [`with_layout_as_given`] on this thread.
    static LAYOUT_AS_GIVEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Test fixtures only: run `f` with every candidate built on this thread
/// with its wing where its vector puts it and its main gear at its published
/// stations (no derived wing position, no gear carried with the wing). The
/// gear-placement rule's own tests need a centre of gravity a balanced wing
/// never produces, to exercise the rule against each ground mechanism.
#[cfg(test)]
pub(crate) fn with_layout_as_given<T>(f: impl FnOnce() -> T) -> T {
    LAYOUT_AS_GIVEN.with(|flag| flag.set(true));
    let out = f();
    LAYOUT_AS_GIVEN.with(|flag| flag.set(false));
    out
}

/// Whether a test fixture asked for the layout as given on this thread
/// (always false outside tests).
pub(crate) fn layout_as_given() -> bool {
    #[cfg(test)]
    {
        LAYOUT_AS_GIVEN.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        false
    }
}

/// Whether candidate `x` under `config` has its wing position derived: a
/// redesign of a registered aircraft, not that aircraft itself.
fn applies(config: &AlasConfig, x: &[f64]) -> bool {
    if layout_as_given() || !config.derives_wing_position() {
        return false;
    }
    let Ok(design) = DesignVector::from_array(x) else {
        return false;
    };
    alas_config::presets::get(&config.preset)
        .is_ok_and(|registered| !design.same_geometry(&registered.design_vector))
}

/// Candidate `x` with its wing where it balances, or `None` when the
/// position is not derived for it (the caller then builds `x` as given).
///
/// # Errors
///
/// `geometry_build` when a trial cannot be built, and `wing_position` when no
/// position inside [`MAX_TRAVEL_FUSELAGE_FRACTION`] of the fuselage balances
/// the candidate.
pub(crate) fn balanced(
    config: &AlasConfig,
    x: &[f64],
    preserve_explicit_fuselage_length: bool,
) -> Result<Option<Built>, CandidateFailure> {
    let Some(index) = position_index().filter(|_| applies(config, x)) else {
        return Ok(None);
    };
    let Some(target) = nominal(config, preserve_explicit_fuselage_length) else {
        return Ok(None);
    };
    // The gear at its published stations: see the module documentation.
    let published;
    let measured_config = if config.landing_gear.derived_main_gear.is_some() {
        let mut cleared = config.clone();
        cleared.landing_gear.derived_main_gear = None;
        published = cleared;
        &published
    } else {
        config
    };
    let key = (
        config_key(measured_config),
        x.iter().map(|value| value.to_bits()).collect::<Vec<u64>>(),
    );
    let at = |position_m: f64| {
        let mut moved = x.to_vec();
        moved[index] = position_m;
        moved
    };
    // The candidate at a solved position, under the caller's configuration.
    let placed = |position_m: f64| {
        build(config, &at(position_m), preserve_explicit_fuselage_length).map(Some)
    };
    let trial = |position_m: f64| {
        built(
            measured_config,
            &at(position_m),
            preserve_explicit_fuselage_length,
        )
    };
    let memo = SOLVED.get_or_init(Mutex::default);
    let known = memo
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
        .copied();
    if let Some(position_m) = known {
        return placed(position_m);
    }
    let remember = |position_m: f64| {
        let mut solved = memo.lock().unwrap_or_else(PoisonError::into_inner);
        if solved.len() >= SOLVED_CAPACITY {
            solved.clear();
        }
        solved.insert(key.clone(), position_m);
    };
    let start_m = x[index];
    let (first, fraction) = trial(start_m)?;
    let mut residual = fraction - target;
    if residual.abs() <= BALANCE_TOLERANCE {
        remember(start_m);
        return placed(start_m);
    }
    let chord_m = first.2.c_ref;
    let travel_m = MAX_TRAVEL_FUSELAGE_FRACTION * first.1.fuselage_length_m;
    // Moving the wing aft by `s` carries the chord `s` and the centre of
    // gravity the share of the mass that travels with the wing: the fraction
    // falls by `(1 - share) s / MAC`.
    let mut slope = -(1.0 - FIRST_STEP_WING_GROUP_SHARE) / chord_m;
    let mut position_m = start_m;
    // The trials either side of the target once both signs have been seen:
    // (position, residual) with the residual positive and negative.
    let mut bracket: PositionBracket = (None, None);
    let note = |position_m: f64, residual: f64, bracket: &mut PositionBracket| {
        let side = if residual > 0.0 {
            &mut bracket.0
        } else {
            &mut bracket.1
        };
        if side.is_none_or(|(_, held)| residual.abs() < held.abs()) {
            *side = Some((position_m, residual));
        }
    };
    note(position_m, residual, &mut bracket);
    for _ in 0..MAX_ITERATIONS {
        let secant_m = position_m
            + (-residual / slope).clamp(-MAX_STEP_CHORDS * chord_m, MAX_STEP_CHORDS * chord_m);
        let next_m = match bracket {
            // Between two trials of opposite sign the secant is kept only
            // inside them; the loading is laid out in whole seats and
            // containers, so the fraction has steps the secant can jump.
            (Some((above_m, _)), Some((below_m, _))) => {
                let (low_m, high_m) = (above_m.min(below_m), above_m.max(below_m));
                if high_m - low_m <= POSITION_RESOLUTION_M {
                    break;
                }
                if secant_m > low_m && secant_m < high_m {
                    secant_m
                } else {
                    0.5 * (low_m + high_m)
                }
            }
            _ => secant_m.clamp(start_m - travel_m, start_m + travel_m),
        };
        if next_m == position_m {
            break;
        }
        let (_, fraction) = trial(next_m)?;
        let next_residual = fraction - target;
        if next_residual.abs() <= BALANCE_TOLERANCE {
            remember(next_m);
            return placed(next_m);
        }
        let measured = (next_residual - residual) / (next_m - position_m);
        // A step across a loading step measures the step, not the slope.
        if measured.is_finite() && measured < 0.2 * slope && measured > 5.0 * slope {
            slope = measured;
        }
        (position_m, residual) = (next_m, next_residual);
        note(position_m, residual, &mut bracket);
    }
    Err(CandidateFailure::new("wing_position"))
}

// The tests build their own configurations, so a failed `expect` is the
// assertion failing rather than a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn reference(preset: &str) -> AlasConfig {
        AlasConfig::from_value(&serde_json::json!({
            "preset": preset,
            "optimizer": { "design_space": { "mode": "reference_adaptation" } },
        }))
        .unwrap()
    }

    #[test]
    fn the_registered_aircraft_keeps_its_wing_and_a_sandbox_derives_nothing() {
        let config = reference("A320-200");
        let registered = alas_config::presets::get("A320-200").unwrap().design_vector;
        assert!(balanced(&config, &registered.to_array(), false)
            .unwrap()
            .is_none());
        let mut sandbox = config.clone();
        sandbox.optimizer.design_space.mode = alas_config::DesignMode::BaselineSandbox;
        let mut moved = registered;
        moved.sweep_deg += 2.0;
        assert!(balanced(&sandbox, &moved.to_array(), false)
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_swept_back_wing_is_moved_until_the_aircraft_balances_as_registered() {
        for preset in ["A320-200", "B747-400"] {
            let config = reference(preset);
            let registered = alas_config::presets::get(preset).unwrap().design_vector;
            let target = nominal(&config, false).unwrap();
            let mut swept = registered;
            swept.sweep_deg += 2.5;
            // As requested the swept wing is out of balance: the chord went
            // aft under the loading (A320-200), or the wing's own fuel and
            // engines went further aft than the chord (B747-400).
            let (_, as_requested) = built(&config, &swept.to_array(), false).unwrap();
            let miss = as_requested - target;
            assert!(miss.abs() > BALANCE_TOLERANCE, "{preset}: {miss}");
            let (_, dv, plane) = balanced(&config, &swept.to_array(), false)
                .unwrap()
                .expect("derived");
            // A centre of gravity too far aft on the chord sends the wing aft.
            let moved_m = dv.wing_x_shift_m - registered.wing_x_shift_m;
            assert!(
                moved_m.abs() > 0.02 && moved_m.signum() == miss.signum(),
                "{preset}: wing moved {moved_m} m for a miss of {miss}"
            );
            // The solved aircraft, built again from its own vector, balances
            // within the tolerance and is kept bit for bit.
            let (_, fraction) = built(&config, &dv.to_array(), false).unwrap();
            assert!((fraction - target).abs() <= BALANCE_TOLERANCE, "{preset}");
            let (_, again, replane) = balanced(&config, &dv.to_array(), false)
                .unwrap()
                .expect("derived");
            assert_eq!(again, dv, "{preset}");
            assert_eq!(replane.c_ref.to_bits(), plane.c_ref.to_bits(), "{preset}");
        }
    }
}
