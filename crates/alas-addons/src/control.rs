// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Live feedback from, and cancellation of, a running add-on.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

/// Something a running process add-on reported.
#[derive(Debug, Clone, PartialEq)]
pub enum RunEvent {
    /// A `ALAS-PROGRESS` line on stdout: an optional completion fraction
    /// (0 to 1) and the status text.
    Progress {
        /// Completion, when the program gave a percentage.
        fraction: Option<f64>,
        /// What the program is doing.
        text: String,
    },
    /// One line of stderr, as it arrives.
    Log(String),
}

/// Stdout lines starting with this word are progress reports, not the reply.
pub const PROGRESS_PREFIX: &str = "ALAS-PROGRESS";

/// Cancellation flag and event sink handed to a run.
#[derive(Debug, Clone, Default)]
pub struct RunControl {
    /// Set to stop the run; the process tree is killed.
    pub cancel: Arc<AtomicBool>,
    /// Receives live events; `None` discards them.
    pub events: Option<Sender<RunEvent>>,
}

impl RunControl {
    /// No cancellation, no events.
    pub fn none() -> Self {
        Self::default()
    }

    /// Whether a stop was requested.
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub(crate) fn emit(&self, event: RunEvent) {
        if let Some(events) = &self.events {
            let _ = events.send(event);
        }
    }
}

/// Parse one stdout line as a progress report: `ALAS-PROGRESS [percent] text`.
pub fn parse_progress(line: &str) -> Option<RunEvent> {
    let rest = line.trim().strip_prefix(PROGRESS_PREFIX)?.trim();
    let (first, tail) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let percent = first
        .trim_end_matches('%')
        .parse::<f64>()
        .ok()
        .filter(|p| p.is_finite() && (0.0..=100.0).contains(p));
    Some(match percent {
        Some(p) => RunEvent::Progress {
            fraction: Some(p / 100.0),
            text: tail.trim().to_owned(),
        },
        None => RunEvent::Progress {
            fraction: None,
            text: rest.to_owned(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_lines_carry_an_optional_percentage() {
        assert_eq!(
            parse_progress("ALAS-PROGRESS 40 meshing"),
            Some(RunEvent::Progress {
                fraction: Some(0.4),
                text: "meshing".to_owned()
            })
        );
        assert_eq!(
            parse_progress("ALAS-PROGRESS solving the polar"),
            Some(RunEvent::Progress {
                fraction: None,
                text: "solving the polar".to_owned()
            })
        );
        assert_eq!(parse_progress("{\"outputs\": {}}"), None);
        // Out of range is text, not a fraction.
        assert!(matches!(
            parse_progress("ALAS-PROGRESS 250 x"),
            Some(RunEvent::Progress { fraction: None, .. })
        ));
    }
}
