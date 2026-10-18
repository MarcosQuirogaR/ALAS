// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in wall-clock stage timers for one optimizer evaluation.
//!
//! Off by default and zero-cost when off: [`scope`] and [`count`] read one
//! relaxed atomic and return. Switch on with the environment variable
//! `ALAS_EVAL_PROFILE=1` (read once, at the first call) or with [`enable`].
//! The timers never touch a numerical value, so results are bit-identical
//! with the profile on and off.
//!
//! Times are *exclusive*: a scope entered inside another scope on the same
//! thread pauses the parent's clock, so the stage times of one thread sum to
//! the wall time of its outermost scope and "where does the time go" has one
//! answer per instant. Work a stage hands to the rayon pool is charged to
//! the stage on the calling thread (its wall time). With one search worker
//! the table is a wall-clock split; with several workers the per-stage sums
//! are core-seconds-like and only the fractions are meaningful.
//!
//! [`Report`] also carries the *inclusive* wall time of each stage (its
//! scopes from entry to drop, children included); inclusive times of nested
//! stages overlap and do not sum, and a stage entered inside itself counts
//! twice. Use them to ask what a whole step (for example the pipeline mission
//! stage) costs, and the exclusive times to split an evaluation.
//!
//! Units: seconds (reported), event counts (dimensionless).

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::time::Instant;

/// A timed stage of one candidate evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Stage {
    /// Whole-evaluation scope; its exclusive time is the unattributed remainder.
    Evaluation,
    /// Candidate geometry build.
    Geometry,
    /// First mass and centre-of-gravity pass, including takeoff fuel loading.
    MassFirstPass,
    /// Mass passes inside the closure (structure sizing, payload layout).
    MassClosurePass,
    /// Trim solve and performance evaluation, excluding the lattice work.
    Trim,
    /// Vortex-lattice meshing, influence matrix and LU factorization.
    VlmAssembly,
    /// Vortex-lattice back-solves and force integration.
    VlmSolve,
    /// Influence-matrix and near-field kernel entries recomputed for a lattice
    /// that differs from the cached one in some panels (e.g. a rotated tail).
    VlmFill,
    /// Dense LU of the influence matrix after a partial update.
    VlmFactor,
    /// Influence-matrix and kernel fill for a lattice with no usable cache.
    VlmColdFill,
    /// Dense LU of the influence matrix with no usable cache.
    VlmColdFactor,
    /// Trefftz-plane drag operator and evaluation.
    VlmTrefftz,
    /// Trimmed drag table, excluding the lattice work.
    DragTable,
    /// Route, deck and mission-model construction before the closure.
    MissionSetup,
    /// Dispatch iteration: the mission trips flown for every closure pass.
    MissionTrips,
    /// Off-design route flight of the closed aircraft.
    Offdesign,
    /// Main-gear placement rule and ledgers.
    GearPlacement,
    /// Residual table and cost assembly.
    Residuals,
    /// Pipeline mission stage: fuel-loading plan and the route priced on the
    /// segment mission model (the same closure the optimizer flies).
    PipelineRouteDispatch,
    /// Pipeline mission stage: the pseudospectral native flight (telemetry).
    PipelineTelemetryFlight,
    /// Lift surrogate training (legacy vorlax lattice grid) inside the
    /// pipeline mission stage.
    LiftSurrogateTrain,
    /// Pipeline mission stage: fuel-loading plan of the report.
    PipelineFuelLoading,
    /// Pipeline mission stage: segment mission model rebuilt from the report.
    PipelineMissionModel,
    /// Pipeline mission stage: planned dispatch iteration of the route.
    PipelinePlannedDispatch,
}

/// Number of [`Stage`] variants.
pub const STAGE_COUNT: usize = 24;

/// Every stage, in report order.
pub const STAGES: [Stage; STAGE_COUNT] = [
    Stage::Evaluation,
    Stage::Geometry,
    Stage::MassFirstPass,
    Stage::MassClosurePass,
    Stage::Trim,
    Stage::VlmAssembly,
    Stage::VlmSolve,
    Stage::VlmFill,
    Stage::VlmFactor,
    Stage::VlmColdFill,
    Stage::VlmColdFactor,
    Stage::VlmTrefftz,
    Stage::DragTable,
    Stage::MissionSetup,
    Stage::MissionTrips,
    Stage::Offdesign,
    Stage::GearPlacement,
    Stage::Residuals,
    Stage::PipelineRouteDispatch,
    Stage::PipelineTelemetryFlight,
    Stage::LiftSurrogateTrain,
    Stage::PipelineFuelLoading,
    Stage::PipelineMissionModel,
    Stage::PipelinePlannedDispatch,
];

