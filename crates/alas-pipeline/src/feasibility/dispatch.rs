// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The reserve plan the flown mission was sized to, and its findings.
//!
//! The mission stage chooses a takeoff mass from the fuel policy and flies
//! it. This module carries that choice into the feasibility report: the
//! analyzed load case becomes the flown one, the plan is retained quantity
//! by quantity, and a route whose policy fuel does not fit under the
//! takeoff-mass limit or in the tanks is a finding, because the flight that
//! was flown carried less than the rule requires.

use alas_mass::dispatch::DispatchStatus;
use alas_mass::fuel_plan::FuelPlan;
use alas_mission::MissionResult;

use crate::mission_stage::dispatch::field_climb::{first_climb_refusal, FirstClimbRefusal};
use crate::mission_stage::dispatch::{DesignMissionCase, NativeTripCheck, PolicyClosureCase};
use crate::mission_stage::plan::{PlanClosure, TrimDragStatus};
use crate::mission_stage::{LoadCaseSelection, SelectedLoadCase};

use super::declared_level::declared_level_finding;
use super::{
    error, warning, CarriedFuelBasis, FindingCode, FuelLoadingAssessment, PhysicalFinding,
};

/// How the policy closure ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchOutcome {
    /// The route was flown at the mass the policy requires.
    Converged,
    /// The policy fuel exceeds what the takeoff-mass limit admits.
    MtowLimited,
    /// The policy fuel exceeds what the usable tanks admit.
    TankLimited,
    /// The dispatch closure did not converge within its budget.
    NotConverged,
    /// The frozen maximum-available-fuel case was flown by choice.
    MaximumAvailableFuel,
    /// The policy could not be priced, so the maximum-available-fuel case was flown.
    Unavailable,
}

impl DispatchOutcome {
    /// Stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Converged => "converged",
            Self::MtowLimited => "mtow_limited",
            Self::TankLimited => "tank_limited",
            Self::NotConverged => "not_converged",
            Self::MaximumAvailableFuel => "maximum_available_fuel",
            Self::Unavailable => "unavailable",
        }
    }
}

/// The fuel policy as applied to the flown mission.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DispatchAssessment {
    /// How the closure ended.
    pub outcome: DispatchOutcome,
    /// The plan at the flown takeoff mass, when the policy was priced.
    pub plan: Option<FuelPlan>,
    /// Mass at brake release, kg.
    pub takeoff_mass_kg: f64,
    /// Fuel the route requires beyond what the limits admit, kg.
    pub shortfall_kg: f64,
    /// Still-air distance of the route the plan was priced over, m, when the
    /// policy was priced.
    pub route_distance_m: Option<f64>,
    /// Loadable takeoff fuel less the route's trip plus reserves, kg, when
    /// the policy was priced; negative when the route cannot carry them.
    pub reserve_margin_kg: Option<f64>,
    /// The design mission a sized aircraft was closed on, reported beside
    /// the route it flies off-design.
    pub design_mission: Option<DesignMissionCase>,
    /// The native mission's trip at the flown mass (telemetry), when it was flown.
    pub native_check: Option<NativeTripCheck>,
    /// Operating empty plus payload of the route's dispatch, kg, when the
    /// policy was priced: the report's zero-fuel mass less
    /// [`Self::offloaded_payload_kg`].
    pub zero_fuel_mass_kg: Option<f64>,
    /// Payload of the report the flown route leaves behind, kg: zero unless
    /// the route is landing-weight limited
    /// (`mission_stage::dispatch::landing_limit`).
    pub offloaded_payload_kg: f64,
}

