// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The last few sized candidates of a pipeline run, so a second request for
//! the same closure returns the first one's outcome instead of sizing the
//! aircraft again.
//!
//! One analysis asks for the same closure more than once: the takeoff-mass
//! finding and the held-engine check assess the delivered design under the
//! same configuration, and a reference adaptation's relative-balance nominal
//! sizes the preset design vector, which is the delivered design when the
//! preset is analysed as registered. The closure is a deterministic function
//! of its inputs, so the stored outcome is the one a re-run would produce,
//! bit for bit.
//!
//! The key is the complete configuration (its `Debug` text, as the nominal
//! caches key it), the design vector's bits and the fuselage policy. The key
//! does not see the files and tool settings a closure reads beside the
//! configuration (engine decks, airport data), so the memo answers only
//! inside a run ([`run_scope`]) and forgets everything when a run starts and
//! when the last one ends: a closure asked for outside any run (a preview, a
//! test) is sized as before.
//!
//! Only plain closures are kept: no external polar and the default
//! [`SizingControls`]. An optimiser candidate under a work budget or
//! warm-started from a neighbour is asked for once, and storing it would push
//! out the delivered design's closure. A failure, cancelled or not, is always
//! recomputed. A hit moves its entry to the back, so the closures a run keeps
//! asking for are the last to go.

use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use alas_config::AlasConfig;

use super::SizingOutcome;
use crate::mdo::types::SizingControls;

/// How many outcomes are kept: a pipeline run asks for at most a few
/// distinct closures, and each outcome holds a built airplane.
const CAPACITY: usize = 4;

#[derive(PartialEq, Eq)]
struct Key {
    config: String,
    x: Vec<u64>,
    preserve_fuselage: bool,
}

/// The run bookkeeping and the stored outcomes, least recently used first.
struct Store<K, V> {
    /// Runs in progress (live [`RunScope`]s).
    runs: usize,
    /// Advanced by every run that starts. A closure that started before the
    /// latest run began does not store its outcome: the files beside the
    /// configuration may have changed in between.
    generation: u64,
    entries: Vec<(K, V)>,
}

impl<K: PartialEq, V: Clone> Store<K, V> {
    const fn new() -> Self {
        Self {
            runs: 0,
            generation: 0,
            entries: Vec::new(),
        }
    }

    fn begin_run(&mut self) {
        self.runs += 1;
        self.generation = self.generation.wrapping_add(1);
        self.entries.clear();
    }

    fn end_run(&mut self) {
        self.runs = self.runs.saturating_sub(1);
        if self.runs == 0 {
            self.entries.clear();
        }
    }

    /// The stored value of `key`, refreshed to most recently used; or the
    /// generation a computation of it starts in.
    fn find(&mut self, key: &K) -> Result<V, u64> {
        match self.entries.iter().position(|(stored, _)| stored == key) {
            Some(index) => {
                let entry = self.entries.remove(index);
                let value = entry.1.clone();
                self.entries.push(entry);
                Ok(value)
            }
            None => Err(self.generation),
        }
    }

    /// Keep `value`, computed from `generation` on, unless a run started
    /// since, no run is live, or the key is already stored.
    fn keep(&mut self, key: K, value: V, generation: u64) {
        let current = self.runs > 0 && self.generation == generation;
        if current && !self.entries.iter().any(|(stored, _)| *stored == key) {
            if self.entries.len() == CAPACITY {
                self.entries.remove(0);
            }
            self.entries.push((key, value));
        }
    }
}

static STORE: Mutex<Store<Key, SizingOutcome>> = Mutex::new(Store::new());

fn store() -> MutexGuard<'static, Store<Key, SizingOutcome>> {
    STORE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn disabled() -> bool {
    static OFF: OnceLock<bool> = OnceLock::new();
    *OFF.get_or_init(|| {
        matches!(
            std::env::var("ALAS_SIZING_MEMO")
                .ok()
                .as_deref()
                .map(str::trim),
            Some("0" | "off" | "false")
        )
    })
}

