// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Warm-started revision of a climb rung's commanded-rate scale.

use alas_mass::fuel_plan::FuelModelError;

use super::adapt::{CLIMB_RATE_BRACKET_FACTOR, CLIMB_RATE_BRACKET_STEPS, CLIMB_RATE_RESOLUTION};

/// Trials of the warm-started revision before it gives the search back to
/// the full bracket (the hint was too far from the answer to pay).
const HINT_TRIAL_LIMIT: usize = 8;

/// The largest scale of a climb rung that clears it, found from the scale of
/// the last revision of the same rung instead of by the full bracket.
///
/// The full search ([`SegmentMissionModel::revise_rung`]) scans a geometric
/// bracket for the first scale that clears, `[low, high]`, and bisects it
/// until it is no wider than [`CLIMB_RATE_RESOLUTION`]. Where the scales that
/// clear are an interval from zero up to some threshold (the bisection
/// assumes it), the answer is the lower end of the leaf of that bisection
/// that holds the threshold: one of the `2^n` lattice points fixed by `low`
/// and `high` alone ([`Lattice`]). The last revision of the rung is a point of
/// the same lattice when the bracket is the same, and consecutive probes of a
/// dispatch move the threshold by a few points, so a gallop from it to the
/// first point that fails, then a bisection of the gap, takes two to six
/// trials where the full search takes about ten. The lattice points come from
/// the full bisection's own arithmetic, so the answer is the full search's to
/// the bit and does not depend on call history.
///
/// When the lowest point of the lattice is the floor scale itself and fails
/// (every larger scale of the scan having failed), no gentler climb the
/// schedule admits clears the rung: [`Hinted::CannotClimb`] is the verdict the
/// full scan reaches.
///
/// `None` hands the revision to the full search: the hint is not a point of
/// this probe's lattice, a trial leaves the interval case (the route no
/// longer fits, a model error), the bracket changed (the lowest point does not
/// clear, or the scale above the lattice does), or the hint was too far to pay
/// within [`HINT_TRIAL_LIMIT`] trials.
///
/// Where the scales that clear are not an interval the two searches can end on
/// different islands of the clearing set: the full scan on the highest one it
/// meets from above, this one on the island of the last revision. That is
/// not rare on the heavy wide-bodies under a route-initialised profile (the
/// shadow comparison on the A380, B787-9 and A340 differed in 0 to 40 % of the
/// revisions, mostly by one lattice cell, `2^-10` of the commanded rate, and
/// otherwise by up to 6 % of it); the climb rate is a guidance target, not
/// an aircraft capability (see `revise_rung`), and the full search on the
/// A380 spent 40 times the flights for it, so the warm answer is kept.
pub(super) fn revise_from_hint(
    hint: f64,
    floor: f64,
    clears: &impl Fn(f64) -> Result<Option<bool>, FuelModelError>,
) -> Option<Hinted> {
    // The bracket the full scan would find for a threshold at `hint`.
    let mut bracket = None;
    let mut failed = 1.0;
    let mut scale = 1.0;
    for _ in 0..CLIMB_RATE_BRACKET_STEPS {
        scale = (scale * CLIMB_RATE_BRACKET_FACTOR).max(floor);
        if hint >= scale {
            bracket = Some((scale, failed));
            break;
        }
        failed = scale;
        if scale <= floor {
            break;
        }
    }
    let (low0, high0) = bracket?;
    let lattice = Lattice::new(low0, high0)?;
    let hinted = lattice.index_of(hint)?;
    let mut trials = 0;
    let mut test = |point: i64| -> Option<bool> {
        trials += 1;
        if trials > HINT_TRIAL_LIMIT {
            return None;
        }
        match clears(lattice.point(point)?) {
            Ok(Some(clear)) => Some(clear),
            _ => None,
        }
    };
    // `clear` is the highest point known to clear, `fail` the lowest known
    // to fail; `lattice.count` stands for `high0`, which the scan found
    // failing at its own mass and is verified below.
    let (mut clear, mut fail);
    if test(hinted)? {
        clear = hinted;
        let mut step = 1;
        loop {
            let probe = clear + step;
            if probe >= lattice.count {
                fail = lattice.count;
                break;
            }
            if test(probe)? {
                clear = probe;
                step *= 2;
            } else {
                fail = probe;
                break;
            }
        }
    } else {
        fail = hinted;
        let mut step = 1;
        loop {
            if fail == 0 {
                // Even the lowest point fails: with the scan stopping at the
                // floor that is the verdict, otherwise the bracket changed.
                return (low0 == floor).then_some(Hinted::CannotClimb);
            }
            let probe = (fail - step).max(0);
            if test(probe)? {
                clear = probe;
                break;
            }
            fail = probe;
            step *= 2;
        }
    }
    while fail - clear > 1 {
        let middle = clear + (fail - clear) / 2;
        if test(middle)? {
            clear = middle;
        } else {
            fail = middle;
        }
    }
    if fail == lattice.count && clears(high0).ok()? != Some(false) {
        return None;
    }
    lattice.point(clear).map(Hinted::Scale)
}

