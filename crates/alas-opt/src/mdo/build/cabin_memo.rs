// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Memo of the cabin-derived fuselage length ([`super::size_fuselage_from_cabin`]).
//!
//! The solve is a coarse walk plus a 24-step bisection over the fuselage
//! length, each probe building the airframe and packing the cabin: 1 to 2
//! seconds on a transport, and a pure function of the configuration, the
//! design vector and the airfoil registry. It is repeated unchanged by the main-gear second closure
//! (same design vector, configuration differing only in the gear
//! translation), by `canonicalize_design` followed by the evaluation of the
//! same vector, and by any re-evaluation of a candidate.
//!
//! # Key
//!
//! The key is exact, never a hash: the complete [`AlasConfig`] compared with
//! `==` and every design-vector component compared bit for bit
//! (`f64::to_bits`, so `-0.0` and NaN payloads are distinct keys), plus the
//! airfoil-registry generation (`alas_geom::airfoil_io::generation()`, which
//! changes on every registry mutation; the registry resolves the airfoil names
//! the builder loads, and is not part of the configuration). The one
//! normalisation is `landing_gear.derived_main_gear`, cleared on both sides:
//! the solve reads the geometry, cabin, requirements and design-space
//! sections only (`alas-geom` and `alas-payload` never name the landing gear;
//! `tests::the_solve_does_not_read_the_main_gear_translation` pins it). A
//! configuration holding a NaN never compares equal to itself, so it simply
//! misses. Only successful solves are stored; a failure is recomputed.
//!
//! The input `fuselage_length_m` stays in the key (E6a M4, E6c). An applicable
//! solve overrides it, but a non-applicable one (no cabin sizing, or cargo)
//! returns the input unchanged, so dropping it from the key would hand a
//! stored length to a different input. The saving on a first assessment from
//! a registered vector therefore needs a key that knows applicability; it is
//! not taken here.
//!
//! The value is the solved length alone. The caller sets it exactly as the
//! solve does, so a hit is bit-identical to a miss by construction.
//! `ALAS_SPEEDUPS_OFF=cabin_memo` bypasses the memo for a bit-diff.

use std::sync::{Mutex, PoisonError};

use alas_config::{AlasConfig, DesignVector};

use crate::mdo::speedups::{self, Lever};

/// Entries kept (oldest dropped first). One entry holds a configuration
/// clone, so the bound keeps the memo to a few megabytes.
const CAPACITY: usize = 16;

struct Entry {
    config: AlasConfig,
    design_bits: Vec<u64>,
    length_m: f64,
    /// `alas_geom::airfoil_io::generation()` the solve ran under.
    airfoil_generation: u64,
}

static MEMO: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

fn without_placement(config: &AlasConfig) -> std::borrow::Cow<'_, AlasConfig> {
    if config.landing_gear.derived_main_gear.is_none() {
        return std::borrow::Cow::Borrowed(config);
    }
    let mut cleared = config.clone();
    cleared.landing_gear.derived_main_gear = None;
    std::borrow::Cow::Owned(cleared)
}

fn bits(design: &DesignVector) -> Vec<u64> {
    design.to_array().iter().map(|v| v.to_bits()).collect()
}

