// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Units-aware score expression language, rule engine and competition
//! presets.
//!
//! A preset (`spec::score::Preset`, one JSON file per rulebook edition) holds
//! expression strings over typed names. [`ScoreEngine`] parses them, checks
//! that every name is declared and every dimension is consistent *before* any
//! flight is evaluated, then computes terms, the round score, the rule gates
//! (a margin >= 0 means satisfied) and the multi-round flight score.
//!
//! Conventions: values are SI; a literal may carry a unit (`350 g`, `30 A`,
//! `100 Wh`, `55 lb`, `2 Wh/km`) and is converted on parse; scores are
//! dimensionless points; angles are radians (`deg` converts). Evaluation
//! failures are typed [`ScoreError`]s, never panics.

mod engine;
pub mod eval;
pub mod outputs;
mod parser;
pub mod presets;
pub mod quantity;
pub mod units;

pub use engine::{GateResult, RoundInputs, RoundResult, ScoreEngine};
pub use parser::{parse, Expr};
pub use quantity::{Dim, Quantity, Series};

/// A failure to parse, check or evaluate a score expression or preset.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ScoreError {
    /// Syntax error at a character position.
    #[error("parse error at {pos}: {message}")]
    Parse {
        /// Character offset.
        pos: usize,
        /// What was wrong.
        message: String,
    },
    /// Dimension or type mismatch.
    #[error("dimension error: {0}")]
    Dimension(String),
    /// A name that is neither declared nor a unit.
    #[error("unknown name {0}")]
    UnknownName(String),
    /// Wrong argument count.
    #[error("arity error: {0}")]
    Arity(String),
    /// A term or the round score is not a finite number.
    #[error("{0} is not finite")]
    NonFinite(String),
    /// Malformed preset.
    #[error("preset error: {0}")]
    Preset(String),
    /// Invalid round inputs.
    #[error("input error: {0}")]
    Input(String),
}
