// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The verdict of a mission history: the envelope check at every instant and
//! the operations the plan asked for and the tanks could not carry out.

use alas_config::MissionPlan;

use super::{MissionCgPoint, MissionHistory};
use crate::feasibility::{FindingCode, FindingSeverity, PhysicalFinding};

/// The findings of `history` for `plan`.
///
/// One error per stage whose centre of gravity leaves the model envelope at
/// some instant (the worst instant is reported), one for the ramp state, one
/// per operation that could not be carried out in full, and a warning when the
/// envelope could not be assessed at all.
pub(in crate::feasibility) fn history_findings(
    history: &MissionHistory,
    plan: &MissionPlan,
    mtow_kg: f64,
) -> Vec<PhysicalFinding> {
    let mut findings = Vec::new();
    // Fuel taken on in flight can carry the aircraft above the mass it was
    // designed to leave the ground at: the gross mass is checked at every
    // instant after brake release (the ramp state may exceed it by the taxi fuel).
    if let Some(point) = history
        .points
        .iter()
        .filter(|point| point.stage.is_some() && point.mass_kg > mtow_kg * (1.0 + 1.0e-9))
        .max_by(|a, b| a.mass_kg.total_cmp(&b.mass_kg))
    {
        findings.push(PhysicalFinding {
            code: FindingCode::MissionPlanMassLimitViolation,
            severity: FindingSeverity::Error,
            message: format!(
                "the mission plan takes the aircraft to {:.0} kg at '{}', above the maximum take-off mass of {mtow_kg:.0} kg",
                point.mass_kg, point.label
            ),
            actual: Some(point.mass_kg),
            limit: Some(mtow_kg),
            unit: "kg",
        });
    }
    if history.points.iter().any(|point| {
        !point.forward_limit_pct_mac.is_finite() || !point.aft_limit_pct_mac.is_finite()
    }) {
        findings.push(PhysicalFinding {
            code: FindingCode::ModelCgAssessmentUnavailable,
            severity: FindingSeverity::Warning,
            message: "the model centre-of-gravity envelope could not be assessed at some instants of the mission plan; those instants are not judged".to_owned(),
            actual: None,
            limit: None,
            unit: "",
        });
    }
    let mut scopes: Vec<(String, Vec<&MissionCgPoint>)> = Vec::new();
    scopes.push((
        "the ramp state".to_owned(),
        history
            .points
            .iter()
            .filter(|point| point.stage.is_none())
            .collect(),
    ));
    for (index, stage) in plan.stages.iter().enumerate() {
        scopes.push((
            format!("stage '{}'", stage.id),
            history
                .points
                .iter()
                .filter(|point| point.stage == Some(index))
                .collect(),
        ));
    }
    for (scope, points) in scopes {
        let worst = points
            .iter()
            .filter_map(|point| margin(point).map(|margin| (margin, *point)))
            .filter(|(margin, _)| *margin < 0.0)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((margin, point)) = worst else {
            continue;
        };
        let (limit, side) = if point.cg_pct_mac < point.forward_limit_pct_mac {
            (point.forward_limit_pct_mac, "forward")
        } else {
            (point.aft_limit_pct_mac, "aft")
        };
        findings.push(PhysicalFinding {
            code: FindingCode::CgEnvelopeViolation,
            severity: FindingSeverity::Error,
            message: format!(
                "the centre of gravity of {scope} leaves the model envelope: {:.2} % MAC at '{}' ({:.0} kg), {:.2} % MAC beyond the {side} limit of {limit:.2} % MAC",
                point.cg_pct_mac, point.label, point.mass_kg, -margin
            ),
            actual: Some(point.cg_pct_mac),
            limit: Some(limit),
            unit: "% MAC",
        });
    }
    for note in &history.notes {
        findings.push(PhysicalFinding {
            code: FindingCode::MissionPlanOperationFailed,
            severity: FindingSeverity::Error,
            message: format!("mission plan: {note}"),
            actual: None,
            limit: None,
            unit: "",
        });
    }
    findings
}

