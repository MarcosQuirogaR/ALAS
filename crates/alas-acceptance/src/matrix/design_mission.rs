// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Flying and grading each preset's design mission.
//!
//! The mission is the preset's charted design point
//! ([`alas_config::PayloadRangeDesignPoint`]) flown on the delivered analysis
//! report by [`alas_pipeline::design_point::fly_design_point`],
//! the routine behind the design database's `characteristics.design_point_range`
//! (the value the matrix range cell scores): the design payload at the
//! payload-range takeoff-mass limit, under the reserve rule the source states,
//! flown as a range search. The interactive route of the run is not used.
//!
//! [`PresetDesignMissionStatus::Passed`] needs all three of:
//!
//! - (a) the closure converged: the point was flown, the long-range cruise
//!   speed (when the source asks for it) settled, the range is positive, and
//!   the reserve-inclusive fuel plan balances the carried takeoff fuel on
//!   both sides of the flown range
//!   ([`alas_pipeline::design_point::FuelBalance::closes`]): the plan at the
//!   flown range fits the carried fuel, and the fuel it leaves unspent is no
//!   more than the plan's fuel worth of one range-search termination
//!   tolerance (the plan one tolerance beyond, less the plan at the range),
//!   so a range search stopped by its iteration cap or bounded by a leg the
//!   burn model could not price fails; and the analysis' own mass closure
//!   left positive fuel;
//! - (b) fuel feasible: usable tank capacity evidence exists, reserve fuel
//!   is carried, and the reserve rule is sourced (stated with the point, or
//!   a reserve-fuel provenance that clears the promotion bar). The ramp-fuel
//!   against usable-tanks and takeoff-mass against takeoff-limit checks hold
//!   by construction (the point's fuel is the smaller of the tank less taxi
//!   and the takeoff-mass budget); they guard that construction and are not
//!   independent evidence;
//! - (c) the flown range at the design payload meets the preset's criterion
//!   ([`alas_config::presets::design_range_criterion`]: the range cell's grade
//!   band, or a notional aircraft's own requirement as a floor).
//!
//! [`PresetDesignMissionStatus::Failed`] is a flown mission that missed any of
//! them; [`PresetDesignMissionStatus::NotEvaluated`] is a preset with no design
//! point. Ranges are reported in nmi (1 nmi = 1,852 m), masses in kg.

use alas_config::presets::{self, DesignRangeCriterion};
use alas_config::{AlasConfig, MissingDesignMissionDatum, MissionEvidenceApplicability};
use alas_pipeline::design_point::fly_design_point;
use alas_pipeline::{AnalysisReport, FeasibilityReport, FindingCode, FuelCapacityEvidence};

use super::PresetDesignMissionStatus;

/// Metres per international nautical mile.
const M_PER_NMI: f64 = 1_852.0;

/// Relative rounding allowance of the construction guards in (b) and of the
/// payload cap in (c): numerical, not a design margin. The fuel balance of
/// (a) uses the range search's own tolerance instead.
const BALANCE_RELATIVE_TOLERANCE: f64 = 1.0e-3;

/// The flown design mission of one preset and its verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignMissionGrade {
    /// The verdict.
    pub status: PresetDesignMissionStatus,
    /// What the flown range had to reach, when the criterion table has a row.
    pub criterion: Option<DesignRangeCriterion>,
    /// Flown still-air range at the design payload, nmi.
    pub flown_range_nmi: Option<f64>,
    /// Payload flown, kg.
    pub payload_kg: Option<f64>,
    /// Takeoff mass at brake release, kg.
    pub takeoff_mass_kg: Option<f64>,
    /// Fuel loaded at the ramp (takeoff fuel plus taxi), kg.
    pub ramp_fuel_kg: Option<f64>,
    /// Usable tank capacity, kg.
    pub tank_capacity_kg: Option<f64>,
    /// Reserve fuel carried beyond the trip, kg.
    pub reserve_fuel_kg: Option<f64>,
    /// Requirement (a).
    pub closure_converged: bool,
    /// Requirement (b).
    pub fuel_feasible: bool,
    /// Requirement (c).
    pub range_accepted: bool,
    /// Every reason a requirement is not met, in (a), (b), (c) order.
    pub reasons: Vec<String>,
}

impl DesignMissionGrade {
    fn new(status: PresetDesignMissionStatus) -> Self {
        Self {
            status,
            criterion: None,
            flown_range_nmi: None,
            payload_kg: None,
            takeoff_mass_kg: None,
            ramp_fuel_kg: None,
            tank_capacity_kg: None,
            reserve_fuel_kg: None,
            closure_converged: false,
            fuel_feasible: false,
            range_accepted: false,
            reasons: Vec::new(),
        }
    }

