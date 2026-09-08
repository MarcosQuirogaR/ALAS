// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What each physical finding means to a reader of the results page: its
//! title, its meaning, where to look next, the disciplines it touches, and
//! how its measured value and limit are labelled and compared.
//!
//! The tables are exhaustive over [`FindingCode`] on purpose: a finding the
//! pipeline can raise but this page cannot explain is a compile error rather
//! than a blank row.

use alas_pipeline::feasibility::FindingCode;

use crate::views::tr;

pub(super) fn finding_title(code: FindingCode) -> String {
    tr(match code {
        FindingCode::InvalidCruiseAerodynamics => "Cruise aerodynamics unavailable",
        FindingCode::NonPositiveFuel => "No usable fuel mass in the MTOW budget",
        FindingCode::TankLimitedTakeoffMass => "Takeoff mass is tank-limited",
        FindingCode::FuelCapacityUnavailable => "Fuel capacity unavailable",
        FindingCode::CgEnvelopeViolation => "CG envelope violation",
        FindingCode::ModelCgAssessmentUnavailable => "Model CG assessment unavailable",
        FindingCode::ModelCgForwardRangeViolation => "Model CG is forward of its range",
        FindingCode::NoseGearStrengthViolation => "Nose-gear load limit exceeded",
        FindingCode::MainGearStrengthViolation => "Main-gear load limit exceeded",
        FindingCode::MinimumNoseGearLoadViolation => "Insufficient nose-gear load",
        FindingCode::PublicPlanningCgEnvelopeViolation => "Public planning CG envelope exceeded",
        FindingCode::TrimUnavailable => "Cruise trim not demonstrated",
        FindingCode::InsufficientStaticMargin => "Static margin below the configured floor",
        FindingCode::WingAreaLimit => "Wing-area limit exceeded",
        FindingCode::MissionUnavailable => "Mission telemetry unavailable",
        FindingCode::MissionNotConverged => "Mission did not converge",
        FindingCode::InvalidMissionFuelBurn => "Mission fuel burn invalid",
        FindingCode::MissionFuelShortfall => "Mission stopped after fuel shortfall",
        FindingCode::InvalidCruiseForceBalance => "Cruise force balance invalid",
        FindingCode::FieldPerformanceUnavailable => "Field performance unavailable",
        FindingCode::FieldTakeoffDistanceViolation => "Takeoff distance exceeds available runway",
        FindingCode::FieldLandingDistanceViolation => {
            "Landing requirement exceeds available runway"
        }
        FindingCode::LandingMassLimitViolation => "Maximum landing mass exceeded",
        FindingCode::ThrustMarginViolation => "Insufficient takeoff thrust margin",
        FindingCode::MissionThrottleLimitViolation => {
            "Mission throttle exceeds the modeled envelope"
        }
        FindingCode::PassengerCapacityShortfall => "Passenger seating shortfall",
        FindingCode::CargoCapacityShortfall => "Cargo capacity shortfall",
        FindingCode::MaximumZeroFuelWeightViolation => "Maximum zero-fuel weight exceeded",
        FindingCode::StructuralPayloadLimitViolation => "Structural payload limit exceeded",
        FindingCode::ReserveFuelShortfall => "Policy fuel does not fit the load case",
        FindingCode::DispatchNotConverged => "Fuel-policy takeoff mass did not settle",
        FindingCode::FuelPolicyUnavailable => "Fuel policy could not be priced",
        FindingCode::MassLedgerUnavailable => "Mass ledger unavailable",
        FindingCode::FuelTankLayoutUnavailable => "Fuel-tank arrangement unavailable",
        FindingCode::MassModelDisagreement => "Ledger and lumped mass models disagree",
        FindingCode::InvalidEnvelopeSpeedOrder => "Maneuver envelope speeds out of order",
    })
}

