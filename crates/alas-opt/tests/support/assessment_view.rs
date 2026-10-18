// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A storable view of a [`CandidateAssessment`], and the shared-result entry
//! points that fill it.
//!
//! About forty call sites in this directory run `assess_product_candidate` on
//! the same twelve preset nominals. Nextest gives every test its own process,
//! so no in-process cache can share the result; `alas_testkit::memo` persists
//! it across processes instead (enabled by `cargo xtask test`, off for a plain
//! `cargo test`). The full assessment carries an `Arc` of the closure's drag
//! deck and plan, which has no serialized form, so the store holds this view:
//! the residual table and the scalar totals, which is what the residual-level
//! tests read. A test that needs the sized candidate itself keeps calling
//! `assess_product_candidate`.
//!
//! The stored key is derived from the **inputs** (the whole serialized
//! configuration and the design vector), never from a name a caller chose, so
//! two tests share a result exactly when they ask the same question.

// Each test binary compiles its own copy of this module and uses part of it.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, MtowSizing};
use alas_opt::mdo::ResidualRole;
use alas_opt::{
    assess_product_candidate, CandidateAssessment, ConstraintFamily, ConstraintResidual,
};
use alas_testkit::memo::{self, lossless_f64};
use serde::{Deserialize, Serialize};

/// A `&'static str`, spelled through an alias so that serde does not infer a
/// borrow from the input for it.
pub type Static = &'static str;

/// One evaluated requirement, field for field as [`ConstraintResidual`] holds
/// it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResidualView {
    /// Stable identifier of the requirement.
    #[serde(with = "static_text")]
    pub id: Static,
    /// Requirement family.
    #[serde(with = "family_text")]
    pub family: ConstraintFamily,
    /// Value the candidate achieves.
    #[serde(with = "lossless_f64")]
    pub actual: f64,
    /// Governing threshold.
    #[serde(with = "lossless_f64")]
    pub limit: f64,
    /// Unit shared by `actual` and `limit`.
    #[serde(with = "static_text")]
    pub unit: Static,
    /// Signed physical residual; positive means a violation.
    #[serde(with = "lossless_f64")]
    pub raw_residual: f64,
    /// `max(raw_residual, 0) / scale`, dimensionless.
    #[serde(with = "lossless_f64")]
    pub normalized_violation: f64,
    /// How the requirement takes part in the ranking.
    #[serde(with = "role_text")]
    pub role: ResidualRole,
    /// Model or input failure context.
    pub detail: Option<String>,
}

impl ResidualView {
    /// Whether this residual is on the infeasible side of its limit
    /// (`ConstraintResidual::violated`).
    pub fn violated(&self) -> bool {
        !self.normalized_violation.is_finite() || self.normalized_violation > 0.0
    }
}

/// The residual table and scalar totals of one assessment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssessmentView {
    /// Every evaluated requirement, in evaluation order.
    pub residuals: Vec<ResidualView>,
    /// Whether every hard requirement is satisfied.
    pub hard_feasible: bool,
    /// Sum of normalized violations across hard-role residuals.
    #[serde(with = "lossless_f64")]
    pub hard_violation_sum: f64,
    /// Sum of normalized violations across soft-role residuals.
    #[serde(with = "lossless_f64")]
    pub soft_violation_sum: f64,
    /// The configured mission quantity, before normalization.
    #[serde(with = "lossless_f64")]
    pub objective_value: f64,
    /// The scalar cost the search ranks candidates by.
    #[serde(with = "lossless_f64")]
    pub cost: f64,
}

impl AssessmentView {
    /// Identifiers of every violated hard-role residual, in evaluation order
    /// (`CandidateAssessment::violated_hard_ids`).
    pub fn violated_hard_ids(&self) -> Vec<&'static str> {
        self.residuals
            .iter()
            .filter(|residual| residual.role == ResidualRole::Constraint && residual.violated())
            .map(|residual| residual.id)
            .collect()
    }

    /// Whether every hard requirement is satisfied.
    pub fn is_strictly_feasible(&self) -> bool {
        self.hard_feasible
    }
}

