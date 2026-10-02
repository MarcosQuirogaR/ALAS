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

use crate::mission_stage::dispatch::{DesignMissionCase, NativeTripCheck, PolicyClosureCase};
use crate::mission_stage::{LoadCaseSelection, SelectedLoadCase};

use super::{error, CarriedFuelBasis, FindingCode, FuelLoadingAssessment, PhysicalFinding};

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
            "plan": plan,
        })
    }
}

/// Rewrite the analyzed load case to the flown one and record its findings.
///
/// Without a load case (a caller that did not fly the mission) the
/// takeoff-mass-closure case stands, exactly as before the policy existed.
pub(super) fn apply_load_case(
    fuel_loading: &mut FuelLoadingAssessment,
    load_case: Option<&SelectedLoadCase>,
    findings: &mut Vec<PhysicalFinding>,
) {
    let Some(load_case) = load_case else {
        return;
    };
    let takeoff_mass_kg = load_case.takeoff_mass_kg;
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
                },
            });
        }
        LoadCaseSelection::PolicyClosure(case) => {
            fuel_loading.analyzed_takeoff_mass_kg = takeoff_mass_kg;
            fuel_loading.analyzed_carried_fuel_kg = takeoff_mass_kg - load_case.zero_fuel_mass_kg;
            fuel_loading.carried_fuel_basis = CarriedFuelBasis::ReservePolicyClosure;
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
    } = case;
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
        }
    }

    fn policy_closure_case(converged: bool, shortfall_kg: f64) -> SelectedLoadCase {
        SelectedLoadCase {
            takeoff_mass_kg: 100_000.0,
            zero_fuel_mass_kg: 80_000.0,
            native_error: None,
            selection: LoadCaseSelection::PolicyClosure(Box::new(PolicyClosureCase {
                plan: zero_plan(),
                analytic: zero_analytic(),
                native_check: None,
                converged,
                route_distance_m: 1.0e6,
                reserve_margin_kg: -shortfall_kg,
                shortfall_kg,
                design_mission: None,
            })),
        }
    }

    fn maximum_available(reason: Option<&str>) -> SelectedLoadCase {
        SelectedLoadCase {
            takeoff_mass_kg: 100_000.0,
            zero_fuel_mass_kg: 80_000.0,
            native_error: None,
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
        apply_load_case(&mut fuel_loading, Some(&load_case), &mut findings);

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
        apply_load_case(&mut fuel_loading, Some(&load_case), &mut findings);

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
        apply_load_case(&mut fuel_loading, Some(&load_case), &mut findings);

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
        apply_load_case(&mut fuel_loading, Some(&load_case), &mut findings);

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
            selection: LoadCaseSelection::MaximumAvailableFuel {
                reason: None,
                route_check: Some(case),
            },
        };
        apply_load_case(&mut fuel_loading, Some(&load_case), &mut findings);

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
        apply_load_case(&mut fuel_loading, Some(&load_case), &mut findings);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, FindingCode::ReserveFuelShortfall);
        assert_eq!(findings[0].severity, FindingSeverity::Error);
        let dispatch = fuel_loading.dispatch.unwrap();
        assert_eq!(dispatch.outcome, DispatchOutcome::MtowLimited);
        assert_eq!(dispatch.reserve_margin_kg, Some(-250.0));
        assert!(!is_feasible(findings));
    }
}
