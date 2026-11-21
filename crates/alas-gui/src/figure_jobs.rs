// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Result figures built off the UI thread.
//!
//! Building the ~46 result figures of a run cost 6 s (A320-200) to 13 s
//! (ATR72-600) on the UI thread (measured), once at the end of a run and again
//! whenever the theme, language or solver view changed the gallery keys. A
//! gallery card that misses the cache now queues its figure here and shows a
//! placeholder; worker threads build it from a snapshot of the result and the
//! UI thread stores the scene in `result_figure_cache` when it arrives
//! ([`AppState::poll_figure_jobs`]), so the cache, its revisions and every
//! consumer (fullscreen view, export) are unchanged.
//!
//! Thread safety: a figure is built from `(PipelineResult, SolverResultView,
//! config, theme, camera)` alone (`build_result_figure_detached`). The only
//! thread-local state a build reads is the display language
//! (`alas_i18n::set_language`, set per job) and the plot-data capture guard
//! (opened and closed inside the build), so a build on a worker equals one on
//! the UI thread.
//!
//! Jobs carry the epoch of the cache they were queued against. Every place that
//! clears `result_figure_cache` goes through
//! [`AppState::clear_result_figures`], which bumps the epoch, so a scene built
//! from superseded data is dropped on arrival instead of being cached.

use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use alas_report::scene::{Camera3D, Scene};

use crate::state::AppState;
use crate::views::results_view::SolverResultView;

struct Job {
    key: String,
    id: String,
    epoch: u64,
    result: Arc<alas_pipeline::PipelineResult>,
    view: SolverResultView,
    theme: String,
    language: String,
    camera: Option<Camera3D>,
}

struct Done {
    key: String,
    epoch: u64,
    scene: Option<Scene>,
}

#[derive(Default)]
struct Queue {
    /// Figures a visible card waits for.
    urgent: VecDeque<Job>,
    /// The background completeness check of a finished run.
    background: VecDeque<Job>,
    closed: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    epoch: AtomicU64,
}

/// What a gallery card gets for a figure key.
pub(crate) enum FigureLookup {
    /// The cached scene (`None`: this run has no data for the figure).
    Ready(Option<Arc<Scene>>),
    /// Queued or building; the card shows a placeholder.
    Pending,
}

/// The queue, the workers and what is in flight.
pub(crate) struct FigureJobs {
    shared: Arc<Shared>,
    tx: Sender<Done>,
    rx: Receiver<Done>,
    ctx: Option<egui::Context>,
    enabled: bool,
    epoch: u64,
    snapshot: Option<Arc<alas_pipeline::PipelineResult>>,
    /// Keys queued or building, visible cards and background check together.
    pending: BTreeSet<String>,
    /// The subset a visible card waits for.
    urgent: BTreeSet<String>,
    workers_started: bool,
    /// The run whose figures the background check covers, until it is reported.
    final_check_epoch: Option<u64>,
}

impl Default for FigureJobs {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            shared: Arc::new(Shared {
                queue: Mutex::new(Queue::default()),
                wake: Condvar::new(),
                epoch: AtomicU64::new(0),
            }),
            tx,
            rx,
            ctx: None,
            enabled: false,
            epoch: 0,
            snapshot: None,
            pending: BTreeSet::new(),
            urgent: BTreeSet::new(),
            workers_started: false,
            final_check_epoch: None,
        }
    }
}

impl Drop for FigureJobs {
    fn drop(&mut self) {
        if let Ok(mut queue) = self.shared.queue.lock() {
            queue.closed = true;
            queue.urgent.clear();
            queue.background.clear();
        }
        self.shared.wake.notify_all();
    }
}

impl FigureJobs {
    /// Called every frame by the shell: workers need the context to wake the
    /// UI when a figure is ready. Unit tests keep building on the calling
    /// thread unless a test enables the workers explicitly.
    pub(crate) fn attach(&mut self, ctx: &egui::Context) {
        if self.ctx.is_none() {
            self.ctx = Some(ctx.clone());
            self.enabled = !cfg!(test);
        }
    }

