// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Typed, timestamped telemetry for cooperative cancellation.
//!
//! # What this measures, and why a second number was needed
//!
//! The search stack stops on a cooperative flag: a supervisor sets an
//! [`AtomicBool`], and the run reads it at a boundary it chooses. That makes
//! stopping *bounded*, never immediate, and the size of the bound is a
//! property of the boundary that happens to be active - not a constant, and
//! not something a caller can read off the source. A single "wall time minus
//! guard" figure conflates two very different quantities:
//!
//! - **acknowledgement latency** - request to the first time the running
//!   search *observed* the flag. This is what a user perceives as "the cancel
//!   button did something", and it is bounded by how often the active loop
//!   polls.
//! - **drain latency** - request to the search actually returning. This is
//!   acknowledgement plus whatever uninterruptible work was already in flight
//!   when the flag was read: one coupled evaluation, one screening block, or
//!   one supervised external-solver call.
//!
//! The 2026-09-22 A320-200 `quick_draft` smoke reported 28.76 s of drain with
//! no way to say which of the two it was, or which phase it was spent in. The
//! instrumentation here answers that: every phase entry, the phase and
//! evaluation index in flight when the request arrived, the first observation,
//! the return, and the per-evaluation cost that sets the bound.
//!
//! # How it attaches without changing any signature
//!
//! Cancellation already threads through the pipeline, the GUI and the search
//! kernels as `Option<&AtomicBool>`. Widening that to a telemetry-carrying
//! type would touch every caller including the GUI. Instead a [`CancelWatch`]
//! *owns* the flag it hands out, and registers itself against that flag's
//! address; instrumentation points look the watch up from the `&AtomicBool`
//! they already hold ([`CancelScope::attach`]). A caller that passes a bare
//! `AtomicBool` - which is every existing caller - finds no watch and every
//! telemetry call becomes a no-op, so behaviour, determinism and cost are
//! unchanged for them.
//!
//! # Units and frames
//!
//! Every duration in a [`CancelSnapshot`] is **seconds**, floating point,
//! measured on the monotonic clock ([`Instant`]) from the watch's own origin.
//! `origin_unix_s` is that origin on the wall clock (seconds since the Unix
//! epoch, UTC), so a snapshot can be placed against a log without the process
//! that produced it. Counters are dimensionless counts of evaluations or
//! processes. `None` means "did not happen", never "zero".

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, OnceLock, RwLock, Weak};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// Sentinel for an unset nanosecond timestamp.
const UNSET: u64 = u64::MAX;

/// How many telemetry events one watch retains.
///
/// Events are recorded at phase transitions and cancellation boundaries, not
/// per evaluation, so a full search records tens of them. The cap exists so a
/// pathological configuration cannot grow the buffer without limit; overflow
/// is counted and reported rather than silently dropped.
const EVENT_CAPACITY: usize = 512;

/// The part of a run that was executing when the cancellation flag was read.
///
/// This is the vocabulary the bound is expressed in: a request that lands in
/// [`Self::DeGeneration`] waits at most one coupled evaluation, one that lands
/// in [`Self::ExternalSolverCall`] waits for that process to be polled and
/// terminated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelPhase {
    /// Nothing instrumented has started yet.
    NotStarted,
    /// A pipeline stage outside the search itself.
    PipelineStage,
    /// One block of the Stage-A broad screening scan.
    ScreeningScanBlock,
    /// Full-fidelity re-evaluation of the Stage-A finalists.
    ScanVerification,
    /// Scoring the initial Differential Evolution population.
    DeInitialPopulation,
    /// One Differential Evolution generation.
    DeGeneration,
    /// The bounded MADS search phase that precedes polling.
    MadsSearchBlock,
    /// One MADS poll block.
    MadsPollBlock,
    /// One SQP major iteration.
    SqpMajorIteration,
    /// A supervised external solver process is in flight.
    ExternalSolverCall,
    /// The reporting-fidelity re-evaluation of the delivered candidate.
    ReportingFidelityVerification,
    /// The search has returned.
    SearchFinished,
}

impl CancelPhase {
    /// Stable encoding for the atomic phase slot.
    const fn code(self) -> u8 {
        match self {
            Self::NotStarted => 0,
            Self::PipelineStage => 1,
            Self::ScreeningScanBlock => 2,
            Self::ScanVerification => 3,
            Self::DeInitialPopulation => 4,
            Self::DeGeneration => 5,
            Self::MadsSearchBlock => 6,
            Self::MadsPollBlock => 7,
            Self::SqpMajorIteration => 8,
            Self::ExternalSolverCall => 9,
            Self::ReportingFidelityVerification => 10,
            Self::SearchFinished => 11,
        }
    }

