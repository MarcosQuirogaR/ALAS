// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! An opt-in performance tour of every page.
//!
//! Launch the app with `ALAS_PERF_TOUR=<output.csv>` to load a preset
//! (`ALAS_PERF_PRESET`, default `A380-800`), visit every page at Basic and at
//! Advanced detail, open the auxiliary windows and the Sandbox, and record per
//! frame the time `App::update` takes (`cpu_*`, milliseconds of wall time on
//! the UI thread) and the interval between consecutive updates (`interval_*`).
//! Every collapsing card is forced open while the tour runs (see
//! [`force_open`]) so the measured frame contains the whole page. The CSV has
//! one row per step; the process then exits.
//!
//! Each row also carries the update time of the step's first frame
//! (`first_frame_ms`, the cost of entering the page: its figures are built in
//! it) and the largest update time among the warm-up frames (`warmup_max_ms`).
//!
//! With `ALAS_PERF_RUN` set the tour first analyses the loaded preset as it is
//! drawn (no search, the external solvers and exports off, nothing written to
//! the output folder) and waits for the result, so the pages are measured with
//! a completed analysis; it then also visits every tab of the Results page.
//!
//! Figures are built and rasterized on worker threads, so the first frame of a
//! page no longer says when the page is complete: `time_to_ready_ms` is the
//! wall time from entering the step until no figure is queued and no
//! rasterization is running (placeholders gone), and sampling starts only
//! once the step is ready. After the analysis two `after_run` rows give the
//! update time of the frame that took the result (`results_visible`, the
//! freeze before the Results page is usable) and the time from that frame's
//! start until every figure of the run is built (`all_figures_ready`).
//!
//! With `ALAS_PERF_RUN` the tour also sweeps a synthetic pointer across the
//! Mission profile and the Aerodynamics polar (`hover:*`), pans and zooms
//! their 2-D views (`panzoom:*`) and watches a Results tab and the Aircraft
//! page for 3 s without requesting a repaint (`idle:*`; `idle_frames_per_s`
//! is the frames the app drew by itself after a 0.25 s settle, per second;
//! about 0 when it sleeps).
//!
//! The tour measures with the swap wait removed (see
//! [`crate::branding::native_options`]; `ALAS_PRESENT_MODE=vsync` restores it):
//! a detached viewport presents synchronously inside `App::update`, so with
//! vsync each native window would add up to a refresh interval of waiting to
//! a continuously repainting frame, which is not UI work. With
//! `ALAS_PERF_TRACE` set every step start and the boot/preset milestones are
//! written to stderr with the seconds since the tour began, which tells a
//! stalled tour (no frames at all) from a slow one.
//!
//! With the variable unset nothing is allocated and the only cost in the
//! normal path is one thread-local read per collapsing header.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::Context;

use crate::detail::Detail;
use crate::sandbox::fields::Discipline;
use crate::state::AppState;

/// Frames drawn after a step starts and before sampling begins.
pub const WARMUP_FRAMES: usize = 30;
/// Frames sampled per step.
pub const SAMPLE_FRAMES: usize = 120;
/// The preset the tour loads when `ALAS_PERF_PRESET` is unset.
pub const DEFAULT_PRESET: &str = "A380-800";

thread_local! {
    // The UI thread only: a tour in one test must not open another test's cards.
    static FORCE_OPEN: Cell<bool> = const { Cell::new(false) };
}

/// `Some(true)` while a tour runs, so collapsing cards draw their content;
/// `None` otherwise (the card keeps its own state). Pass to
/// `CollapsingHeader::open`.
pub fn force_open() -> Option<bool> {
    FORCE_OPEN.with(Cell::get).then_some(true)
}

/// Open every collapsing card while a test draws a page.
#[cfg(test)]
pub(crate) fn set_force_open_for_test(open: bool) {
    FORCE_OPEN.with(|flag| flag.set(open));
}

