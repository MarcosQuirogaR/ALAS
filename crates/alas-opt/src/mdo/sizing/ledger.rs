// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in closure ledger (`ALAS_CLOSURE_LEDGER=<file>`): one tab-separated
//! line per sizing closure with its wall time, hashes of its inputs, the
//! lattice and deck work it did (from `alas_math::stage_profile`, so only
//! with `ALAS_EVAL_PROFILE=1`) and the `alas_*` frames that asked for it.
//!
//! Two lines with the same configuration, design and controls hashes are the
//! same closure asked for twice. The ledger reads inputs and finished results
//! only and changes none; with the variable unset it costs one `OnceLock`
//! read per closure.
//!
//! Columns: `seq start_s wall_s thread config_hash x_hash controls_hash
//! external preserve_fuselage reuse ok vlm_assemblies vlm_cold_assemblies closures
//! closure_passes deck_evals trip_flights callers`. Counter deltas are
//! process-wide, so closures that overlap in time share them.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Instant;

use alas_config::AlasConfig;

use super::super::types::{CandidateFailure, ExternalPolar, SizingControls};
use super::{SizingOutcome, TrimReuse};
use crate::cancellation::EvaluationCancellation;

struct Sink {
    file: Mutex<std::fs::File>,
    origin: Instant,
}

fn sink() -> Option<&'static Sink> {
    static SINK: OnceLock<Option<Sink>> = OnceLock::new();
    SINK.get_or_init(|| {
        let path = std::env::var_os("ALAS_CLOSURE_LEDGER")?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()?;
        Some(Sink {
            file: Mutex::new(file),
            origin: Instant::now(),
        })
    })
    .as_ref()
}

static SEQ: AtomicU64 = AtomicU64::new(0);

const COUNTERS: [&str; 6] = [
    "vlm_assemblies",
    "vlm_cold_assemblies",
    "closures",
    "closure_passes",
    "deck_evals",
    "trip_flights",
];

fn hash_of(value: &impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn counters() -> [u64; 6] {
    let report = alas_math::stage_profile::report();
    COUNTERS.map(|key| {
        report
            .counters
            .iter()
            .find(|(name, _)| *name == key)
            .map_or(0, |(_, count)| *count)
    })
}

/// The `alas_*` frames above the closure, outermost last.
fn callers() -> String {
    let trace = std::backtrace::Backtrace::force_capture().to_string();
    let mut frames: Vec<&str> = Vec::new();
    for line in trace.lines().map(str::trim) {
        let Some((_, name)) = line.split_once(": ") else {
            continue;
        };
        if line.starts_with("at ")
            || !name.starts_with("alas_")
            || name.starts_with("alas_opt::mdo::sizing::")
            || name.starts_with("alas_math::")
        {
            continue;
        }
        if frames.last() != Some(&name) {
            frames.push(name);
        }
    }
    frames.truncate(14);
    frames.join(" < ")
}

/// With `ALAS_CLOSURE_LEDGER_CONFIGS=<dir>`, the pretty `Debug` text of
/// every distinct closure configuration and its design vector, as
/// `<dir>/<config_hash>.txt`, so the inputs of two closures can be diffed.
fn dump_config(config_hash: u64, config: &AlasConfig, x: &[f64]) {
    let Some(dir) = std::env::var_os("ALAS_CLOSURE_LEDGER_CONFIGS") else {
        return;
    };
    let path = std::path::Path::new(&dir).join(format!("{config_hash:016x}.txt"));
    if path.exists() {
        return;
    }
    let _ = std::fs::write(path, format!("x = {x:?}\n{config:#?}\n"));
}

struct Entry {
    seq: u64,
    started: Instant,
    before: [u64; 6],
    key: String,
    callers: String,
}

fn open(
    config: &AlasConfig,
    x: &[f64],
    controls: &SizingControls,
    external: bool,
    preserve: bool,
    reuse: bool,
) -> Entry {
    let bits: Vec<u64> = x.iter().map(|value| value.to_bits()).collect();
    let config_hash = hash_of(&format!("{config:?}"));
    dump_config(config_hash, config, x);
    let key = format!(
        "{:016x}\t{:016x}\t{:016x}\t{}\t{}\t{}",
        config_hash,
        hash_of(&bits),
        hash_of(&format!("{controls:?}")),
        u8::from(external),
        u8::from(preserve),
        u8::from(reuse),
    );
    let callers = callers();
    Entry {
        seq: SEQ.fetch_add(1, Ordering::Relaxed),
        started: Instant::now(),
        before: counters(),
        key,
        callers,
    }
}

fn close(sink: &Sink, entry: Entry, ok: bool) {
    let after = counters();
    let mut deltas = String::new();
    for (before, after) in entry.before.iter().zip(after.iter()) {
        deltas.push('\t');
        deltas.push_str(&after.saturating_sub(*before).to_string());
    }
    let line = format!(
        "{}\t{:.3}\t{:.3}\t{:?}\t{}\t{}{}\t{}\n",
        entry.seq,
        entry
            .started
            .saturating_duration_since(sink.origin)
            .as_secs_f64(),
        entry.started.elapsed().as_secs_f64(),
        std::thread::current().id(),
        entry.key,
        u8::from(ok),
        deltas,
        entry.callers,
    );
    let mut file = sink.file.lock().unwrap_or_else(PoisonError::into_inner);
    let _ = file.write_all(line.as_bytes());
}

/// [`super::run_candidate_reusing_unlogged`], with a ledger line when
/// `ALAS_CLOSURE_LEDGER` names a file.
pub(crate) fn run_candidate_reusing(
    config: &AlasConfig,
    x: &[f64],
    external: Option<&ExternalPolar>,
    preserve_explicit_fuselage_length: bool,
    cancellation: Option<EvaluationCancellation>,
    controls: SizingControls,
    reuse: Option<&TrimReuse>,
) -> Result<SizingOutcome, CandidateFailure> {
    let Some(sink) = sink() else {
        return super::run_candidate_reusing_unlogged(
            config,
            x,
            external,
            preserve_explicit_fuselage_length,
            cancellation,
            controls,
            reuse,
        );
    };
    let entry = open(
        config,
        x,
        &controls,
        external.is_some(),
        preserve_explicit_fuselage_length,
        reuse.is_some(),
    );
    let result = super::run_candidate_reusing_unlogged(
        config,
        x,
        external,
        preserve_explicit_fuselage_length,
        cancellation,
        controls,
        reuse,
    );
    close(sink, entry, result.is_ok());
    result
}