impl From<&CandidateAssessment> for AssessmentView {
    fn from(assessment: &CandidateAssessment) -> Self {
        Self {
            residuals: assessment
                .residuals
                .iter()
                .map(ResidualView::from)
                .collect(),
            hard_feasible: assessment.hard_feasible,
            hard_violation_sum: assessment.hard_violation_sum,
            soft_violation_sum: assessment.soft_violation_sum,
            objective_value: assessment.objective_value,
            cost: assessment.cost,
        }
    }
}

impl From<&ConstraintResidual> for ResidualView {
    fn from(residual: &ConstraintResidual) -> Self {
        Self {
            id: residual.id,
            family: residual.family,
            actual: residual.actual,
            limit: residual.limit,
            unit: residual.unit,
            raw_residual: residual.raw_residual,
            normalized_violation: residual.normalized_violation,
            role: residual.role,
            detail: residual.detail.clone(),
        }
    }
}

/// What a test can ask of an assessment, whether the live one or its view.
pub trait Assessed {
    /// Identifiers of every violated hard-role residual.
    fn violated_hard_ids(&self) -> Vec<&'static str>;
}

impl Assessed for CandidateAssessment {
    fn violated_hard_ids(&self) -> Vec<&'static str> {
        CandidateAssessment::violated_hard_ids(self)
    }
}

impl Assessed for AssessmentView {
    fn violated_hard_ids(&self) -> Vec<&'static str> {
        AssessmentView::violated_hard_ids(self)
    }
}

/// `assess_product_candidate(config, design)`, shared across test processes.
///
/// The result is `Err` with the evaluator's own reason when the candidate
/// cannot be assessed; that outcome is stored too.
pub fn assess_view(config: &AlasConfig, design: &DesignVector) -> Result<AssessmentView, String> {
    let config_json = serde_json::to_string(config).expect("a configuration serializes");
    let name = format!(
        "opt-assessment/{}/{}",
        config.preset,
        memo::digest(&[&config_json, &format!("{design:?}")])
    );
    memo::cached(&name, || {
        assess_product_candidate(config, design).map(|assessment| AssessmentView::from(&assessment))
    })
}

/// The registered nominal design of `preset` assessed in `mode`, with every
/// other setting as a bare `{"preset": ...}` document loads it.
pub fn nominal_assessment(preset: &str, mode: MtowSizing) -> Result<AssessmentView, String> {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    config.optimizer.objective.mtow_sizing = mode;
    let design = alas_config::presets::get(preset)
        .expect("registered preset")
        .design_vector;
    assess_view(&config, &design)
}

/// Serde adapters: the identifiers are `&'static str` in the live type, so
/// reading them back interns each distinct string once.
mod static_text {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(text: &Static, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(text)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Static, D::Error> {
        static INTERNED: OnceLock<Mutex<HashMap<String, Static>>> = OnceLock::new();
        let text = String::deserialize(deserializer)?;
        let mut table = INTERNED
            .get_or_init(Mutex::default)
            .lock()
            .expect("the intern table is not poisoned");
        let interned = *table
            .entry(text)
            .or_insert_with_key(|key| Box::leak(key.clone().into_boxed_str()));
        Ok(interned)
    }
}

mod role_text {
    use super::*;
    use serde::de::Error;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(role: &ResidualRole, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match role {
            ResidualRole::Constraint => "constraint",
            ResidualRole::Preference => "preference",
            ResidualRole::Diagnostic => "diagnostic",
        })
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<ResidualRole, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "constraint" => Ok(ResidualRole::Constraint),
            "preference" => Ok(ResidualRole::Preference),
            "diagnostic" => Ok(ResidualRole::Diagnostic),
            other => Err(D::Error::custom(format!("unknown residual role {other}"))),
        }
    }
}

mod family_text {
    use super::*;
    use serde::de::Error;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        family: &ConstraintFamily,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match family {
            ConstraintFamily::Mass => "mass",
            ConstraintFamily::Balance => "balance",
            ConstraintFamily::Performance => "performance",
            ConstraintFamily::Geometry => "geometry",
            ConstraintFamily::Structure => "structure",
        })
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<ConstraintFamily, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "mass" => Ok(ConstraintFamily::Mass),
            "balance" => Ok(ConstraintFamily::Balance),
            "performance" => Ok(ConstraintFamily::Performance),
            "geometry" => Ok(ConstraintFamily::Geometry),
            "structure" => Ok(ConstraintFamily::Structure),
            other => Err(D::Error::custom(format!(
                "unknown constraint family {other}"
            ))),
        }
    }
}
