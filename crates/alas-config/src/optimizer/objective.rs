// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the design search minimises, and which requirements bound it.
//!
//! The frozen objective rewards cruise lift-to-drag and prices every way of
//! cheating it with a weighted penalty. A conceptual transport is not
//! designed that way: it is sized by a mission -- payload over a design range
//! under a reserve policy -- and judged by what that mission costs, with the
//! certification and operating requirements as boundaries rather than as
//! prices. This group selects that formulation. The objective is a mission
//! quantity such as block fuel or takeoff mass; the maximum takeoff mass is
//! closed by the mission rather than typed in; and each family of
//! requirements is declared hard (a candidate that misses it is infeasible),
//! soft (it ranks behind feasibility but ahead of the objective), diagnostic
//! (reported, never ranked) or off.
//!
//! The legacy formulation remains selectable, because the parity fixtures
//! replay it and because a fixed-mass aerodynamic study is still a valid
//! question to ask.

use serde::{Deserialize, Serialize};

use crate::{ConfigNode, Kind, Leaf};

/// The scalar the search minimises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveKind {
    /// The frozen weighted-penalty objective on cruise lift-to-drag.
    #[default]
    LegacyLiftToDrag,
    /// Block fuel for the sizing mission: taxi plus trip fuel.
    BlockFuel,
    /// Maximum takeoff mass closed by the sizing mission.
    TakeoffMass,
    /// Operating empty mass at the closed takeoff mass.
    OperatingEmptyMass,
    /// Block fuel divided by design passengers and design range.
    FuelPerSeatKilometre,
}

impl ObjectiveKind {
    /// Stable serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LegacyLiftToDrag => "legacy_lift_to_drag",
            Self::BlockFuel => "block_fuel",
            Self::TakeoffMass => "takeoff_mass",
            Self::OperatingEmptyMass => "operating_empty_mass",
            Self::FuelPerSeatKilometre => "fuel_per_seat_kilometre",
        }
    }

    /// Whether the objective needs the mission-sized candidate evaluation.
    pub const fn is_mission_sized(self) -> bool {
        !matches!(self, Self::LegacyLiftToDrag)
    }
}

impl Leaf for ObjectiveKind {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

/// Whether the takeoff mass is an input or a closed output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtowSizing {
    /// The takeoff mass is the requirement value and the mission must fit.
    #[default]
    FixedRequirement,
    /// The takeoff mass is iterated until empty mass, payload and required
    /// fuel sum to it, bounded above by the requirement value.
    SizedByMission,
}

impl MtowSizing {
    /// Stable serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FixedRequirement => "fixed_requirement",
            Self::SizedByMission => "sized_by_mission",
        }
    }
}

impl Leaf for MtowSizing {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

/// How a family of requirements takes part in the ranking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintPolicy {
    /// A violation makes the candidate infeasible.
    #[default]
    Hard,
    /// A violation ranks the candidate behind compliant ones, ahead of the objective.
    Soft,
    /// The residual is reported and never ranked.
    Diagnostic,
    /// The family is not evaluated.
    Off,
}

impl ConstraintPolicy {
    /// Stable serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hard => "hard",
            Self::Soft => "soft",
            Self::Diagnostic => "diagnostic",
            Self::Off => "off",
        }
    }
}

