// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the design search minimises, and which requirements bound it.
//!
//! A conceptual transport is sized by a mission: payload over a design
//! range under a reserve policy, and judged by what that mission costs,
//! with the certification and operating requirements as boundaries rather
//! than as prices. This group selects that formulation. The objective is a
//! mission quantity such as block fuel or takeoff mass; the maximum takeoff
//! mass is closed by the mission rather than typed in; and each family of
//! requirements is declared hard (a candidate that misses it is infeasible),
//! soft (it ranks behind feasibility but ahead of the objective), diagnostic
//! (reported, never ranked) or off.
//!
//! This group belongs to the mission-sized product profile. The default
//! `scipy_legacy` profile instead uses the original weighted lift-to-drag
//! objective and the penalty table in [`super::ObjectiveWeights`].

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
    /// The takeoff mass is iterated exactly as under `SizedByMission`, but
    /// the requirement value is used only to seed the first pass: it is
    /// never re-applied as a dispatch ceiling, an Aitken-extrapolation
    /// admissibility bound, or a landing-mass-fraction basis on any later
    /// pass, and the mission-required mass is not checked against it. This
    /// is a calibration/validation mode; it asks what the closure lands on
    /// with nothing but the seed pinned to the declared aircraft, not a
    /// sizing mode for producing a certifiable design against a declared
    /// requirement.
    Unconstrained,
    /// The takeoff mass is closed on the design mission (design range at
    /// design payload, with the fuel-policy reserves), seeded at the MTOW
    /// target `T` and clamped at `T (1 + p)`. A closure outside
    /// `[T (1 - p), T (1 + p)]` is rejected on either side; inside the band
    /// nothing pulls it toward `T`. The selected route is then flown
    /// off-design at the closed mass as a check.
    MtowBand,
    /// The takeoff mass is closed with no ceiling at the configured payload
    /// on the selected route, or on the design range when one is set, with
    /// the fuel-policy reserves, seeded from the declared MTOW.
    PayloadAdjusted,
}

impl MtowSizing {
    /// Stable serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FixedRequirement => "fixed_requirement",
            Self::SizedByMission => "sized_by_mission",
            Self::Unconstrained => "unconstrained",
            Self::MtowBand => "mtow_band",
            Self::PayloadAdjusted => "payload_adjusted",
        }
    }

    /// Every variant, in the order the form lists them.
    pub const ALL: [Self; 5] = [
        Self::FixedRequirement,
        Self::SizedByMission,
        Self::Unconstrained,
        Self::MtowBand,
        Self::PayloadAdjusted,
    ];

    /// The stable serialized names of [`Self::ALL`], in the same order: the
    /// schema's option list.
    pub const NAMES: [&'static str; 5] = [
        "fixed_requirement",
        "sized_by_mission",
        "unconstrained",
        "mtow_band",
        "payload_adjusted",
    ];

    /// Whether the mode closes the takeoff mass on a mission rather than
    /// taking it as the requirement value.
    pub const fn closes_mass(self) -> bool {
        !matches!(self, Self::FixedRequirement)
    }

    /// Whether the mode is evaluated by the mission-sized closure under
    /// every optimizer method. The two design modes are; the three original
    /// modes keep the reference replay under the legacy profile.
    pub const fn requires_mission_sized_evaluation(self) -> bool {
        matches!(self, Self::MtowBand | Self::PayloadAdjusted)
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
        help = "Used by the mission-sized differential_evolution profile. That profile sizes each candidate by the design mission and ranks it feasibility first. scipy_legacy ignores this selector and minimizes the original weighted L/D cost plus its penalty table."
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
        help = "Whether the maximum takeoff mass is the fixed requirement value, is iterated by the mission up to the requirement value, or is iterated by the mission with the requirement used only to seed the first pass. Sizing by mission makes the structural mass follow the closed takeoff mass, which is what lets a lighter wing pay for itself. Unconstrained runs the same mission-sized iteration with the requirement dropped as a ceiling after the seed, for asking what the closure converges to on its own rather than for producing a design bounded by a declared requirement. MTOW band closes the takeoff mass on the design mission (design range at design payload, with reserves), rejects a closure outside the target plus or minus the band fraction, and then flies the selected route off-design at the closed mass as a check. Payload adjusted closes it with no ceiling at the configured payload on the route, or on the design range when one is set. In both design modes the structure is sized at the closed mass."
    )]
    pub mtow_sizing: MtowSizing,

    /// Target MTOW of the band mode, kg; zero uses the requirement MTOW.
    #[config(
        label = "MTOW target",
        unit = "kg",
        help = "Centre of the MTOW band: the takeoff mass the design mission is expected to close at. Zero uses the requirement MTOW. Read only by the MTOW band mode."
    )]
    pub mtow_target_kg: f64,

    /// Half-width of the MTOW band as a fraction of its target.
    #[config(
        label = "MTOW band fraction",
        help = "Half-width of the MTOW band as a fraction of the target: a closed takeoff mass below the target times one minus this fraction, or above the target times one plus it, is rejected. Inside the band the closure is free and nothing pulls it toward the target. Read only by the MTOW band mode."
    )]
    pub mtow_band_fraction: f64,

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
        help = "Largest wingspan allowed by the selected aerodrome reference-code case. The 36/52/65/80 m values are study inputs representing codes C/D/E/F; this configuration does not infer a code from an ICAO identifier or runway length. Zero disables the limit."
    )]
    pub max_span_m: f64,

    /// Highest approach speed the design may have.
    #[config(
        label = "Maximum approach speed",
        unit = "kt",
        help = "Upper bound on reference approach speed at maximum landing mass for the selected operating/aerodrome category. The category and source must be selected explicitly; the configuration does not infer them from an ICAO identifier. Zero disables the limit."
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
            mtow_target_kg: 0.0,
            mtow_band_fraction: DEFAULT_MTOW_BAND_FRACTION,
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