pub(super) fn finding_meaning(code: FindingCode) -> String {
    tr(match code {
        FindingCode::InvalidCruiseAerodynamics => "The cruise aerodynamic result did not contain a positive finite lift-to-drag ratio, so performance derived from it is not trustworthy.",
        FindingCode::NonPositiveFuel => "Operating empty mass plus payload consumed the configured MTOW budget, leaving no positive finite fuel allocation.",
        FindingCode::TankLimitedTakeoffMass => "The MTOW mass budget could accept more fuel than the established usable tank capacity. The analyzed aircraft therefore departs below MTOW.",
        FindingCode::FuelCapacityUnavailable => "Neither published preset evidence nor the geometry estimate established a usable-fuel capacity for this aircraft.",
        FindingCode::CgEnvelopeViolation => "A retained legacy CG check reported that the analyzed loading state lies outside its allowed envelope.",
        FindingCode::ModelCgAssessmentUnavailable => "The model could not construct the typed CG and landing-gear assessment needed for a physical verdict.",
        FindingCode::ModelCgForwardRangeViolation => "The analyzed CG lies forward of the longitudinal range represented by the current landing-gear and loading model.",
        FindingCode::NoseGearStrengthViolation => "The modeled nose-gear vertical load exceeds the configured tire or gear capacity.",
        FindingCode::MainGearStrengthViolation => "The modeled main-gear vertical load exceeds the configured tire or gear capacity.",
        FindingCode::MinimumNoseGearLoadViolation => "The modeled nose load is below the configured minimum needed to retain preliminary steering authority.",
        FindingCode::PublicPlanningCgEnvelopeViolation => "The point lies outside a manufacturer public planning curve. That curve is preliminary evidence; the actual aircraft weight-and-balance manual controls operations.",
        FindingCode::TrimUnavailable => "ALAS did not retain a finite cruise point that simultaneously satisfies required lift and zero pitching moment. Any displayed untrimmed L/D is a fallback.",
        FindingCode::InsufficientStaticMargin => "The calculated longitudinal static margin is below the physical floor configured for this analysis.",
        FindingCode::WingAreaLimit => "The projected XY wing reference area is non-finite or exceeds the configured maximum.",
        FindingCode::MissionUnavailable => "Mission analysis was requested but returned no usable trajectory telemetry.",
        FindingCode::MissionNotConverged => "At least one native mission segment failed its numerical convergence criteria; totals from the incomplete trajectory are not final requirements.",
        FindingCode::InvalidMissionFuelBurn => "The mission produced a non-finite or non-positive fuel-burn value.",
        FindingCode::MissionFuelShortfall => "Cumulative modeled burn crossed the fuel loaded into this load case. If the mission stopped, the reported deficit is only the overrun observed at the stopping point, not the completed-trip requirement.",
        FindingCode::InvalidCruiseForceBalance => "At least one cruise telemetry record contains a non-finite force or equilibrium result.",
        FindingCode::FieldPerformanceUnavailable => "The selected airport could not be resolved or the mass, wing, thrust, or runway inputs required by the preliminary field model were invalid.",
        FindingCode::FieldTakeoffDistanceViolation => "Modeled takeoff distance required is greater than takeoff distance available at the selected departure conditions.",
        FindingCode::FieldLandingDistanceViolation => "Modeled landing distance or landing wing loading exceeds the selected arrival-field limit.",
        FindingCode::LandingMassLimitViolation => "The analyzed arrival mass is above the configured maximum landing mass. Fuel burn, payload, or the mission/loading definition must change before arrival.",
        FindingCode::ThrustMarginViolation => "Static thrust-to-weight is below the preliminary value required by the selected departure field.",
        FindingCode::MissionThrottleLimitViolation => "At least one mission control point requires a throttle command above the modeled full-throttle limit of 1.0.",
        FindingCode::PassengerCapacityShortfall => "The generated cabin placed fewer passenger seats than the requested passenger count.",
        FindingCode::CargoCapacityShortfall => "The generated ULD layout delivered less net cargo than requested.",
        FindingCode::MaximumZeroFuelWeightViolation => "The modeled zero-fuel mass exceeds the published maximum zero-fuel weight for this unchanged preset.",
        FindingCode::StructuralPayloadLimitViolation => "The modeled payload exceeds the configured structural payload limit.",
        FindingCode::ReserveFuelShortfall => "The taxi, trip, contingency, alternate and final-reserve fuel the selected scheme requires for this route exceeds what the takeoff-mass limit or the usable tanks admit. The mission was flown at the admissible mass, so it carried less than the rule requires.",
        FindingCode::DispatchNotConverged => "The takeoff mass the fuel policy requires did not settle within the native refinement budget; the last iterate was flown and its reserves are approximate.",
        FindingCode::FuelPolicyUnavailable => "The analytic burn model needed to price the fuel policy could not be built from this report, so the frozen maximum-available-fuel case was flown instead.",
        FindingCode::MassLedgerUnavailable => "The item-level mass ledger could not be built for this geometry, so only the lumped ten-group model backs the mass and balance evidence.",
        FindingCode::FuelTankLayoutUnavailable => "The configured tank arrangement could not be resolved on the built wing, so tank capacities, fuel centroids and the inertia tensor are not available.",
        FindingCode::MassModelDisagreement => "The item ledger and the lumped model place the takeoff centre of gravity more than five percent of the mean chord apart; the component stations and the lumped coordinates need reconciling before the balance result is trusted.",
        FindingCode::InvalidEnvelopeSpeedOrder => "The V-n envelope speeds do not satisfy VS < VA <= VC < VD, so the maneuver diagram is not a valid envelope.",
    })
}