/// What one tour step shows.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Page(&'static str, Detail),
    /// A tab of the Results page, with the analysis `ALAS_PERF_RUN` ran.
    ResultsTab(&'static str),
    Window(&'static str),
    SandboxOverview,
    SandboxDiscipline(&'static str),
    SandboxAdvanced(String),
    /// A camera drag in a 3-D view: the camera turns every frame, as a mouse
    /// drag does. `wing_analysis`, `sandbox` or `dock`.
    Orbit(&'static str),
    /// Pointer sweeping across one result figure (`tab`, `figure`) of the
    /// analysed design, 120 frames of synthetic pointer motion.
    Hover(&'static str, &'static str),
    /// Pan and zoom of the 2-D view of one result figure, set through the view
    /// state every frame as a drag and a wheel would.
    PanZoom(&'static str, &'static str),
    /// No repaint requested for 3 s on a page (or, with a tab, on a Results
    /// tab): counts the frames the app draws by itself.
    Idle(&'static str, Option<&'static str>),
}

impl Step {
    fn labels(&self) -> (String, String) {
        match self {
            Self::Page(id, detail) => ((*id).to_owned(), detail.label().to_ascii_lowercase()),
            Self::ResultsTab(id) => (format!("results:{id}"), "advanced".to_owned()),
            Self::Window(name) => (format!("window:{name}"), "advanced".to_owned()),
            Self::SandboxOverview => ("sandbox".to_owned(), "overview".to_owned()),
            Self::SandboxDiscipline(id) => ("sandbox".to_owned(), format!("discipline:{id}")),
            Self::SandboxAdvanced(tab) => ("sandbox".to_owned(), format!("advanced:{tab}")),
            Self::Orbit(view) => (format!("orbit:{view}"), "drag".to_owned()),
            Self::Hover(_, figure) => (format!("hover:{figure}"), "sweep".to_owned()),
            Self::PanZoom(_, figure) => (format!("panzoom:{figure}"), "pan+zoom".to_owned()),
            Self::Idle(page, None) => (format!("idle:{page}"), "3s".to_owned()),
            Self::Idle(_, Some(tab)) => (format!("idle:results:{tab}"), "3s".to_owned()),
        }
    }
}

/// Frame statistics of one step.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// Page id or window name.
    pub page: String,
    /// Detail level or sub-view.
    pub detail: String,
    /// Sampled frames.
    pub frames: usize,
    /// Mean, 95th percentile and maximum of the update time, ms.
    pub cpu_ms: (f64, f64, f64),
    /// Mean and 95th percentile of the update-to-update interval, ms.
    pub interval_ms: (f64, f64),
    /// Update time of the step's first frame and the largest update time of
    /// its warm-up frames, ms.
    pub entry_ms: (f64, f64),
    /// Wall time from entering the step until nothing was pending any more
    /// (every figure of the page built and drawn, no placeholder), ms.
    pub ready_ms: f64,
    /// Frames per second the app drew by itself in an idle step.
    pub idle_fps: Option<f64>,
}

impl Row {
    /// The CSV line of this row.
    pub fn csv(&self) -> String {
        format!(
            "{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{}",
            self.page,
            self.detail,
            self.frames,
            self.cpu_ms.0,
            self.cpu_ms.1,
            self.cpu_ms.2,
            self.interval_ms.0,
            self.interval_ms.1,
            self.entry_ms.0,
            self.entry_ms.1,
            self.ready_ms,
            self.idle_fps
                .map_or(String::new(), |fps| format!("{fps:.3}"))
        )
    }
}

/// The CSV header.
pub const CSV_HEADER: &str =
    "page,detail,frames,cpu_mean_ms,cpu_p95_ms,cpu_max_ms,interval_mean_ms,interval_p95_ms,first_frame_ms,warmup_max_ms,time_to_ready_ms,idle_frames_per_s";

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f64 * fraction).ceil() as usize;
    sorted[index.min(sorted.len() - 1)]
}

fn summarize(
    step: &Step,
    cpu: &[f64],
    interval: &[f64],
    entry_ms: (f64, f64),
    ready_ms: f64,
) -> Row {
    let sorted = |values: &[f64]| {
        let mut copy = values.to_vec();
        copy.sort_by(f64::total_cmp);
        copy
    };
    let (cpu_sorted, interval_sorted) = (sorted(cpu), sorted(interval));
    let (page, detail) = step.labels();
    Row {
        page,
        detail,
        frames: cpu.len(),
        cpu_ms: (
            mean(cpu),
            percentile(&cpu_sorted, 0.95),
            cpu_sorted.last().copied().unwrap_or(0.0),
        ),
        interval_ms: (mean(interval), percentile(&interval_sorted, 0.95)),
        entry_ms,
        ready_ms,
        idle_fps: None,
    }
}