impl Stage {
    /// Stable report key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Evaluation => "other_unattributed",
            Self::Geometry => "geometry_build",
            Self::MassFirstPass => "mass_cg_first_pass",
            Self::MassClosurePass => "mass_cg_closure_passes",
            Self::Trim => "trim_excl_vlm",
            Self::VlmAssembly => "vlm_assembly",
            Self::VlmSolve => "vlm_solves",
            Self::VlmFill => "vlm_incremental_fill",
            Self::VlmFactor => "vlm_incremental_lu",
            Self::VlmColdFill => "vlm_cold_fill",
            Self::VlmColdFactor => "vlm_cold_lu",
            Self::VlmTrefftz => "vlm_trefftz",
            Self::DragTable => "drag_table_excl_vlm",
            Self::MissionSetup => "mission_setup",
            Self::MissionTrips => "mission_trips",
            Self::Offdesign => "offdesign_route",
            Self::GearPlacement => "gear_placement",
            Self::Residuals => "residuals_cost",
            Self::PipelineRouteDispatch => "pipeline_mission_route_dispatch",
            Self::PipelineTelemetryFlight => "pipeline_mission_telemetry_flight",
            Self::LiftSurrogateTrain => "lift_surrogate_train_vorlax",
            Self::PipelineFuelLoading => "pipeline_mission_fuel_loading",
            Self::PipelineMissionModel => "pipeline_mission_model_build",
            Self::PipelinePlannedDispatch => "pipeline_mission_planned_dispatch",
        }
    }
}

/// An event counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Counter {
    /// Evaluations (outermost scopes).
    Evaluations,
    /// Complete sizing closures (a gear re-placement adds a second one).
    Closures,
    /// Outer takeoff-mass passes across all closures.
    ClosurePasses,
    /// Trim solves after the first of a closure.
    Retrims,
    /// Lattice assemblies requested.
    VlmAssemblies,
    /// Assemblies that refactored an influence matrix.
    VlmRefactorizations,
    /// Assemblies with no usable cached lattice (first of a candidate).
    VlmColdAssemblies,
    /// Lattice back-solves.
    VlmSolves,
    /// Mission trip flights integrated.
    TripFlights,
    /// Engine deck evaluations.
    DeckEvals,
    /// Process-wide nominal-cache lookups (balance nominal etc.).
    NominalLookups,
    /// Nominal-cache lookups that had to resolve (size the registered design).
    NominalResolutions,
}

/// Number of [`Counter`] variants.
pub const COUNTER_COUNT: usize = 12;

/// Every counter, in report order.
pub const COUNTERS: [Counter; COUNTER_COUNT] = [
    Counter::Evaluations,
    Counter::Closures,
    Counter::ClosurePasses,
    Counter::Retrims,
    Counter::VlmAssemblies,
    Counter::VlmRefactorizations,
    Counter::VlmColdAssemblies,
    Counter::VlmSolves,
    Counter::TripFlights,
    Counter::DeckEvals,
    Counter::NominalLookups,
    Counter::NominalResolutions,
];

impl Counter {
    /// Stable report key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Evaluations => "evaluations",
            Self::Closures => "closures",
            Self::ClosurePasses => "closure_passes",
            Self::Retrims => "retrims",
            Self::VlmAssemblies => "vlm_assemblies",
            Self::VlmRefactorizations => "vlm_refactorizations",
            Self::VlmColdAssemblies => "vlm_cold_assemblies",
            Self::VlmSolves => "vlm_solves",
            Self::TripFlights => "trip_flights",
            Self::DeckEvals => "deck_evals",
            Self::NominalLookups => "nominal_lookups",
            Self::NominalResolutions => "nominal_resolutions",
        }
    }
}

const UNSET: u8 = 0;
const OFF: u8 = 1;
const ON: u8 = 2;

static STATE: AtomicU8 = AtomicU8::new(UNSET);
static NANOS: [AtomicU64; STAGE_COUNT] = [const { AtomicU64::new(0) }; STAGE_COUNT];
static CALLS: [AtomicU64; STAGE_COUNT] = [const { AtomicU64::new(0) }; STAGE_COUNT];
static INCLUSIVE_NANOS: [AtomicU64; STAGE_COUNT] = [const { AtomicU64::new(0) }; STAGE_COUNT];
static EVENTS: [AtomicU64; COUNTER_COUNT] = [const { AtomicU64::new(0) }; COUNTER_COUNT];

struct Frame {
    stage: Stage,
    entered: Instant,
    resumed: Instant,
}