/// The verdict of a warm-started revision.
pub(super) enum Hinted {
    /// The largest scale of the rung that clears it.
    Scale(f64),
    /// No scale the schedule admits clears the rung.
    CannotClimb,
}

/// The final brackets of the bisection of `[low, high]` that
/// [`SegmentMissionModel::revise_rung`] runs: `count = 2^depth` leaves whose
/// lower ends `point(0..count)` are the possible answers, the first being
/// `low`, computed with the loop's own arithmetic (`0.5 * (low + high)`).
struct Lattice {
    low: f64,
    high: f64,
    depth: u32,
    count: i64,
}

impl Lattice {
    /// The lattice of `[low, high]`; `None` when the bisection does not stop
    /// at one depth along every path (the width test is within rounding of
    /// the resolution), which only the full search then handles.
    fn new(low: f64, high: f64) -> Option<Self> {
        let mut width = high - low;
        let mut depth = 0_u32;
        while width > CLIMB_RATE_RESOLUTION {
            width *= 0.5;
            depth += 1;
            if depth > 20 {
                return None;
            }
        }
        let lattice = Self {
            low,
            high,
            depth,
            count: 1_i64 << depth,
        };
        // Every path must stop exactly at `depth`: check the widest and the
        // narrowest leaf paths, the extremes of the rounding.
        for index in [0, lattice.count - 1] {
            lattice.leaf(index)?;
        }
        Some(lattice)
    }

    /// The lower end of leaf `index`, or `None` outside the lattice.
    fn point(&self, index: i64) -> Option<f64> {
        self.leaf(index).map(|(low, _)| low)
    }

    fn leaf(&self, index: i64) -> Option<(f64, f64)> {
        if !(0..self.count).contains(&index) {
            return None;
        }
        let (mut low, mut high) = (self.low, self.high);
        for level in (0..self.depth).rev() {
            if high - low <= CLIMB_RATE_RESOLUTION {
                return None;
            }
            let middle = 0.5 * (low + high);
            if (index >> level) & 1 == 1 {
                low = middle;
            } else {
                high = middle;
            }
        }
        (high - low <= CLIMB_RATE_RESOLUTION).then_some((low, high))
    }

