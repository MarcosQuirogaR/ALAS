// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mission plan: an ordered list of stages with mass and tank events.
//!
//! The standard mission is one fixed sequence (take-off, climb ladder,
//! cruise legs, descent ladder, landing) closed over the selected route. A
//! student's mission is rarely that: holds, a missed approach, a diversion,
//! a payload drop, a launched drone, an air-to-air refuelling, a trim-tank
//! transfer. A [`MissionPlan`] states such a mission as data. An empty plan
//! (the default) means the standard mission, so every saved configuration
//! and every registered aircraft flies exactly as before; a plan with stages
//! is flown stage by stage by the pipeline's plan engine.
//!
//! The order of stages is validated ([`MissionPlan::validate`]): the aircraft
//! is on the ground, takes off first and lands last; a go-around follows an
//! approach. [`MissionPlan::move_stage`] reorders with that check and leaves
//! the plan unchanged when the new order is refused. Events belong to their
//! stage and move with it.
//!
//! Templates: [`RaymerProfile`] builds the four example sizing mission
//! profiles of Raymer, *Aircraft Design: A Conceptual Approach*, 6th ed.,
//! Fig. 3.2 (p. 32), as stage lists.

mod kinds;
mod profile_view;
mod templates;
mod validate;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

pub use kinds::{
    CruiseExtent, EventAction, EventTime, PlanStage, StageEvent, StageKind, TakeoffField,
};
pub use templates::{RaymerProfile, TemplateInputs, CATEGORY_I_DECISION_HEIGHT_FT};
pub use validate::PlanError;

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// A user-defined mission: stages in flight order.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionPlan {
    /// Display name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The stages, in the order they are flown. Empty: the standard mission.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<PlanStage>,
    /// Contingency fuel kept at the end of the plan, as a fraction of the fuel
    /// burned by the stages that are not reserves.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub contingency_fraction: f64,
    /// Fuel loaded at brake release, kg. Absent: the plan is closed, that is
    /// the fuel it burns plus the contingency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub takeoff_fuel_kg: Option<f64>,
    /// Centre-of-gravity target of the payload auto-balance, percent MAC:
    /// movable payload items are shifted within their regions towards it.
    /// Absent: the cabin's cargo target, else the cargo loader's default
    /// (25 % MAC). Only items flagged `movable` ever move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance_target_pct_mac: Option<f64>,
    /// Lock Layout: the auto-balance never moves a payload item.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lock_layout: bool,
}

impl MissionPlan {
    /// Whether no stage is declared (the standard mission flies).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
            && self.name.is_empty()
            && self.contingency_fraction == 0.0
            && self.takeoff_fuel_kg.is_none()
            && self.balance_target_pct_mac.is_none()
            && !self.lock_layout
    }

    /// Whether the plan has stages to fly.
    #[must_use]
    pub fn has_stages(&self) -> bool {
        !self.stages.is_empty()
    }

    /// [`Self::validate`] as a blocking validation issue, if it fails.
    pub(crate) fn issue(&self) -> Option<crate::ValidationIssue> {
        self.validate().err().map(|error| crate::ValidationIssue {
            severity: crate::Severity::Error,
            field_path: "mission.plan".to_owned(),
            message: error.to_string(),
        })
    }

    /// Move the stage at `from` to position `to`, keeping the order valid.
    ///
    /// # Errors
    ///
    /// [`PlanError`] naming the rule the new order breaks (take-off must come
    /// first, landing last); the plan is unchanged then.
    pub fn move_stage(&mut self, from: usize, to: usize) -> Result<(), PlanError> {
        if from >= self.stages.len() || to >= self.stages.len() {
            return Err(PlanError::NoSuchStage(from.max(to)));
        }
        let mut moved = self.stages.clone();
        let stage = moved.remove(from);
        moved.insert(to, stage);
        validate::order(&moved)?;
        self.stages = moved;
        Ok(())
    }

    /// The index of the stage with `id`.
    #[must_use]
    pub fn position(&self, id: &str) -> Option<usize> {
        self.stages.iter().position(|stage| stage.id == id)
    }
}