    /// Inverse of [`Self::code`]; an unknown code reads as [`Self::NotStarted`]
    /// rather than panicking, because telemetry must never be able to stop a
    /// run.
    const fn from_code(code: u8) -> Self {
        match code {
            1 => Self::PipelineStage,
            2 => Self::ScreeningScanBlock,
            3 => Self::ScanVerification,
            4 => Self::DeInitialPopulation,
            5 => Self::DeGeneration,
            6 => Self::MadsSearchBlock,
            7 => Self::MadsPollBlock,
            8 => Self::SqpMajorIteration,
            9 => Self::ExternalSolverCall,
            10 => Self::ReportingFidelityVerification,
            11 => Self::SearchFinished,
            _ => Self::NotStarted,
        }
    }

    /// A short stable label, for logs and report tables.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::PipelineStage => "pipeline_stage",
            Self::ScreeningScanBlock => "screening_scan_block",
            Self::ScanVerification => "scan_verification",
            Self::DeInitialPopulation => "de_initial_population",
            Self::DeGeneration => "de_generation",
            Self::MadsSearchBlock => "mads_search_block",
            Self::MadsPollBlock => "mads_poll_block",
            Self::SqpMajorIteration => "sqp_major_iteration",
            Self::ExternalSolverCall => "external_solver_call",
            Self::ReportingFidelityVerification => "reporting_fidelity_verification",
            Self::SearchFinished => "search_finished",
        }
    }
}

/// Why a run stopped, as one closed set.
///
/// The four outcomes this task has to keep apart - an external wall-clock
/// guard, a cooperative cancellation, an external tool being terminated, and
/// the search's own convergence - are four distinct variants here, so a caller
/// cannot report one as another by reading a free-text field loosely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// The search met its own convergence criterion. The only reason that is
    /// a positive result.
    Converged,
    /// The coupled-analysis budget was exhausted without convergence.
    EvaluationBudget,
    /// The configured iteration or generation count was exhausted without
    /// convergence.
    IterationLimit,
    /// The mesh contracted below its floor without meeting the improvement
    /// criterion.
    MeshLimit,
    /// The search's own internal wall-clock watchdog fired. Not convergence
    /// and not cancellation: a safety limit inside the optimizer.
    Watchdog,
    /// A supervisor set the cooperative cancellation flag and the search
    /// stopped on it.
    Cancelled,
    /// An external wall-clock guard outside the search expired. Distinct from
    /// [`Self::Cancelled`]: the guard is what *requests* cancellation, and a
    /// row that reports this reason is naming the requester.
    ExternalGuardTimeout,
    /// A supervised external solver process was polled out or force-killed.
    ExternalToolTerminated,
    /// The request itself was rejected before any search ran.
    InvalidInput,
    /// A reason the caller could not classify, kept verbatim rather than
    /// mapped onto a neighbour.
    Unclassified,
}

impl StopReason {
    /// Map the optimizer's own `termination` vocabulary onto this set.
    ///
    /// Unknown strings become [`Self::Unclassified`]; they are never folded
    /// into a nearby variant, because "we do not know why it stopped" and "it
    /// converged" must not be able to alias.
    pub fn from_termination(termination: &str) -> Self {
        match termination {
            "converged" => Self::Converged,
            "evaluation_budget" => Self::EvaluationBudget,
            "iteration_limit" => Self::IterationLimit,
            "mesh_limit" => Self::MeshLimit,
            "watchdog" => Self::Watchdog,
            "cancelled" => Self::Cancelled,
            "invalid_input" => Self::InvalidInput,
            _ => Self::Unclassified,
        }
    }

    /// Whether this reason permits a run to be reported as converged.
    ///
    /// Exactly one variant does.
    pub const fn is_convergence(self) -> bool {
        matches!(self, Self::Converged)
    }
}

/// What a recorded telemetry event is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelEventKind {
    /// A supervisor requested cancellation.
    Requested,
    /// The run entered a new phase.
    PhaseEntered,
    /// The running search read the flag for the first time after the request.
    FirstObserved,
    /// Work that would have started was skipped because the flag was set.
    WorkSkipped,
    /// A supervised external process was launched.
    ExternalProcessStarted,
    /// A supervised external process was not launched because the flag was
    /// already set.
    ExternalProcessSkipped,
    /// A supervised external process was terminated.
    ExternalProcessTerminated,
    /// A progress notice emitted while draining.
    Progress,
    /// The search returned.
    SearchFinished,
    /// The supervisor collected the worker.
    WorkerJoined,
}