/// The memo of one pipeline run, live until dropped ([`run_scope`]).
#[must_use = "the memo answers only while the scope is alive"]
pub struct RunScope(());

/// Start a run: forget what earlier runs stored, and answer repeated plain
/// closures until the returned scope is dropped.
pub fn run_scope() -> RunScope {
    store().begin_run();
    RunScope(())
}

impl Drop for RunScope {
    fn drop(&mut self) {
        store().end_run();
    }
}

/// [`super::run_candidate_cancellable`]: a closure with an external polar,
/// or whose cancellation is already requested, is sized directly; any other
/// goes through [`sized`].
pub(super) fn run_cancellable(
    config: &AlasConfig,
    x: &[f64],
    external: Option<&super::ExternalPolar>,
    preserve_explicit_fuselage_length: bool,
    cancellation: Option<crate::cancellation::EvaluationCancellation>,
    controls: SizingControls,
) -> Result<SizingOutcome, super::CandidateFailure> {
    let run = |cancellation| {
        super::run_candidate_reusing(
            config,
            x,
            external,
            preserve_explicit_fuselage_length,
            cancellation,
            controls,
            None,
        )
    };
    if external.is_some() || cancellation.as_ref().is_some_and(|token| token.requested()) {
        return run(cancellation);
    }
    sized(
        config,
        x,
        preserve_explicit_fuselage_length,
        &controls,
        || run(cancellation),
    )
}

/// The stored outcome of this closure, or `size()`'s, which is then stored
/// when it succeeded. Outside a run, or for a closure that is not plain
/// (module docs), `size()` alone.
pub(super) fn sized<E>(
    config: &AlasConfig,
    x: &[f64],
    preserve: bool,
    controls: &SizingControls,
    size: impl FnOnce() -> Result<SizingOutcome, E>,
) -> Result<SizingOutcome, E> {
    if disabled() || *controls != SizingControls::default() || store().runs == 0 {
        return size();
    }
    let key = Key {
        config: format!("{config:?}"),
        x: x.iter().map(|value| value.to_bits()).collect(),
        preserve_fuselage: preserve,
    };
    let generation = match store().find(&key) {
        Ok(outcome) => return Ok(outcome),
        Err(generation) => generation,
    };
    let outcome = size()?;
    store().keep(key, outcome.clone(), generation);
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::{Store, CAPACITY};

    #[test]
    fn a_run_keeps_and_refreshes_its_entries_and_forgets_them_at_the_end() {
        let mut store = Store::<u32, &str>::new();
        assert_eq!(store.find(&1), Err(0));
        store.keep(1, "no run", 0);
        assert!(store.entries.is_empty(), "nothing is kept outside a run");

        store.begin_run();
        let generation = store.find(&1).unwrap_err();
        store.keep(1, "one", generation);
        assert_eq!(store.find(&1), Ok("one"));
        for key in 2..=CAPACITY as u32 {
            store.keep(key, "other", generation);
        }
        // A hit refreshes: key 1 is the most recently used, so the next
        // insertion evicts key 2 instead.
        assert_eq!(store.find(&1), Ok("one"));
        store.keep(99, "new", generation);
        assert_eq!(store.find(&2), Err(generation));
        assert_eq!(store.find(&1), Ok("one"));
        store.end_run();
        assert!(store.entries.is_empty());
        assert_eq!(store.runs, 0);
    }

    #[test]
    fn a_closure_started_before_another_run_began_is_not_kept() {
        let mut store = Store::<u32, &str>::new();
        store.begin_run();
        let first = store.find(&7).unwrap_err();
        store.begin_run();
        store.keep(7, "stale", first);
        assert!(
            store.find(&7).is_err(),
            "computed before the second run began"
        );
        let second = store.find(&7).unwrap_err();
        store.keep(7, "fresh", second);
        store.end_run();
        assert_eq!(store.find(&7), Ok("fresh"), "one run is still live");
        store.end_run();
        assert!(store.find(&7).is_err());
    }
}
