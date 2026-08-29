// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Electrical screening for the Air Cargo Challenge 2026 rule set.
//!
//! This is an eligibility aid, not a substitute for event scrutineering.  It
//! checks only the electrical clauses that can be supported by the selected
//! catalogue records and a calculated battery-side operating point.  Connector
//! gender, balance-lead pitch, pack maximum voltage, and physical installation
//! remain explicit verification items when their source records do not state
//! them.

use crate::catalog::ComponentKind;
use crate::Catalog;

use super::{CataloguePowertrainSelection, ElectricPropulsionError, ElectricPropulsionResult};

/// One electrical ACC 2026 screening result.
#[derive(Debug, Clone, PartialEq)]
pub enum Acc2026ElectricalFinding {
    /// A stated rule limit is violated by a source-backed value or calculated point.
    Failure {
        /// Concise rule topic.
        requirement: &'static str,
        /// Specific reason for the failure.
        message: String,
    },
    /// A current value is below disqualification but incurs the published penalty.
    CurrentPenalty {
        /// Battery-side current at the evaluated point.
        battery_current_a: f64,
    },
    /// The source data cannot establish a required physical detail.
    Unverified {
        /// Concise rule topic.
        requirement: &'static str,
        /// Evidence that remains unavailable.
        message: String,
    },
}

/// Electrical-only ACC 2026 assessment for one selection and operating point.
#[derive(Debug, Clone, PartialEq)]
pub struct Acc2026ElectricalAssessment {
    /// Findings in stable rule-check order.
    pub findings: Vec<Acc2026ElectricalFinding>,
}

impl Acc2026ElectricalAssessment {
    /// Whether no source-backed failure was found.
    pub fn has_failure(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| matches!(finding, Acc2026ElectricalFinding::Failure { .. }))
    }

    /// Whether every electrical item checked here is both evidenced and compliant.
    pub fn verified_compliant(&self) -> bool {
        !self.has_failure()
            && self
                .findings
                .iter()
                .all(|finding| !matches!(finding, Acc2026ElectricalFinding::Unverified { .. }))
    }
}

/// Screen a selected electric powertrain against ACC 2026 electrical clauses.
pub fn assess_acc2026_electrical(
    catalog: &Catalog,
    selection: &CataloguePowertrainSelection,
    point: &ElectricPropulsionResult,
) -> Result<Acc2026ElectricalAssessment, ElectricPropulsionError> {
    let battery_record = catalog.get(&selection.battery_id).ok_or_else(|| {
        ElectricPropulsionError::MissingComponent {
            id: selection.battery_id.clone(),
        }
    })?;
    let ComponentKind::Battery(battery) = &battery_record.kind else {
        return Err(ElectricPropulsionError::WrongComponentFamily {
            id: selection.battery_id.clone(),
            expected: "battery",
        });
    };
    let mut findings = Vec::new();
    check_motor(selection, &mut findings);
    check_battery(battery, &selection.battery_id, &mut findings);
    check_measured_point(point, &mut findings);
    Ok(Acc2026ElectricalAssessment { findings })
}

fn check_motor(
    selection: &CataloguePowertrainSelection,
    findings: &mut Vec<Acc2026ElectricalFinding>,
) {
    if selection.motor_count == 0 || selection.motor_count > 2 {
        findings.push(failure(
            "motor count",
            format!(
                "ACC 2026 permits at most two propulsion motors; {} were selected",
                selection.motor_count
            ),
        ));
    }
    if selection.motor_id != "tmotor-at2814-900kv" {
        findings.push(failure(
            "motor model",
            format!(
                "ACC 2026 specifies the unmodified T-MOTOR AT2814 900KV; '{}' was selected",
                selection.motor_id
            ),
        ));
    } else {
        findings.push(unverified(
            "motor modification",
            "a catalogue identifier cannot establish that the installed AT2814 900KV motor is unmodified".to_owned(),
        ));
    }
}

fn check_battery(
    battery: &crate::catalog::BatterySpec,
    battery_id: &str,
    findings: &mut Vec<Acc2026ElectricalFinding>,
) {
    if !matches!(battery.chemistry.as_str(), "LiPo" | "Li-ion" | "LiFePO4") {
        findings.push(failure(
            "battery chemistry",
            format!(
                "ACC 2026 allows LiPo, Li-ion, or LiFePO4 propulsion packs; '{battery_id}' is '{}'",
                battery.chemistry
            ),
        ));
    }
    match battery.series_cells {
        Some(cells) if cells > 3 => findings.push(failure(
            "battery series cells",
            format!("ACC 2026 permits at most 3S; '{battery_id}' is {cells}S"),
        )),
        Some(_) => {}
        None => findings.push(unverified(
            "battery series cells",
            format!("'{battery_id}' has no published series-cell count"),
        )),
    }
    match battery.nominal_energy_wh() {
        Some(energy_wh) if energy_wh > 100.0 => findings.push(failure(
            "battery energy",
            format!("ACC 2026 permits at most 100 Wh per pack; '{battery_id}' is {energy_wh:.2} Wh nominal"),
        )),
        Some(_) => {}
        None => findings.push(unverified(
            "battery energy",
            format!("'{battery_id}' has no published voltage and capacity pair"),
        )),
    }
    match battery.rated_discharge_current_a() {
        Some(current_a) if current_a < 40.0 => findings.push(failure(
            "battery continuous current",
            format!(
                "ACC 2026 requires at least 40 A continuous; '{battery_id}' names {current_a:.2} A"
            ),
        )),
        Some(_) => {}
        None => findings.push(unverified(
            "battery continuous current",
            format!("'{battery_id}' has no published capacity and continuous C rating"),
        )),
    }
    match battery.connector.as_deref() {
        Some(connector) if connector.contains("XT60") => findings.push(unverified(
            "battery connector",
            format!("'{battery_id}' identifies an XT60 connector, but its required female gender is not in the catalogue schema"),
        )),
        Some(connector) => findings.push(failure(
            "battery connector",
            format!("ACC 2026 requires a female XT60 battery connector; '{battery_id}' names '{connector}'"),
        )),
        None => findings.push(unverified(
            "battery connector",
            format!("'{battery_id}' has no published main connector"),
        )),
    }
    findings.push(unverified(
        "battery maximum voltage and balance lead",
        format!(
            "'{battery_id}' does not establish the 12.6 V maximum-pack voltage or the required 2.45 mm individual-cell balance lead"
        ),
    ));
}

fn check_measured_point(
    point: &ElectricPropulsionResult,
    findings: &mut Vec<Acc2026ElectricalFinding>,
) {
    if point.battery_terminal_voltage_v > 12.75 {
        findings.push(failure(
            "battery voltage",
            format!(
                "ACC 2026 disqualifies a measured propulsion voltage above 12.75 V; this point is {:.3} V",
                point.battery_terminal_voltage_v
            ),
        ));
    }
    if point.battery_current_a > 70.0 {
        findings.push(failure(
            "battery current",
            format!(
                "ACC 2026 disqualifies a battery-side current above 70 A; this point is {:.3} A",
                point.battery_current_a
            ),
        ));
    } else if point.battery_current_a > 30.0 {
        findings.push(Acc2026ElectricalFinding::CurrentPenalty {
            battery_current_a: point.battery_current_a,
        });
    }
}

fn failure(requirement: &'static str, message: String) -> Acc2026ElectricalFinding {
    Acc2026ElectricalFinding::Failure {
        requirement,
        message,
    }
}

fn unverified(requirement: &'static str, message: String) -> Acc2026ElectricalFinding {
    Acc2026ElectricalFinding::Unverified {
        requirement,
        message,
    }
}