impl DispatchAssessment {
    /// The exported record: the route's plan quantity by quantity, its
    /// reserve margin, and the design mission beside it.
    pub fn to_json(&self) -> serde_json::Value {
        let plan = self.plan.map(|plan| {
            let quantity = |quantity: alas_mass::fuel_plan::FuelQuantity| {
                serde_json::json!({ "kg": quantity.kg, "rule": format!("{:?}", quantity.rule) })
            };
            serde_json::json!({
                "scheme": plan.scheme.as_str(),
                "taxi": quantity(plan.taxi),
                "trip": quantity(plan.trip),
                "contingency": quantity(plan.contingency),
                "alternate": quantity(plan.alternate),
                "final_reserve": quantity(plan.final_reserve),
                "additional": quantity(plan.additional),
                "extra": quantity(plan.extra),
                "takeoff_fuel_kg": plan.takeoff_fuel_kg(),
                "ramp_fuel_kg": plan.ramp_fuel_kg(),
                "block_fuel_kg": plan.block_fuel_kg(),
                "trip_time_s": plan.trip_time_s,
                "destination_landing_mass_kg": plan.destination_landing_mass_kg,
                "reserve_landing_mass_kg": plan.reserve_landing_mass_kg,
            })
        });
        let design_mission = self.design_mission.map(|design| {
            serde_json::json!({
                "range_m": design.range_m,
                "takeoff_mass_kg": design.takeoff_mass_kg,
                "takeoff_fuel_kg": design.takeoff_fuel_kg,
                "trip_fuel_kg": design.trip_fuel_kg,
            })
        });
        serde_json::json!({
            "outcome": self.outcome.as_str(),
            "takeoff_mass_kg": self.takeoff_mass_kg,
            "shortfall_kg": self.shortfall_kg,
            "route_distance_m": self.route_distance_m,
            "reserve_margin_kg": self.reserve_margin_kg,
            "design_mission": design_mission,
            "native_trip_fuel_kg": self.native_check.map(|check| check.native_trip_fuel_kg),
            "zero_fuel_mass_kg": self.zero_fuel_mass_kg,
            "offloaded_payload_kg": self.offloaded_payload_kg,
            "plan": plan,
        })
    }
}

