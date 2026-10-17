// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Competition presets: rule gates, score definition and aggregation.
//!
//! A preset is versioned data (one JSON file per rulebook edition), so a new
//! year is a data change plus review. The same types describe the four
//! families supported from day one: Air Cargo Challenge, XtraChallenge, SAE
//! Aero Design and AIAA DBF. Score and gate expressions are strings over the
//! typed mission outputs (`mission::OUTPUTS`) and the preset parameters; the
//! dimension-checked language that evaluates them lives in `crate::score`.

use serde::{Deserialize, Serialize};

use super::mission::Segment;

/// Competition family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    /// Air Cargo Challenge.
    Acc,
    /// XtraChallenge (Xtra2 UPV).
    XtraChallenge,
    /// SAE Aero Design (Regular, Micro, Advanced classes).
    SaeAeroDesign,
    /// AIAA Design/Build/Fly.
    AiaaDbf,
    /// A user-defined competition.
    Custom,
}

/// How firmly the rule text is established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetStatus {
    /// Transcribed from the primary rule document.
    Official,
    /// Built from secondary sources or earlier editions; the GUI shows a banner.
    Provisional,
}

/// A hard rule or a soft engineering margin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleGate {
    /// Stable id.
    pub id: String,
    /// Human description.
    pub description: String,
    /// Expression whose value is >= 0 when satisfied (normalised violation otherwise).
    pub margin_expr: String,
    /// Hard rule (infeasible when violated) or soft margin.
    pub hard: bool,
    /// Rulebook citation (document and page).
    pub citation: String,
}

/// A named score term.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreTerm {
    /// Term name usable in later expressions.
    pub name: String,
    /// Expression.
    pub expr: String,
    /// Whether the term is an additive bonus (bonuses are not invariant to
    /// the reference-field normaliser, so the editor marks them).
    pub additive_bonus: bool,
}

/// How per-round scores combine into the flight score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Aggregation {
    /// Mean of the best `k` of at least `min_rounds` (ACC 2026: 2 of >= 3).
    MeanBestK {
        /// Rounds counted.
        k: u32,
        /// Rounds needed before only the best `k` count (fewer: best one).
        min_rounds: u32,
    },
    /// Mean of all rounds except the worst (XC 2025 clubs).
    MeanExceptWorst,
    /// Mean of all rounds, dropping the worst only when more than `rounds`
    /// were flown (XC 2023: the worst of more than three flights is skipped).
    MeanDropWorstAbove {
        /// Round count above which the worst round is dropped.
        rounds: u32,
    },
    /// Weights by rank from worst to best (ACC 2024: 0.25, 0.5, then equal).
    RankWeighted {
        /// Weights worst first; remaining rounds share the rest equally.
        weights: Vec<f64>,
    },
    /// Sum of the best `k` (SAE Micro and Advanced).
    SumBestK {
        /// Rounds counted.
        k: u32,
    },
    /// Mean of the best `k` plus a separate bonus expression (SAE Regular).
    MeanBestKPlusBonus {
        /// Rounds counted.
        k: u32,
        /// Bonus expression evaluated once.
        bonus_expr: String,
    },
    /// A single scored mission (DBF mission scores multiply the report score).
    Single,
}

/// A normaliser relative to the field (best team of the round).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReferenceParam {
    /// Name used in expressions (for example `s_raw_best`).
    pub name: String,
    /// Default value, SI. A published prior-year figure; flagged an assumption.
    pub default: f64,
    /// SI unit.
    pub unit_si: String,
    /// Where the default comes from.
    pub note: String,
}

/// A course or rule parameter with its range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamDef {
    /// Name.
    pub name: String,
    /// SI unit.
    pub unit_si: String,
    /// Default value.
    pub default: f64,
    /// Smallest allowed value.
    pub min: f64,
    /// Largest allowed value.
    pub max: f64,
    /// Meaning and citation.
    pub doc: String,
}

/// The score definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreDef {
    /// Terms in evaluation order.
    pub terms: Vec<ScoreTerm>,
    /// Round score expression over terms and outputs.
    pub round_expr: String,
    /// Round aggregation.
    pub aggregation: Aggregation,
    /// Field-relative normalisers.
    pub reference_field: Vec<ReferenceParam>,
}

/// An alternative reading of ambiguous rules (for example the ACC current-penalty forms).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreVariant {
    /// Variant id.
    pub id: String,
    /// What differs.
    pub description: String,
    /// Replacement expressions by term name.
    pub overrides: Vec<(String, String)>,
    /// Whether this is the default reading.
    pub default: bool,
}

/// A competition preset (one rulebook edition).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    /// Stable id (for example `acc-2026`).
    pub id: String,
    /// Data version, bumped when the transcription changes.
    pub version: u32,
    /// Family.
    pub family: Family,
    /// Display name.
    pub name: String,
    /// Established or provisional.
    pub status: PresetStatus,
    /// Rulebook citation (document, version, date).
    pub citation: String,
    /// Rule gates.
    pub gates: Vec<RuleGate>,
    /// Course and rule parameters.
    pub parameters: Vec<ParamDef>,
    /// Mission segment template.
    pub segments: Vec<Segment>,
    /// Score definition.
    pub score: ScoreDef,
    /// Alternative readings.
    pub variants: Vec<ScoreVariant>,
}