/// A row for a single measured event (a frame, a wait) rather than a sampled
/// step: `ms` fills the mean, 95th percentile, maximum and entry columns.
fn event_row(page: &str, detail: &str, ms: f64) -> Row {
    Row {
        page: page.to_owned(),
        detail: detail.to_owned(),
        frames: 1,
        cpu_ms: (ms, ms, ms),
        interval_ms: (0.0, 0.0),
        entry_ms: (ms, ms),
        ready_ms: ms,
        idle_fps: None,
    }
}

/// The analysis `ALAS_PERF_RUN` asks for before the pages are measured.
#[derive(Debug, Clone, Copy, PartialEq)]
enum RunPhase {
    /// No analysis: the pages are measured with the preset only.
    NotAsked,
    /// Asked for, not started yet.
    Pending,
    /// Running since the instant.
    Running(Instant),
    /// The worker's result was taken in the frame that began at the instant;
    /// the figures of the run are still being built and drawn.
    Settling(Instant),
    /// Finished; the pages are measured with its result.
    Done,
}

/// Seconds an idle step watches the app without asking it to repaint.
const IDLE_SECONDS: f64 = 3.0;
/// The start of an idle window is not counted: egui draws one follow-up frame
/// after the one that armed it (measured: 0.00-0.03 s in), which is a settle
/// frame and not the app drawing by itself.
const IDLE_SETTLE_SECONDS: f64 = 0.25;
/// Longest a step waits for its figures before the tour carries on.
const READY_TIMEOUT: Duration = Duration::from_secs(120);

/// One idle observation: frames drawn without anybody asking for them.
#[derive(Debug, Clone, Copy)]
struct IdleWatch {
    armed: Instant,
    arm_frame: usize,
    frames: usize,
}