/// One timestamped telemetry record.
#[derive(Debug, Clone, Serialize)]
pub struct CancelEvent {
    /// Seconds since the watch's origin, monotonic clock.
    pub at_s: f64,
    /// What happened.
    pub kind: CancelEventKind,
    /// The phase in flight when it happened.
    pub phase: CancelPhase,
    /// Phase-local index: generation, block or evaluation number, whichever
    /// the phase counts.
    pub phase_index: u64,
    /// Coupled evaluations completed when it happened.
    pub evaluations_completed: u64,
    /// Free text, for the detail the typed fields do not carry.
    pub detail: String,
}

/// An immutable reading of one watch.
///
/// Every duration is seconds on the monotonic clock; see the module
/// documentation for the frame.
#[derive(Debug, Clone, Serialize)]
pub struct CancelSnapshot {
    /// The watch's origin on the wall clock: seconds since the Unix epoch,
    /// UTC. Added to any `*_s` field below it gives an absolute timestamp.
    pub origin_unix_s: f64,
    /// Seconds from the origin to this reading.
    pub elapsed_s: f64,
    /// Whether cancellation has been requested.
    pub requested: bool,
    /// When the request was made.
    pub requested_at_s: Option<f64>,
    /// The phase in flight at the moment of the request - the phase whose
    /// boundary the drain is waiting for.
    pub requested_during: CancelPhase,
    /// Phase-local index at the moment of the request.
    pub requested_during_index: u64,
    /// Coupled evaluations completed at the moment of the request.
    pub requested_at_evaluation: u64,
    /// When the running search first read the set flag.
    pub first_observed_at_s: Option<f64>,
    /// The phase that read it.
    pub first_observed_in: CancelPhase,
    /// Request to first observation. This is the *acknowledgement* latency,
    /// the figure a responsiveness contract is stated in.
    pub acknowledgement_latency_s: Option<f64>,
    /// When the search returned.
    pub search_finished_at_s: Option<f64>,
    /// Request to search return. This is the *drain* latency: acknowledgement
    /// plus the work already in flight.
    pub drain_latency_s: Option<f64>,
    /// When the supervisor collected the worker.
    pub worker_joined_at_s: Option<f64>,
    /// Request to worker join, the figure an external guard measures.
    pub join_latency_s: Option<f64>,
    /// The phase in flight at this reading.
    pub phase: CancelPhase,
    /// Phase-local index at this reading.
    pub phase_index: u64,
    /// How long the current phase has been running.
    pub phase_elapsed_s: f64,
    /// Coupled evaluations started.
    pub evaluations_started: u64,
    /// Coupled evaluations completed.
    pub evaluations_completed: u64,
    /// Coupled evaluations completed after the request. With a per-evaluation
    /// check this is the drain's whole cost in analyses, and it should be at
    /// most one per active worker.
    pub evaluations_after_request: u64,
    /// Longest single coupled evaluation observed. This is the physical bound
    /// on a per-evaluation cancellation check.
    pub longest_evaluation_s: Option<f64>,
    /// Longest uninterruptible evaluation block observed. This is the bound
    /// wherever cancellation is checked per block rather than per evaluation.
    pub longest_block_s: Option<f64>,
    /// Evaluations in the longest block, so the block bound can be read
    /// against the per-evaluation cost.
    pub longest_block_evaluations: u64,
    /// Supervised external processes launched.
    pub external_processes_started: u64,
    /// External process launches suppressed because cancellation was already
    /// requested.
    pub external_processes_skipped: u64,
    /// Supervised external processes terminated.
    pub external_processes_terminated: u64,
    /// The classified stop reason, once the run has one.
    pub stop_reason: Option<StopReason>,
    /// The optimizer's own termination string, verbatim.
    pub termination: Option<String>,
    /// Telemetry events, oldest first.
    pub events: Vec<CancelEvent>,
    /// Events dropped because the buffer was full.
    pub events_dropped: u64,
}

impl CancelSnapshot {
    /// Whether the acknowledgement contract held for this run.
    ///
    /// The contract is stated on acknowledgement, not on completion: the
    /// search must *observe* a request within `limit`. Completion is bounded
    /// separately by the active evaluation, which no flag can shorten.
    ///
    /// A run that was never asked to cancel satisfies it vacuously.
    pub fn acknowledged_within(&self, limit_s: f64) -> bool {
        match self.acknowledgement_latency_s {
            Some(latency) => latency <= limit_s,
            None => !self.requested,
        }
    }
}