/// The solved fuselage length for exactly this `config` and `design`, run
/// through `solve` on a miss. `solve` receives the design vector and sets its
/// `fuselage_length_m` on success, as the uncached solve does.
pub(super) fn solve_or_recall<E>(
    config: &AlasConfig,
    design: &mut DesignVector,
    solve: impl FnOnce(&AlasConfig, &mut DesignVector) -> Result<(), E>,
) -> Result<(), E> {
    if !speedups::enabled(Lever::CabinMemo) {
        return solve(config, design);
    }
    let key = without_placement(config);
    // The builder resolves airfoil names through a runtime-mutable registry
    // (`alas_geom::airfoil_io`); re-importing a name changes the geometry under
    // an equal configuration. Read once, before the lookup and the solve, so a
    // swap during the solve stores the older generation and misses next time.
    let generation = alas_geom::airfoil_io::generation();
    let design_bits = bits(design);
    {
        let memo = MEMO.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(hit) = memo.iter().find(|e| {
            e.airfoil_generation == generation && e.design_bits == design_bits && e.config == *key
        }) {
            design.fuselage_length_m = hit.length_m;
            return Ok(());
        }
    }
    solve(config, design)?;
    let mut memo = MEMO.lock().unwrap_or_else(PoisonError::into_inner);
    if memo.len() >= CAPACITY {
        memo.remove(0);
    }
    memo.push(Entry {
        config: key.into_owned(),
        design_bits,
        length_m: design.fuselage_length_m,
        airfoil_generation: generation,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::DerivedMainGearStation;

    fn clean_sheet_case() -> (AlasConfig, DesignVector) {
        let config = AlasConfig::default();
        (config, DesignVector::default())
    }

    #[test]
    fn a_hit_returns_the_bits_of_a_miss_without_solving_again() {
        if !speedups::enabled(Lever::CabinMemo) {
            return; // `ALAS_SPEEDUPS_OFF=cabin_memo` is the documented bit-diff run
        }
        let (config, design) = clean_sheet_case();
        let mut uncached = design;
        super::super::solve_fuselage_from_cabin(&config, &mut uncached).unwrap();
        let mut first = design;
        solve_or_recall(&config, &mut first, super::super::solve_fuselage_from_cabin).unwrap();
        let mut second = design;
        let mut solved_again = false;
        solve_or_recall(&config, &mut second, |_, _| -> Result<(), ()> {
            solved_again = true;
            Ok(())
        })
        .unwrap();
        assert!(!solved_again, "an identical request must be recalled");
        assert_eq!(
            first.fuselage_length_m.to_bits(),
            uncached.fuselage_length_m.to_bits()
        );
        assert_eq!(
            second.fuselage_length_m.to_bits(),
            uncached.fuselage_length_m.to_bits()
        );
    }

    #[test]
    fn any_changed_input_misses() {
        let (mut config, design) = clean_sheet_case();
        // Fake solvers store under their key: keep it apart from the other tests'.
        config.requirements.num_passengers = 11;
        let mut a = design;
        solve_or_recall(&config, &mut a, |_, d| -> Result<(), ()> {
            d.fuselage_length_m = 1.0;
            Ok(())
        })
        .unwrap();
        // A design component one ulp away is another key.
        let mut nudged = design;
        nudged.span_m = f64::from_bits(design.span_m.to_bits() + 1);
        let mut solved = false;
        solve_or_recall(&config, &mut nudged, |_, d| -> Result<(), ()> {
            solved = true;
            d.fuselage_length_m = 2.0;
            Ok(())
        })
        .unwrap();
        assert!(solved);
        // So is a changed configuration.
        let mut other = config.clone();
        other.requirements.num_passengers = 12;
        let mut solved = false;
        let mut again = design;
        solve_or_recall(&other, &mut again, |_, d| -> Result<(), ()> {
            solved = true;
            d.fuselage_length_m = 3.0;
            Ok(())
        })
        .unwrap();
        assert!(solved);
    }

    #[test]
    fn a_failure_is_not_stored() {
        if !speedups::enabled(Lever::CabinMemo) {
            return;
        }
        let (mut config, design) = clean_sheet_case();
        config.requirements.num_passengers = 7;
        let mut d = design;
        assert!(solve_or_recall(&config, &mut d, |_, _| Err("no")).is_err());
        let mut solved = false;
        let mut d = design;
        solve_or_recall(&config, &mut d, |_, _| -> Result<(), ()> {
            solved = true;
            Ok(())
        })
        .unwrap();
        assert!(solved, "a failed solve must be retried, not recalled");
    }

    #[test]
    fn the_solve_does_not_read_the_main_gear_translation() {
        // Several translations, not one sample (E6a M5): the solve is bitwise
        // independent of the gear translation for each of them, including a
        // forward and a zero station.
        let (config, design) = clean_sheet_case();
        let mut without = design;
        super::super::solve_fuselage_from_cabin(&config, &mut without).unwrap();
        for translation_m in [0.0, 0.7, 2.5, -1.2] {
            let mut placed = config.clone();
            placed.landing_gear.derived_main_gear = Some(DerivedMainGearStation { translation_m });
            let mut with_gear = design;
            super::super::solve_fuselage_from_cabin(&placed, &mut with_gear).unwrap();
            assert_eq!(
                with_gear.fuselage_length_m.to_bits(),
                without.fuselage_length_m.to_bits(),
                "translation {translation_m} m"
            );
        }
    }

    #[test]
    fn a_registry_mutation_invalidates_the_memo() {
        if !speedups::enabled(Lever::CabinMemo) {
            return;
        }
        let (mut config, design) = clean_sheet_case();
        // Fake solvers store under their key: keep it apart from the other tests'.
        config.requirements.num_passengers = 13;
        let mut first = design;
        solve_or_recall(&config, &mut first, |_, d| -> Result<(), ()> {
            d.fuselage_length_m = 1.0;
            Ok(())
        })
        .unwrap();
        let mut recalled = design;
        let mut solved = false;
        solve_or_recall(&config, &mut recalled, |_, _| -> Result<(), ()> {
            solved = true;
            Ok(())
        })
        .unwrap();
        // The registry generation can move under a parallel test too; only
        // the hit that precedes our own mutation is asserted.
        if !solved {
            assert_eq!(recalled.fuselage_length_m.to_bits(), 1.0_f64.to_bits());
        }
        // Re-registering the same records is a registry mutation: the
        // builder could now resolve a name differently, so the memo must miss.
        alas_geom::airfoil_io::replace_records(alas_geom::airfoil_io::records()).unwrap();
        let mut after = design;
        let mut solved = false;
        solve_or_recall(&config, &mut after, |_, d| -> Result<(), ()> {
            solved = true;
            d.fuselage_length_m = 2.0;
            Ok(())
        })
        .unwrap();
        assert!(
            solved,
            "a changed airfoil registry must not be answered from the memo"
        );
        assert_eq!(after.fuselage_length_m.to_bits(), 2.0_f64.to_bits());
    }
}