thread_local! {
    // The screen rectangles the Results gallery drew each figure in, so a
    // synthetic pointer can be swept across one. Written only during a tour.
    static FIGURE_RECTS: RefCell<Vec<(&'static str, egui::Rect)>> = const { RefCell::new(Vec::new()) };
}

/// Remember where a result figure was drawn (visible part, points). A no-op
/// outside a tour.
pub fn note_figure_rect(id: &'static str, rect: egui::Rect) {
    if FORCE_OPEN.with(Cell::get) {
        FIGURE_RECTS.with(|rects| {
            let mut rects = rects.borrow_mut();
            match rects.iter_mut().find(|(known, _)| *known == id) {
                Some(entry) => entry.1 = rect,
                None => rects.push((id, rect)),
            }
        });
    }
}

/// Nothing a page waits for is pending: every queued figure has arrived and
/// every background rasterization has been uploaded.
fn nothing_pending(state: &AppState) -> bool {
    state.figure_jobs.urgent_pending() == 0 && alas_viz::pending_background_rasters() == 0
}

/// The tour state machine; see the module documentation.
pub struct PerfTour {
    output: Option<PathBuf>,
    preset: String,
    steps: Vec<Step>,
    next: usize,
    current: Option<Step>,
    frame_in_step: usize,
    cpu: Vec<f64>,
    interval: Vec<f64>,
    frame_start: Option<Instant>,
    previous_start: Option<Instant>,
    preset_loaded: bool,
    trace: Option<Instant>,
    sampling: (usize, usize),
    entry_ms: (f64, f64),
    run: RunPhase,
    /// When the current step began, and when it was first ready.
    step_started: Option<Instant>,
    ready_ms: Option<f64>,
    /// The idle observation of the current idle step, once begun.
    idle: Option<IdleWatch>,
    /// Start of the frame `before_frame` last saw.
    last_before: Instant,
    /// A synthetic pointer was placed over the window by the last step.
    pointer_down: bool,
    /// Rows measured so far.
    pub rows: Vec<Row>,
    /// Set once every step has run.
    pub finished: bool,
}

impl PerfTour {
    /// The tour the environment asks for, or `None` (the normal case).
    pub fn from_env() -> Option<Self> {
        let output = std::env::var_os("ALAS_PERF_TOUR")?;
        if output.is_empty() {
            return None;
        }
        let preset = std::env::var("ALAS_PERF_PRESET")
            .ok()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| DEFAULT_PRESET.to_owned());
        let with_run = std::env::var_os("ALAS_PERF_RUN").is_some_and(|value| !value.is_empty());
        let mut steps = all_steps();
        if with_run {
            // After the pages, before the windows: the Results tabs in order,
            // then the interaction and idle steps on the analysed design.
            let at = steps
                .iter()
                .position(|step| matches!(step, Step::Window(_)))
                .unwrap_or(steps.len());
            let tabs = crate::views::results_view::result_tab_ids();
            let mut results: Vec<Step> = tabs.into_iter().map(Step::ResultsTab).collect();
            results.extend([
                Step::Hover("mission", "mission_profile"),
                Step::Hover("aero", "polar_comparison"),
                Step::PanZoom("mission", "mission_profile"),
                Step::PanZoom("aero", "polar_comparison"),
                Step::Idle("results", Some("mission")),
            ]);
            steps.splice(at..at, results);
        }
        // `ALAS_PERF_STEPS=orbit,window:wing` keeps only the steps whose page
        // label starts with one of the comma-separated prefixes.
        if let Some(filter) = std::env::var("ALAS_PERF_STEPS")
            .ok()
            .filter(|f| !f.is_empty())
        {
            let prefixes: Vec<&str> = filter.split(',').collect();
            steps.retain(|step| {
                let page = step.labels().0;
                prefixes.iter().any(|prefix| page.starts_with(prefix))
            });
        }
        let mut tour = Self::new(Some(PathBuf::from(output)), preset, steps);
        if with_run {
            tour.run = RunPhase::Pending;
        }
        Some(tour)
    }

    /// A tour over explicit pages for tests: no output file, no exit.
    pub fn for_pages(preset: &str, pages: &[&'static str], sampling: (usize, usize)) -> Self {
        let mut steps = Vec::new();
        for detail in [Detail::Basic, Detail::Advanced] {
            for id in pages {
                steps.push(Step::Page(id, detail));
            }
        }
        let mut tour = Self::new(None, preset.to_owned(), steps);
        tour.sampling = sampling;
        tour
    }

    /// A tour over the drag-orbit steps of `views` (`dock`, `sandbox`,
    /// `wing_analysis`) for tests: no output file, no exit.
    #[cfg(test)]
    pub fn for_orbits(preset: &str, views: &[&'static str], sampling: (usize, usize)) -> Self {
        let steps = views.iter().map(|view| Step::Orbit(view)).collect();
        let mut tour = Self::new(None, preset.to_owned(), steps);
        tour.sampling = sampling;
        tour
    }

    fn new(output: Option<PathBuf>, preset: String, steps: Vec<Step>) -> Self {
        FORCE_OPEN.with(|flag| flag.set(true));
        Self {
            output,
            preset,
            steps,
            next: 0,
            current: None,
            frame_in_step: 0,
            cpu: Vec::new(),
            interval: Vec::new(),
            frame_start: None,
            previous_start: None,
            preset_loaded: false,
            trace: std::env::var_os("ALAS_PERF_TRACE")
                .filter(|value| !value.is_empty())
                .map(|_| Instant::now()),
            sampling: (WARMUP_FRAMES, SAMPLE_FRAMES),
            entry_ms: (0.0, 0.0),
            run: RunPhase::NotAsked,
            step_started: None,
            ready_ms: None,
            idle: None,
            last_before: Instant::now(),
            pointer_down: false,
            rows: Vec::new(),
            finished: false,
        }
    }

    /// Call before the frame is drawn: starts the next step when due.
    pub fn before_frame(&mut self, ctx: &Context, state: &mut AppState) {
        if self.finished {
            return;
        }
        self.last_before = Instant::now();
        if let Some(watch) = self.idle.as_mut() {
            // The idle window: nobody asks for a repaint but the one that ends
            // it, so every other frame is one the app asked for itself.
            if watch.frames == usize::MAX {
                watch.frames = 0;
                watch.armed = Instant::now();
                watch.arm_frame = self.frame_in_step;
                // One wake-up to end the window. A timer thread, like the
                // figure workers, rather than `request_repaint_after`, which
                // did not wake this window after a quiet frame (measured).
                let wake = ctx.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs_f64(IDLE_SECONDS + 0.2));
                    wake.request_repaint();
                });
            }
        } else {
            ctx.request_repaint();
        }
        if state.boot_frames_remaining > 0 {
            return;
        }
        if !self.preset_loaded {
            self.trace_line(&format!("boot done, loading preset {}", self.preset));
            state.load_preset(&self.preset);
            state.reset_undo_baseline();
            self.preset_loaded = true;
        }
        if !self.analysis_ready(state) {
            return;
        }
        let now = Instant::now();
        self.previous_start = self.frame_start.replace(now);
        if self.current.is_none() {
            let Some(step) = self.steps.get(self.next).cloned() else {
                return;
            };
            self.next += 1;
            let (page, detail) = step.labels();
            self.trace_line(&format!(
                "step {}/{} {page} {detail}",
                self.next,
                self.steps.len()
            ));
            apply(&step, ctx, state);
            self.current = Some(step);
            self.frame_in_step = 0;
            self.cpu.clear();
            self.interval.clear();
            self.previous_start = None;
            self.step_started = Some(now);
            self.ready_ms = None;
            self.idle = None;
        }
        match &self.current {
            Some(Step::Orbit(view)) => animate_orbit(view, state),
            Some(Step::PanZoom(_, figure)) => animate_pan_zoom(figure, self.frame_in_step, state),
            _ => {}
        }
    }

    /// Start the analysis `ALAS_PERF_RUN` asks for and report whether the
    /// pages can be measured: at once without one, else when it has finished
    /// and its figures are ready.
    fn analysis_ready(&mut self, state: &mut AppState) -> bool {
        match self.run {
            RunPhase::NotAsked | RunPhase::Done => true,
            RunPhase::Pending => {
                // The aircraft as drawn with the native analyses only: the
                // external solvers and exports depend on what is installed.
                for (group, names) in [
                    ("mses", &["enabled"][..]),
                    (
                        "downstream",
                        &["openvsp", "vspaero", "avl", "flowunsteady"][..],
                    ),
                    ("structures", &["run_nastran", "run_patran_export"][..]),
                ] {
                    if let Some(values) = state.config_values.get_mut(group) {
                        for name in names {
                            crate::views::form_page::placement::toggle_bool(values, name, false);
                        }
                    }
                }
                state.bump_config_revision();
                state.run_options.optimize = false;
                state.run_options.write_outputs = false;
                state.start_pipeline(false);
                self.trace_line("analysis started");
                self.run = RunPhase::Running(Instant::now());
                false
            }
            RunPhase::Running(started) => {
                // `after_frame` moves on in the frame that takes the result;
                // a run that was never started or ended at once lands here.
                if !state.is_running {
                    self.trace_line(&format!(
                        "analysis finished in {:.1} s, complete: {}",
                        started.elapsed().as_secs_f64(),
                        state.pipeline_result_complete
                    ));
                    self.run = RunPhase::Settling(Instant::now());
                }
                false
            }
            RunPhase::Settling(from) => {
                if !(nothing_pending(state) && state.figures_idle()) {
                    return false;
                }
                let ms = from.elapsed().as_secs_f64() * 1e3;
                self.trace_line(&format!(
                    "all figures of the run ready {ms:.0} ms after the result was taken"
                ));
                self.rows
                    .push(event_row("after_run", "all_figures_ready", ms));
                state.run_log_open = false;
                self.run = RunPhase::Done;
                true
            }
        }
    }

    // Diagnostic output on request; stderr is the tour's only channel.
    #[allow(clippy::print_stderr)]
    fn trace_line(&self, message: &str) {
        if let Some(start) = self.trace {
            eprintln!("perf tour {:8.2}s {message}", start.elapsed().as_secs_f64());
        }
    }

    /// Synthetic pointer input for the frame about to be drawn. Called from
    /// the shell's `raw_input_hook`, because pointer state is read from the
    /// frame's raw input before `update` runs.
    pub fn inject_input(&mut self, raw_input: &mut egui::RawInput) {
        let figure = match &self.current {
            Some(Step::Hover(_, figure)) => Some(*figure),
            _ => None,
        };
        let Some(figure) = figure else {
            if std::mem::take(&mut self.pointer_down) {
                raw_input.events.push(egui::Event::PointerGone);
            }
            return;
        };
        let rect = FIGURE_RECTS.with(|rects| {
            rects
                .borrow()
                .iter()
                .find(|(id, _)| *id == figure)
                .map(|(_, rect)| *rect)
        });
        let Some(rect) = rect.filter(|rect| rect.is_positive()) else {
            return;
        };
        // A left-to-right sweep every 90 frames with a vertical wobble, so the
        // pointer crosses the plot area and its labels as a hand would.
        let phase = (self.frame_in_step % 90) as f32 / 90.0;
        let x = rect.left() + 4.0 + (rect.width() - 8.0).max(0.0) * phase;
        let y = rect.center().y + rect.height() * 0.25 * (phase * std::f32::consts::TAU).sin();
        raw_input
            .events
            .push(egui::Event::PointerMoved(egui::pos2(x, y)));
        self.pointer_down = true;
    }

    /// Call after the frame is drawn with the time it took.
    pub fn after_frame(&mut self, state: &mut AppState, elapsed: Duration) {
        if let RunPhase::Running(started) = self.run {
            if !state.is_running {
                // This frame took the worker's result: its update time is the
                // freeze between "run finished" and a usable Results page.
                let ms = self.last_before.elapsed().as_secs_f64() * 1e3;
                self.trace_line(&format!(
                    "analysis finished in {:.1} s, complete: {}; the frame that took the result lasted {ms:.0} ms",
                    started.elapsed().as_secs_f64(),
                    state.pipeline_result_complete
                ));
                self.rows
                    .push(event_row("after_run", "results_visible", ms));
                self.run = RunPhase::Settling(self.last_before);
            }
        }
        let Some(step) = self.current.clone() else {
            if self.preset_loaded && self.next >= self.steps.len() {
                self.finish();
            }
            return;
        };
        self.frame_in_step += 1;
        let (warmup, samples) = self.sampling;
        let elapsed_ms = elapsed.as_secs_f64() * 1e3;
        if self.frame_in_step == 1 {
            self.entry_ms = (elapsed_ms, elapsed_ms);
        } else if self.frame_in_step <= warmup {
            self.entry_ms.1 = self.entry_ms.1.max(elapsed_ms);
        }
        if self.ready_ms.is_none() {
            let waited = self.step_started.map(|at| at.elapsed());
            let timed_out = waited.is_some_and(|waited| waited > READY_TIMEOUT);
            if nothing_pending(state) || timed_out {
                let ms = waited.map_or(0.0, |waited| waited.as_secs_f64() * 1e3);
                self.ready_ms = Some(ms);
                let (page, detail) = step.labels();
                self.trace_line(&format!(
                    "ready {page} {detail} after {ms:.0} ms{}",
                    if timed_out { " (timed out)" } else { "" }
                ));
            }
        }
        if matches!(step, Step::Idle(..)) {
            self.idle_frame(&step, state, elapsed_ms);
            return;
        }
        if self.frame_in_step > warmup && self.ready_ms.is_some() {
            self.cpu.push(elapsed.as_secs_f64() * 1e3);
            if let (Some(start), Some(previous)) = (self.frame_start, self.previous_start) {
                self.interval
                    .push(start.duration_since(previous).as_secs_f64() * 1e3);
            }
        }
        if self.cpu.len() >= samples {
            self.rows.push(summarize(
                &step,
                &self.cpu,
                &self.interval,
                self.entry_ms,
                self.ready_ms.unwrap_or(0.0),
            ));
            self.current = None;
            if self.next >= self.steps.len() {
                self.finish();
            }
        }
    }

    /// One frame of an idle step: once the page is ready, watch for
    /// [`IDLE_SECONDS`] without a repaint request and count what is drawn.
    fn idle_frame(&mut self, step: &Step, state: &AppState, elapsed_ms: f64) {
        let _ = state;
        let Some(watch) = self.idle.as_mut() else {
            if self.frame_in_step >= self.sampling.0 && self.ready_ms.is_some() {
                // `before_frame` arms the window on the next frame.
                self.idle = Some(IdleWatch {
                    armed: Instant::now(),
                    arm_frame: self.frame_in_step,
                    frames: usize::MAX,
                });
            }
            return;
        };
        if watch.frames == usize::MAX {
            return;
        }
        // The frame the window itself asked for at its start is not the app's.
        let mut counted = None;
        if self.frame_in_step > watch.arm_frame + 1 {
            let at = watch.armed.elapsed().as_secs_f64();
            if (IDLE_SETTLE_SECONDS..IDLE_SECONDS).contains(&at) {
                watch.frames += 1;
                self.cpu.push(elapsed_ms);
                counted = Some((at, elapsed_ms));
            }
        }
        let finished = watch.armed.elapsed().as_secs_f64() >= IDLE_SECONDS;
        let frames = watch.frames;
        if let Some((at, ms)) = counted {
            self.trace_line(&format!(
                "idle: the app drew a frame by itself {at:.2} s into the window ({ms:.2} ms)"
            ));
        }
        if finished {
            let mut row = summarize(
                step,
                &self.cpu,
                &[],
                self.entry_ms,
                self.ready_ms.unwrap_or(0.0),
            );
            row.frames = frames;
            row.idle_fps = Some(frames as f64 / (IDLE_SECONDS - IDLE_SETTLE_SECONDS));
            self.rows.push(row);
            self.idle = None;
            self.current = None;
            if self.next >= self.steps.len() {
                self.finish();
            }
        }
    }

    // The tour exits right after; stderr is its only channel for a write error.
    #[allow(clippy::print_stderr)]
    fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.trace_line("finished");
        FORCE_OPEN.with(|flag| flag.set(false));
        let Some(path) = &self.output else {
            return;
        };
        let mut text = String::from(CSV_HEADER);
        text.push('\n');
        for row in &self.rows {
            text.push_str(&row.csv());
            text.push('\n');
        }
        if let Err(error) = std::fs::write(path, text) {
            eprintln!("perf tour: cannot write {}: {error}", path.display());
        }
        std::process::exit(0);
    }
}