    /// The index whose point is exactly `value`.
    fn index_of(&self, value: f64) -> Option<i64> {
        let (mut lo, mut hi) = (0_i64, self.count - 1);
        while lo <= hi {
            let middle = lo + (hi - lo) / 2;
            let point = self.point(middle)?;
            if point == value {
                return Some(middle);
            } else if point < value {
                lo = middle + 1;
            } else {
                hi = middle - 1;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// The full search of `SegmentMissionModel::revise_rung` on a predicate
    /// that clears every scale up to `threshold`: the reference the warm start
    /// must reproduce. `None` is the "cannot climb" verdict.
    fn full_search(threshold: f64, floor: f64, trials: &Cell<usize>) -> Option<f64> {
        let clears = |scale: f64| {
            trials.set(trials.get() + 1);
            scale <= threshold
        };
        let mut bracket = None;
        let mut failed = 1.0;
        let mut scale = 1.0;
        for _ in 0..CLIMB_RATE_BRACKET_STEPS {
            scale = (scale * CLIMB_RATE_BRACKET_FACTOR).max(floor);
            if clears(scale) {
                bracket = Some((scale, failed));
                break;
            }
            failed = scale;
            if scale <= floor {
                break;
            }
        }
        let (mut low, mut high) = bracket?;
        while high - low > CLIMB_RATE_RESOLUTION {
            let middle = 0.5 * (low + high);
            if clears(middle) {
                low = middle;
            } else {
                high = middle;
            }
        }
        Some(low)
    }

    fn warm_search(
        hint: f64,
        threshold: f64,
        floor: f64,
        trials: &Cell<usize>,
    ) -> Option<Option<f64>> {
        let clears = |scale: f64| {
            trials.set(trials.get() + 1);
            Ok(Some(scale <= threshold))
        };
        revise_from_hint(hint, floor, &clears).map(|hinted| match hinted {
            Hinted::Scale(scale) => Some(scale),
            Hinted::CannotClimb => None,
        })
    }

    /// Every answer of the full bisection is a point of the lattice of its
    /// bracket, bit for bit, whichever floor clamps the bracket.
    #[test]
    fn the_lattice_holds_every_answer_of_the_full_bisection() {
        for (low0, high0) in [(0.0759, 0.125), (0.5, 1.0), (0.25, 0.5), (0.0625, 0.125)] {
            let lattice = Lattice::new(low0, high0)
                .unwrap_or_else(|| panic!("no lattice for [{low0}, {high0}]"));
            assert_eq!(lattice.point(0), Some(low0));
            for step in 0..=400 {
                let threshold = low0 + (high0 - low0) * f64::from(step) / 400.0 * 0.9999;
                let (mut low, mut high) = (low0, high0);
                while high - low > CLIMB_RATE_RESOLUTION {
                    let middle = 0.5 * (low + high);
                    if middle <= threshold {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                let index = lattice
                    .index_of(low)
                    .unwrap_or_else(|| panic!("{low} is not a point of [{low0}, {high0}]"));
                assert_eq!(lattice.point(index).map(f64::to_bits), Some(low.to_bits()));
            }
        }
    }

    /// For clearing scales that form an interval the warm start returns the
    /// full search's scale to the bit, from any earlier answer it accepts, and
    /// the "cannot climb" verdict of a threshold below the floor; the trials it
    /// spends never exceed its limit and, near the hint, undercut the full search.
    #[test]
    fn a_warm_started_revision_equals_the_full_search() {
        let floor = 0.0759;
        let mut accepted = 0_usize;
        let mut cheaper = 0_usize;
        for previous in 0..60 {
            let previous_threshold = 0.07 + 0.0125 * f64::from(previous);
            let Some(hint) = full_search(previous_threshold, floor, &Cell::new(0)) else {
                continue;
            };
            for current in 0..120 {
                let threshold = previous_threshold + 0.0007 * f64::from(current - 60);
                let (full_trials, warm_trials) = (Cell::new(0), Cell::new(0));
                let full = full_search(threshold, floor, &full_trials);
                if let Some(warm) = warm_search(hint, threshold, floor, &warm_trials) {
                    assert_eq!(
                        warm.map(f64::to_bits),
                        full.map(f64::to_bits),
                        "hint {hint}, threshold {threshold}"
                    );
                    assert!(warm_trials.get() <= HINT_TRIAL_LIMIT + 1);
                    accepted += 1;
                    cheaper += usize::from(warm_trials.get() < full_trials.get());
                }
            }
        }
        assert!(accepted > 1_000, "the warm start accepted only {accepted}");
        assert!(
            cheaper * 2 > accepted,
            "cheaper in {cheaper} of {accepted} accepted revisions"
        );
    }
}