    /// The verdict of a flown mission from its three requirements.
    fn settle(mut self) -> Self {
        self.status = verdict(
            self.closure_converged,
            self.fuel_feasible,
            self.range_accepted,
        );
        self
    }
}

/// Passed only when all three requirements hold; a flown mission that misses
/// any of them is Failed.
fn verdict(
    closure_converged: bool,
    fuel_feasible: bool,
    range_accepted: bool,
) -> PresetDesignMissionStatus {
    if closure_converged && fuel_feasible && range_accepted {
        PresetDesignMissionStatus::Passed
    } else {
        PresetDesignMissionStatus::Failed
    }
}

/// Whether the reserve rule the design mission flies is sourced: stated with
/// the design point, or a reserve-fuel provenance that clears the promotion
/// bar (tier admitted for reserve fuel, exact variant, stated conditions).
fn reserves_sourced(preset: &alas_config::AircraftPreset) -> bool {
    let stated_with_point = preset
        .reference
        .design_point
        .is_some_and(|point| point.reserve.is_some());
    let provenance = preset
        .reference
        .mission_provenance
        .reserve_fuel
        .is_some_and(|datum| {
            datum
                .tier
                .admits_promotion_for(MissingDesignMissionDatum::ReserveFuel)
                && datum.variant_match == MissionEvidenceApplicability::ExactPreset
                && datum.stated_conditions.is_some()
        });
    stated_with_point || provenance
}

fn within(value: f64, limit: f64) -> bool {
    value.is_finite() && limit.is_finite() && value <= limit * (1.0 + BALANCE_RELATIVE_TOLERANCE)
}

