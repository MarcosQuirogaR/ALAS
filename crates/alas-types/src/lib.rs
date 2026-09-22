// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The status contract every analysis stage reports through.
//!
//! ALAS is built out of stages that can legitimately fail to happen: an
//! external solver is not installed, a user turned an analysis off, a trim
//! solution does not exist for the aircraft that was asked for. The rule the
//! program is written to is that none of those may quietly become a number.
//!
//! [`Stage`] is that rule expressed as a type. A stage either produced a
//! result, or failed and says why, or did not run and says why. There is no
//! fourth case and no way to read a result out of the other two by accident.
//!
//! The serialized form matches the reference implementation field for field,
//! which is what lets the two be compared without a translation layer in
//! between:
//!
//! ```json
//! {"status": "ok", "error": null, "tip_deflection_m": {"pull_up": 1.83}}
//! {"status": "not_run", "error": "MSES is not configured"}
//! ```
//!
//! The payload is flattened alongside `status` rather than nested under a key,
//! because that is what `dataclasses.asdict` produces there.

// A test asserts on values it constructed, so a failed unwrap there is the
// assertion failing, which is the test doing its job, not a panic escaping
// into a user's run.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::fmt;

use serde::{Deserialize, Serialize};

/// Which of the three outcomes a stage reached.
///
/// Serializes to the strings the reference implementation uses. Its
/// `not_configured`, which only ever described the mission subprocess, has no
/// counterpart: there is no subprocess to be unconfigured now, and a mission
/// that does not run is [`Status::NotRun`] like every other stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// The stage ran and produced a result.
    Ok,
    /// The stage ran and failed.
    Error,
    /// The stage did not run.
    NotRun,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::NotRun => "not_run",
        };
        f.write_str(text)
    }
}

/// Why a stage did not run.
///
/// Kept apart from [`Stage::Error`] because the two mean different things to a
/// user and should read differently in the interface. A missing MSES
/// installation is ordinary and expected; an MSES run that diverged is not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotRun {
    /// The user turned this analysis off.
    Disabled,
    /// An external tool this stage needs is not configured or not installed.
    ToolUnavailable {
        /// The tool as a user would name it, such as `MSES` or `MSC Nastran`.
        tool: String,
    },
    /// A stage this one depends on did not produce a result.
    MissingInput {
        /// What was missing, in terms a user can act on.
        what: String,
    },
}

impl fmt::Display for NotRun {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => f.write_str("disabled in the configuration"),
            Self::ToolUnavailable { tool } => write!(f, "{tool} is not configured"),
            Self::MissingInput { what } => write!(f, "{what} is not available"),
        }
    }
}

/// The outcome of one analysis stage.
///
/// Construct with [`Stage::ok`], [`Stage::failed`] or [`Stage::not_run`], and
/// read with [`Stage::result`] or [`Stage::status`]. There is deliberately no
/// method that yields a `T` unconditionally: a caller that wants a number from
/// a stage that did not run has to decide what to do about it.
#[derive(Debug, Clone, PartialEq)]
pub enum Stage<T> {
    /// The stage ran and produced this.
    Ok(T),
    /// The stage ran and failed with this message.
    Error(String),
    /// The stage did not run, for this reason.
    NotRun(NotRun),
}

impl<T> Stage<T> {
    /// A stage that produced a result.
    pub fn ok(value: T) -> Self {
        Self::Ok(value)
    }

    /// A stage that ran and failed.
    pub fn failed(message: impl Into<String>) -> Self {
        Self::Error(message.into())
    }

    /// A stage that did not run.
    pub fn not_run(reason: NotRun) -> Self {
        Self::NotRun(reason)
    }

    /// A stage skipped because an external tool is unavailable.
    pub fn tool_unavailable(tool: impl Into<String>) -> Self {
        Self::NotRun(NotRun::ToolUnavailable { tool: tool.into() })
    }

    /// A stage skipped because something it needed was missing.
    pub fn missing_input(what: impl Into<String>) -> Self {
        Self::NotRun(NotRun::MissingInput { what: what.into() })
    }

    /// Which outcome this is.
    pub fn status(&self) -> Status {
        match self {
            Self::Ok(_) => Status::Ok,
            Self::Error(_) => Status::Error,
            Self::NotRun(_) => Status::NotRun,
        }
    }

    /// The result, if there is one.
    pub fn result(&self) -> Option<&T> {
        match self {
            Self::Ok(value) => Some(value),
            _ => None,
        }
    }