/// Pan and zoom the 2-D view of a result figure by one frame, through the
/// state a drag and a wheel notch change: the view oscillates between 1x and
/// 2x while its centre travels a small ellipse.
fn animate_pan_zoom(figure: &str, frame: usize, state: &mut AppState) {
    let angle = frame as f32 * 0.12;
    let key = state.gallery_figure_key(figure);
    let view = state.view_state_mut(key);
    view.auto_fit = false;
    view.zoom = 1.5 + 0.5 * angle.sin();
    view.pan = egui::vec2(40.0 * angle.cos(), 25.0 * (angle * 0.7).sin());
}

/// Turn the camera of the named 3-D view by one drag frame (2 px), through the
/// same calls the drag handlers make.
fn animate_orbit(view: &str, state: &mut AppState) {
    let delta = egui::vec2(2.0, 0.0);
    match view {
        "wing_analysis" => {
            let theme = state.theme.figure_theme_name().to_owned();
            crate::views::wing_analysis_view::wing_analysis::with_window(|window| {
                window.camera.apply_orbit_motion(delta);
                window.rebuild_preview(&theme);
            });
        }
        "sandbox" => {
            state
                .preview_camera_mut(crate::sandbox::scene::SANDBOX_CAMERA_ID)
                .apply_orbit_motion(delta);
            state.reproject_sandbox_scene();
        }
        _ => {
            state
                .preview_camera_mut("aircraft_3d")
                .apply_orbit_motion(delta);
            state.update_preview_scene();
        }
    }
}

