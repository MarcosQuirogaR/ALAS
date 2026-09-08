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

use crate::mission_stage::dispatch::PolicyClosureCase;
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
    /// The native refinement did not settle within its budget.
    NotConverged,
    /// The frozen maximum-available-fuel case was flown by choice.
    LegacyMaximumFuel,
    /// The policy could not be priced, so the legacy case was flown.
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
            Self::LegacyMaximumFuel => "legacy_maximum_fuel",
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
    /// Native mission flights spent on the closure.
    pub native_flights: usize,
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
    match &load_case.selection {
        LoadCaseSelection::MaximumAvailableFuel { reason } => {
            let outcome = match reason {
                Some(reason) => {
                    // `reason` is only `Some` when the analytic dispatch model
                    // could not be built or failed to solve (see
                    // `select_load_case` in `mission_stage::dispatch`), never
                    // for a deliberate maximum-available-fuel policy choice
                    // (that is `reason: None`, `DispatchOutcome::LegacyMaximumFuel`,
                    // left un-findinged below) and never for a load the
                    // limits reject (that is `PolicyClosure` with
                    // `shortfall_kg > 0`, reported as `ReserveFuelShortfall`
                    // below). An unpriced policy means this mission was never
                    // actually analyzed against its fuel policy, so it cannot
                    // be certified feasible: it is an analysis failure, not
                    // evidence either way about physical feasibility.
                    findings.push(error(
                        FindingCode::FuelPolicyUnavailable,
                        format!(
                            "the fuel-policy dispatch analysis could not be completed for this mission (not evidence of physical infeasibility), so the maximum-available-fuel case was flown instead: {reason}"
                        ),
                        None,
                        None,
                        "",
                    ));
                    DispatchOutcome::Unavailable
                }
                None => DispatchOutcome::LegacyMaximumFuel,
            };
            fuel_loading.dispatch = Some(DispatchAssessment {
                outcome,
                plan: None,
                takeoff_mass_kg: load_case.takeoff_mass_kg,
                shortfall_kg: 0.0,
                native_flights: 0,
            });
        }
        LoadCaseSelection::PolicyClosure(case) => {
            let PolicyClosureCase {
                plan,
                analytic,
                native_flights,
                converged,
                shortfall_kg,
            } = case.as_ref();
            let takeoff_mass_kg = load_case.takeoff_mass_kg;
            fuel_loading.analyzed_takeoff_mass_kg = takeoff_mass_kg;
            fuel_loading.analyzed_carried_fuel_kg = takeoff_mass_kg - load_case.zero_fuel_mass_kg;
            fuel_loading.carried_fuel_basis = CarriedFuelBasis::ReservePolicyClosure;
            let outcome = if *shortfall_kg > 0.0 {
                match analytic.status {
                    DispatchStatus::TankLimited { .. } => DispatchOutcome::TankLimited,
                    _ => DispatchOutcome::MtowLimited,
                }
            } else if *converged {
                DispatchOutcome::Converged
            } else {
                DispatchOutcome::NotConverged
            };
            if *shortfall_kg > 0.0 {
                let bound = match outcome {
                    DispatchOutcome::TankLimited => "usable tank capacity",
                    _ => "takeoff-mass limit",
                };
                findings.push(error(
                    FindingCode::ReserveFuelShortfall,
                    format!(
                        "the route needs {:.1} kg of takeoff fuel under the {} scheme, {:.1} kg more than the {} admits; the mission was flown at the admissible mass",
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
                // As with `FuelPolicyUnavailable` above, a takeoff mass that
                // did not settle within the native refinement budget means
                // this mission's dispatch was never actually closed: the
                // last (unsettled) iterate was flown regardless. That is an
                // analysis failure, not evidence either way about physical
                // feasibility, so it is `Error`-severity and must fail
                // `is_feasible()` rather than pass through as a Warning.
                findings.push(error(
                    FindingCode::DispatchNotConverged,
                    format!(
                        "the fuel-policy takeoff mass did not settle within {native_flights} native flights (analysis did not converge, not evidence of physical infeasibility); the last, unsettled iterate was flown"
                    ),
                    Some(takeoff_mass_kg),
                    None,
                    "kg",
                ));
            }
            fuel_loading.dispatch = Some(DispatchAssessment {
                outcome,
                plan: Some(*plan),
                takeoff_mass_kg,
                shortfall_kg: *shortfall_kg,
                native_flights: *native_flights,
            });
        }
    }
}

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
            selection: LoadCaseSelection::PolicyClosure(Box::new(PolicyClosureCase {
                plan: zero_plan(),
                analytic: zero_analytic(),
                native_flights: 3,
                converged,
                shortfall_kg,
            })),
        }
    }

    fn maximum_available(reason: Option<&str>) -> SelectedLoadCase {
        SelectedLoadCase {
            takeoff_mass_kg: 100_000.0,
            zero_fuel_mass_kg: 80_000.0,
            selection: LoadCaseSelection::MaximumAvailableFuel {
                reason: reason.map(str::to_owned),
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
            DispatchOutcome::LegacyMaximumFuel
        );
        assert!(is_feasible(findings));
    }

    /// A native takeoff-mass refinement that does not settle (no shortfall,
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
}