pub(super) fn finding_next_step(code: FindingCode) -> String {
    tr(match code {
        FindingCode::InvalidCruiseAerodynamics | FindingCode::InvalidCruiseForceBalance => {
            "Aerodynamics, then Mission & Route"
        }
        FindingCode::TrimUnavailable | FindingCode::InsufficientStaticMargin => {
            "Aerodynamics and Weight & Balance"
        }
        FindingCode::NonPositiveFuel
        | FindingCode::TankLimitedTakeoffMass
        | FindingCode::FuelCapacityUnavailable => "Weight & Balance and Mission & Route",
        FindingCode::MissionUnavailable
        | FindingCode::MissionNotConverged
        | FindingCode::InvalidMissionFuelBurn
        | FindingCode::MissionFuelShortfall
        | FindingCode::MissionThrottleLimitViolation => "Mission & Route",
        FindingCode::CgEnvelopeViolation
        | FindingCode::ModelCgAssessmentUnavailable
        | FindingCode::ModelCgForwardRangeViolation
        | FindingCode::NoseGearStrengthViolation
        | FindingCode::MainGearStrengthViolation
        | FindingCode::MinimumNoseGearLoadViolation
        | FindingCode::PublicPlanningCgEnvelopeViolation => "Weight & Balance",
        FindingCode::WingAreaLimit | FindingCode::InvalidEnvelopeSpeedOrder => {
            "Aerodynamics and Optimization"
        }
        FindingCode::FieldPerformanceUnavailable
        | FindingCode::FieldTakeoffDistanceViolation
        | FindingCode::FieldLandingDistanceViolation
        | FindingCode::LandingMassLimitViolation
        | FindingCode::ThrustMarginViolation => "Field Performance and Weight & Balance",
        FindingCode::PassengerCapacityShortfall | FindingCode::CargoCapacityShortfall => {
            "Weight & Balance payload layout"
        }
        FindingCode::ReserveFuelShortfall
        | FindingCode::DispatchNotConverged
        | FindingCode::FuelPolicyUnavailable => "Mission & Route and the fuel policy",
        FindingCode::MassLedgerUnavailable
        | FindingCode::FuelTankLayoutUnavailable
        | FindingCode::MassModelDisagreement => "Weight & Balance and the tank arrangement",
        FindingCode::MaximumZeroFuelWeightViolation => "Weight & Balance and Payload",
        FindingCode::StructuralPayloadLimitViolation => "Structures and Weight & Balance",
    })
}