/// The cancellation flag a supervisor owns, with its telemetry.
///
/// Construct one with [`CancelWatch::new`], hand [`CancelWatch::flag`] to the
/// existing `&AtomicBool` API, and read [`CancelWatch::snapshot`] afterwards.
#[derive(Debug)]
pub struct CancelWatch {
    /// The flag handed to the run. Its address is this watch's registry key,
    /// so it must not move: the watch is always behind an `Arc`.
    flag: AtomicBool,
    origin: Instant,
    origin_unix_s: f64,
    requested_ns: AtomicU64,
    requested_phase: AtomicU8,
    requested_phase_index: AtomicU64,
    requested_evaluation: AtomicU64,
    first_observed_ns: AtomicU64,
    first_observed_phase: AtomicU8,
    search_finished_ns: AtomicU64,
    worker_joined_ns: AtomicU64,
    phase: AtomicU8,
    phase_index: AtomicU64,
    phase_started_ns: AtomicU64,
    evaluations_started: AtomicU64,
    evaluations_completed: AtomicU64,
    evaluations_after_request: AtomicU64,
    longest_evaluation_ns: AtomicU64,
    longest_block_ns: AtomicU64,
    longest_block_evaluations: AtomicU64,
    external_started: AtomicU64,
    external_skipped: AtomicU64,
    external_terminated: AtomicU64,
    stop_reason: RwLock<Option<StopReason>>,
    termination: RwLock<Option<String>>,
    events: RwLock<Vec<CancelEvent>>,
    events_dropped: AtomicU64,
}

type Registry = RwLock<Vec<(usize, Weak<CancelWatch>)>>;

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(Vec::new()))
}

/// The watch that owns `flag`, if any.
///
/// A flag that no watch owns - every pre-existing caller, including the GUI -
/// yields `None`, and every telemetry call on the resulting scope is a no-op.
pub fn watch_for(flag: &AtomicBool) -> Option<Arc<CancelWatch>> {
    let key = std::ptr::from_ref(flag) as usize;
    let registry = registry().read().ok()?;
    // `find_map` over `upgrade`, not `find` on the key: an entry whose watch
    // has already been dropped must not shadow a live watch that the
    // allocator later placed at the same address.
    registry
        .iter()
        .filter(|(candidate, _)| *candidate == key)
        .find_map(|(_, weak)| weak.upgrade())
}