/// Rewrite the analyzed load case to the flown one and record its findings.
///
/// Without a load case (a caller that did not fly the mission) the
/// takeoff-mass-closure case stands, exactly as before the policy existed.
/// `mission` is the flight flown at the load case, when there is one.
pub(super) fn apply_load_case(
    fuel_loading: &mut FuelLoadingAssessment,
    load_case: Option<&SelectedLoadCase>,
    mission: Option<&MissionResult>,
    findings: &mut Vec<PhysicalFinding>,
) {
    let Some(load_case) = load_case else {
        return;
    };
    let takeoff_mass_kg = load_case.takeoff_mass_kg;
    let first_climb = mission.and_then(first_climb_refusal);
    if let Some(refusal) = &first_climb {
        findings.push(first_climb_finding(load_case, refusal));
    }
    match &load_case.selection {
        LoadCaseSelection::MaximumAvailableFuel {
            reason: Some(reason),
            ..
        } => {
            // `reason` is only `Some` when the segment mission model could
            // not be built or failed to solve the route (see
            // `select_load_case` in `mission_stage::dispatch`). The route was
            // then never priced against its fuel policy, so it cannot be
            // certified feasible: it is an analysis failure, not evidence
            // either way about physical feasibility.
            findings.push(error(
                FindingCode::FuelPolicyUnavailable,
                format!(
                    "the fuel-policy dispatch analysis could not be completed for this mission (not evidence of physical infeasibility), so the maximum-available-fuel case was flown instead: {reason}"
                ),
                None,
                None,
                "",
            ));
            fuel_loading.dispatch = Some(DispatchAssessment {
                outcome: DispatchOutcome::Unavailable,
                plan: None,
                takeoff_mass_kg,
                shortfall_kg: 0.0,
                route_distance_m: None,
                reserve_margin_kg: None,
                design_mission: None,
                native_check: None,
                zero_fuel_mass_kg: None,
                offloaded_payload_kg: 0.0,
            });
        }
        LoadCaseSelection::MaximumAvailableFuel {
            reason: None,
            route_check,
        } => {
            // A deliberate maximum-available-fuel flight: the load stands,
            // and the route's policy case priced beside it still decides
            // whether trip plus reserves fit the loadable fuel.
            fuel_loading.dispatch = Some(match route_check {
                Some(case) => route_assessment(
                    case,
                    DispatchOutcome::MaximumAvailableFuel,
                    takeoff_mass_kg,
                    findings,
                ),
                None => DispatchAssessment {
                    outcome: DispatchOutcome::MaximumAvailableFuel,
                    plan: None,
                    takeoff_mass_kg,
                    shortfall_kg: 0.0,
                    route_distance_m: None,
                    reserve_margin_kg: None,
                    design_mission: None,
                    native_check: None,
                    zero_fuel_mass_kg: None,
                    offloaded_payload_kg: 0.0,
                },
            });
        }
        LoadCaseSelection::Plan(case) => {
            // A user-defined plan is flown at the fuel it closed on; there is no
            // route fuel plan to price beside it. The analyzed state is the
            // aircraft at brake release, the mass the flight starts at; the
            // ramp fuel the tanks were loaded with is the plan's own.
            fuel_loading.analyzed_takeoff_mass_kg = load_case.takeoff_mass_kg;
            fuel_loading.analyzed_carried_fuel_kg = case.brake_release_fuel_kg();
            fuel_loading.carried_fuel_basis = CarriedFuelBasis::MissionPlanClosure;
            if case.shortfall_kg > 0.0 {
                findings.push(error(
                    FindingCode::ReserveFuelShortfall,
                    format!(
                        "the mission plan needs {:.1} kg of ramp fuel, {:.1} kg more than the usable tank capacity or the takeoff-mass limit admits",
                        case.ramp_fuel_kg, case.shortfall_kg
                    ),
                    Some(case.ramp_fuel_kg),
                    Some(case.ramp_fuel_kg - case.shortfall_kg),
                    "kg",
                ));
            }
            match &case.trim_drag.status {
                TrimDragStatus::CannotTrim(message) => findings.push(error(
                    FindingCode::TrimUnavailable,
                    format!("mission plan: {message}"),
                    None,
                    None,
                    "",
                )),
                TrimDragStatus::ExternalPolar => findings.push(warning(
                    FindingCode::TrimUnavailable,
                    "mission plan: the aircraft's drag is an external solver's polar, so the trim drag of the centre-of-gravity movement of the plan is not followed",
                    None,
                    None,
                    "",
                )),
                _ => {}
            }
            if case.trim_drag.unsettled {
                findings.push(warning(
                    FindingCode::MissionPlanTrimUnsettled,
                    "mission plan: the trim drag of the plan's centre of gravity had not settled after the last pass (a leg's increment still changed by a fifth of a drag count or more); the fuel is closed with the trim drag of the previous pass",
                    None,
                    None,
                    "",
                ));
            }
            // A flight that stopped did not fail to close: the finding of its stop
            // says why it ended, and no fuel would have changed it. A stop on the
            // first climb is the exception: the fuel loaded set the mass it could
            // not climb at, so the closure ended on a load it had not settled.
            if case.closure == PlanClosure::NotClosed
                && (load_case.native_error.is_none() || first_climb.is_some())
            {
                let ended = match &first_climb {
                    Some(refusal) => format!(
                        "ended unsettled after {} flights, the last stopped on its first leg, {}, at the mass the fuel loaded",
                        case.iterations, refusal.tag
                    ),
                    None => format!("did not settle in {} flights", case.iterations),
                };
                findings.push(error(
                    FindingCode::DispatchNotConverged,
                    format!(
                        "the mission plan's fuel closure {ended}; the flight reported is the last one, flown with {:.1} kg of ramp fuel",
                        case.ramp_fuel_kg
                    ),
                    Some(case.ramp_fuel_kg),
                    None,
                    "kg",
                ));
            }
        }
        LoadCaseSelection::PolicyClosure(case) => {
            let flown_fuel_kg = takeoff_mass_kg - load_case.zero_fuel_mass_kg;
            // A landing-weight-limited route flies less traffic load than the
            // report carries (`mission_stage::dispatch::landing_limit`).
            let offloaded_kg = case.offloaded_payload_kg;
            if offloaded_kg > 0.0 {
                findings.push(warning(
                    FindingCode::LandingMassLimitViolation,
                    format!(
                        "the route is landing-weight limited: the analysed load would land above the maximum landing mass, so the dispatch offloads {offloaded_kg:.1} kg of payload and flies a zero-fuel mass of {:.1} kg (allowed takeoff mass = min(MTOW, MZFW + takeoff fuel, MLW + trip fuel))",
                        load_case.zero_fuel_mass_kg
                    ),
                    Some(load_case.zero_fuel_mass_kg + offloaded_kg),
                    Some(load_case.zero_fuel_mass_kg),
                    "kg",
                ));
            }
            if fuel_loading.design_takeoff_loading.is_some() {
                // Hard MTOW: the analyzed state stays the sized design
                // loading, one closure with the report's sized takeoff mass;
                // the route is flown at its own dispatch mass beside it.
                fuel_loading.flown_carried_fuel_kg = Some(flown_fuel_kg);
            } else {
                fuel_loading.analyzed_takeoff_mass_kg = takeoff_mass_kg;
                fuel_loading.analyzed_carried_fuel_kg = flown_fuel_kg;
                fuel_loading.carried_fuel_basis = CarriedFuelBasis::ReservePolicyClosure;
            }
            let outcome = if case.shortfall_kg > 0.0 {
                match case.analytic.status {
                    DispatchStatus::TankLimited { .. } => DispatchOutcome::TankLimited,
                    _ => DispatchOutcome::MtowLimited,
                }
            } else if case.converged {
                DispatchOutcome::Converged
            } else {
                DispatchOutcome::NotConverged
            };
            fuel_loading.dispatch =
                Some(route_assessment(case, outcome, takeoff_mass_kg, findings));
        }
    }
}