/// Fly and grade `preset_name`'s design mission on `report` (the delivered
/// analysis of `config`), with the run's `feasibility` for the analysis' own
/// mass closure.
pub(super) fn grade_design_mission(
    preset_name: &str,
    config: &AlasConfig,
    report: Option<&AnalysisReport>,
    feasibility: &FeasibilityReport,
) -> DesignMissionGrade {
    let Ok(preset) = presets::get(preset_name) else {
        let mut grade = DesignMissionGrade::new(PresetDesignMissionStatus::Unverified);
        grade.reasons.push("no registered preset record".to_owned());
        return grade;
    };
    if preset.reference.design_point.is_none() {
        let mut grade = DesignMissionGrade::new(PresetDesignMissionStatus::NotEvaluated);
        grade
            .reasons
            .push("the preset registers no design mission".to_owned());
        return grade;
    }
    let mut grade = DesignMissionGrade::new(PresetDesignMissionStatus::Failed);
    let criterion = presets::design_range_criterion(preset);
    if let Err(reason) = &criterion {
        grade
            .reasons
            .push(format!("(c) no range criterion: {reason}"));
    }
    grade.criterion = criterion.ok();
    let Some(report) = report else {
        grade.reasons.insert(
            0,
            "(a) not flown: the run delivered no analysis report".to_owned(),
        );
        return grade;
    };
    let flight = match fly_design_point(report, config, None) {
        Ok(flight) => flight,
        Err(reason) => {
            grade.reasons.insert(0, format!("(a) not flown: {reason}"));
            return grade;
        }
    };
    grade.tank_capacity_kg = Some(flight.chart.masses.tank_capacity_kg);
    let found = match &flight.range {
        Ok(found) => *found,
        Err(reason) => {
            grade.reasons.insert(
                0,
                format!("(a) the design point could not be priced: {reason}"),
            );
            return grade;
        }
    };
    let range_nmi = found.range_m / M_PER_NMI;
    let ramp_fuel_kg = found.fuel_kg + found.taxi_kg;
    grade.flown_range_nmi = Some(range_nmi);
    grade.payload_kg = Some(flight.payload_kg);
    grade.takeoff_mass_kg = Some(found.tow_kg);
    grade.ramp_fuel_kg = Some(ramp_fuel_kg);
    grade.reserve_fuel_kg = Some(found.reserve_fuel_kg);
    let mut reasons = Vec::new();

    // (a) The closure converged.
    let mut closure = true;
    if let Some((_, Err(reason))) = &flight.long_range {
        closure = false;
        reasons.push(format!(
            "(a) the long-range cruise speed the source quotes the range at did not settle: {reason}"
        ));
    }
    if !(range_nmi.is_finite() && range_nmi > 0.0) {
        closure = false;
        reasons.push(format!(
            "(a) the range search found no positive range ({range_nmi:.1} nmi)"
        ));
    }
    match flight.fuel_balance(report, &found) {
        Ok(balance) if balance.closes() => {}
        Ok(balance) => {
            closure = false;
            reasons.push(match &balance.required_beyond_kg {
                Ok(_) => format!(
                    "(a) the fuel plan at the flown range needs {:.0} kg against {:.0} kg carried; a converged range search leaves 0 to {:.0} kg (one {:.0} m search tolerance of fuel) unspent",
                    balance.required_kg,
                    balance.carried_kg,
                    balance.tolerance_fuel_kg().unwrap_or(f64::NAN),
                    balance.tolerance_m
                ),
                Err(reason) => format!(
                    "(a) the fuel plan {:.0} m beyond the flown range cannot be priced ({reason}): the range is bounded by the burn model, not by the carried fuel",
                    balance.tolerance_m
                ),
            });
        }
        Err(reason) => {
            closure = false;
            reasons.push(format!(
                "(a) the fuel plan at the flown range could not be priced again: {reason}"
            ));
        }
    }
    if feasibility.contains(FindingCode::NonPositiveFuel) {
        closure = false;
        reasons.push("(a) the analysis' MTOW mass closure left no positive fuel".to_owned());
    }
    grade.closure_converged = closure;

    // (b) Fuel feasible.
    let mut fuel = true;
    if flight.chart.tank_evidence == FuelCapacityEvidence::Unavailable {
        fuel = false;
        reasons.push("(b) no usable tank capacity evidence".to_owned());
    }
    if !within(ramp_fuel_kg, flight.chart.masses.tank_capacity_kg) {
        fuel = false;
        reasons.push(format!(
            "(b) ramp fuel {ramp_fuel_kg:.0} kg exceeds the {:.0} kg usable tanks",
            flight.chart.masses.tank_capacity_kg
        ));
    }
    if !within(found.tow_kg, flight.chart.masses.mtow_kg) {
        fuel = false;
        reasons.push(format!(
            "(b) takeoff mass {:.0} kg exceeds the {:.0} kg limit",
            found.tow_kg, flight.chart.masses.mtow_kg
        ));
    }
    if !(found.reserve_fuel_kg.is_finite() && found.reserve_fuel_kg > 0.0) {
        fuel = false;
        reasons.push("(b) the flown plan carries no reserve fuel".to_owned());
    }
    if !reserves_sourced(preset) {
        fuel = false;
        reasons.push(
            "(b) the reserve rule is not sourced: no rule stated with the design point and no admissible reserve-fuel provenance"
                .to_owned(),
        );
    }
    grade.fuel_feasible = fuel;

    // (c) The flown range at the design payload meets the criterion.
    let mut range_ok = true;
    if flight.payload_kg < flight.charted_payload_kg * (1.0 - BALANCE_RELATIVE_TOLERANCE) {
        range_ok = false;
        reasons.push(format!(
            "(c) the design payload {:.0} kg exceeds the model's {:.0} kg maximum payload; the range is not at the design payload",
            flight.charted_payload_kg, flight.payload_kg
        ));
    }
    match &grade.criterion {
        Some(criterion) if criterion.accepts(range_nmi) => {}
        Some(criterion) => {
            range_ok = false;
            reasons.push(format!(
                "(c) flown {range_nmi:.0} nmi is outside {} ({:+.1} %)",
                criterion.describe(),
                (range_nmi / criterion.reference_range_nmi() - 1.0) * 100.0
            ));
        }
        None => range_ok = false,
    }
    grade.range_accepted = range_ok;
    // The criterion-table reason (if any) stays last, after the flown checks.
    reasons.append(&mut grade.reasons);
    grade.reasons = reasons;
    grade.settle()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn only_all_three_requirements_pass() {
        assert_eq!(verdict(true, true, true), PresetDesignMissionStatus::Passed);
        for (a, b, c) in [
            (false, true, true),
            (true, false, true),
            (true, true, false),
            (false, false, false),
        ] {
            assert_eq!(verdict(a, b, c), PresetDesignMissionStatus::Failed);
        }
    }

    #[test]
    fn every_registered_preset_flies_a_sourced_reserve_rule() {
        for preset in presets::registry() {
            assert!(reserves_sourced(preset), "{}", preset.name);
        }
    }

    /// Registering a source is not a verdict: a preset whose mission could
    /// not be flown is Failed with the reason, never Passed or NotEvaluated.
    #[test]
    fn a_registered_mission_that_is_not_flown_fails_with_its_reason() {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
        let grade = grade_design_mission("A320-200", &config, None, &FeasibilityReport::default());
        assert_eq!(grade.status, PresetDesignMissionStatus::Failed);
        assert!(
            grade.reasons[0].starts_with("(a) not flown"),
            "{:?}",
            grade.reasons
        );
        assert!(grade.criterion.is_some());
    }

    #[test]
    fn an_unknown_preset_is_unverified() {
        let config = AlasConfig::default();
        let grade = grade_design_mission(
            "no-such-preset",
            &config,
            None,
            &FeasibilityReport::default(),
        );
        assert_eq!(grade.status, PresetDesignMissionStatus::Unverified);
    }
}