/// Close every auxiliary window the tour opens.
fn close_windows(state: &mut AppState) {
    state.screening.window_open = false;
    state.cfd.window_open = false;
    state.files.manager_open = false;
    crate::views::wing_analysis_view::close_wing_analysis();
}

fn apply(step: &Step, ctx: &Context, state: &mut AppState) {
    close_windows(state);
    state.sandbox.layout.open_disciplines.clear();
    state.sandbox.layout.advanced_settings_open = false;
    match step {
        Step::Page(id, detail) => {
            state.shell.detail = *detail;
            crate::app::go_to_page(state, id);
        }
        Step::ResultsTab(id) => {
            state.shell.detail = Detail::Advanced;
            crate::app::go_to_page(state, "results");
            state.results_tab = (*id).to_owned();
        }
        Step::Window(name) => {
            state.shell.detail = Detail::Advanced;
            crate::app::go_to_page(state, "results");
            match *name {
                "airfoil_screening" => state.screening.window_open = true,
                "airfoil_cfd" => state.cfd.window_open = true,
                "wing_analysis" => crate::views::wing_analysis_view::open_wing_analysis(ctx),
                _ => {
                    crate::library_window::open(state, crate::library_window::LibraryTab::Aircraft)
                }
            }
        }
        Step::SandboxOverview => {
            state.shell.detail = Detail::Advanced;
            state.enter_sandbox(false);
        }
        Step::SandboxDiscipline(id) => {
            state.sandbox.layout.open_disciplines = vec![(*id).to_owned()];
        }
        Step::Hover(tab, _) | Step::PanZoom(tab, _) => {
            state.shell.detail = Detail::Advanced;
            crate::app::go_to_page(state, "results");
            state.results_tab = (*tab).to_owned();
        }
        Step::Idle(page, tab) => {
            state.shell.detail = Detail::Advanced;
            crate::app::go_to_page(state, page);
            if let Some(tab) = tab {
                state.results_tab = (*tab).to_owned();
            }
        }
        Step::Orbit(view) => {
            state.shell.detail = Detail::Advanced;
            match *view {
                "wing_analysis" => {
                    crate::app::go_to_page(state, "results");
                    crate::views::wing_analysis_view::open_wing_analysis(ctx);
                }
                "sandbox" => {
                    state.enter_sandbox(false);
                }
                _ => {
                    state.preview_open = true;
                    crate::app::go_to_page(state, "inputs");
                }
            }
        }
        Step::SandboxAdvanced(tab) => {
            state.sandbox.layout.advanced_settings_open = true;
            state.sandbox.advanced_tab = tab.clone();
        }
    }
}