    /// Whether figures are built on worker threads.
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    /// Visible cards still waiting for a figure.
    pub(crate) fn urgent_pending(&self) -> usize {
        self.urgent.len()
    }

    /// Figures queued or building, including the background check.
    #[cfg(test)]
    pub(crate) fn pending(&self) -> usize {
        self.pending.len()
    }

    #[cfg(test)]
    pub(crate) fn enable_for_test(&mut self, ctx: &egui::Context) {
        self.ctx = Some(ctx.clone());
        self.enabled = true;
    }

    fn invalidate(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.shared.epoch.store(self.epoch, Ordering::SeqCst);
        self.snapshot = None;
        self.pending.clear();
        self.urgent.clear();
        self.final_check_epoch = None;
        if let Ok(mut queue) = self.shared.queue.lock() {
            queue.urgent.clear();
            queue.background.clear();
        }
    }

    fn ensure_workers(&mut self) {
        if self.workers_started {
            return;
        }
        self.workers_started = true;
        let threads = std::thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get)
            .div_euclid(4)
            .clamp(1, 3);
        for index in 0..threads {
            let shared = Arc::clone(&self.shared);
            let tx = self.tx.clone();
            let ctx = self.ctx.clone();
            let _ = crate::worker_stack::spawn_analysis_thread(
                &format!("alas-figures-{index}"),
                move || worker_loop(&shared, &tx, ctx.as_ref()),
            );
        }
    }
}

fn next_job(shared: &Shared) -> Option<Job> {
    let mut queue = shared.queue.lock().ok()?;
    loop {
        if queue.closed {
            return None;
        }
        if let Some(job) = queue
            .urgent
            .pop_front()
            .or_else(|| queue.background.pop_front())
        {
            return Some(job);
        }
        queue = shared.wake.wait(queue).ok()?;
    }
}

fn worker_loop(shared: &Shared, tx: &Sender<Done>, ctx: Option<&egui::Context>) {
    while let Some(job) = next_job(shared) {
        if job.epoch != shared.epoch.load(Ordering::SeqCst) {
            continue;
        }
        alas_i18n::set_language(Some(&job.language));
        let started = Instant::now();
        // A figure builder that panics must not take the worker (and every
        // later figure) with it: the card reports the figure as unavailable.
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::scene::build_result_figure_detached(
                &job.result,
                job.view,
                &job.id,
                &job.result.config,
                &job.theme,
                job.camera,
            )
        }));
        trace_build(&job.id, started.elapsed());
        let scene = built.ok().flatten().flatten();
        let _ = tx.send(Done {
            key: job.key,
            epoch: job.epoch,
            scene,
        });
        if let Some(ctx) = ctx {
            ctx.request_repaint();
        }
    }
}

// Diagnostic output on request; stderr is the tour's only channel.
#[allow(clippy::print_stderr)]
fn trace_build(id: &str, elapsed: std::time::Duration) {
    if std::env::var_os("ALAS_PERF_TRACE").is_some_and(|value| !value.is_empty()) {
        eprintln!("figure build {id}: {:.1} ms", elapsed.as_secs_f64() * 1e3);
    }
}

impl AppState {
    /// Drop every cached result figure and everything queued for the old data.
    pub(crate) fn clear_result_figures(&mut self) {
        self.result_figure_cache.clear();
        self.figure_jobs.invalidate();
    }