/// The finding of a flight that stopped on its first leg, a climb it could not
/// fly from where it starts. A plan's flight is the mission, so it is an
/// error; the standard mission's native flight is telemetry that never gates
/// (the route is judged on the frozen plan), so there it is a warning.
fn first_climb_finding(
    load_case: &SelectedLoadCase,
    refusal: &FirstClimbRefusal,
) -> PhysicalFinding {
    let plan = matches!(load_case.selection, LoadCaseSelection::Plan(_));
    let flight = if plan {
        "the mission plan's flight"
    } else {
        "the native flight"
    };
    let message = format!("{flight}: {}", refusal.message);
    let actual = refusal.excess_power_m_s;
    let limit = actual.map(|_| refusal.residual_climb_m_s);
    let unit = if actual.is_some() { "m/s" } else { "" };
    let code = FindingCode::MissionFirstClimbRefused;
    if plan {
        error(code, message, actual, limit, unit)
    } else {
        warning(code, message, actual, limit, unit)
    }
}

/// The route's policy case as assessed, with its reserve-shortfall and
/// convergence findings.
fn route_assessment(
    case: &PolicyClosureCase,
    outcome: DispatchOutcome,
    takeoff_mass_kg: f64,
    findings: &mut Vec<PhysicalFinding>,
) -> DispatchAssessment {
    let PolicyClosureCase {
        plan,
        analytic,
        native_check,
        converged,
        route_distance_m,
        reserve_margin_kg,
        shortfall_kg,
        design_mission,
        declared_level,
        offloaded_payload_kg,
    } = case;
    declared_level_finding(declared_level.as_ref(), takeoff_mass_kg, findings);
    if *shortfall_kg > 0.0 {
        let bound = match analytic.status {
            DispatchStatus::TankLimited { .. } => "usable tank capacity",
            _ => "takeoff-mass limit",
        };
        findings.push(error(
            FindingCode::ReserveFuelShortfall,
            format!(
                "the {:.0} nmi route needs {:.1} kg of takeoff fuel (trip plus reserves) under the {} scheme, {:.1} kg more than the {} admits",
                route_distance_m / alas_units::NAUTICAL_MILE,
                plan.takeoff_fuel_kg(),
                plan.scheme.as_str(),
                shortfall_kg,
                bound
            ),
            Some(plan.takeoff_fuel_kg()),
            Some(plan.takeoff_fuel_kg() - shortfall_kg),
            "kg",
        ));
    } else if !converged {
        // A takeoff mass that did not converge in the dispatch closure means
        // the route's dispatch was never closed: an analysis failure, not
        // evidence either way about physical feasibility, so it is
        // `Error`-severity and must fail `is_feasible()`.
        findings.push(error(
            FindingCode::DispatchNotConverged,
            "the fuel-policy takeoff mass did not converge in the dispatch closure (analysis did not converge, not evidence of physical infeasibility); the last, unsettled iterate was used".to_owned(),
            Some(takeoff_mass_kg),
            None,
            "kg",
        ));
    }
    DispatchAssessment {
        outcome,
        plan: Some(*plan),
        takeoff_mass_kg,
        shortfall_kg: *shortfall_kg,
        route_distance_m: Some(*route_distance_m),
        reserve_margin_kg: Some(*reserve_margin_kg),
        design_mission: *design_mission,
        native_check: *native_check,
        zero_fuel_mass_kg: Some(analytic.zero_fuel_mass_kg),
        offloaded_payload_kg: *offloaded_payload_kg,
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::FuelScheme;
    use alas_mass::dispatch::DispatchSolution;
    use alas_mass::fuel_plan::FuelQuantity;

    use crate::feasibility::{FeasibilityReport, FindingSeverity};

    fn zero_plan() -> FuelPlan {
        FuelPlan {
            scheme: FuelScheme::TripFuelOnly,
            taxi: FuelQuantity::NONE,
            trip: FuelQuantity::NONE,
            contingency: FuelQuantity::NONE,
            alternate: FuelQuantity::NONE,
            final_reserve: FuelQuantity::NONE,
            additional: FuelQuantity::NONE,
            extra: FuelQuantity::NONE,
            trip_time_s: 0.0,
            destination_landing_mass_kg: 0.0,
            reserve_landing_mass_kg: 0.0,
        }
    }

    fn zero_analytic() -> DispatchSolution {
        DispatchSolution {
            status: DispatchStatus::Converged,
            zero_fuel_mass_kg: 0.0,
            takeoff_mass_kg: 0.0,
            ramp_mass_kg: 0.0,
            destination_landing_mass_kg: 0.0,
            plan: zero_plan(),
            iterates: Vec::new(),
            landing_mass_exceeds_mlw: false,
            zero_fuel_mass_exceeds_mzfw: false,
            settled_at_discontinuity: false,
            stalled_at_evaluable_edge: false,
        }
    }

    fn policy_closure_case(converged: bool, shortfall_kg: f64) -> SelectedLoadCase {
        SelectedLoadCase {
            takeoff_mass_kg: 100_000.0,
            zero_fuel_mass_kg: 80_000.0,
            native_error: None,
            native_supplement: None,
            flown_level_path_changes: Vec::new(),
            propulsion_validity: Default::default(),
            selection: LoadCaseSelection::PolicyClosure(Box::new(PolicyClosureCase {
                plan: zero_plan(),
                analytic: zero_analytic(),
                native_check: None,
                converged,
                route_distance_m: 1.0e6,
                reserve_margin_kg: -shortfall_kg,
                shortfall_kg,
                design_mission: None,
                declared_level: None,
                offloaded_payload_kg: 0.0,
            })),
        }
    }

    fn maximum_available(reason: Option<&str>) -> SelectedLoadCase {
        SelectedLoadCase {
            takeoff_mass_kg: 100_000.0,
            zero_fuel_mass_kg: 80_000.0,
            native_error: None,
            native_supplement: None,
            flown_level_path_changes: Vec::new(),
            propulsion_validity: Default::default(),
            selection: LoadCaseSelection::MaximumAvailableFuel {
                reason: reason.map(str::to_owned),
                route_check: None,
            },
        }
    }

    fn is_feasible(findings: Vec<PhysicalFinding>) -> bool {
        FeasibilityReport {
            findings,
            ..Default::default()
        }
        .is_feasible()
    }

    /// A fuel policy that could not be priced (the analytic model failed or
    /// the mission would not settle) is an analysis failure and must not be
    /// advertised as a feasible design.
    #[test]
    fn a_fuel_policy_that_could_not_be_priced_is_never_feasible() {
        let mut fuel_loading = FuelLoadingAssessment::default();
        let mut findings = Vec::new();
        let load_case =
            maximum_available(Some("leg did not converge: speed schedule not attained"));
        apply_load_case(&mut fuel_loading, Some(&load_case), None, &mut findings);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, FindingCode::FuelPolicyUnavailable);
        assert_eq!(findings[0].severity, FindingSeverity::Error);
        assert!(
            findings[0].message.contains("not evidence of physical infeasibility"),
            "message must distinguish an unpriced analysis from a physically infeasible mission: {}",
            findings[0].message
        );
        assert_eq!(
            fuel_loading.dispatch.unwrap().outcome,
            DispatchOutcome::Unavailable
        );
        assert!(!is_feasible(findings));
    }

    /// A dispatch closure that settles with no shortfall is unaffected by the
    /// fuel-policy-unavailable fix: no finding is added and the case remains
    /// feasible on that basis.
    #[test]
    fn a_converged_policy_closure_is_unaffected() {
        let mut fuel_loading = FuelLoadingAssessment::default();
        let mut findings = Vec::new();
        let load_case = policy_closure_case(true, 0.0);
        apply_load_case(&mut fuel_loading, Some(&load_case), None, &mut findings);

        assert!(findings.is_empty());
        assert_eq!(
            fuel_loading.dispatch.unwrap().outcome,
            DispatchOutcome::Converged
        );
        assert!(is_feasible(findings));
    }

    /// A deliberate maximum-available-fuel load case (the policy is
    /// disabled; `reason: None`) is a valid, intentional operating mode, not
    /// an analysis failure, and must not be flagged.
    #[test]
    fn an_intentional_maximum_available_fuel_policy_is_unaffected() {
        let mut fuel_loading = FuelLoadingAssessment::default();
        let mut findings = Vec::new();
        let load_case = maximum_available(None);
        apply_load_case(&mut fuel_loading, Some(&load_case), None, &mut findings);

        assert!(findings.is_empty());
        assert_eq!(
            fuel_loading.dispatch.unwrap().outcome,
            DispatchOutcome::MaximumAvailableFuel
        );
        assert!(is_feasible(findings));
    }

    /// A dispatch closure that does not converge (no shortfall,
    /// so this is not `ReserveFuelShortfall`) shares the same defect as an
    /// unpriced fuel policy: the dispatch was never actually closed, so it
    /// must not be advertised feasible either.
    #[test]
    fn a_not_converged_policy_closure_is_never_feasible() {
        let mut fuel_loading = FuelLoadingAssessment::default();
        let mut findings = Vec::new();
        let load_case = policy_closure_case(false, 0.0);
        apply_load_case(&mut fuel_loading, Some(&load_case), None, &mut findings);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, FindingCode::DispatchNotConverged);
        assert_eq!(findings[0].severity, FindingSeverity::Error);
        assert!(
            findings[0].message.contains("not evidence of physical infeasibility"),
            "message must distinguish a non-converged analysis from a physically infeasible mission: {}",
            findings[0].message
        );
        assert_eq!(
            fuel_loading.dispatch.unwrap().outcome,
            DispatchOutcome::NotConverged
        );
        assert!(!is_feasible(findings));
    }

    /// Flying the maximum available fuel by choice does not exempt the route
    /// from its reserve check: trip plus reserves above the loadable fuel is
    /// the same hard finding, and the flown load is left as it was.
    #[test]
    fn a_maximum_available_fuel_flight_still_checks_the_route_reserves() {
        let mut fuel_loading = FuelLoadingAssessment::default();
        let mut findings = Vec::new();
        let LoadCaseSelection::PolicyClosure(case) = policy_closure_case(true, 250.0).selection
        else {
            unreachable!("policy_closure_case builds a policy closure");
        };
        let load_case = SelectedLoadCase {
            takeoff_mass_kg: 100_000.0,
            zero_fuel_mass_kg: 80_000.0,
            native_error: None,
            native_supplement: None,
            flown_level_path_changes: Vec::new(),
            propulsion_validity: Default::default(),
            selection: LoadCaseSelection::MaximumAvailableFuel {
                reason: None,
                route_check: Some(case),
            },
        };
        apply_load_case(&mut fuel_loading, Some(&load_case), None, &mut findings);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, FindingCode::ReserveFuelShortfall);
        let dispatch = fuel_loading.dispatch.unwrap();
        assert_eq!(dispatch.outcome, DispatchOutcome::MaximumAvailableFuel);
        assert_eq!(dispatch.reserve_margin_kg, Some(-250.0));
        assert!(fuel_loading.analyzed_carried_fuel_kg.is_nan());
        assert!(!is_feasible(findings));
    }

    /// A route whose trip plus reserves exceed the loadable fuel is a hard
    /// finding even though the dispatch solve itself settled at the limit.
    #[test]
    fn a_route_beyond_the_loadable_fuel_is_never_feasible() {
        let mut fuel_loading = FuelLoadingAssessment::default();
        let mut findings = Vec::new();
        let load_case = policy_closure_case(true, 250.0);
        apply_load_case(&mut fuel_loading, Some(&load_case), None, &mut findings);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, FindingCode::ReserveFuelShortfall);
        assert_eq!(findings[0].severity, FindingSeverity::Error);
        let dispatch = fuel_loading.dispatch.unwrap();
        assert_eq!(dispatch.outcome, DispatchOutcome::MtowLimited);
        assert_eq!(dispatch.reserve_margin_kg, Some(-250.0));
        assert!(!is_feasible(findings));
    }

    /// A native flight that stopped on its first leg, a climb at the rating below the residual
    /// climb from the field, is said by name with its excess power against the residual climb.
    /// The standard mission's native flight is telemetry, so it is a warning there.
    #[test]
    fn a_flight_stopped_on_its_first_climb_is_reported() {
        use crate::mission_stage::dispatch::field_climb::test_support::{
            climb, climbing_at, flight, solution,
        };
        let first = climb("takeoff@00", Some(100.0), 600.0, true);
        let mission = flight(vec![(climbing_at(&first, 0.2), solution(true))], 5);
        let mut fuel_loading = FuelLoadingAssessment::default();
        let mut findings = Vec::new();
        let load_case = policy_closure_case(true, 0.0);
        apply_load_case(
            &mut fuel_loading,
            Some(&load_case),
            Some(&mission),
            &mut findings,
        );

        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert_eq!(finding.code, FindingCode::MissionFirstClimbRefused);
        assert_eq!(finding.severity, FindingSeverity::Warning);
        assert!(
            finding
                .message
                .starts_with("the native flight: the first leg of the flight, takeoff@00"),
            "{}",
            finding.message
        );
        assert!((finding.actual.unwrap() - 0.2).abs() < 1.0e-9);
        assert_eq!(finding.unit, "m/s");
        assert!(is_feasible(findings));
    }

    /// A mission plan whose fuel closure ended on a flight stopped on its first climb: the stop is
    /// an error (the plan's flight is the mission), and the unsettled closure names the leg that
    /// ended it rather than a closure that ran out of flights.
    #[test]
    fn a_plan_closure_ended_by_a_first_climb_stop_names_it() {
        use crate::mission_stage::dispatch::field_climb::test_support::{
            climb, climbing_at, flight, solution,
        };
        use crate::mission_stage::plan::{PlanLoadCase, TrimDragReport};
        let first = climb("takeoff@00", Some(100.0), 600.0, true);
        let mission = flight(vec![(climbing_at(&first, 0.2), solution(true))], 5);
        let load_case = SelectedLoadCase {
            takeoff_mass_kg: 60_000.0,
            zero_fuel_mass_kg: 50_000.0,
            native_error: Some("the plan was not flown to its end".to_owned()),
            native_supplement: None,
            flown_level_path_changes: Vec::new(),
            propulsion_validity: Default::default(),
            selection: LoadCaseSelection::Plan(Box::new(PlanLoadCase {
                plan: alas_config::MissionPlan::default(),
                ramp_fuel_kg: 10_000.0,
                takeoff_mass_kg: 60_000.0,
                flight_burn_kg: 0.0,
                taxi_fuel_kg: 0.0,
                taxi_before_takeoff_kg: 0.0,
                net_offload_kg: 0.0,
                contingency_kg: 0.0,
                closure: PlanClosure::NotClosed,
                iterations: 3,
                shortfall_kg: 0.0,
                events: Vec::new(),
                declared_items: Vec::new(),
                stage_of_segment: Vec::new(),
                trim_drag: TrimDragReport::default(),
            })),
        };
        let mut fuel_loading = FuelLoadingAssessment::default();
        let mut findings = Vec::new();
        apply_load_case(
            &mut fuel_loading,
            Some(&load_case),
            Some(&mission),
            &mut findings,
        );

        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].code, FindingCode::MissionFirstClimbRefused);
        assert_eq!(findings[0].severity, FindingSeverity::Error);
        assert!(
            findings[0].message.starts_with(
                "the mission plan's flight: the first leg of the flight, takeoff@00, cannot climb"
            ),
            "{}",
            findings[0].message
        );
        assert_eq!(findings[1].code, FindingCode::DispatchNotConverged);
        assert!(
            findings[1].message.contains(
                "ended unsettled after 3 flights, the last stopped on its first leg, takeoff@00"
            ),
            "{}",
            findings[1].message
        );
        assert!(!is_feasible(findings));
    }

    /// A landing-weight-limited route is flown at its offloaded zero-fuel
    /// mass: a warning, and the offload is exported with the dispatch.
    #[test]
    fn a_landing_weight_limited_route_reports_its_offload() {
        let mut load_case = policy_closure_case(true, 0.0);
        let LoadCaseSelection::PolicyClosure(case) = &mut load_case.selection else {
            unreachable!("policy_closure_case builds a policy closure");
        };
        case.analytic.zero_fuel_mass_kg = 80_000.0;
        case.offloaded_payload_kg = 1_364.0;
        let mut fuel_loading = FuelLoadingAssessment {
            zero_fuel_mass_kg: 81_364.0,
            ..Default::default()
        };
        let mut findings = Vec::new();
        apply_load_case(&mut fuel_loading, Some(&load_case), None, &mut findings);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, FindingCode::LandingMassLimitViolation);
        assert_eq!(findings[0].severity, FindingSeverity::Warning);
        assert_eq!(fuel_loading.offloaded_payload_kg(), 1_364.0);
        assert_eq!(fuel_loading.flown_zero_fuel_mass_kg(), 80_000.0);
        let exported = fuel_loading.dispatch.unwrap().to_json();
        assert_eq!(exported["offloaded_payload_kg"].as_f64(), Some(1_364.0));
        assert_eq!(exported["zero_fuel_mass_kg"].as_f64(), Some(80_000.0));
        assert!(is_feasible(findings));
    }

    /// A route whose landing limit no offload can meet (less payload than
    /// excess) keeps its full load: no offload, no warning, and its arrival
    /// above the maximum landing mass stays the hard landing-mass error.
    #[test]
    fn a_landing_limit_no_offload_can_meet_stays_a_hard_error() {
        use crate::mission_stage::dispatch::landing_limit::limit_to_landing_mass;
        use crate::mission_stage::dispatch::landing_limit::tests::{limits, policy, Proportional};
        let model = Proportional;
        let full = alas_mass::dispatch::solve_dispatch_with_initial_guess(
            181_000.0,
            240_000.0,
            5.0e6,
            &policy(),
            &model,
            &limits(None),
            50,
            1.0,
        );
        let mlw_kg = full.destination_landing_mass_kg - 1_500.0;
        // 500 kg of payload against a 1,500 kg excess.
        let flown = limit_to_landing_mass(
            full.clone(),
            full.zero_fuel_mass_kg - 500.0,
            5.0e6,
            &policy(),
            &model,
            &limits(Some(mlw_kg)),
            50,
            1.0,
            0.0,
        );
        let offloaded_payload_kg = full.zero_fuel_mass_kg - flown.zero_fuel_mass_kg;
        assert_eq!(offloaded_payload_kg, 0.0);
        let load_case = SelectedLoadCase {
            takeoff_mass_kg: flown.takeoff_mass_kg,
            zero_fuel_mass_kg: flown.zero_fuel_mass_kg,
            native_error: None,
            native_supplement: None,
            flown_level_path_changes: Vec::new(),
            propulsion_validity: Default::default(),
            selection: LoadCaseSelection::PolicyClosure(Box::new(PolicyClosureCase {
                plan: flown.plan,
                analytic: flown.clone(),
                native_check: None,
                converged: true,
                route_distance_m: 5.0e6,
                reserve_margin_kg: 0.0,
                shortfall_kg: 0.0,
                design_mission: None,
                declared_level: None,
                offloaded_payload_kg,
            })),
        };
        let mut fuel_loading = FuelLoadingAssessment::default();
        let mut findings = Vec::new();
        apply_load_case(&mut fuel_loading, Some(&load_case), None, &mut findings);
        assert!(findings.is_empty(), "{findings:?}");
        assert_eq!(fuel_loading.offloaded_payload_kg(), 0.0);
        // The arrival the report checks (`feasibility.rs`): takeoff less trip.
        let dispatch = fuel_loading.dispatch.unwrap();
        let arrival_kg = dispatch.takeoff_mass_kg - dispatch.plan.unwrap().trip.kg;
        super::super::design_mass::landing_mass_findings(
            None,
            Some(arrival_kg),
            mlw_kg,
            &mut findings,
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, FindingCode::LandingMassLimitViolation);
        assert_eq!(findings[0].severity, FindingSeverity::Error);
        assert!(!is_feasible(findings));
    }
}
