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
//! | `MtowBand` | `m` | `m` | `m` | none | route, one pass | `m` |
//! | `PayloadAdjusted` | cap | none | none | none | design range, or route | closure |
//!
//! The design range is the objective's explicit value, else the registered
//! preset's charted payload/range point; `PayloadAdjusted` closes on the route
//! only when neither exists. It flies the configured payload, on the charted
//! point no more than the charted payload.
//!
//! "cap" is `requirements.mtow_kg`; `T` is `mtow_target_kg` (zero means the
//! cap) and `p` is `mtow_band_fraction`. `MtowBand` is `FixedRequirement`
//! with the cap replaced by a mass `m` in `[T (1 - p), T (1 + p)]` that the
//! search chooses (the 17th element of the design vector,
//! [`AlasConfig::at_band_mass`] writes it into `requirements.mtow_kg`), so the
//! two modes share one sizing path and the plan of a band configuration at
//! `m` equals the fixed plan at `m` field for field except [`MtowPlan::mode`],
//! [`MtowPlan::band_edges_kg`] and the objective scale
//! [`MtowPlan::normalisation_kg`] (the target `T`, so objectives stay
//! comparable across `m`). An explicit
//! `flops_structure.design_gross_mass_kg` keeps the structure at that
//! declared weight in every mode.

use crate::design_variables::DesignVector;
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
    /// A payload-defined mission (`reference.design_point` with a payload and
    /// no range): the range is the model's own output at the declared
    /// takeoff mass, the largest range whose full reserve fuel plan fits the
    /// fuel left after the closure's empty mass and the design payload
    /// (capped by the usable tanks less taxi fuel). It is recomputed every
    /// closure pass and reported as the sized design range; no number is
    /// declared.
    AtCertifiedMtow,
}

impl DesignRange {
    /// The declared range, nmi, or `None` when it is the route distance.
    pub const fn declared_nmi(self) -> Option<f64> {
        match self {
            Self::Objective(nmi) | Self::ChartedPoint(nmi) | Self::FlopsDesignRange(nmi) => {
                Some(nmi)
            }
            Self::Route | Self::AtCertifiedMtow => None,
        }
    }

    /// Whether the closure computes the range from the declared takeoff mass.
    pub const fn is_model_defined(self) -> bool {
        matches!(self, Self::AtCertifiedMtow)
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
    /// The configured load case, but no more than the payload charted with
    /// the design range (`payload_kg`, kg): a charted range is a point of
    /// the aircraft's payload/range envelope only up to its charted payload.
    ConfiguredWithinChart,
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
    /// The reserve rule the charted range is quoted under, when the range is
    /// the preset's charted point and its source states one
    /// (`reference.design_point.reserve`).
    pub reserve: Option<crate::DesignPointReserve>,
}

impl DesignMission {
    /// The fuel policy the design mission is flown under: the configured
    /// policy with the charted point's stated reserve terms in place of its
    /// own. The route keeps the configured policy.
    pub fn fuel_policy(&self, configured: &crate::FuelPolicyConfig) -> crate::FuelPolicyConfig {
        self.reserve.map_or_else(
            || configured.clone(),
            |reserve| reserve.applied_to(configured),
        )
    }
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
    /// `(T (1 - p), T (1 + p))`, kg, under the band mode: the interval the
    /// chosen mass is searched in. A report value; no residual reads it.
    pub band_edges_kg: Option<(f64, f64)>,
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
        !self.mode.sizes_at_declared_mass()
    }
}

impl AlasConfig {
    /// The registered preset's design point when it fixes a mission: a
    /// charted range, or a payload with the range left to the model.
    ///
    /// The point is read from the registered variant's own payload/range
    /// chart. A file that turns the preset into another aircraft type (a
    /// freighter flown on its passenger donor's preset) describes an aircraft
    /// that chart was never drawn for and inherits none, the rule
    /// [`crate::presets::published_mzfw_kg`] applies to the MZFW: its design
    /// range is then the objective's or the declared FLOPS one. The DC-10-30F
    /// otherwise closed its 73.9 t cargo on the passenger chart's 4,120 nmi
    /// corner (read at 46.0 t) instead of its declared 3,300 nmi.
    fn declared_design_point(&self) -> Option<crate::PayloadRangeDesignPoint> {
        crate::presets::get(&self.preset)
            .ok()
            .filter(|preset| self.requirements.aircraft_type == preset.requirements.aircraft_type)
            .and_then(|preset| preset.reference.design_point)
            .filter(|point| {
                (point.range_nmi.is_finite() && point.range_nmi > 0.0) || point.is_payload_defined()
            })
    }

