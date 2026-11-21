// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Live progress of a running search, for the supervisor that started it:
//! the stage in flight, the analyses that stage has finished out of those it
//! plans, and the best feasible cost found so far.
//!
//! A supervisor [`SearchProgress::register`]s a progress record against the
//! run's cancellation flag, the one handle every search entry point already
//! takes, and reads it from its own thread ([`SearchProgress::read`]) while
//! the search runs; the search finds it with [`progress_for`]. A flag with no
//! record (every caller that did not ask) makes every update a no-op. The
//! record only observes: nothing in the search reads it back, so a run is the
//! same with or without one.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock, Weak};

/// The stage of a staged search a reading belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStage {
    /// The screening model's exploration of the design space.
    Screening,
    /// Refinement-model analyses that calibrate the screening margins.
    Calibration,
    /// The refinement model's search from the promoted candidates.
    Refinement,
    /// Reporting-fidelity replays and verification of the finalists.
    Verification,
}

impl SearchStage {
    const fn code(self) -> u8 {
        match self {
            Self::Screening => 1,
            Self::Calibration => 2,
            Self::Refinement => 3,
            Self::Verification => 4,
        }
    }

    const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Screening),
            2 => Some(Self::Calibration),
            3 => Some(Self::Refinement),
            4 => Some(Self::Verification),
            _ => None,
        }
    }

    /// Display label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Screening => "screening",
            Self::Calibration => "calibration",
            Self::Refinement => "refinement",
            Self::Verification => "verification",
        }
    }
}

/// One reading of a [`SearchProgress`].
#[derive(Debug, Clone, PartialEq)]
pub struct ProgressReading {
    /// The stage in flight, `None` before the first one starts.
    pub stage: Option<SearchStage>,
    /// Analyses the stage has finished.
    pub done: u64,
    /// Analyses the stage plans (its evaluation budget); 0 when unknown.
    pub planned: u64,
    /// Feasible analyses of the whole run so far.
    pub feasible: u64,
    /// Lowest cost of a feasible analysis so far (the search's cost: lower
    /// is better), `None` before the first.
    pub best_feasible_cost: Option<f64>,
    /// Lines the search wrote to its run log since the previous reading.
    pub lines: Vec<String>,
}

impl ProgressReading {
    /// The stage's finished fraction, when it plans a known count.
    #[must_use]
    pub fn fraction(&self) -> Option<f64> {
        (self.planned > 0).then(|| (self.done as f64 / self.planned as f64).min(1.0))
    }
}

/// The live progress record of one search.
#[derive(Debug, Default)]
pub struct SearchProgress {
    stage: AtomicU8,
    done: AtomicU64,
    planned: AtomicU64,
    feasible: AtomicU64,
    /// `f64` bits of the best feasible cost; `u64::MAX` (a NaN) when none.
    best_bits: AtomicU64,
    lines: Mutex<Vec<String>>,
}

type Registry = RwLock<Vec<(usize, Weak<SearchProgress>)>>;

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(Vec::new()))
}

/// The progress record registered against `flag`, if any.
pub fn progress_for(flag: Option<&AtomicBool>) -> Option<Arc<SearchProgress>> {
    let key = std::ptr::from_ref(flag?) as usize;
    let registry = registry().read().ok()?;
    registry
        .iter()
        .filter(|(candidate, _)| *candidate == key)
        .find_map(|(_, weak)| weak.upgrade())
}

impl SearchProgress {
    /// A new record that a search run with `flag` reports to, held for as
    /// long as the returned handle lives.
    pub fn register(flag: &AtomicBool) -> Arc<Self> {
        let progress = Arc::new(Self {
            best_bits: AtomicU64::new(u64::MAX),
            ..Self::default()
        });
        let key = std::ptr::from_ref(flag) as usize;
        if let Ok(mut registry) = registry().write() {
            registry.retain(|(_, weak)| weak.strong_count() > 0);
            registry.push((key, Arc::downgrade(&progress)));
        }
        progress
    }

    /// A stage starts, planning `planned` analyses (0 when unknown).
    pub fn enter(&self, stage: SearchStage, planned: u64) {
        self.done.store(0, Ordering::Relaxed);
        self.planned.store(planned, Ordering::Relaxed);
        self.stage.store(stage.code(), Ordering::Relaxed);
    }

    /// The stage plans `planned` analyses after all.
    pub fn replan(&self, planned: u64) {
        self.planned.store(planned, Ordering::Relaxed);
    }

    /// Analyses finished, as `(cost, feasible)`.
    pub fn finished(&self, scores: &[(f64, bool)]) {
        self.done.fetch_add(scores.len() as u64, Ordering::Relaxed);
        for &(cost, feasible) in scores {
            if !feasible || !cost.is_finite() {
                continue;
            }
            self.feasible.fetch_add(1, Ordering::Relaxed);
            let _ = self
                .best_bits
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |bits| {
                    let best = f64::from_bits(bits);
                    (best.is_nan() || cost < best).then_some(cost.to_bits())
                });
        }
    }

    /// A line for the run log.
    pub fn line(&self, line: &str) {
        if let Ok(mut lines) = self.lines.lock() {
            lines.push(line.to_owned());
        }
    }

    /// The current state, taking the log lines written since the last read.
    pub fn read(&self) -> ProgressReading {
        let best = f64::from_bits(self.best_bits.load(Ordering::Relaxed));
        ProgressReading {
            stage: SearchStage::from_code(self.stage.load(Ordering::Relaxed)),
            done: self.done.load(Ordering::Relaxed),
            planned: self.planned.load(Ordering::Relaxed),
            feasible: self.feasible.load(Ordering::Relaxed),
            best_feasible_cost: (!best.is_nan()).then_some(best),
            lines: self
                .lines
                .lock()
                .map(|mut lines| std::mem::take(&mut *lines))
                .unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_search_finds_the_record_of_its_flag_and_updates_it() {
        let flag = AtomicBool::new(false);
        let other = AtomicBool::new(false);
        assert!(progress_for(Some(&flag)).is_none());
        let progress = SearchProgress::register(&flag);
        assert!(progress_for(Some(&other)).is_none());
        let found = progress_for(Some(&flag)).expect("registered");
        found.enter(SearchStage::Refinement, 4);
        found.finished(&[(2.0, true), (1.0, false), (3.0, true)]);
        found.line("refinement | budget 4");
        let reading = progress.read();
        assert_eq!(reading.stage, Some(SearchStage::Refinement));
        assert_eq!((reading.done, reading.planned, reading.feasible), (3, 4, 2));
        assert_eq!(reading.best_feasible_cost, Some(2.0));
        assert_eq!(reading.fraction(), Some(0.75));
        assert_eq!(reading.lines, vec!["refinement | budget 4".to_owned()]);
        assert!(progress.read().lines.is_empty(), "lines are taken once");
        drop((found, progress));
        assert!(
            progress_for(Some(&flag)).is_none(),
            "dropped with its handle"
        );
    }
}