/// Distance of the point inside its limits, % MAC (negative: outside), `None`
/// when the limits are not finite.
fn margin(point: &MissionCgPoint) -> Option<f64> {
    if !point.forward_limit_pct_mac.is_finite() || !point.aft_limit_pct_mac.is_finite() {
        return None;
    }
    Some(
        (point.cg_pct_mac - point.forward_limit_pct_mac)
            .min(point.aft_limit_pct_mac - point.cg_pct_mac),
    )
}

// The tests build the histories they judge, so a failed unwrap is the
// assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use alas_config::{PlanStage, StageKind};

    use super::*;

    fn plan() -> MissionPlan {
        MissionPlan {
            stages: vec![
                PlanStage::new("takeoff", StageKind::Takeoff { field: None }),
                PlanStage::new("cruise", StageKind::Takeoff { field: None }),
            ],
            ..MissionPlan::default()
        }
    }

    fn point(label: &str, stage: Option<usize>, pct: f64, mass_kg: f64) -> MissionCgPoint {
        MissionCgPoint {
            label: label.to_owned(),
            stage,
            time_s: 0.0,
            range_m: 0.0,
            mass_kg,
            cg_m: [0.0; 3],
            cg_pct_mac: pct,
            forward_limit_pct_mac: 10.0,
            aft_limit_pct_mac: 30.0,
            within_envelope: (10.0..=30.0).contains(&pct),
            fuel_kg: 0.0,
            trim_fuel_kg: 0.0,
            tank_fuel_kg: Vec::new(),
        }
    }

    fn history(points: Vec<MissionCgPoint>, notes: Vec<String>) -> MissionHistory {
        MissionHistory {
            points,
            events: Vec::new(),
            stages: Vec::new(),
            notes,
            leg_cg_x_m: Vec::new(),
            max_mass_mismatch_kg: 0.0,
        }
    }

    #[test]
    fn a_point_outside_the_envelope_is_an_error_for_its_stage_with_the_worst_margin() {
        let h = history(
            vec![
                point("ramp", None, 20.0, 1_000.0),
                point("a", Some(1), 31.0, 1_000.0),
                point("b", Some(1), 33.0, 1_000.0),
                point("c", Some(0), 5.0, 1_000.0),
            ],
            Vec::new(),
        );
        let findings = history_findings(&h, &plan(), 2_000.0);
        assert_eq!(findings.len(), 2, "{findings:?}");
        let cruise = findings
            .iter()
            .find(|f| f.message.contains("'cruise'"))
            .unwrap();
        assert_eq!(cruise.code, FindingCode::CgEnvelopeViolation);
        assert_eq!(cruise.severity, FindingSeverity::Error);
        assert_eq!(cruise.actual, Some(33.0));
        assert_eq!(cruise.limit, Some(30.0));
        let takeoff = findings
            .iter()
            .find(|f| f.message.contains("'takeoff'"))
            .unwrap();
        assert_eq!(takeoff.limit, Some(10.0));
    }

    #[test]
    fn a_walker_note_is_an_error_finding() {
        let h = history(
            vec![point("ramp", None, 20.0, 1_000.0)],
            vec!["offload of 9000 kg: not enough fuel".to_owned()],
        );
        let findings = history_findings(&h, &plan(), 2_000.0);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, FindingCode::MissionPlanOperationFailed);
        assert_eq!(findings[0].severity, FindingSeverity::Error);
    }

    #[test]
    fn mass_above_the_maximum_after_brake_release_is_reported_the_ramp_state_is_not() {
        let h = history(
            vec![
                point("ramp", None, 20.0, 2_050.0),
                point("refuelled", Some(1), 20.0, 2_300.0),
            ],
            Vec::new(),
        );
        let findings = history_findings(&h, &plan(), 2_000.0);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, FindingCode::MissionPlanMassLimitViolation);
        assert_eq!(findings[0].actual, Some(2_300.0));
    }

    #[test]
    fn instants_without_an_assessed_envelope_are_a_warning_not_a_verdict() {
        let mut unassessed = point("a", Some(0), 50.0, 1_000.0);
        unassessed.forward_limit_pct_mac = f64::NAN;
        unassessed.aft_limit_pct_mac = f64::NAN;
        unassessed.within_envelope = false;
        let findings = history_findings(&history(vec![unassessed], Vec::new()), &plan(), 2_000.0);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, FindingSeverity::Warning);
    }
}
