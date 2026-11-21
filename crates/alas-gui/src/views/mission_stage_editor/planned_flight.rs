// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The flight the Mission page draws: the one the run will fly.
//!
//! The pipeline's mission stage flies the standard mission on a built aircraft
//! (dispatch, ladder fitted under the level the aircraft holds, reserves, the
//! sizing passes on the flown fuel). The page cannot know that from the profile
//! alone, so it runs that same flight (`alas_pipeline::quick_analysis::planned_flight`)
//! on a worker thread whenever the case changes, and draws the phases it flew.
//!
//! - **Debounce:** a change starts the flight after [`DEBOUNCE`] without
//!   another change; a further change cancels the flight in progress.
//! - **One worker:** a new flight starts only when the previous one has ended;
//!   a result for a case that is no longer the current one is cached under its
//!   own key and never shown for the current one.
//! - **Cache:** the result is kept by a hash of the configuration and the
//!   drawn aircraft, so returning to a case is instant.
//! - **Never stale:** until the flight of the current case lands the page draws
//!   the nominal schematic, dimmed, and says the flight is being computed.

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_pipeline::{Phase, ReachNote, SizingOutcome};
use alas_route::route::Route;
use egui::{Context, Id};

/// Quiet time after the last change before the flight starts.
const DEBOUNCE: Duration = Duration::from_secs(1);

/// Cases whose flight is kept.
const CACHE_CASES: usize = 8;

/// What a flight gave: the phases as flown and the level note.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Flown {
    pub phases: Vec<Phase>,
    pub reach_note: Option<ReachNote>,
    /// How the sizing of the takeoff mass on the flown mission ended.
    pub sizing: Option<SizingOutcome>,
}

/// Where the current case's flight stands.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Status {
    /// The flight of the current case.
    Ready(Flown),
    /// Waiting for the debounce, or flying.
    Computing,
    /// The flight did not complete: the reason.
    Failed(String),
}

type Outcome = Result<Flown, String>;

struct Running {
    key: u64,
    cancel: Arc<AtomicBool>,
    results: Receiver<(u64, Outcome)>,
}

#[derive(Default)]
struct Planner {
    wanted: Option<u64>,
    changed_at: Option<Instant>,
    running: Option<Running>,
    cache: HashMap<u64, Outcome>,
    order: VecDeque<u64>,
}

/// The planner shared through the egui memory.
#[derive(Clone, Default)]
struct Handle(Arc<Mutex<Planner>>);

/// The key of a case: the configuration the run uses and the aircraft drawn.
pub(crate) fn case_key(config: &AlasConfig, design: &DesignVector, route: Option<&Route>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(config)
        .unwrap_or_default()
        .hash(&mut hasher);
    format!("{design:?}").hash(&mut hasher);
    route
        .map(|route| route.total_distance_m().to_bits())
        .hash(&mut hasher);
    hasher.finish()
}

/// Fly the case on the calling thread (what the worker runs).
pub(crate) fn fly(
    config: &AlasConfig,
    design: &DesignVector,
    route: Option<Route>,
    cancel: &AtomicBool,
) -> Outcome {
    alas_pipeline::quick_analysis::planned_flight(config, design, route, cancel).map(|flight| {
        let sizing = flight
            .load_case
            .native_supplement
            .as_deref()
            .and_then(|supplement| supplement.sizing);
        Flown {
            phases: flight.phases,
            reach_note: flight.reach_note,
            sizing,
        }
    })
}

/// The status of the flight of `config` on `design`, starting, restarting or cancelling the
/// worker as the case changes.
/// The worker that flies a case: [`fly`], or a stand-in in a test.
pub(crate) type Worker = fn(&AlasConfig, &DesignVector, Option<Route>, &AtomicBool) -> Outcome;

/// [`poll_with`] on the real flight.
#[cfg(test)]
pub(crate) fn poll(
    ctx: &Context,
    config: &AlasConfig,
    design: &DesignVector,
    route: Option<&Route>,
) -> Status {
    poll_with(ctx, config, design, route, fly)
}

#[cfg(test)]
pub(crate) fn poll_with(
    ctx: &Context,
    config: &AlasConfig,
    design: &DesignVector,
    route: Option<&Route>,
    worker: Worker,
) -> Status {
    let key = case_key(config, design, route);
    poll_keyed(ctx, key, (config, design, route), worker)
}

