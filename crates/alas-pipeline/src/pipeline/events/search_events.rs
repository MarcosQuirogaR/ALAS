// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The optimisation stage's live progress as run events.
//!
//! While the search runs, a reader thread polls the search's progress record
//! ([`alas_opt::search_progress`]) about once a second and turns it into
//! [`RunEventKind::Progress`] events for the status line (stage, analyses
//! done of those planned, feasible count, best feasible cost) and into one
//! logged event per line the search writes to its run log (each stage's
//! start and plan). It only observes: the search is the same with or
//! without it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use alas_opt::search_progress::{ProgressReading, SearchProgress};

use super::emit_event;
use crate::runs::{RunEvent, RunEventKind, RunEventSeverity};

const STAGE: &str = "optimization";
const OPTIMIZATION_STAGE_INDEX: u8 = 2;
const POLL: Duration = Duration::from_millis(100);
const EVERY: Duration = Duration::from_secs(1);

/// The status-line text of one reading.
fn status(reading: &ProgressReading) -> Option<String> {
    let stage = reading.stage?;
    let mut text = format!("Design-space optimization: {}", stage.label());
    if reading.planned > 0 {
        text.push_str(&format!(" {}/{} analyses", reading.done, reading.planned));
    } else {
        text.push_str(&format!(" {} analyses", reading.done));
    }
    match reading.best_feasible_cost {
        Some(best) => text.push_str(&format!(
            " | {} feasible, best cost {best:.5}",
            reading.feasible
        )),
        None => text.push_str(" | no feasible design yet"),
    }
    Some(text)
}

fn forward(
    events: &(dyn Fn(RunEvent) + Sync),
    run_clock: Instant,
    reading: &ProgressReading,
    last: &mut Option<String>,
) {
    for line in &reading.lines {
        emit_event(
            Some(events),
            run_clock,
            RunEvent {
                stage: STAGE.to_owned(),
                message: line.clone(),
                fraction: None,
                kind: RunEventKind::Diagnostic,
                severity: RunEventSeverity::Info,
                stage_index: Some(OPTIMIZATION_STAGE_INDEX),
                stage_count: None,
                elapsed_ms: 0,
                duration_ms: None,
            },
        );
    }
    let Some(text) = status(reading) else {
        return;
    };
    if last.as_ref() == Some(&text) {
        return;
    }
    emit_event(
        Some(events),
        run_clock,
        RunEvent {
            stage: STAGE.to_owned(),
            message: text.clone(),
            fraction: reading.fraction(),
            kind: RunEventKind::Progress,
            severity: RunEventSeverity::Info,
            stage_index: Some(OPTIMIZATION_STAGE_INDEX),
            stage_count: None,
            elapsed_ms: 0,
            duration_ms: None,
        },
    );
    *last = Some(text);
}

/// Run `work` (the optimisation stage) with its search's progress forwarded
/// to `events`. Without a cancellation flag to key the record on, or nobody
/// listening, `work` runs alone.
pub(in crate::pipeline) fn with_search_progress<T>(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    cancel: Option<&AtomicBool>,
    work: impl FnOnce() -> T,
) -> T {
    let (Some(events), Some(flag)) = (events, cancel) else {
        return work();
    };
    let progress = SearchProgress::register(flag);
    let finished = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let mut last = None;
            let mut next = Instant::now() + EVERY;
            while !finished.load(Ordering::Relaxed) {
                std::thread::sleep(POLL);
                if Instant::now() >= next {
                    forward(events, run_clock, &progress.read(), &mut last);
                    next += EVERY;
                }
            }
            forward(events, run_clock, &progress.read(), &mut last);
        });
        let value = work();
        finished.store(true, Ordering::Relaxed);
        value
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_opt::search_progress::{progress_for, SearchStage};
    use std::sync::Mutex;

    #[test]
    fn the_stage_work_and_best_reach_the_status_line_and_the_log() {
        let seen = Mutex::new(Vec::new());
        let events = |event: RunEvent| {
            if let Ok(mut seen) = seen.lock() {
                seen.push(event);
            }
        };
        let flag = AtomicBool::new(false);
        let value = with_search_progress(Some(&events), Instant::now(), Some(&flag), || {
            let progress = progress_for(Some(&flag)).expect("registered for the stage");
            progress.line("refinement | budget 4");
            progress.enter(SearchStage::Refinement, 4);
            progress.finished(&[(0.5, true), (0.4, false)]);
            7
        });
        assert_eq!(value, 7);
        let seen = seen.into_inner().expect("events");
        assert!(seen
            .iter()
            .any(|event| event.kind == RunEventKind::Diagnostic
                && event.message == "refinement | budget 4"));
        let status = seen
            .iter()
            .rev()
            .find(|event| event.kind == RunEventKind::Progress)
            .expect("a status reading");
        assert_eq!(
            status.message,
            "Design-space optimization: refinement 2/4 analyses | 1 feasible, best cost 0.50000"
        );
        assert_eq!(status.fraction, Some(0.5));
        assert!(
            progress_for(Some(&flag)).is_none(),
            "released with the stage"
        );
    }
}
