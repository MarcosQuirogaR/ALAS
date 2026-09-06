// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the design search minimises, and which requirements bound it.
//!
//! A conceptual transport is sized by a mission -- payload over a design
//! range under a reserve policy -- and judged by what that mission costs,
//! with the certification and operating requirements as boundaries rather
//! than as prices. This group selects that formulation. The objective is a
//! mission quantity such as block fuel or takeoff mass; the maximum takeoff
//! mass is closed by the mission rather than typed in; and each family of
//! requirements is declared hard (a candidate that misses it is infeasible),
//! soft (it ranks behind feasibility but ahead of the objective), diagnostic
//! (reported, never ranked) or off.
//!
//! The frozen weighted lift-to-drag objective of the Python reference is not
//! a product objective: it survives only inside the parity replay
//! (`DesignObjective::new_reference_compatibility`), where the fixtures need
//! it, and cannot be selected here.

use serde::{Deserialize, Serialize};

use crate::{ConfigNode, Kind, Leaf};

/// The scalar the search minimises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveKind {
    /// Block fuel for the sizing mission: taxi plus trip fuel.
    #[default]
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
            Self::BlockFuel => "block_fuel",
            Self::TakeoffMass => "takeoff_mass",
            Self::OperatingEmptyMass => "operating_empty_mass",
            Self::FuelPerSeatKilometre => "fuel_per_seat_kilometre",
        }
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
    FixedRequirement,
    /// The takeoff mass is iterated until empty mass, payload and required
    /// fuel sum to it, bounded above by the requirement value.
    #[default]
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
        help = "Quantity the design search minimises. Every choice sizes each candidate by the design mission under the fuel policy and ranks it feasibility first: block fuel is the operating cost of the mission, takeoff mass the structural and airport cost, operating empty mass the manufacturing cost, and fuel per seat-kilometre the block fuel normalised by the design passengers and range."
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

    /// Centre-of-gravity shift between sizing passes above which the
    /// cruise trim and drag polar are re-evaluated.
    #[config(
        label = "Re-trim CG tolerance",
        unit = "% MAC",
        help = "When the takeoff mass is sized by the mission, the sizing loop re-trims the aircraft and re-evaluates its drag polar at the updated mass whenever the centre of gravity has moved by more than this fraction of the mean aerodynamic chord since the last trim, so the converged design is trimmed at its own weight. Zero keeps the single trim at the takeoff-mass ceiling."
    )]
    pub retrim_cg_tolerance_pct_mac: f64,

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
            kind: ObjectiveKind::BlockFuel,
            design_range_nmi: 0.0,
            mtow_sizing: MtowSizing::SizedByMission,
            sizing_max_iterations: 30,
            sizing_tolerance_kg: 1.0,
            retrim_cg_tolerance_pct_mac: 0.1,
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
            (
                "retrim_cg_tolerance_pct_mac",
                self.retrim_cg_tolerance_pct_mac,
            ),
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
    fn the_default_objective_is_block_fuel_over_a_mission_sized_takeoff_mass() {
        let objective = ObjectiveConfig::default();
        assert_eq!(objective.kind, ObjectiveKind::BlockFuel);
        assert_eq!(objective.mtow_sizing, MtowSizing::SizedByMission);
        assert!(objective.is_default());
        assert!(objective.validate().is_ok());
    }

    #[test]
    fn every_objective_is_a_mission_quantity_with_a_stable_name() {
        for kind in [
            ObjectiveKind::BlockFuel,
            ObjectiveKind::TakeoffMass,
            ObjectiveKind::OperatingEmptyMass,
            ObjectiveKind::FuelPerSeatKilometre,
        ] {
            assert_eq!(
                serde_json::to_value(kind).ok(),
                Some(serde_json::json!(kind.as_str()))
            );
        }
        // The frozen lift-to-drag objective is a parity-replay path, not a
        // saved-configuration value.
        assert!(
            serde_json::from_value::<ObjectiveKind>(serde_json::json!("legacy_lift_to_drag"))
                .is_err()
        );
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