impl CancelWatch {
    /// A new, unset watch, registered against its own flag's address.
    pub fn new() -> Arc<Self> {
        let watch = Arc::new(Self {
            flag: AtomicBool::new(false),
            origin: Instant::now(),
            origin_unix_s: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0.0, |since| since.as_secs_f64()),
            requested_ns: AtomicU64::new(UNSET),
            requested_phase: AtomicU8::new(CancelPhase::NotStarted.code()),
            requested_phase_index: AtomicU64::new(0),
            requested_evaluation: AtomicU64::new(0),
            first_observed_ns: AtomicU64::new(UNSET),
            first_observed_phase: AtomicU8::new(CancelPhase::NotStarted.code()),
            search_finished_ns: AtomicU64::new(UNSET),
            worker_joined_ns: AtomicU64::new(UNSET),
            phase: AtomicU8::new(CancelPhase::NotStarted.code()),
            phase_index: AtomicU64::new(0),
            phase_started_ns: AtomicU64::new(0),
            evaluations_started: AtomicU64::new(0),
            evaluations_completed: AtomicU64::new(0),
            evaluations_after_request: AtomicU64::new(0),
            longest_evaluation_ns: AtomicU64::new(0),
            longest_block_ns: AtomicU64::new(0),
            longest_block_evaluations: AtomicU64::new(0),
            external_started: AtomicU64::new(0),
            external_skipped: AtomicU64::new(0),
            external_terminated: AtomicU64::new(0),
            stop_reason: RwLock::new(None),
            termination: RwLock::new(None),
            events: RwLock::new(Vec::new()),
            events_dropped: AtomicU64::new(0),
        });
        let key = std::ptr::from_ref(&watch.flag) as usize;
        if let Ok(mut entries) = registry().write() {
            entries.retain(|(_, weak)| weak.strong_count() > 0);
            entries.push((key, Arc::downgrade(&watch)));
        }
        watch
    }

    /// The flag to hand to the existing `Option<&AtomicBool>` API.
    pub fn flag(&self) -> &AtomicBool {
        &self.flag
    }

    /// Seconds from this watch's origin, monotonic.
    fn now_ns(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_nanos()).unwrap_or(u64::MAX - 1)
    }

    /// Request cooperative cancellation.
    ///
    /// Records what was in flight *before* setting the flag, so the recorded
    /// phase is the one the drain is waiting on rather than whichever phase
    /// the run happened to reach while the record was being written. Setting
    /// the flag is the last step and uses `Release` ordering, so a reader that
    /// sees the flag set also sees the record.
    pub fn request_cancellation(&self) {
        let at = self.now_ns();
        let phase = self.phase.load(Ordering::Relaxed);
        self.requested_phase.store(phase, Ordering::Relaxed);
        self.requested_phase_index
            .store(self.phase_index.load(Ordering::Relaxed), Ordering::Relaxed);
        self.requested_evaluation.store(
            self.evaluations_completed.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        // `compare_exchange` keeps the first request's timestamp if a
        // supervisor asks twice; a second request does not restart the clock.
        let _ = self
            .requested_ns
            .compare_exchange(UNSET, at, Ordering::Relaxed, Ordering::Relaxed);
        self.record(
            CancelEventKind::Requested,
            String::from("supervisor requested cooperative cancellation"),
        );
        self.flag.store(true, Ordering::Release);
    }

    /// Record a free-text progress notice, so the drain's notices are part of
    /// the same record as its measurements.
    pub fn note_progress(&self, detail: impl Into<String>) {
        self.record(CancelEventKind::Progress, detail.into());
    }

    /// Record that the supervisor collected the worker.
    pub fn mark_worker_joined(&self) {
        let at = self.now_ns();
        let _ =
            self.worker_joined_ns
                .compare_exchange(UNSET, at, Ordering::Relaxed, Ordering::Relaxed);
        self.record(
            CancelEventKind::WorkerJoined,
            String::from("supervisor joined the worker"),
        );
    }

    /// Record the classified reason the run stopped and the optimizer's own
    /// termination string.
    pub fn set_stop_reason(&self, reason: StopReason, termination: Option<&str>) {
        if let Ok(mut slot) = self.stop_reason.write() {
            *slot = Some(reason);
        }
        if let Ok(mut slot) = self.termination.write() {
            *slot = termination.map(str::to_owned);
        }
    }

    /// Read the telemetry.
    pub fn snapshot(&self) -> CancelSnapshot {
        let optional = |value: u64| {
            if value == UNSET {
                None
            } else {
                Some(value as f64 * 1.0e-9)
            }
        };
        let requested_at_s = optional(self.requested_ns.load(Ordering::Relaxed));
        let first_observed_at_s = optional(self.first_observed_ns.load(Ordering::Relaxed));
        let search_finished_at_s = optional(self.search_finished_ns.load(Ordering::Relaxed));
        let worker_joined_at_s = optional(self.worker_joined_ns.load(Ordering::Relaxed));
        let since_request = |moment: Option<f64>| match (requested_at_s, moment) {
            (Some(requested), Some(moment)) => Some((moment - requested).max(0.0)),
            _ => None,
        };
        let positive = |value: u64| {
            if value == 0 {
                None
            } else {
                Some(value as f64 * 1.0e-9)
            }
        };
        let phase_started = self.phase_started_ns.load(Ordering::Relaxed) as f64 * 1.0e-9;
        let elapsed_s = self.now_ns() as f64 * 1.0e-9;
        CancelSnapshot {
            origin_unix_s: self.origin_unix_s,
            elapsed_s,
            requested: self.flag.load(Ordering::Acquire),
            requested_at_s,
            requested_during: CancelPhase::from_code(self.requested_phase.load(Ordering::Relaxed)),
            requested_during_index: self.requested_phase_index.load(Ordering::Relaxed),
            requested_at_evaluation: self.requested_evaluation.load(Ordering::Relaxed),
            first_observed_at_s,
            first_observed_in: CancelPhase::from_code(
                self.first_observed_phase.load(Ordering::Relaxed),
            ),
            acknowledgement_latency_s: since_request(first_observed_at_s),
            search_finished_at_s,
            drain_latency_s: since_request(search_finished_at_s),
            worker_joined_at_s,
            join_latency_s: since_request(worker_joined_at_s),
            phase: CancelPhase::from_code(self.phase.load(Ordering::Relaxed)),
            phase_index: self.phase_index.load(Ordering::Relaxed),
            phase_elapsed_s: (elapsed_s - phase_started).max(0.0),
            evaluations_started: self.evaluations_started.load(Ordering::Relaxed),
            evaluations_completed: self.evaluations_completed.load(Ordering::Relaxed),
            evaluations_after_request: self.evaluations_after_request.load(Ordering::Relaxed),
            longest_evaluation_s: positive(self.longest_evaluation_ns.load(Ordering::Relaxed)),
            longest_block_s: positive(self.longest_block_ns.load(Ordering::Relaxed)),
            longest_block_evaluations: self.longest_block_evaluations.load(Ordering::Relaxed),
            external_processes_started: self.external_started.load(Ordering::Relaxed),
            external_processes_skipped: self.external_skipped.load(Ordering::Relaxed),
            external_processes_terminated: self.external_terminated.load(Ordering::Relaxed),
            stop_reason: self.stop_reason.read().ok().and_then(|slot| *slot),
            termination: self
                .termination
                .read()
                .ok()
                .and_then(|slot| slot.as_ref().cloned()),
            events: self
                .events
                .read()
                .ok()
                .map(|events| events.clone())
                .unwrap_or_default(),
            events_dropped: self.events_dropped.load(Ordering::Relaxed),
        }
    }

    fn record(&self, kind: CancelEventKind, detail: String) {
        let event = CancelEvent {
            at_s: self.now_ns() as f64 * 1.0e-9,
            kind,
            phase: CancelPhase::from_code(self.phase.load(Ordering::Relaxed)),
            phase_index: self.phase_index.load(Ordering::Relaxed),
            evaluations_completed: self.evaluations_completed.load(Ordering::Relaxed),
            detail,
        };
        // A poisoned or full buffer loses the event and says so. Telemetry
        // never propagates a failure into the run it is measuring.
        match self.events.write() {
            Ok(mut events) if events.len() < EVENT_CAPACITY => events.push(event),
            Ok(_) | Err(_) => {
                self.events_dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

impl Drop for CancelWatch {
    /// Deregister before the allocation is released, so a later watch that
    /// lands on the same address can never be confused with this one.
    fn drop(&mut self) {
        let key = std::ptr::from_ref(&self.flag) as usize;
        if let Ok(mut entries) = registry().write() {
            entries.retain(|(candidate, weak)| *candidate != key && weak.strong_count() > 0);
        }
    }
}

/// The instrumentation handle a search phase holds.
///
/// Attaching is one registry lookup; every method is a handful of relaxed
/// atomic operations, or nothing at all when no watch owns the flag.
#[derive(Debug, Clone)]
pub struct CancelScope<'a> {
    flag: Option<&'a AtomicBool>,
    watch: Option<Arc<CancelWatch>>,
}

impl<'a> CancelScope<'a> {
    /// Attach to whatever telemetry owns `flag`.
    pub fn attach(flag: Option<&'a AtomicBool>) -> Self {
        let watch = flag.and_then(watch_for);
        Self { flag, watch }
    }

    /// The flag, for handing on to an inner API that takes one.
    pub fn flag(&self) -> Option<&'a AtomicBool> {
        self.flag
    }

    /// Whether cancellation has been requested.
    ///
    /// The first call that sees the flag set records the observation and the
    /// phase that made it: that pair is the acknowledgement latency and where
    /// it was paid.
    pub fn requested(&self) -> bool {
        let requested = self.flag.is_some_and(|flag| flag.load(Ordering::Acquire));
        if requested {
            if let Some(watch) = self.watch.as_ref() {
                let at = watch.now_ns();
                if watch
                    .first_observed_ns
                    .compare_exchange(UNSET, at, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    watch
                        .first_observed_phase
                        .store(watch.phase.load(Ordering::Relaxed), Ordering::Relaxed);
                    watch.record(
                        CancelEventKind::FirstObserved,
                        String::from("the running search observed the cancellation request"),
                    );
                }
            }
        }
        requested
    }

    /// Enter `phase` with a phase-local index.
    pub fn enter(&self, phase: CancelPhase, index: u64) {
        let Some(watch) = self.watch.as_ref() else {
            return;
        };
        watch.phase.store(phase.code(), Ordering::Relaxed);
        watch.phase_index.store(index, Ordering::Relaxed);
        watch
            .phase_started_ns
            .store(watch.now_ns(), Ordering::Relaxed);
        // Phase entries are the skeleton of the timeline, so they are always
        // recorded; they happen per block or per generation, not per
        // evaluation.
        watch.record(
            CancelEventKind::PhaseEntered,
            format!("{} #{index}", phase.as_str()),
        );
    }

    /// Run and time one coupled evaluation.
    ///
    /// The duration recorded here *is* the cancellation bound wherever the
    /// flag is checked once per evaluation, so it is measured rather than
    /// assumed.
    pub fn evaluation<T>(&self, work: impl FnOnce() -> T) -> T {
        let Some(watch) = self.watch.as_ref() else {
            return work();
        };
        watch.evaluations_started.fetch_add(1, Ordering::Relaxed);
        let started = Instant::now();
        let value = work();
        let elapsed = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX - 1);
        watch
            .longest_evaluation_ns
            .fetch_max(elapsed, Ordering::Relaxed);
        watch.evaluations_completed.fetch_add(1, Ordering::Relaxed);
        if watch.flag.load(Ordering::Acquire) {
            watch
                .evaluations_after_request
                .fetch_add(1, Ordering::Relaxed);
        }
        value
    }

    /// Run and time one uninterruptible block of `evaluations` candidates.
    ///
    /// Where a search checks the flag per block rather than per evaluation,
    /// this duration is the bound, and the pair (duration, count) is what
    /// makes a block bound comparable with a per-evaluation one.
    pub fn block<T>(&self, evaluations: u64, work: impl FnOnce() -> T) -> T {
        let Some(watch) = self.watch.as_ref() else {
            return work();
        };
        watch
            .evaluations_started
            .fetch_add(evaluations, Ordering::Relaxed);
        let started = Instant::now();
        let value = work();
        let elapsed = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX - 1);
        if watch.longest_block_ns.fetch_max(elapsed, Ordering::Relaxed) < elapsed {
            watch
                .longest_block_evaluations
                .store(evaluations, Ordering::Relaxed);
        }
        watch
            .evaluations_completed
            .fetch_add(evaluations, Ordering::Relaxed);
        if watch.flag.load(Ordering::Acquire) {
            watch
                .evaluations_after_request
                .fetch_add(evaluations, Ordering::Relaxed);
        }
        value
    }

    /// Record that work was skipped because cancellation was already
    /// requested. This is preserved effort, and the report says how much.
    pub fn work_skipped(&self, detail: impl Into<String>) {
        if let Some(watch) = self.watch.as_ref() {
            watch.record(CancelEventKind::WorkSkipped, detail.into());
        }
    }

    /// Record a supervised external process launch.
    pub fn external_started(&self, detail: impl Into<String>) {
        if let Some(watch) = self.watch.as_ref() {
            watch.external_started.fetch_add(1, Ordering::Relaxed);
            watch.record(CancelEventKind::ExternalProcessStarted, detail.into());
        }
    }

    /// Record a suppressed external process launch.
    pub fn external_skipped(&self, detail: impl Into<String>) {
        if let Some(watch) = self.watch.as_ref() {
            watch.external_skipped.fetch_add(1, Ordering::Relaxed);
            watch.record(CancelEventKind::ExternalProcessSkipped, detail.into());
        }
    }

    /// Record a terminated external process.
    pub fn external_terminated(&self, detail: impl Into<String>) {
        if let Some(watch) = self.watch.as_ref() {
            watch.external_terminated.fetch_add(1, Ordering::Relaxed);
            watch.record(CancelEventKind::ExternalProcessTerminated, detail.into());
        }
    }

    /// Record that the search returned, with its own termination string.
    pub fn search_finished(&self, termination: &str) {
        let Some(watch) = self.watch.as_ref() else {
            return;
        };
        let at = watch.now_ns();
        let _ = watch.search_finished_ns.compare_exchange(
            UNSET,
            at,
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
        watch
            .phase
            .store(CancelPhase::SearchFinished.code(), Ordering::Relaxed);
        watch.set_stop_reason(StopReason::from_termination(termination), Some(termination));
        watch.record(
            CancelEventKind::SearchFinished,
            format!("search returned with termination {termination}"),
        );
    }

    /// The telemetry, if any is attached.
    pub fn snapshot(&self) -> Option<CancelSnapshot> {
        self.watch.as_ref().map(|watch| watch.snapshot())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn an_unwatched_flag_leaves_every_telemetry_call_inert() {
        let flag = AtomicBool::new(false);
        let scope = CancelScope::attach(Some(&flag));
        scope.enter(CancelPhase::DeGeneration, 3);
        assert!(!scope.requested());
        assert_eq!(scope.evaluation(|| 7), 7);
        assert!(scope.snapshot().is_none());
        flag.store(true, Ordering::Relaxed);
        assert!(scope.requested());
    }

    #[test]
    fn a_watch_records_the_phase_in_flight_when_the_request_arrived() {
        let watch = CancelWatch::new();
        let scope = CancelScope::attach(Some(watch.flag()));
        scope.enter(CancelPhase::ScreeningScanBlock, 0);
        scope.enter(CancelPhase::DeGeneration, 4);
        watch.request_cancellation();
        // The search only observes it at its next boundary.
        scope.enter(CancelPhase::DeGeneration, 5);
        assert!(scope.requested());
        scope.search_finished("cancelled");

        let snapshot = watch.snapshot();
        assert!(snapshot.requested);
        assert_eq!(snapshot.requested_during, CancelPhase::DeGeneration);
        assert_eq!(snapshot.requested_during_index, 4);
        assert_eq!(snapshot.first_observed_in, CancelPhase::DeGeneration);
        assert_eq!(snapshot.stop_reason, Some(StopReason::Cancelled));
        assert_eq!(snapshot.termination.as_deref(), Some("cancelled"));
        let acknowledgement = snapshot
            .acknowledgement_latency_s
            .expect("a requested and observed cancellation has an acknowledgement latency");
        let drain = snapshot
            .drain_latency_s
            .expect("a finished search has a drain latency");
        assert!(acknowledgement >= 0.0);
        assert!(drain >= acknowledgement);
        assert!(snapshot.acknowledged_within(1.0));
    }

    #[test]
    fn evaluation_and_block_timings_separate_the_two_cancellation_bounds() {
        let watch = CancelWatch::new();
        let scope = CancelScope::attach(Some(watch.flag()));
        scope.evaluation(|| thread::sleep(Duration::from_millis(5)));
        scope.block(4, || thread::sleep(Duration::from_millis(20)));
        let snapshot = watch.snapshot();
        assert_eq!(snapshot.evaluations_completed, 5);
        assert_eq!(snapshot.longest_block_evaluations, 4);
        let single = snapshot
            .longest_evaluation_s
            .expect("one evaluation was timed");
        let block = snapshot.longest_block_s.expect("one block was timed");
        assert!(single >= 0.004, "{single}");
        assert!(block > single, "block {block} single {single}");
    }

    #[test]
    fn evaluations_after_the_request_are_counted_separately() {
        let watch = CancelWatch::new();
        let scope = CancelScope::attach(Some(watch.flag()));
        scope.evaluation(|| ());
        watch.request_cancellation();
        scope.evaluation(|| ());
        scope.evaluation(|| ());
        let snapshot = watch.snapshot();
        assert_eq!(snapshot.evaluations_completed, 3);
        assert_eq!(snapshot.evaluations_after_request, 2);
        assert_eq!(snapshot.requested_at_evaluation, 1);
    }

    #[test]
    fn a_dropped_watch_leaves_no_registry_entry_behind() {
        let address = {
            let watch = CancelWatch::new();
            let address = std::ptr::from_ref(watch.flag()) as usize;
            assert!(watch_for(watch.flag()).is_some());
            address
        };
        let entries = registry()
            .read()
            .expect("the registry lock is not poisoned")
            .iter()
            .filter(|(key, _)| *key == address)
            .count();
        assert_eq!(entries, 0, "a dropped watch must deregister itself");
    }

    #[test]
    fn only_convergence_counts_as_convergence() {
        assert!(StopReason::from_termination("converged").is_convergence());
        for termination in [
            "cancelled",
            "watchdog",
            "iteration_limit",
            "evaluation_budget",
            "mesh_limit",
            "something_new",
        ] {
            assert!(
                !StopReason::from_termination(termination).is_convergence(),
                "{termination}"
            );
        }
        assert_eq!(
            StopReason::from_termination("something_new"),
            StopReason::Unclassified
        );
    }

    #[test]
    fn two_watches_do_not_see_each_others_telemetry() {
        let first = CancelWatch::new();
        let second = CancelWatch::new();
        let first_scope = CancelScope::attach(Some(first.flag()));
        let second_scope = CancelScope::attach(Some(second.flag()));
        first.request_cancellation();
        assert!(first_scope.requested());
        assert!(!second_scope.requested());
        assert!(first.snapshot().requested);
        assert!(!second.snapshot().requested);
        second_scope.evaluation(|| ());
        assert_eq!(first.snapshot().evaluations_completed, 0);
        assert_eq!(second.snapshot().evaluations_completed, 1);
    }
}