    /// The design range the preset's own design point declares: its charted
    /// range, or the model's output at the certified mass for a
    /// payload-defined point.
    fn design_point_range(&self) -> Option<DesignRange> {
        self.declared_design_point().map(|point| {
            if point.is_payload_defined() {
                DesignRange::AtCertifiedMtow
            } else {
                DesignRange::ChartedPoint(point.range_nmi)
            }
        })
    }

    /// The design range when the objective declares none: the preset's
    /// charted range, otherwise the declared FLOPS range, otherwise the route.
    fn design_range_default(&self) -> DesignRange {
        if let Some(range) = self.design_point_range() {
            return range;
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
            reserve: self.charted_reserve(range),
        }
    }

    /// The charted point's stated reserve rule when `range` is that point's
    /// range: a range declared any other way carries no quoted rule.
    fn charted_reserve(&self, range: DesignRange) -> Option<crate::DesignPointReserve> {
        match range {
            DesignRange::ChartedPoint(_) | DesignRange::AtCertifiedMtow => {
                self.declared_design_point().and_then(|point| point.reserve)
            }
            _ => None,
        }
    }

    /// The fuel policy the closure's design mission is flown under (see
    /// [`DesignMission::fuel_policy`]), or the configured policy when the
    /// mode flies no design mission.
    pub fn design_mission_fuel_policy(&self) -> crate::FuelPolicyConfig {
        self.mtow_plan().design_mission.map_or_else(
            || self.fuel_policy.clone(),
            |mission| mission.fuel_policy(&self.fuel_policy),
        )
    }

