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

use super::{
    error, CarriedFuelBasis, FindingCode, FindingSeverity, FuelLoadingAssessment, PhysicalFinding,
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
                    findings.push(PhysicalFinding {
                        code: FindingCode::FuelPolicyUnavailable,
                        severity: FindingSeverity::Warning,
                        message: format!(
                            "the fuel policy could not be priced, so the maximum-available-fuel case was flown: {reason}"
                        ),
                        actual: None,
                        limit: None,
                        unit: "",
                    });
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
                findings.push(PhysicalFinding {
                    code: FindingCode::DispatchNotConverged,
                    severity: FindingSeverity::Warning,
                    message: format!(
                        "the fuel-policy takeoff mass did not settle within {native_flights} native flights; the last iterate was flown"
                    ),
                    actual: Some(takeoff_mass_kg),
                    limit: None,
                    unit: "kg",
                });
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