/// [`poll_with`] for a caller that already holds the case's [`case_key`] (hashing the whole
/// configuration is not something to do in every frame).
pub(crate) fn poll_keyed(
    ctx: &Context,
    key: u64,
    (config, design, route): (&AlasConfig, &DesignVector, Option<&Route>),
    worker: Worker,
) -> Status {
    let handle: Handle = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Handle>(Id::new("planned_flight"))
            .clone()
    });
    let Ok(mut planner) = handle.0.lock() else {
        return Status::Computing;
    };
    let planner = &mut *planner;
    // Collect what the worker finished; a result is kept under its own key.
    let mut landed = None;
    if let Some(running) = &planner.running {
        match running.results.try_recv() {
            Ok(result) => landed = Some(result),
            // The worker ended without a result (it panicked): the case failed, it is not
            // left computing for ever.
            Err(TryRecvError::Disconnected) => {
                landed = Some((
                    running.key,
                    Err("the flight worker stopped without a result".to_owned()),
                ));
            }
            Err(TryRecvError::Empty) => {}
        }
    }
    if let Some((landed_key, outcome)) = landed {
        planner.running = None;
        if !matches!(&outcome, Err(error) if error.contains("cancelled")) {
            planner.cache.insert(landed_key, outcome);
            planner.order.retain(|kept| *kept != landed_key);
            planner.order.push_back(landed_key);
            while planner.order.len() > CACHE_CASES {
                if let Some(old) = planner.order.pop_front() {
                    planner.cache.remove(&old);
                }
            }
        }
    }
    if let Some(outcome) = planner.cache.get(&key) {
        return match outcome {
            Ok(flown) => Status::Ready(flown.clone()),
            Err(error) => Status::Failed(error.clone()),
        };
    }
    // A new case: restart the quiet time and stop the flight of the old one.
    if planner.wanted != Some(key) {
        planner.wanted = Some(key);
        planner.changed_at = Some(Instant::now());
        if let Some(running) = &planner.running {
            if running.key != key {
                running.cancel.store(true, Ordering::Relaxed);
            }
        }
    }
    let quiet = planner.changed_at.map_or(Duration::ZERO, |at| at.elapsed());
    if planner.running.is_none() {
        if quiet >= DEBOUNCE {
            let cancel = Arc::new(AtomicBool::new(false));
            let (sender, results) = channel();
            let (config, design, route, flag, repaint) = (
                config.clone(),
                *design,
                route.cloned(),
                Arc::clone(&cancel),
                ctx.clone(),
            );
            let spawned =
                crate::worker_stack::spawn_analysis_thread("alas-planned-flight", move || {
                    let outcome = worker(&config, &design, route, &flag);
                    let _ = sender.send((key, outcome));
                    repaint.request_repaint();
                });
            match spawned {
                Ok(_) => {
                    planner.running = Some(Running {
                        key,
                        cancel,
                        results,
                    });
                }
                Err(error) => {
                    planner.cache.insert(
                        key,
                        Err(format!("the flight worker could not start: {error}")),
                    );
                    planner.order.push_back(key);
                    ctx.request_repaint();
                }
            }
        } else {
            ctx.request_repaint_after(DEBOUNCE - quiet + Duration::from_millis(20));
        }
    } else {
        ctx.request_repaint_after(Duration::from_millis(250));
    }
    Status::Computing
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(name: &str) -> (AlasConfig, DesignVector) {
        let mut config =
            AlasConfig::from_value(&serde_json::json!({ "preset": name })).expect("preset");
        config.structures.run_nastran = false;
        config.structures.run_patran_export = false;
        let design = alas_config::presets::get(name)
            .expect("preset")
            .design_vector;
        (config, design)
    }

    /// The page never blocks a frame: a poll returns at once with the nominal schematic's
    /// status while the debounce runs and while the worker flies; the flight of the case lands,
    /// is the flight the pipeline's mission stage flies, is cached by case, and a changed case is
    /// never shown the flight of the earlier one.
    #[test]
    fn a_case_is_flown_in_the_background_after_the_debounce_and_cached() {
        let ctx = Context::default();
        let (config, design) = case("A320-200");
        let timed = |config: &AlasConfig| {
            let start = Instant::now();
            let status = poll(&ctx, config, &design, None);
            (status, start.elapsed())
        };
        let (first, took) = timed(&config);
        assert_eq!(first, Status::Computing);
        assert!(took < Duration::from_millis(50), "a poll took {took:?}");
        let mut slowest = took;
        let flown = loop {
            std::thread::sleep(Duration::from_millis(100));
            let (status, took) = timed(&config);
            slowest = slowest.max(took);
            match status {
                Status::Ready(flown) => break flown,
                Status::Computing => {}
                Status::Failed(error) => panic!("the flight failed: {error}"),
            }
        };
        assert!(
            slowest < Duration::from_millis(50),
            "slowest poll {slowest:?}"
        );
        assert!(flown.phases.iter().any(|phase| phase.is_reserve()));
        // Revisiting the case is instant and the same flight.
        let (again, took) = timed(&config);
        assert_eq!(again, Status::Ready(flown.clone()));
        assert!(took < Duration::from_millis(50));
        // It is the flight a direct call flies.
        let direct = fly(&config, &design, None, &AtomicBool::new(false)).expect("direct flight");
        assert_eq!(direct, flown);
        // A changed case starts again from the nominal schematic, never the earlier flight.
        let mut changed = config.clone();
        changed.mission.profile.cruise_1_air_speed_m_s += 1.0;
        assert_ne!(
            case_key(&changed, &design, None),
            case_key(&config, &design, None)
        );
        assert_eq!(timed(&changed).0, Status::Computing);
    }
    fn panicking(_: &AlasConfig, _: &DesignVector, _: Option<Route>, _: &AtomicBool) -> Outcome {
        panic!("the worker went down");
    }

    fn failing(_: &AlasConfig, _: &DesignVector, _: Option<Route>, _: &AtomicBool) -> Outcome {
        Err("the baseline analysis failed".to_owned())
    }

    /// A worker that panics or fails ends in "Flight not computed", never in Computing for ever,
    /// and the page can go on to another case.
    #[test]
    fn a_panicking_or_failing_worker_is_reported_not_left_computing() {
        for (worker, expected) in [
            (panicking as Worker, "stopped without a result"),
            (failing as Worker, "baseline analysis failed"),
        ] {
            let ctx = Context::default();
            let (config, design) = case("A320-200");
            let started = Instant::now();
            let error = loop {
                match poll_with(&ctx, &config, &design, None, worker) {
                    Status::Failed(error) => break error,
                    Status::Ready(_) => panic!("a stand-in worker gave a flight"),
                    Status::Computing => {
                        assert!(
                            started.elapsed() < Duration::from_secs(20),
                            "left computing"
                        );
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            };
            assert!(error.contains(expected), "{error}");
            // The failure is the case's answer until the case changes; a new case is computed.
            assert_eq!(
                poll_with(&ctx, &config, &design, None, worker),
                Status::Failed(error)
            );
        }
    }
}
