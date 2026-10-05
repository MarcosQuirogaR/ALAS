// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One resolver for what a takeoff-mass sizing mode means numerically.
//!
//! [`MtowSizing`] names a policy; the sizing loop, the residual table, the
//! cost normalisation and the structural stage each need numbers from it: the
//! mass the first pass is evaluated at, the dispatch clamp, the admissible
//! bound of the Aitken extrapolation, the upper and lower mass residual
//! limits, which mission the takeoff mass is closed on, and which mass the
//! structure is designed for. [`AlasConfig::mtow_plan`] resolves all of them
//! once, so no consumer re-derives a mode rule of its own.
//!
//! All masses are kg and all ranges are nautical miles (`nmi`,
//! 1 nmi = 1,852 m exactly), as in the configuration they come from.
//!
//! | Mode | Seed | Dispatch clamp | Upper limit | Lower limit | Closed on | Structure |
//! |---|---|---|---|---|---|---|
//! | `FixedRequirement` | cap | cap | cap | none | route, one pass | cap |
//! | `SizedByMission` | cap | cap | cap | none | route | design-mode basis |
//! | `Unconstrained` | cap | none | none | none | route | design-mode basis |
//! | `MtowBand` | `T` | `T (1 + p)` | `T (1 + p)` | `T (1 - p)` | design mission | closure |
//! | `PayloadAdjusted` | cap | none | none | none | design range, or route | closure |
//!
//! The design range is the objective's explicit value, else the registered
//! preset's charted payload/range point; `PayloadAdjusted` closes on the route
//! only when neither exists.
//!
//! "cap" is `requirements.mtow_kg`; `T` is `mtow_target_kg` (zero means the
//! cap) and `p` is `mtow_band_fraction`. An explicit
//! `flops_structure.design_gross_mass_kg` keeps the structure at that
//! declared weight in every mode.

use crate::optimizer::{DesignMode, MtowSizing};
use crate::{AlasConfig, MassSizingBasis};

/// Dispatch clamp of a mode with no MTOW ceiling, kg.
///
/// A finite sentinel far above any transport takeoff mass: the dispatch
/// solver validates a finite MTOW limit, so an infinite one would fail every
/// pass as an input error before the closure runs.
pub const UNBOUNDED_DISPATCH_MTOW_KG: f64 = 1.0e9;

/// Which mass the FLOPS structural design gross mass `DG` is taken at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralBasis {
    /// A declared weight: the requirement MTOW, the registered aircraft's
    /// design weights, or an explicit `design_gross_mass_kg` override.
    DeclaredCap,
    /// The takeoff mass the sizing loop closes at.
    ClosureMass,
}

impl StructuralBasis {
    /// Stable report label.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeclaredCap => "declared_cap",
            Self::ClosureMass => "closure_mass",
        }
    }
}

/// Where the design-mission range comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DesignRange {
    /// `optimizer.objective.design_range_nmi`, nmi.
    Objective(f64),
    /// The registered preset's charted design range
    /// (`reference.design_point`), nmi.
    ChartedPoint(f64),
    /// The declared FLOPS design range `DESRNG`,
    /// `mass_model.flops_transport.design_range_nmi`, nmi: a mass-equation
    /// input, used as a mission only when the preset charts no design point.
    FlopsDesignRange(f64),
    /// Neither is declared: the great-circle distance between the selected
    /// aerodromes, which the evaluator resolves from their records.
    Route,
}

impl DesignRange {
    /// The declared range, nmi, or `None` when it is the route distance.
    pub const fn declared_nmi(self) -> Option<f64> {
        match self {
            Self::Objective(nmi) | Self::ChartedPoint(nmi) | Self::FlopsDesignRange(nmi) => {
                Some(nmi)
            }
            Self::Route => None,
        }
    }
}

/// Where the design payload comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesignPayloadSource {
    /// The payload charted with the preset's design range
    /// (`reference.design_point`), carried as a mass.
    ChartedPoint,
    /// The registered preset's planning cabin, seats.
    PlanningSeats(i64),
    /// The configured load case: passengers at the combined passenger and
    /// baggage mass, or the cargo payload of a freighter.
    ConfiguredPayload,
}

/// A mission the takeoff mass is closed on instead of the selected route.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DesignMission {
    /// Still-air range.
    pub range: DesignRange,
    /// Payload flown, kg, or `None` for the configured load case the payload
    /// layout places on the candidate. The mission-sized evaluator uses a
    /// planning-seat value as the laid-out load case with its seated
    /// passengers replaced by the planning seats, so the non-passenger
    /// payload the layout carries (container tare, revenue freight) is the
    /// same on the design mission and on the route; a configured-payload
    /// design mission flies the laid-out load case itself.
    pub payload_kg: Option<f64>,
    /// Where `payload_kg` comes from.
    pub payload_source: DesignPayloadSource,
}