    /// The figure for a gallery card: the cached scene, else a queued build.
    ///
    /// Without workers (unit tests, headless callers) or without a result the
    /// figure is built on the calling thread, as before.
    pub(crate) fn result_figure_background(
        &mut self,
        key: &str,
        id: &str,
        theme: &str,
        camera: Option<Camera3D>,
    ) -> FigureLookup {
        if let Some((scene, _)) = self.result_figure_cache.get(key) {
            return FigureLookup::Ready(scene.clone());
        }
        if !self.figure_jobs.enabled || self.pipeline_result.is_none() {
            let config = self
                .pipeline_result
                .as_ref()
                .map(|result| result.config.clone())
                .unwrap_or_default();
            return FigureLookup::Ready(
                self.cached_result_figure_with_camera(key, id, &config, theme, camera),
            );
        }
        if self.figure_jobs.urgent.contains(key) {
            return FigureLookup::Pending;
        }
        if self.figure_jobs.pending.contains(key) {
            // Queued by the background check: a visible card goes first.
            if let Ok(mut queue) = self.figure_jobs.shared.queue.lock() {
                if let Some(at) = queue.background.iter().position(|job| job.key == key) {
                    if let Some(job) = queue.background.remove(at) {
                        queue.urgent.push_back(job);
                    }
                }
            }
            self.figure_jobs.urgent.insert(key.to_owned());
            self.figure_jobs.shared.wake.notify_one();
            return FigureLookup::Pending;
        }
        self.queue_figure(key, id, theme, camera, true);
        FigureLookup::Pending
    }

    fn queue_figure(
        &mut self,
        key: &str,
        id: &str,
        theme: &str,
        camera: Option<Camera3D>,
        urgent: bool,
    ) {
        let Some(result) = self.pipeline_result.as_ref() else {
            return;
        };
        let jobs = &mut self.figure_jobs;
        let snapshot = match &jobs.snapshot {
            Some(snapshot) => Arc::clone(snapshot),
            None => {
                let started = Instant::now();
                let snapshot = Arc::new(result.clone());
                trace_build("result snapshot clone", started.elapsed());
                jobs.snapshot = Some(Arc::clone(&snapshot));
                snapshot
            }
        };
        let job = Job {
            key: key.to_owned(),
            id: id.to_owned(),
            epoch: jobs.epoch,
            result: snapshot,
            view: self.selected_solver_view,
            theme: theme.to_owned(),
            language: alas_i18n::get_language(),
            camera,
        };
        jobs.pending.insert(key.to_owned());
        if urgent {
            jobs.urgent.insert(key.to_owned());
        }
        jobs.ensure_workers();
        if let Ok(mut queue) = jobs.shared.queue.lock() {
            if urgent {
                queue.urgent.push_back(job);
            } else {
                queue.background.push_back(job);
            }
        }
        jobs.shared.wake.notify_one();
    }

    /// Store the figures the workers finished. Call once per frame.
    pub(crate) fn poll_figure_jobs(&mut self) {
        let mut arrived = false;
        while let Ok(done) = self.figure_jobs.rx.try_recv() {
            if done.epoch != self.figure_jobs.epoch {
                continue;
            }
            self.figure_jobs.pending.remove(&done.key);
            self.figure_jobs.urgent.remove(&done.key);
            if !self.result_figure_cache.contains_key(&done.key) {
                self.insert_result_figure(&done.key, done.scene.map(Arc::new));
            }
            arrived = true;
        }
        if arrived || self.figure_jobs.final_check_epoch.is_some() {
            self.finish_final_figure_check_when_ready();
        }
    }

    /// Queue every result figure of the finished run that is not cached yet
    /// and report the final figure check when the last one has arrived.
    pub(crate) fn begin_final_figure_check(&mut self) {
        if !self.pipeline_result_complete || self.pipeline_result.is_none() {
            return;
        }
        if !self.figure_jobs.enabled {
            self.verify_final_result_figures();
            return;
        }
        let theme = self.theme.figure_theme_name().to_owned();
        for figure in alas_report::RESULT_FIGURES {
            let key = self.gallery_figure_key(figure.id);
            if self.result_figure_cache.contains_key(&key)
                || self.figure_jobs.pending.contains(&key)
            {
                continue;
            }
            let camera = crate::views::result_3d::is_orbitable_result(figure.id).then(|| {
                let camera_key =
                    crate::views::result_3d::result_camera_key(self.run_identity, figure.id);
                crate::views::result_3d::result_camera(self, &camera_key)
            });
            self.queue_figure(&key, figure.id, &theme, camera, false);
        }
        self.figure_jobs.final_check_epoch = Some(self.figure_jobs.epoch);
        self.finish_final_figure_check_when_ready();
    }