/// Default half-width of the MTOW band, as a fraction of the target.
///
/// Engineering estimate: plus or minus five per cent is of the order of the
/// MTOW step between successive weight variants of one transport type, so a
/// closure outside it describes a different variant from the targeted one.
pub const DEFAULT_MTOW_BAND_FRACTION: f64 = 0.05;

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
        if !self.mtow_target_kg.is_finite() || self.mtow_target_kg < 0.0 {
            return Err("MTOW target must be finite and nonnegative".to_owned());
        }
        if !(self.mtow_band_fraction > 0.0 && self.mtow_band_fraction < 1.0) {
            return Err("MTOW band fraction must lie strictly between zero and one".to_owned());
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
        // The Python weighted L/D cost is selected by the solver profile,
        // not represented as one of the mission-objective enum values.
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
        assert_eq!(
            serde_json::to_value(MtowSizing::Unconstrained).ok(),
            Some(serde_json::json!("unconstrained"))
        );
        for (sizing, name) in MtowSizing::ALL.into_iter().zip(MtowSizing::NAMES) {
            assert_eq!(sizing.as_str(), name);
            assert_eq!(
                serde_json::to_value(sizing).ok(),
                Some(serde_json::json!(sizing.as_str()))
            );
        }
        assert_eq!(MtowSizing::MtowBand.as_str(), "mtow_band");
        assert_eq!(MtowSizing::PayloadAdjusted.as_str(), "payload_adjusted");
    }

    #[test]
    fn a_saved_objective_without_the_band_fields_loads_with_their_defaults() {
        let saved = serde_json::json!({"kind": "block_fuel", "mtow_sizing": "sized_by_mission"});
        let objective: ObjectiveConfig =
            serde_json::from_value(saved).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(objective.mtow_target_kg, 0.0);
        assert_eq!(objective.mtow_band_fraction, DEFAULT_MTOW_BAND_FRACTION);
        assert!(objective.is_default());
        let band: ObjectiveConfig = serde_json::from_value(serde_json::json!({
            "mtow_sizing": "mtow_band",
            "mtow_target_kg": 80_000.0,
            "mtow_band_fraction": 0.08
        }))
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(band.mtow_sizing, MtowSizing::MtowBand);
        assert!(band.validate().is_ok());
    }

    #[test]
    fn a_band_fraction_outside_the_open_unit_interval_is_rejected() {
        for fraction in [0.0, 1.0, -0.1, f64::NAN] {
            let objective = ObjectiveConfig {
                mtow_band_fraction: fraction,
                ..Default::default()
            };
            assert!(objective.validate().is_err(), "{fraction}");
        }
        let negative_target = ObjectiveConfig {
            mtow_target_kg: -1.0,
            ..Default::default()
        };
        assert!(negative_target.validate().is_err());
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