    /// Whether the stage produced a result.
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }

    /// The explanation shown to a user, which is `None` only when the stage
    /// succeeded.
    ///
    /// A skipped stage explains itself here as well as a failed one. A user
    /// reading "not available" wants to know why, and the reference
    /// implementation put that text in the same field.
    pub fn message(&self) -> Option<String> {
        match self {
            Self::Ok(_) => None,
            Self::Error(message) => Some(message.clone()),
            Self::NotRun(reason) => Some(reason.to_string()),
        }
    }

    /// Apply a function to the result, leaving the other two cases alone.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Stage<U> {
        match self {
            Self::Ok(value) => Stage::Ok(f(value)),
            Self::Error(message) => Stage::Error(message),
            Self::NotRun(reason) => Stage::NotRun(reason),
        }
    }

    /// Convert to a `Stage` of a different payload, keeping the reason a
    /// non-result carries. Used where a stage's failure propagates to a later
    /// stage that has a different result type.
    pub fn carry_over<U>(&self) -> Option<Stage<U>> {
        match self {
            Self::Ok(_) => None,
            Self::Error(message) => Some(Stage::Error(message.clone())),
            Self::NotRun(reason) => Some(Stage::NotRun(reason.clone())),
        }
    }
}

impl<T, E: fmt::Display> From<Result<T, E>> for Stage<T> {
    fn from(result: Result<T, E>) -> Self {
        match result {
            Ok(value) => Self::Ok(value),
            Err(error) => Self::Error(error.to_string()),
        }
    }
}

/// The wire form: `status` and `error`, then the payload's own fields beside
/// them rather than nested under a key.
///
/// A stage that did not produce a result still writes the payload's default,
/// because the reference implementation's dataclasses carry defaults and
/// `asdict` emits them. Comparing the two outputs is easier if neither side has
/// to special-case a missing field.
#[derive(Serialize, Deserialize)]
struct Wire<T> {
    status: Status,
    error: Option<String>,
    #[serde(flatten)]
    data: T,
}

impl<T: Serialize + Default + Clone> Serialize for Stage<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = Wire {
            status: self.status(),
            error: self.message(),
            data: self.result().cloned().unwrap_or_default(),
        };
        wire.serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Stage<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = Wire::<T>::deserialize(deserializer)?;
        Ok(match wire.status {
            Status::Ok => Self::Ok(wire.data),
            Status::Error => Self::Error(wire.error.unwrap_or_default()),
            // The wire form records the sentence, not which of the three
            // reasons produced it, so a round trip lands here.
            Status::NotRun => Self::NotRun(NotRun::MissingInput {
                what: wire.error.unwrap_or_default(),
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
    struct Payload {
        tip_deflection_m: f64,
        modes: Vec<f64>,
    }

    fn json(stage: &Stage<Payload>) -> serde_json::Value {
        serde_json::to_value(stage).expect("a stage serializes")
    }

    #[test]
    fn ok_serializes_with_the_payload_beside_the_status() {
        let stage = Stage::ok(Payload {
            tip_deflection_m: 1.83,
            modes: vec![2.5, 7.1],
        });
        assert_eq!(
            json(&stage),
            serde_json::json!({
                "status": "ok",
                "error": null,
                "tip_deflection_m": 1.83,
                "modes": [2.5, 7.1],
            })
        );
    }

    #[test]
    fn a_skipped_stage_explains_itself_and_writes_payload_defaults() {
        let stage: Stage<Payload> = Stage::tool_unavailable("MSC Nastran");
        assert_eq!(
            json(&stage),
            serde_json::json!({
                "status": "not_run",
                "error": "MSC Nastran is not configured",
                "tip_deflection_m": 0.0,
                "modes": [],
            })
        );
    }

    #[test]
    fn a_failed_stage_carries_its_message() {
        let stage: Stage<Payload> = Stage::failed("solution diverged");
        assert_eq!(json(&stage)["status"], "error");
        assert_eq!(json(&stage)["error"], "solution diverged");
    }

    #[test]
    fn status_strings_match_the_reference_implementation() {
        assert_eq!(Status::Ok.to_string(), "ok");
        assert_eq!(Status::Error.to_string(), "error");
        assert_eq!(Status::NotRun.to_string(), "not_run");
    }

    #[test]
    fn only_a_result_can_be_read_out() {
        let ok = Stage::ok(Payload::default());
        assert!(ok.result().is_some());
        assert!(Stage::<Payload>::failed("x").result().is_none());
        assert!(Stage::<Payload>::not_run(NotRun::Disabled)
            .result()
            .is_none());
    }

    #[test]
    fn a_non_result_carries_over_to_a_later_stage() {
        let earlier: Stage<Payload> = Stage::tool_unavailable("MSES");
        let later: Stage<f64> = earlier.carry_over().expect("a skipped stage carries over");
        assert_eq!(later.status(), Status::NotRun);
        assert!(Stage::ok(Payload::default()).carry_over::<f64>().is_none());
    }

    #[test]
    fn a_result_round_trips() {
        let stage = Stage::ok(Payload {
            tip_deflection_m: 0.5,
            modes: vec![1.0],
        });
        let text = serde_json::to_string(&stage).expect("serializes");
        let back: Stage<Payload> = serde_json::from_str(&text).expect("deserializes");
        assert_eq!(stage, back);
    }

    #[test]
    fn an_error_from_a_result_keeps_its_message() {
        let failed: Result<Payload, std::io::Error> = Err(std::io::Error::other("no such file"));
        let stage: Stage<Payload> = failed.into();
        assert_eq!(stage.message().as_deref(), Some("no such file"));
    }
}