    fn finish_final_figure_check_when_ready(&mut self) {
        let Some(epoch) = self.figure_jobs.final_check_epoch else {
            return;
        };
        if epoch != self.figure_jobs.epoch {
            self.figure_jobs.final_check_epoch = None;
            return;
        }
        if !self.figure_jobs.pending.is_empty() {
            return;
        }
        self.figure_jobs.final_check_epoch = None;
        let started = Instant::now();
        self.report_final_figure_check();
        trace_build("final figure check report", started.elapsed());
    }

    /// Whether the figures of the last finished run are all in the cache and
    /// its final check has been reported (the tour waits for it).
    pub(crate) fn figures_idle(&self) -> bool {
        self.figure_jobs.pending.is_empty() && self.figure_jobs.final_check_epoch.is_none()
    }

    /// Wait for every queued figure (tests and headless callers).
    #[cfg(test)]
    pub(crate) fn drain_figure_jobs(&mut self) {
        let started = Instant::now();
        while !self.figures_idle() && started.elapsed().as_secs() < 300 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            self.poll_figure_jobs();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completed_state() -> AppState {
        let mut config = alas_config::AlasConfig::default();
        config.mission.enabled = false;
        config.mses.enabled = false;
        config.structures.enabled = false;
        let result = alas_pipeline::DesignPipeline::new(config)
            .run(
                &alas_pipeline::PipelineOptions {
                    optimize: false,
                    compare_baseline: false,
                    quiet: true,
                    output_dir: None,
                    ..Default::default()
                },
                &alas_exec::RunEnvironment::default(),
            )
            .expect("native fixed-design result");
        let mut state = AppState::default();
        state.set_completed_pipeline_result(result);
        state.figure_jobs.enable_for_test(&egui::Context::default());
        state
    }

    /// A figure built on a worker is the figure the UI thread would build,
    /// in the display language of the thread that asked for it.
    #[test]
    fn worker_built_figures_equal_the_ui_thread_build_in_spanish() {
        alas_i18n::es::install();
        alas_i18n::set_language(Some("es"));
        let mut state = completed_state();
        let theme = state.theme.figure_theme_name().to_owned();
        let config = state.pipeline_result.as_ref().unwrap().config.clone();
        let figures = alas_report::RESULT_FIGURES;
        for figure in figures {
            let key = state.gallery_figure_key(figure.id);
            assert!(matches!(
                state.result_figure_background(&key, figure.id, &theme, None),
                FigureLookup::Pending
            ));
        }
        assert!(state.figure_jobs.urgent_pending() > 0);
        state.drain_figure_jobs();
        assert_eq!(state.figure_jobs.urgent_pending(), 0);
        for figure in figures {
            let key = state.gallery_figure_key(figure.id);
            let cached = state.result_figure_cache.get(&key).expect("arrived");
            let direct =
                crate::scene::build_result_figure(&state, figure.id, &config, &theme).flatten();
            assert_eq!(
                format!("{:?}", cached.0.as_deref()),
                format!("{:?}", direct.as_ref()),
                "{} differs between worker and UI thread",
                figure.id
            );
        }
        alas_i18n::set_language(Some("en"));
    }

    #[test]
    fn a_figure_built_for_superseded_data_is_dropped() {
        let mut state = completed_state();
        let theme = state.theme.figure_theme_name().to_owned();
        let key = state.gallery_figure_key("polar_comparison");
        assert!(matches!(
            state.result_figure_background(&key, "polar_comparison", &theme, None),
            FigureLookup::Pending
        ));
        state.clear_result_figures();
        assert_eq!(state.figure_jobs.pending(), 0);
        std::thread::sleep(std::time::Duration::from_millis(500));
        state.poll_figure_jobs();
        assert!(state.result_figure_cache.is_empty());
    }

    #[test]
    fn the_final_check_is_reported_once_every_figure_has_arrived() {
        let mut state = completed_state();
        state.begin_final_figure_check();
        assert!(!state.figures_idle());
        state.drain_figure_jobs();
        assert!(state.figures_idle());
        let reports = state
            .logs
            .iter()
            .filter(|line| line.text.starts_with("Final figure check:"))
            .count();
        assert_eq!(reports, 1);
    }
}