impl Leaf for ConstraintPolicy {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

/// The mission-sized objective and its constraint policies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(default, deny_unknown_fields)]
pub struct ObjectiveConfig {
    /// The scalar the search minimises.
    #[config(
        options = ObjectiveKind,
        label = "Objective",
        help = "Quantity the design search minimises. The legacy lift-to-drag objective keeps the frozen weighted-penalty formulation; every other choice sizes each candidate by the design mission under the fuel policy and ranks it feasibility first."
    )]
    pub kind: ObjectiveKind,

    /// Still-air range of the sizing mission.
    #[config(
        label = "Design range",
        unit = "nmi",
        help = "Still-air distance from takeoff to the intended destination that the design mission is flown over, with the design payload. Zero uses the great-circle distance between the selected departure and arrival aerodromes. Reserve segments never receive range credit."
    )]
    pub design_range_nmi: f64,

    /// Whether the takeoff mass is closed by the mission.
    #[config(
        options = MtowSizing,
        label = "Takeoff mass sizing",
        help = "Whether the maximum takeoff mass is the fixed requirement value or is iterated until empty mass, payload and the fuel the policy requires sum to it. Sizing by mission makes the structural mass follow the closed takeoff mass, which is what lets a lighter wing pay for itself."
    )]
    pub mtow_sizing: MtowSizing,

    /// How many closure passes the sizing loop may take.
    #[config(
        label = "Sizing iteration limit",
        help = "Maximum fixed-point passes of empty mass, fuel and takeoff mass before a candidate is reported as not closed. Transport closures normally settle in under ten passes."
    )]
    pub sizing_max_iterations: i64,

    /// Takeoff-mass change below which the loop is closed.
    #[config(
        label = "Sizing tolerance",
        unit = "kg",
        help = "Change in takeoff mass between two passes below which the sizing loop is taken as closed."
    )]
    pub sizing_tolerance_kg: f64,

    /// Policy for the mass and fuel-volume requirements.
    #[config(
        options = ConstraintPolicy,
        label = "Mass and fuel constraints",
        help = "How the fuel-capacity, maximum-zero-fuel, maximum-landing and takeoff-mass-ceiling requirements take part in the ranking: hard makes a miss infeasible, soft ranks it behind compliant candidates, diagnostic only reports it."
    )]
    pub mass_constraints: ConstraintPolicy,

    /// Policy for the balance requirements.
    #[config(
        options = ConstraintPolicy,
        label = "Balance constraints",
        help = "How the centre-of-gravity envelope, gear reactions and static-margin floor take part in the ranking across the loading states from empty to takeoff."
    )]
    pub balance_constraints: ConstraintPolicy,

    /// Policy for the airworthiness performance requirements.
    #[config(
        options = ConstraintPolicy,
        label = "Performance constraints",
        help = "How the CS-25.121 engine-out second-segment climb gradient, the takeoff and landing field lengths at the selected aerodromes, the cruise thrust margin and the approach-speed limit take part in the ranking."
    )]
    pub performance_constraints: ConstraintPolicy,

    /// Policy for the geometric and accommodation requirements.
    #[config(
        options = ConstraintPolicy,
        label = "Geometry constraints",
        help = "How the span limit, the maximum wing area, the minimum wing loading, the tail volume window and the passenger-capacity requirement take part in the ranking."
    )]
    pub geometry_constraints: ConstraintPolicy,

    /// Largest wingspan the aerodrome code admits.
    #[config(
        label = "Maximum wingspan",
        unit = "m",
        help = "Largest wingspan the intended aerodrome reference code admits: 36 m for code C, 52 m for D, 65 m for E and 80 m for F (ICAO Annex 14). Zero disables the limit."
    )]
    pub max_span_m: f64,

    /// Highest approach speed the design may have.
    #[config(
        label = "Maximum approach speed",
        unit = "kt",
        help = "Upper bound on the reference landing speed at the maximum landing mass. Approach category C ends at 140 kt and D at 165 kt (ICAO PANS-OPS). Zero disables the limit."
    )]
    pub max_approach_speed_kt: f64,

    /// Weight of the soft-residual sum relative to the objective.
    #[config(
        label = "Soft-constraint penalty weight",
        help = "Scale applied to the sum of normalised soft-constraint violations before it is added to the normalised objective. Soft families rank behind hard feasibility regardless of this weight; it only decides how much a soft miss costs against the objective."
    )]
    pub soft_penalty_weight: f64,
}

impl Default for ObjectiveConfig {
    fn default() -> Self {
        Self {
            kind: ObjectiveKind::LegacyLiftToDrag,
            design_range_nmi: 0.0,
            mtow_sizing: MtowSizing::FixedRequirement,
            sizing_max_iterations: 30,
            sizing_tolerance_kg: 1.0,
            mass_constraints: ConstraintPolicy::Hard,
            balance_constraints: ConstraintPolicy::Hard,
            performance_constraints: ConstraintPolicy::Hard,
            geometry_constraints: ConstraintPolicy::Hard,
            max_span_m: 80.0,
            max_approach_speed_kt: 0.0,
            soft_penalty_weight: 10.0,
        }
    }
}

impl ObjectiveConfig {
    /// Whether the serialized group equals the defaults.
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Reject values the sizing loop cannot run with.
    pub fn validate(&self) -> Result<(), String> {
        if !self.design_range_nmi.is_finite() || self.design_range_nmi < 0.0 {
            return Err("design range must be finite and nonnegative".to_owned());
        }
        if self.sizing_max_iterations < 1 {
            return Err("sizing iteration limit must be at least one".to_owned());
        }
        if !self.sizing_tolerance_kg.is_finite() || self.sizing_tolerance_kg <= 0.0 {
            return Err("sizing tolerance must be positive".to_owned());
        }
        for (name, value) in [
            ("max_span_m", self.max_span_m),
            ("max_approach_speed_kt", self.max_approach_speed_kt),
            ("soft_penalty_weight", self.soft_penalty_weight),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(format!("objective {name} must be finite and nonnegative"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_objective_keeps_the_frozen_lift_to_drag_formulation() {
        let objective = ObjectiveConfig::default();
        assert_eq!(objective.kind, ObjectiveKind::LegacyLiftToDrag);
        assert!(!objective.kind.is_mission_sized());
        assert_eq!(objective.mtow_sizing, MtowSizing::FixedRequirement);
        assert!(objective.is_default());
        assert!(objective.validate().is_ok());
    }

    #[test]
    fn every_mission_objective_reports_itself_as_mission_sized() {
        for kind in [
            ObjectiveKind::BlockFuel,
            ObjectiveKind::TakeoffMass,
            ObjectiveKind::OperatingEmptyMass,
            ObjectiveKind::FuelPerSeatKilometre,
        ] {
            assert!(kind.is_mission_sized(), "{}", kind.as_str());
            assert_eq!(
                serde_json::to_value(kind).ok(),
                Some(serde_json::json!(kind.as_str()))
            );
        }
    }

    #[test]
    fn policy_and_sizing_names_are_stable_in_saved_configuration() {
        assert_eq!(
            serde_json::to_value(ConstraintPolicy::Diagnostic).ok(),
            Some(serde_json::json!("diagnostic"))
        );
        assert_eq!(
            serde_json::to_value(MtowSizing::SizedByMission).ok(),
            Some(serde_json::json!("sized_by_mission"))
        );
    }

    #[test]
    fn a_zero_iteration_limit_is_rejected() {
        let objective = ObjectiveConfig {
            sizing_max_iterations: 0,
            ..Default::default()
        };
        assert!(objective.validate().is_err());
    }
}
