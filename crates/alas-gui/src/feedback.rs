// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Quiet, value-specific feedback for parameter editing.
//!
//! Numeric controls report a change for every wheel tick and drag frame. The
//! desktop should acknowledge the final value without turning that interaction
//! into hundreds of run-log lines, so the latest edit is retained briefly and
//! emitted once the gesture becomes idle.

use std::time::{Duration, Instant};

use crate::state::{AppState, LogKind};

/// Idle time after which the final value of one edit gesture reaches the log.
pub(crate) const PARAMETER_FEEDBACK_DEBOUNCE: Duration = Duration::from_millis(650);

/// A parameter value waiting for its quiet log acknowledgement.
#[derive(Debug, Clone)]
pub struct ParameterFeedback {
    label: String,
    value: String,
    changed_at: Instant,
}

impl ParameterFeedback {
    fn remaining_delay(&self, now: Instant) -> Option<Duration> {
        let elapsed = now.saturating_duration_since(self.changed_at);
        (elapsed < PARAMETER_FEEDBACK_DEBOUNCE).then(|| PARAMETER_FEEDBACK_DEBOUNCE - elapsed)
    }
}

impl AppState {
    /// Acknowledge an edited value immediately in the status area and in the
    /// run log after the control becomes idle.
    pub(crate) fn note_parameter_modified(&mut self, label: String, value: String) {
        self.status_message = format!("{label}: {value}");
        self.parameter_feedback = Some(ParameterFeedback {
            label,
            value,
            changed_at: Instant::now(),
        });
    }

    /// Emit a buffered parameter acknowledgement when it has remained stable.
    ///
    /// The returned delay lets the caller request exactly one future repaint
    /// instead of continuously waking the GUI while the user is idle.
    pub(crate) fn flush_parameter_feedback(&mut self) -> Option<Duration> {
        let now = Instant::now();
        let delay = self
            .parameter_feedback
            .as_ref()
            .and_then(|feedback| feedback.remaining_delay(now));
        if let Some(delay) = delay {
            return Some(delay);
        }
        if let Some(feedback) = self.parameter_feedback.take() {
            self.log(
                format!("{}: {}", feedback.label, feedback.value),
                LogKind::Info,
            );
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{ParameterFeedback, PARAMETER_FEEDBACK_DEBOUNCE};
    use std::time::{Duration, Instant};

    #[test]
    fn feedback_waits_for_the_edit_gesture_to_become_idle() {
        let now = Instant::now();
        let feedback = ParameterFeedback {
            label: "Range".to_owned(),
            value: "5000 m".to_owned(),
            changed_at: now,
        };

        assert_eq!(
            feedback.remaining_delay(now),
            Some(PARAMETER_FEEDBACK_DEBOUNCE)
        );
        assert_eq!(
            feedback.remaining_delay(now + PARAMETER_FEEDBACK_DEBOUNCE),
            None
        );
        assert_eq!(
            feedback.remaining_delay(now + PARAMETER_FEEDBACK_DEBOUNCE + Duration::from_millis(1)),
            None
        );
    }
}