/// Every number a takeoff-mass sizing mode resolves to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MtowPlan {
    /// The mode resolved.
    pub mode: MtowSizing,
    /// `requirements.mtow_kg`.
    pub declared_mtow_kg: f64,
    /// Band centre `T`, kg (the declared MTOW when no target is set).
    pub target_kg: f64,
    /// Band half-width `p`, fraction of `T`.
    pub band_fraction: f64,
    /// Takeoff mass of the first pass, kg.
    pub seed_kg: f64,
    /// MTOW limit handed to the dispatch solver, kg.
    pub dispatch_ceiling_kg: f64,
    /// Largest admissible Aitken estimate, kg (may be infinite).
    pub aitken_ceiling_kg: f64,
    /// Upper takeoff-mass residual limit, kg, when the mode has one.
    pub upper_bound_kg: Option<f64>,
    /// Lower takeoff-mass residual limit, kg, when the mode has one.
    pub lower_bound_kg: Option<f64>,
    /// Whether the sizing loop iterates rather than taking one pass.
    pub iterates: bool,
    /// The mission closed instead of the route, when the mode flies one; the
    /// route is then flown off-design at the closed mass as a check.
    pub design_mission: Option<DesignMission>,
    /// Which mass the structure is designed for.
    pub structural_basis: StructuralBasis,
    /// Mass scale the objective value is normalised by, kg.
    pub normalisation_kg: f64,
}

impl MtowPlan {
    /// Whether the takeoff mass is closed by a mission.
    pub const fn closes_mass(&self) -> bool {
        self.mode.closes_mass()
    }

    /// Whether the mode is evaluated by the mission-sized closure under
    /// every optimizer method (see
    /// [`MtowSizing::requires_mission_sized_evaluation`]).
    pub const fn requires_mission_sized_evaluation(&self) -> bool {
        self.mode.requires_mission_sized_evaluation()
    }

    /// Whether the selected route is flown off-design at the closed mass.
    pub const fn checks_offdesign(&self) -> bool {
        self.design_mission.is_some()
    }

    /// Whether the report's analysis mass is the closure rather than the cap.
    pub const fn analyses_at_closure(&self) -> bool {
        !matches!(self.mode, MtowSizing::FixedRequirement)
    }
}

impl AlasConfig {
    /// The registered preset's charted design point, when it has one.
    fn charted_design_point(&self) -> Option<crate::PayloadRangeDesignPoint> {
        crate::presets::get(&self.preset)
            .ok()
            .and_then(|preset| preset.reference.design_point)
            .filter(|point| point.range_nmi.is_finite() && point.range_nmi > 0.0)
    }

    /// The design range when the objective declares none: the preset's
    /// charted range, otherwise the declared FLOPS range, otherwise the route.
    fn design_range_default(&self) -> DesignRange {
        if let Some(point) = self.charted_design_point() {
            return DesignRange::ChartedPoint(point.range_nmi);
        }
        match self.mass_model.flops_transport.design_range_nmi {
            Some(nmi) if nmi.is_finite() && nmi > 0.0 => DesignRange::FlopsDesignRange(nmi),
            _ => DesignRange::Route,
        }
    }

    /// The design mission: design range at design payload.
    ///
    /// The range is `optimizer.objective.design_range_nmi` when positive,
    /// otherwise the preset's charted design range
    /// (`reference.design_point`, the range at maximum structural payload
    /// read from the manufacturer's payload/range chart), otherwise the
    /// declared FLOPS design range, otherwise the route. The FLOPS `DESRNG`
    /// is a mass-equation input and stays separate from the mission range
    /// whenever the preset charts one. The payload is the charted payload
    /// when the design point carries one, otherwise the preset's planning
    /// seats at the combined passenger and baggage mass
    /// (`requirements.passenger_mass_kg`) plus the declared revenue belly
    /// freight (`cabin.passenger.belly_cargo_kg`) when the preset declares
    /// `reference.planning_seats`, otherwise the configured payload plus that
    /// freight. A freighter carries its cargo payload.
    pub fn design_mission(&self) -> DesignMission {
        let objective_nmi = self.optimizer.objective.design_range_nmi;
        let range = if objective_nmi.is_finite() && objective_nmi > 0.0 {
            DesignRange::Objective(objective_nmi)
        } else {
            self.design_range_default()
        };
        let (payload_kg, payload_source) = self.design_payload_kg();
        DesignMission {
            range,
            payload_kg: Some(payload_kg),
            payload_source,
        }
    }