thread_local! {
    static STACK: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

/// Whether the profile is recording.
#[inline]
pub fn enabled() -> bool {
    match STATE.load(Ordering::Relaxed) {
        ON => true,
        OFF => false,
        _ => init_from_environment(),
    }
}

#[cold]
fn init_from_environment() -> bool {
    let on = std::env::var("ALAS_EVAL_PROFILE")
        .map(|value| !matches!(value.trim(), "" | "0" | "false" | "off"))
        .unwrap_or(false);
    // A concurrent `enable` wins over the environment default.
    let _ = STATE.compare_exchange(
        UNSET,
        if on { ON } else { OFF },
        Ordering::Relaxed,
        Ordering::Relaxed,
    );
    STATE.load(Ordering::Relaxed) == ON
}

/// Start recording regardless of the environment.
pub fn enable() {
    STATE.store(ON, Ordering::Relaxed);
}

/// Zero every accumulator and counter (not the on/off state).
pub fn reset() {
    for slot in NANOS
        .iter()
        .chain(&CALLS)
        .chain(&INCLUSIVE_NANOS)
        .chain(&EVENTS)
    {
        slot.store(0, Ordering::Relaxed);
    }
}

/// Add `n` to `counter`.
#[inline]
pub fn count(counter: Counter, n: u64) {
    if enabled() {
        EVENTS[counter as usize].fetch_add(n, Ordering::Relaxed);
    }
}

fn charge(stage: Stage, from: Instant, to: Instant) {
    let nanos = u64::try_from(to.saturating_duration_since(from).as_nanos()).unwrap_or(u64::MAX);
    NANOS[stage as usize].fetch_add(nanos, Ordering::Relaxed);
}

/// Guard returned by [`scope`]; the stage's clock stops when it drops.
#[derive(Debug)]
#[must_use = "the stage is timed until this guard drops"]
pub struct Scope {
    active: bool,
}

/// Time `stage` until the returned guard drops, pausing any enclosing stage.
#[inline]
pub fn scope(stage: Stage) -> Scope {
    if !enabled() {
        return Scope { active: false };
    }
    let now = Instant::now();
    CALLS[stage as usize].fetch_add(1, Ordering::Relaxed);
    STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        if let Some(parent) = stack.last() {
            charge(parent.stage, parent.resumed, now);
        }
        stack.push(Frame {
            stage,
            entered: now,
            resumed: now,
        });
    });
    if stage == Stage::Evaluation {
        count(Counter::Evaluations, 1);
    }
    Scope { active: true }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let now = Instant::now();
        STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            if let Some(frame) = stack.pop() {
                charge(frame.stage, frame.resumed, now);
                let nanos = now.saturating_duration_since(frame.entered).as_nanos();
                INCLUSIVE_NANOS[frame.stage as usize]
                    .fetch_add(u64::try_from(nanos).unwrap_or(u64::MAX), Ordering::Relaxed);
            }
            if let Some(parent) = stack.last_mut() {
                parent.resumed = now;
            }
        });
    }
}

/// [`scope`] for [`Stage::MassClosurePass`] (short form for size-budgeted callers).
pub fn mass_closure_pass() -> Scope {
    scope(Stage::MassClosurePass)
}

/// [`scope`] for [`Stage::MissionTrips`].
pub fn mission_trips() -> Scope {
    scope(Stage::MissionTrips)
}

/// [`scope`] for [`Stage::MissionSetup`].
pub fn mission_setup() -> Scope {
    scope(Stage::MissionSetup)
}

/// [`scope`] for [`Stage::Offdesign`].
pub fn offdesign() -> Scope {
    scope(Stage::Offdesign)
}

/// A snapshot of the accumulators.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// `(key, exclusive seconds, scope entries, inclusive seconds)` per
    /// stage, in report order.
    pub stages: Vec<(&'static str, f64, u64, f64)>,
    /// `(key, count)` per counter.
    pub counters: Vec<(&'static str, u64)>,
}

impl Report {
    /// Sum of the exclusive stage times, s.
    pub fn total_s(&self) -> f64 {
        self.stages.iter().map(|stage| stage.1).sum()
    }
}

/// Read the accumulators.
pub fn report() -> Report {
    Report {
        stages: STAGES
            .iter()
            .map(|&stage| {
                (
                    stage.key(),
                    NANOS[stage as usize].load(Ordering::Relaxed) as f64 * 1.0e-9,
                    CALLS[stage as usize].load(Ordering::Relaxed),
                    INCLUSIVE_NANOS[stage as usize].load(Ordering::Relaxed) as f64 * 1.0e-9,
                )
            })
            .collect(),
        counters: COUNTERS
            .iter()
            .map(|&counter| {
                (
                    counter.key(),
                    EVENTS[counter as usize].load(Ordering::Relaxed),
                )
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_scopes_are_exclusive_and_sum_to_the_outer_wall() {
        enable();
        reset();
        let outer = Instant::now();
        {
            let _evaluation = scope(Stage::Evaluation);
            std::thread::sleep(std::time::Duration::from_millis(20));
            {
                let _trim = scope(Stage::Trim);
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
        }
        let wall = outer.elapsed().as_secs_f64();
        let report = report();
        let get = |key: &str| report.stages.iter().find(|s| s.0 == key).map(|s| s.1);
        let other = get("other_unattributed").unwrap_or(f64::NAN);
        let trim = get("trim_excl_vlm").unwrap_or(f64::NAN);
        assert!((0.018..0.2).contains(&other), "other {other}");
        assert!((0.028..0.2).contains(&trim), "trim {trim}");
        assert!((report.total_s() - wall).abs() < 0.01);
    }
}