    /// Design payload, kg, and its source (see [`Self::design_mission`]).
    ///
    /// A charted point's payload comes first, for a freighter too: the design
    /// mission flies the published point (the C295's 2,940 kg normal-weights
    /// point, Jane's 2008-09 p.546), while the configured cargo payload stays
    /// the analysed load case. A freighter without a charted payload flies
    /// its configured cargo payload.
    pub fn design_payload_kg(&self) -> (f64, DesignPayloadSource) {
        let requirements = &self.requirements;
        if let Some(payload_kg) = self
            .declared_design_point()
            .and_then(|point| point.payload_kg)
            .filter(|kg| kg.is_finite() && *kg > 0.0)
        {
            return (payload_kg, DesignPayloadSource::ChartedPoint);
        }
        if requirements.aircraft_type == "cargo" {
            return (
                requirements.cargo_payload_kg.max(0.0),
                DesignPayloadSource::ConfiguredPayload,
            );
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

    /// The band centre `T`, kg: `mtow_target_kg`, or the requirement MTOW
    /// when no target is set.
    pub fn mtow_band_target_kg(&self) -> f64 {
        let target_kg = self.optimizer.objective.mtow_target_kg;
        if target_kg.is_finite() && target_kg > 0.0 {
            target_kg
        } else {
            self.requirements.mtow_kg
        }
    }

    /// The interval `[T (1 - p), T (1 + p)]`, kg, the band mode searches the
    /// takeoff mass in.
    pub fn mtow_band_limits_kg(&self) -> (f64, f64) {
        let target_kg = self.mtow_band_target_kg();
        let p = self.optimizer.objective.mtow_band_fraction;
        (target_kg * (1.0 - p), target_kg * (1.0 + p))
    }

    /// `design` carrying the band mass the configuration's mode needs: under
    /// `MtowBand` the target `T` when it has none, so a vector built without
    /// the 17th element starts the search at the seed; under every other
    /// mode the element is dropped, so the vector matches the sixteen
    /// geometric bounds.
    pub fn band_design(&self, mut design: DesignVector) -> DesignVector {
        if self.optimizer.objective.mtow_sizing == MtowSizing::MtowBand {
            design.mtow_kg = design.mtow_kg.or(Some(self.mtow_band_target_kg()));
        } else {
            design.mtow_kg = None;
        }
        design
    }

    /// This configuration evaluated at the band mass `chosen_kg`: under
    /// `MtowBand`, `requirements.mtow_kg` becomes the chosen mass `m` (the
    /// target `T` is pinned first, so resolving is idempotent), which makes
    /// the configuration the fixed-requirement problem at `m`. `None` means
    /// the configuration's current mass when it lies inside the band (so a
    /// resolved configuration stays resolved), else the target `T`, the seed of
    /// the search. Any other mode is returned unchanged.
    ///
    /// A non-finite or non-positive `chosen_kg` is treated as `None`; a mass
    /// outside the band is kept, and rejected by the evaluator boundary
    /// (`validate_design_space`) rather than silently clamped.
    pub fn at_band_mass(&self, chosen_kg: Option<f64>) -> Self {
        if self.optimizer.objective.mtow_sizing != MtowSizing::MtowBand {
            return self.clone();
        }
        let target_kg = self.mtow_band_target_kg();
        let (lower_kg, upper_kg) = self.mtow_band_limits_kg();
        let current_kg = self.requirements.mtow_kg;
        // No chosen mass keeps a mass already inside the band (a resolved
        // configuration, as the reference aircraft's nominal is evaluated
        // under), else the target.
        let default_kg = if current_kg >= lower_kg - 1.0e-9 * upper_kg
            && current_kg <= upper_kg + 1.0e-9 * upper_kg
        {
            current_kg
        } else {
            target_kg
        };
        let mass_kg = chosen_kg
            .filter(|kg| kg.is_finite() && *kg > 0.0)
            .unwrap_or(default_kg);
        let mut config = self.clone();
        config.optimizer.objective.mtow_target_kg = target_kg;
        config.requirements.mtow_kg = mass_kg;
        config
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
            band_edges_kg: None,
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
            // The band is the fixed plan at the chosen mass `m`, which is
            // `requirements.mtow_kg` once the candidate is resolved. The
            // objective scale is the target, so a lighter `m` is not
            // rescaled into looking equal.
            MtowSizing::MtowBand => MtowPlan {
                iterates: false,
                structural_basis: StructuralBasis::DeclaredCap,
                band_edges_kg: Some((target_kg * (1.0 - p), target_kg * (1.0 + p))),
                normalisation_kg: target_kg,
                ..base
            },
            MtowSizing::SizedByMission => base,
            MtowSizing::Unconstrained => MtowPlan {
                dispatch_ceiling_kg: UNBOUNDED_DISPATCH_MTOW_KG,
                aitken_ceiling_kg: f64::INFINITY,
                upper_bound_kg: None,
                ..base
            },
            MtowSizing::PayloadAdjusted => {
                let explicit_range = objective.design_range_nmi;
                let range = if explicit_range.is_finite() && explicit_range > 0.0 {
                    Some(DesignRange::Objective(explicit_range))
                } else {
                    self.design_point_range()
                };
                // The configured payload is flown, but on the charted range no
                // more than the payload its chart pairs with it: above that
                // payload the charted range is not a point of the aircraft's
                // own payload/range envelope (LJ60: 2,493 nmi charted with
                // four passengers, configured with eight and their baggage).
                // A lighter configured load stays inside the envelope and is
                // flown as configured, so the mode keeps its meaning of
                // closing the takeoff mass on the user's payload.
                let mission = |range: DesignRange| {
                    let charted_kg = match (range, self.design_payload_kg()) {
                        (
                            DesignRange::ChartedPoint(_) | DesignRange::AtCertifiedMtow,
                            (kg, DesignPayloadSource::ChartedPoint),
                        ) => Some(kg),
                        _ => None,
                    };
                    match charted_kg {
                        Some(kg) => DesignMission {
                            range,
                            payload_kg: Some(kg),
                            payload_source: DesignPayloadSource::ConfiguredWithinChart,
                            reserve: self.charted_reserve(range),
                        },
                        None => DesignMission {
                            range,
                            payload_kg: None,
                            payload_source: DesignPayloadSource::ConfiguredPayload,
                            reserve: self.charted_reserve(range),
                        },
                    }
                };
                MtowPlan {
                    dispatch_ceiling_kg: UNBOUNDED_DISPATCH_MTOW_KG,
                    aitken_ceiling_kg: f64::INFINITY,
                    upper_bound_kg: None,
                    design_mission: range.map(mission),
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

        // The band is the fixed plan at the chosen mass: resolved at `m` it
        // equals the fixed plan at `m` except for the mode, the band edges
        // and the objective scale (the target).
        let band_config = with_mode(base.clone(), MtowSizing::MtowBand);
        let chosen_kg = 95_000.0;
        let band = band_config.at_band_mass(Some(chosen_kg)).mtow_plan();
        let mut fixed_config = with_mode(base.clone(), MtowSizing::FixedRequirement);
        fixed_config.requirements.mtow_kg = chosen_kg;
        let fixed_at_m = fixed_config.mtow_plan();
        assert!(!band.requires_mission_sized_evaluation() && !band.checks_offdesign());
        assert!(!band.iterates && !band.closes_mass() && !band.analyses_at_closure());
        assert_eq!(band.seed_kg, chosen_kg);
        assert_eq!(band.upper_bound_kg, Some(chosen_kg));
        assert_eq!(band.lower_bound_kg, None);
        assert_eq!(band.design_mission, None);
        assert_eq!(band.structural_basis, StructuralBasis::DeclaredCap);
        assert_eq!(band.normalisation_kg, 90_000.0);
        let (lower_kg, upper_kg) = band.band_edges_kg.unwrap();
        assert!((lower_kg - 81_000.0).abs() < 1e-9 && (upper_kg - 99_000.0).abs() < 1e-9);
        assert_eq!(
            MtowPlan {
                mode: fixed_at_m.mode,
                band_edges_kg: None,
                normalisation_kg: fixed_at_m.normalisation_kg,
                target_kg: fixed_at_m.target_kg,
                ..band
            },
            fixed_at_m
        );

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
    fn resolving_a_band_mass_is_idempotent_and_leaves_other_modes_alone() {
        let mut band = with_mode(AlasConfig::default(), MtowSizing::MtowBand);
        band.requirements.mtow_kg = 80_000.0;
        assert_eq!(band.optimizer.objective.mtow_band_fraction, 0.10);
        let (lower_kg, upper_kg) = band.mtow_band_limits_kg();
        assert!((lower_kg - 72_000.0).abs() < 1e-9 && (upper_kg - 88_000.0).abs() < 1e-9);
        // No chosen mass means the target, the seed of the search.
        assert_eq!(band.at_band_mass(None).requirements.mtow_kg, 80_000.0);
        let resolved = band.at_band_mass(Some(75_000.0));
        assert_eq!(resolved.requirements.mtow_kg, 75_000.0);
        assert_eq!(resolved.optimizer.objective.mtow_target_kg, 80_000.0);
        assert_eq!(resolved.mtow_band_limits_kg(), band.mtow_band_limits_kg());
        assert_eq!(resolved.at_band_mass(Some(75_000.0)), resolved);
        // A non-finite or non-positive mass is the default, never a silent zero:
        // the current mass of the configuration while it is in the band.
        for bad in [f64::NAN, 0.0, -1.0, f64::INFINITY] {
            assert_eq!(band.at_band_mass(Some(bad)).requirements.mtow_kg, 80_000.0);
            assert_eq!(
                resolved.at_band_mass(Some(bad)).requirements.mtow_kg,
                75_000.0
            );
        }
        assert_eq!(resolved.at_band_mass(None), resolved);
        // A current mass outside the band falls back to the target.
        let mut outside = band.clone();
        outside.requirements.mtow_kg = 200_000.0;
        outside.optimizer.objective.mtow_target_kg = 80_000.0;
        assert_eq!(outside.at_band_mass(None).requirements.mtow_kg, 80_000.0);
        let fixed = with_mode(AlasConfig::default(), MtowSizing::FixedRequirement);
        assert_eq!(fixed.at_band_mass(Some(1.0)), fixed);
        // The vector carries the mass only in the band mode.
        let design = DesignVector::default();
        assert_eq!(band.band_design(design).mtow_kg, Some(80_000.0));
        assert_eq!(fixed.band_design(band.band_design(design)).mtow_kg, None);
        assert_eq!(band.band_design(design).to_array().len(), 17);
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
        let config = AlasConfig::from_value(&json!({"preset": "AVE"})).unwrap();
        let (payload_kg, source) = config.design_payload_kg();
        assert_eq!(source, DesignPayloadSource::PlanningSeats(350));
        let expected = 350.0 * config.requirements.passenger_mass_kg
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
            assert!(
                (point.range_nmi > 0.0 || point.is_payload_defined()) && !point.source.is_empty(),
                "{name}"
            );
            let expected_range = if point.is_payload_defined() {
                DesignRange::AtCertifiedMtow
            } else {
                DesignRange::ChartedPoint(point.range_nmi)
            };
            let config = AlasConfig::from_value(&json!({ "preset": name })).unwrap();
            let mission = config.design_mission();
            assert_eq!(mission.range, expected_range, "{name}");
            let (payload_kg, _) = config.design_payload_kg();
            if let Some(charted_kg) = point.payload_kg {
                assert_eq!(payload_kg, charted_kg, "{name}");
                assert!(
                    charted_kg <= 1.05 * config.requirements.max_structural_payload_kg,
                    "{name}: {charted_kg} kg"
                );
                // A charted payload is the design payload for a freighter
                // too (the A400M and C295); the configured cargo payload is
                // the analysed load case.
                assert_eq!(
                    mission.payload_source,
                    DesignPayloadSource::ChartedPoint,
                    "{name}"
                );
            }
            let adjusted = with_mode(config, MtowSizing::PayloadAdjusted).mtow_plan();
            assert_eq!(
                adjusted.design_mission.map(|mission| mission.range),
                Some(expected_range),
                "{name}"
            );
            // The charted range carries the configured payload, limited to
            // the charted payload when the chart pairs one with it.
            let adjusted_mission = adjusted.design_mission.unwrap();
            if mission.payload_source == DesignPayloadSource::ChartedPoint {
                assert_eq!(adjusted_mission.payload_kg, Some(payload_kg), "{name}");
                assert_eq!(
                    adjusted_mission.payload_source,
                    DesignPayloadSource::ConfiguredWithinChart,
                    "{name}"
                );
            } else {
                assert_eq!(adjusted_mission.payload_kg, None, "{name}");
                assert_eq!(
                    adjusted_mission.payload_source,
                    DesignPayloadSource::ConfiguredPayload,
                    "{name}"
                );
            }
        }
    }

    /// The C295 design mission is payload-defined: the payload is the
    /// normal-weights range point of Jane's 2008-09 p.546 (2,940 kg, 45 min
    /// hold), flown at the TCDS-certified MTOW (EASA.A.186 Issue 08, item 13,
    /// printed p.79), and no range is declared: the closure computes it. The
    /// analysed load case stays the 6,000 kg cargo requirement.
    #[test]
    fn the_c295_design_mission_is_payload_defined_at_the_certified_mtow() {
        let config = AlasConfig::from_value(&json!({ "preset": "C295" })).unwrap();
        assert_eq!(config.requirements.mtow_kg, 21_000.0);
        assert_eq!(config.requirements.cargo_payload_kg, 6_000.0);
        assert_eq!(config.requirements.max_structural_payload_kg, 7_050.0);
        let mission = config.design_mission();
        assert_eq!(mission.range, DesignRange::AtCertifiedMtow);
        assert_eq!(mission.range.declared_nmi(), None);
        assert!(mission.range.is_model_defined());
        assert_eq!(config.design_payload_kg().0, 2_940.0);
        let adjusted = with_mode(config, MtowSizing::PayloadAdjusted).mtow_plan();
        assert_eq!(adjusted.declared_mtow_kg, 21_000.0);
        assert_eq!(
            adjusted.design_mission.map(|mission| mission.range),
            Some(DesignRange::AtCertifiedMtow)
        );
        // An explicit objective range still overrides the preset.
        let mut ranged = AlasConfig::from_value(&json!({ "preset": "C295" })).unwrap();
        ranged.optimizer.objective.design_range_nmi = 1_000.0;
        assert_eq!(
            ranged.design_mission().range,
            DesignRange::Objective(1_000.0)
        );
    }

    /// A charted point quoted under a stated reserve rule flies that rule on
    /// the design mission; the configured policy is untouched (the route
    /// flies it), and a range declared by the objective carries no quoted
    /// rule.
    #[test]
    fn the_design_mission_flies_the_reserve_its_charted_point_is_quoted_under() {
        let adjusted = |name: &str| {
            with_mode(
                AlasConfig::from_value(&json!({ "preset": name })).unwrap(),
                MtowSizing::PayloadAdjusted,
            )
        };
        // MIL-C-5011B: 5 % trip fuel and a 30 min hold at 1,500 ft, no alternate.
        let a400m = adjusted("A400M");
        let policy = a400m.design_mission_fuel_policy();
        assert_eq!(policy.scheme, crate::FuelScheme::StudyConvention);
        assert_eq!(policy.contingency_trip_fraction, 0.05);
        assert_eq!(policy.contingency_minimum_hold_min, 0.0);
        assert_eq!(policy.alternate_distance_nmi, 0.0);
        assert_eq!(policy.final_reserve_hold_min, 30.0);
        assert_eq!(policy.holding_altitude_ft, 1_500.0);
        assert_eq!(a400m.fuel_policy, crate::FuelPolicyConfig::default());
        // Jane's 2008-09 p.546 (C-295M): 45 min hold at 460 m and nothing else,
        // read as given: no EASA 15 min no-alternate hold on the zero alternate.
        let c295 = adjusted("C295");
        let policy = c295.design_mission_fuel_policy();
        assert_eq!(policy.scheme, crate::FuelScheme::StudyConvention);
        assert_eq!(policy.contingency_trip_fraction, 0.0);
        assert_eq!(policy.alternate_distance_nmi, 0.0);
        assert_eq!(policy.final_reserve_hold_min, 45.0);
        // Embraer APM Figure 3.3: 100 nm alternate + 45 min, no contingency;
        // the holding height and scheme are configured.
        let e195 = adjusted("E195-E2");
        let policy = e195.design_mission_fuel_policy();
        assert_eq!(policy.alternate_distance_nmi, 100.0);
        assert_eq!(policy.final_reserve_hold_min, 45.0);
        assert_eq!(policy.contingency_trip_fraction, 0.0);
        assert_eq!(
            crate::FuelPolicyConfig {
                alternate_distance_nmi: e195.fuel_policy.alternate_distance_nmi,
                final_reserve_hold_min: e195.fuel_policy.final_reserve_hold_min,
                contingency_trip_fraction: e195.fuel_policy.contingency_trip_fraction,
                contingency_minimum_hold_min: e195.fuel_policy.contingency_minimum_hold_min,
                ..policy
            },
            e195.fuel_policy
        );
        // FAR 121.645 at a 200 nmi alternate.
        for name in ["DC-10", "B747-400"] {
            let policy = adjusted(name).design_mission_fuel_policy();
            assert_eq!(
                policy.scheme,
                crate::FuelScheme::FaaFlagSupplemental,
                "{name}"
            );
            assert_eq!(policy.alternate_distance_nmi, 200.0, "{name}");
            assert_eq!(policy.flag_flight_time_fraction, 0.10, "{name}");
        }
        // A point with no stated rule, and a range the objective declares, fly
        // the configured policy.
        let a320 = adjusted("A320-200");
        assert_eq!(a320.design_mission_fuel_policy(), a320.fuel_policy);
        let mut declared = adjusted("A400M");
        declared.optimizer.objective.design_range_nmi = 2_000.0;
        assert_eq!(declared.design_mission_fuel_policy(), declared.fuel_policy);
        // Every stated rule names its source and validates.
        for name in crate::presets::available() {
            let preset = crate::presets::get(name).unwrap();
            if let Some(reserve) = preset
                .reference
                .design_point
                .and_then(|point| point.reserve)
            {
                assert!(!reserve.source.is_empty(), "{name}");
                let policy = reserve.applied_to(&crate::FuelPolicyConfig::default());
                assert!(policy.validate().is_ok(), "{name}");
            }
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