    /// Design payload, kg, and its source (see [`Self::design_mission`]).
    pub fn design_payload_kg(&self) -> (f64, DesignPayloadSource) {
        let requirements = &self.requirements;
        if requirements.aircraft_type == "cargo" {
            return (
                requirements.cargo_payload_kg.max(0.0),
                DesignPayloadSource::ConfiguredPayload,
            );
        }
        if let Some(payload_kg) = self
            .charted_design_point()
            .and_then(|point| point.payload_kg)
            .filter(|kg| kg.is_finite() && *kg > 0.0)
        {
            return (payload_kg, DesignPayloadSource::ChartedPoint);
        }
        let freight_kg = self.cabin.passenger.belly_cargo_kg.max(0.0);
        let planning_seats = crate::presets::get(&self.preset)
            .ok()
            .and_then(|preset| preset.reference.planning_seats)
            .filter(|seats| *seats > 0);
        match planning_seats {
            Some(seats) => (
                seats as f64 * requirements.passenger_mass_kg + freight_kg,
                DesignPayloadSource::PlanningSeats(seats),
            ),
            None => (
                requirements.payload_kg().max(0.0) + freight_kg,
                DesignPayloadSource::ConfiguredPayload,
            ),
        }
    }

    /// Resolve the configured takeoff-mass sizing mode into numbers.
    pub fn mtow_plan(&self) -> MtowPlan {
        let objective = &self.optimizer.objective;
        let mode = objective.mtow_sizing;
        let cap = self.requirements.mtow_kg;
        let target_kg = if objective.mtow_target_kg.is_finite() && objective.mtow_target_kg > 0.0 {
            objective.mtow_target_kg
        } else {
            cap
        };
        let p = objective.mtow_band_fraction;
        let declared_structure = self
            .mass_model
            .flops_structure
            .design_gross_mass_kg
            .is_some();
        let follows_design_mode = match self.mass_sizing_basis() {
            MassSizingBasis::Coupled => StructuralBasis::ClosureMass,
            MassSizingBasis::FixedAircraft { .. } => StructuralBasis::DeclaredCap,
        };
        let design_structure = if declared_structure {
            StructuralBasis::DeclaredCap
        } else {
            StructuralBasis::ClosureMass
        };
        let base = MtowPlan {
            mode,
            declared_mtow_kg: cap,
            target_kg,
            band_fraction: p,
            seed_kg: cap,
            dispatch_ceiling_kg: cap,
            aitken_ceiling_kg: cap,
            upper_bound_kg: Some(cap),
            lower_bound_kg: None,
            iterates: true,
            design_mission: None,
            structural_basis: follows_design_mode,
            normalisation_kg: cap,
        };
        match mode {
            // A coupled ledger is closed at the analysis mass, which is the
            // cap in this mode, so the structure is at the cap either way.
            MtowSizing::FixedRequirement => MtowPlan {
                iterates: false,
                structural_basis: StructuralBasis::DeclaredCap,
                ..base
            },
            MtowSizing::SizedByMission => base,
            MtowSizing::Unconstrained => MtowPlan {
                dispatch_ceiling_kg: UNBOUNDED_DISPATCH_MTOW_KG,
                aitken_ceiling_kg: f64::INFINITY,
                upper_bound_kg: None,
                ..base
            },
            MtowSizing::MtowBand => {
                let upper_kg = target_kg * (1.0 + p);
                MtowPlan {
                    seed_kg: target_kg,
                    dispatch_ceiling_kg: upper_kg,
                    aitken_ceiling_kg: upper_kg,
                    upper_bound_kg: Some(upper_kg),
                    lower_bound_kg: Some(target_kg * (1.0 - p)),
                    design_mission: Some(self.design_mission()),
                    structural_basis: design_structure,
                    normalisation_kg: target_kg,
                    ..base
                }
            }
            MtowSizing::PayloadAdjusted => {
                let explicit_range = objective.design_range_nmi;
                let range = if explicit_range.is_finite() && explicit_range > 0.0 {
                    Some(DesignRange::Objective(explicit_range))
                } else {
                    self.charted_design_point()
                        .map(|point| DesignRange::ChartedPoint(point.range_nmi))
                };
                MtowPlan {
                    dispatch_ceiling_kg: UNBOUNDED_DISPATCH_MTOW_KG,
                    aitken_ceiling_kg: f64::INFINITY,
                    upper_bound_kg: None,
                    design_mission: range.map(|range| DesignMission {
                        range,
                        payload_kg: None,
                        payload_source: DesignPayloadSource::ConfiguredPayload,
                    }),
                    structural_basis: design_structure,
                    ..base
                }
            }
        }
    }