/// Every step of the full tour.
fn all_steps() -> Vec<Step> {
    let mut seen = std::collections::HashSet::new();
    let pages: Vec<&'static str> = crate::nav::all_pages()
        .filter(|page| page.kind != crate::nav::PageKind::Sandbox)
        .filter(|page| seen.insert(page.id))
        .map(|page| page.id)
        .collect();
    let mut steps = Vec::new();
    for detail in [Detail::Basic, Detail::Advanced] {
        steps.extend(pages.iter().map(|id| Step::Page(id, detail)));
    }
    // The Aircraft page with nothing asking for a repaint: the idle cost.
    steps.push(Step::Idle("inputs", None));
    steps.extend(
        [
            "airfoil_screening",
            "airfoil_cfd",
            "wing_analysis",
            "library",
        ]
        .into_iter()
        .map(Step::Window),
    );
    steps.push(Step::Orbit("dock"));
    steps.push(Step::Orbit("wing_analysis"));
    steps.push(Step::SandboxOverview);
    steps.push(Step::Orbit("sandbox"));
    steps.extend(
        Discipline::ALL
            .into_iter()
            .map(|discipline| Step::SandboxDiscipline(discipline.id())),
    );
    let mut tabs: Vec<String> = crate::nav::ADVANCED_SETTINGS_PAGES
        .iter()
        .map(|page| page.id.to_owned())
        .collect();
    tabs.push("run_options".to_owned());
    steps.extend(tabs.into_iter().map(Step::SandboxAdvanced));
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_uses_the_upper_rank() {
        let values: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(percentile(&values, 0.95), 96.0);
        assert_eq!(percentile(&[], 0.95), 0.0);
    }

    #[test]
    fn the_full_tour_covers_the_pages_windows_and_sandbox() {
        let steps = all_steps();
        assert!(steps.contains(&Step::Page("inputs", Detail::Basic)));
        assert!(steps.contains(&Step::Page("geometry", Detail::Advanced)));
        assert!(steps.contains(&Step::Window("library")));
        assert!(steps.contains(&Step::SandboxDiscipline("wing")));
        assert!(!steps.contains(&Step::Page("sandbox", Detail::Basic)));
    }

    #[test]
    fn rows_serialise_to_the_documented_columns() {
        let row = summarize(
            &Step::Page("inputs", Detail::Basic),
            &[1.0, 2.0],
            &[16.0, 17.0],
            (3.0, 4.0),
            5.0,
        );
        assert_eq!(row.csv().split(',').count(), CSV_HEADER.split(',').count());
        assert!(row.csv().starts_with("inputs,basic,2,1.500"));
    }
}