pub(super) fn affected_disciplines(code: FindingCode) -> String {
    tr(match code {
        FindingCode::InvalidCruiseAerodynamics | FindingCode::InvalidEnvelopeSpeedOrder => {
            "Aerodynamics | Performance"
        }
        FindingCode::InvalidCruiseForceBalance => "Mission solver | Aerodynamics | Propulsion",
        FindingCode::TrimUnavailable => "Stability & control | Aerodynamics | Weight & balance",
        FindingCode::InsufficientStaticMargin => "Stability & control | Weight & balance",
        FindingCode::NonPositiveFuel
        | FindingCode::TankLimitedTakeoffMass
        | FindingCode::FuelCapacityUnavailable => "Mass properties | Fuel system | Mission",
        FindingCode::MissionUnavailable
        | FindingCode::MissionNotConverged
        | FindingCode::InvalidMissionFuelBurn => "Mission solver | Numerical integration",
        FindingCode::MissionFuelShortfall => "Mission | Mass properties | Propulsion",
        FindingCode::MissionThrottleLimitViolation => "Propulsion | Mission solver | Performance",
        FindingCode::CgEnvelopeViolation
        | FindingCode::ModelCgAssessmentUnavailable
        | FindingCode::ModelCgForwardRangeViolation
        | FindingCode::PublicPlanningCgEnvelopeViolation => {
            "Weight & balance | Stability & control"
        }
        FindingCode::NoseGearStrengthViolation
        | FindingCode::MainGearStrengthViolation
        | FindingCode::MinimumNoseGearLoadViolation => "Landing gear | Weight & balance",
        FindingCode::WingAreaLimit => "Geometry | Aerodynamics | Optimization",
        FindingCode::FieldPerformanceUnavailable
        | FindingCode::FieldTakeoffDistanceViolation
        | FindingCode::FieldLandingDistanceViolation => "Field performance | Airport constraints",
        FindingCode::LandingMassLimitViolation => "Weight & balance | Mission | Field performance",
        FindingCode::ThrustMarginViolation => "Propulsion | Field performance",
        FindingCode::PassengerCapacityShortfall | FindingCode::CargoCapacityShortfall => {
            "Payload layout | Weight & balance"
        }
        FindingCode::MaximumZeroFuelWeightViolation => {
            "Weight & balance | Mass properties | Payload"
        }
        FindingCode::ReserveFuelShortfall
        | FindingCode::DispatchNotConverged
        | FindingCode::FuelPolicyUnavailable => "Fuel policy | Mission | Mass properties",
        FindingCode::MassLedgerUnavailable
        | FindingCode::FuelTankLayoutUnavailable
        | FindingCode::MassModelDisagreement => "Mass properties | Fuel system | Weight & balance",
        FindingCode::StructuralPayloadLimitViolation => {
            "Structures | Payload layout | Weight & balance"
        }
    })
}

pub(super) fn actual_label(code: FindingCode) -> String {
    tr(match code {
        FindingCode::MissionFuelShortfall => "Burn at stop / evaluated burn",
        FindingCode::PassengerCapacityShortfall => "Seats placed",
        FindingCode::CargoCapacityShortfall => "Net cargo loaded",
        FindingCode::FieldTakeoffDistanceViolation => "TODR",
        FindingCode::FieldLandingDistanceViolation => "Required value",
        FindingCode::MissionThrottleLimitViolation => "Maximum throttle",
        FindingCode::TankLimitedTakeoffMass => "MTOW-closure fuel",
        FindingCode::MaximumZeroFuelWeightViolation => "Calculated zero-fuel mass",
        FindingCode::ReserveFuelShortfall => "Required takeoff fuel",
        FindingCode::MassModelDisagreement => "Ledger takeoff CG",
        FindingCode::StructuralPayloadLimitViolation => "Modeled payload",
        _ => "Calculated",
    })
}

pub(super) fn limit_label(code: FindingCode) -> String {
    tr(match code {
        FindingCode::MissionFuelShortfall => "Fuel loaded",
        FindingCode::PassengerCapacityShortfall => "Passengers requested",
        FindingCode::CargoCapacityShortfall => "Net cargo requested",
        FindingCode::FieldTakeoffDistanceViolation => "TODA",
        FindingCode::FieldLandingDistanceViolation => "Available / limiting value",
        FindingCode::MissionThrottleLimitViolation => "Full-throttle limit",
        FindingCode::TankLimitedTakeoffMass => "Usable tank capacity",
        FindingCode::MaximumZeroFuelWeightViolation => "Published MZFW",
        FindingCode::ReserveFuelShortfall => "Admissible takeoff fuel",
        FindingCode::MassModelDisagreement => "Lumped takeoff CG",
        FindingCode::StructuralPayloadLimitViolation => "Structural payload limit",
        _ => "Limit",
    })
}

pub(super) fn finding_margin(code: FindingCode, actual: f64, limit: f64) -> f64 {
    match code {
        FindingCode::NonPositiveFuel
        | FindingCode::InsufficientStaticMargin
        | FindingCode::ThrustMarginViolation
        | FindingCode::MinimumNoseGearLoadViolation
        | FindingCode::PassengerCapacityShortfall
        | FindingCode::CargoCapacityShortfall
        | FindingCode::MaximumZeroFuelWeightViolation
        | FindingCode::StructuralPayloadLimitViolation => actual - limit,
        _ => limit - actual,
    }
}