    /// Whether this configuration's design mode describes a registered
    /// aircraft whose declared weights are inputs.
    pub(crate) fn is_fixed_aircraft_mode(&self) -> bool {
        matches!(
            self.optimizer.design_space.mode,
            DesignMode::BaselineSandbox | DesignMode::ReferenceAdaptation
        )
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn with_mode(mut config: AlasConfig, mode: MtowSizing) -> AlasConfig {
        config.optimizer.objective.mtow_sizing = mode;
        config
    }

    #[test]
    fn the_plan_table_matches_each_mode() {
        let mut base = AlasConfig::default();
        base.requirements.mtow_kg = 100_000.0;
        base.optimizer.objective.mtow_target_kg = 90_000.0;
        base.optimizer.objective.mtow_band_fraction = 0.1;

        let fixed = with_mode(base.clone(), MtowSizing::FixedRequirement).mtow_plan();
        assert!(!fixed.iterates && !fixed.closes_mass() && !fixed.analyses_at_closure());
        assert_eq!(fixed.seed_kg, 100_000.0);
        assert_eq!(fixed.dispatch_ceiling_kg, 100_000.0);
        assert_eq!(fixed.upper_bound_kg, Some(100_000.0));
        assert_eq!(fixed.design_mission, None);
        assert_eq!(fixed.structural_basis, StructuralBasis::DeclaredCap);

        let sized = with_mode(base.clone(), MtowSizing::SizedByMission).mtow_plan();
        assert!(sized.iterates && sized.closes_mass());
        assert!(!sized.requires_mission_sized_evaluation());
        assert_eq!(sized.aitken_ceiling_kg, 100_000.0);
        assert_eq!(sized.normalisation_kg, 100_000.0);

        let free = with_mode(base.clone(), MtowSizing::Unconstrained).mtow_plan();
        assert_eq!(free.upper_bound_kg, None);
        assert_eq!(free.dispatch_ceiling_kg, UNBOUNDED_DISPATCH_MTOW_KG);
        assert!(free.aitken_ceiling_kg.is_infinite());

        let band = with_mode(base.clone(), MtowSizing::MtowBand).mtow_plan();
        assert!(band.requires_mission_sized_evaluation() && band.checks_offdesign());
        assert_eq!(band.seed_kg, 90_000.0);
        assert!((band.dispatch_ceiling_kg - 99_000.0).abs() < 1e-9);
        assert_eq!(band.upper_bound_kg, Some(band.dispatch_ceiling_kg));
        assert!((band.lower_bound_kg.unwrap() - 81_000.0).abs() < 1e-9);
        assert_eq!(band.normalisation_kg, 90_000.0);
        assert_eq!(band.structural_basis, StructuralBasis::ClosureMass);
        let mission = band.design_mission.unwrap();
        assert_eq!(mission.range, DesignRange::FlopsDesignRange(7_600.0));
        assert!(mission.payload_kg.unwrap() > 0.0);

        let adjusted = with_mode(base.clone(), MtowSizing::PayloadAdjusted).mtow_plan();
        assert_eq!(adjusted.seed_kg, 100_000.0);
        assert_eq!(adjusted.upper_bound_kg, None);
        assert_eq!(adjusted.lower_bound_kg, None);
        assert_eq!(adjusted.design_mission, None);
        assert_eq!(adjusted.structural_basis, StructuralBasis::ClosureMass);
        let mut ranged = with_mode(base, MtowSizing::PayloadAdjusted);
        ranged.optimizer.objective.design_range_nmi = 2_000.0;
        let mission = ranged.mtow_plan().design_mission.unwrap();
        assert_eq!(mission.range, DesignRange::Objective(2_000.0));
        assert_eq!(mission.payload_kg, None);
    }

    #[test]
    fn a_zero_target_centres_the_band_on_the_requirement() {
        let mut config = with_mode(AlasConfig::default(), MtowSizing::MtowBand);
        config.requirements.mtow_kg = 80_000.0;
        let plan = config.mtow_plan();
        assert_eq!(plan.target_kg, 80_000.0);
        assert_eq!(plan.seed_kg, 80_000.0);
    }

    #[test]
    fn a_registered_aircraft_is_designed_at_the_closure_in_the_design_modes_only() {
        let mut config = AlasConfig::from_value(&json!({"preset": "A320-200"})).unwrap();
        config.optimizer.design_space.mode = DesignMode::ReferenceAdaptation;
        for mode in MtowSizing::ALL {
            let plan = with_mode(config.clone(), mode).mtow_plan();
            let expected = if mode.requires_mission_sized_evaluation() {
                StructuralBasis::ClosureMass
            } else {
                StructuralBasis::DeclaredCap
            };
            assert_eq!(plan.structural_basis, expected, "{mode:?}");
        }
        config.mass_model.flops_structure.design_gross_mass_kg = Some(79_000.0);
        let plan = with_mode(config, MtowSizing::MtowBand).mtow_plan();
        assert_eq!(plan.structural_basis, StructuralBasis::DeclaredCap);
    }

    #[test]
    fn the_design_range_prefers_the_objective_then_flops_then_the_route() {
        let mut config = AlasConfig::default();
        config.optimizer.objective.design_range_nmi = 3_000.0;
        assert_eq!(
            config.design_mission().range,
            DesignRange::Objective(3_000.0)
        );
        config.optimizer.objective.design_range_nmi = 0.0;
        assert_eq!(
            config.design_mission().range,
            DesignRange::FlopsDesignRange(7_600.0)
        );
        config.mass_model.flops_transport.design_range_nmi = None;
        assert_eq!(config.design_mission().range, DesignRange::Route);
        assert_eq!(DesignRange::Route.declared_nmi(), None);
    }

    #[test]
    fn the_design_payload_uses_planning_seats_when_the_preset_declares_them() {
        let config = AlasConfig::from_value(&json!({"preset": "ATR72-600"})).unwrap();
        let (payload_kg, source) = config.design_payload_kg();
        assert_eq!(source, DesignPayloadSource::PlanningSeats(72));
        let expected = 72.0 * config.requirements.passenger_mass_kg
            + config.cabin.passenger.belly_cargo_kg.max(0.0);
        assert!((payload_kg - expected).abs() < 1e-9);

        let clean = AlasConfig::default();
        let (payload_kg, source) = clean.design_payload_kg();
        assert_eq!(source, DesignPayloadSource::ConfiguredPayload);
        assert!((payload_kg - clean.requirements.payload_kg()).abs() < 1e-9);
    }

    /// Every registered aircraft declares a sourced design point; its
    /// payload is at most the declared maximum structural payload within the
    /// chart read uncertainty (+-5 %), and the design mission flies exactly
    /// that point.
    #[test]
    fn every_registered_aircraft_closes_the_design_modes_on_its_charted_point() {
        for name in crate::presets::available() {
            let preset = crate::presets::get(name).unwrap();
            let point = preset.reference.design_point.expect(name);
            assert!(point.range_nmi > 0.0 && !point.source.is_empty(), "{name}");
            let config = AlasConfig::from_value(&json!({ "preset": name })).unwrap();
            let mission = config.design_mission();
            assert_eq!(mission.range, DesignRange::ChartedPoint(point.range_nmi));
            let (payload_kg, _) = config.design_payload_kg();
            if let Some(charted_kg) = point.payload_kg {
                assert_eq!(payload_kg, charted_kg, "{name}");
                assert!(
                    charted_kg <= 1.05 * config.requirements.max_structural_payload_kg,
                    "{name}: {charted_kg} kg"
                );
                // A cargo aircraft (the A400M) takes its configured payload,
                // which the preset declares equal to the charted one.
                let expected_source = if config.requirements.aircraft_type == "cargo" {
                    DesignPayloadSource::ConfiguredPayload
                } else {
                    DesignPayloadSource::ChartedPoint
                };
                assert_eq!(mission.payload_source, expected_source);
            }
            let adjusted = with_mode(config, MtowSizing::PayloadAdjusted).mtow_plan();
            assert_eq!(
                adjusted.design_mission.map(|mission| mission.range),
                Some(DesignRange::ChartedPoint(point.range_nmi)),
                "{name}"
            );
        }
    }

    /// The FLOPS `DESRNG` is a mass-equation input: it stays what it was
    /// declared as (A340-300, 7,200 nmi) and does not become the mission
    /// range of a preset that charts 5,000 nmi at maximum payload.
    #[test]
    fn the_flops_design_range_stays_separate_from_the_charted_mission_range() {
        let mut config = AlasConfig::from_value(&json!({"preset": "A340-300"})).unwrap();
        assert_eq!(
            config.mass_model.flops_transport.design_range_nmi,
            Some(7_200.0)
        );
        assert_eq!(
            config.design_mission().range,
            DesignRange::ChartedPoint(5_000.0)
        );
        config.optimizer.objective.design_range_nmi = 4_000.0;
        assert_eq!(
            config.design_mission().range,
            DesignRange::Objective(4_000.0)
        );
    }
}
