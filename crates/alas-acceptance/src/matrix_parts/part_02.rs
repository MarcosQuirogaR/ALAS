// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


/// Run the acceptance suite across every registered aircraft preset.
pub fn run_acceptance_matrix() -> AcceptanceMatrixReport {
    let names = presets::available();
    let mut results = Vec::with_capacity(names.len());

    for name in names {
        match evaluate_preset(name) {
            Ok(res) => {
                results.push(res);
            }
            Err(e) => {
                results.push(PresetAcceptanceResult {
                    name: name.to_owned(),
                    geometry_valid: false,
                    mtow_kg: 0.0,
                    oew_kg: 0.0,
                    mtow_closure_fuel_kg: 0.0,
                    usable_fuel_capacity_kg: None,
                    analyzed_carried_fuel_kg: 0.0,
                    analyzed_takeoff_mass_kg: 0.0,
                    mtow_shortfall_kg: 0.0,
                    payload_kg: 0.0,
                    cruise_l_over_d: 0.0,
                    static_margin: 0.0,
                    neutral_point_x: 0.0,
                    cruise_mach: 0.0,
                    wingbox_mass_kg: 0.0,
                    figure_scenes_count: 0,
                    public_design_matches_preset: false,
                    model_cg_envelope_ok: false,
                    model_cg_static_margin_floor: f64::NAN,
                    model_cg_analyzed_takeoff_static_margin: f64::NAN,
                    model_cg_minimum_loading_static_margin: f64::NAN,
                    model_cg_target_static_margin: f64::NAN,
                    model_cg_target_preference_met: false,
                    public_planning_cg_status: PlanningCgStatus::NotEvaluated,
                    model_cg_pct_mac: f64::NAN,
                    public_planning_cg_pct_mac: None,
                    trim_cm_residual: None,
                    mission_converged: false,
                    mission_fuel_within_available: false,
                    cruise_equilibrium: None,
                    fuel_capacity_evidence: FuelCapacityEvidence::Unavailable,
                    design_mission_status: PresetDesignMissionStatus::Unverified,
                    wing_area_within_limit: false,
                    execution_passed: false,
                    physical_passed: false,
                    physical_findings: Vec::new(),
                    model_audit: PresetModelAudit::default(),
                });
                tracing::error!(preset = name, error = %e, "preset acceptance evaluation failed");
            }
        }
    }

    let all_executed = results.iter().all(|result| result.execution_passed);
    let all_physical_passed = results.iter().all(|result| result.physical_passed);
    let all_design_missions_verified = results.iter().all(|result| {
        matches!(
            result.design_mission_status,
            PresetDesignMissionStatus::Passed | PresetDesignMissionStatus::Failed
        )
    });
    let all_passed = combined_acceptance(
        all_executed,
        all_physical_passed,
        all_design_missions_verified,
    );

    AcceptanceMatrixReport {
        presets: results,
        all_executed,
        all_passed,
        all_physical_passed,
        all_design_missions_verified,
    }
}

fn assess_design_mission_status(evidence: &DesignMissionEvidence) -> PresetDesignMissionStatus {
    match evidence {
        DesignMissionEvidence::Unverified => PresetDesignMissionStatus::Unverified,
        DesignMissionEvidence::SourceBacked(_) => PresetDesignMissionStatus::NotEvaluated,
    }
}

fn finding_governs_preset(finding: &PhysicalFinding) -> bool {
    !is_mission_finding(finding.code)
}

fn is_mission_finding(code: FindingCode) -> bool {
    matches!(
        code,
        FindingCode::MissionUnavailable
            | FindingCode::MissionNotConverged
            | FindingCode::InvalidMissionFuelBurn
            | FindingCode::MissionFuelShortfall
    )
}

fn combined_acceptance(
    all_executed: bool,
    all_physical_passed: bool,
    all_design_missions_verified: bool,
) -> bool {
    all_executed && all_physical_passed && all_design_missions_verified
}

fn format_design_mission_status(status: PresetDesignMissionStatus) -> &'static str {
    match status {
        PresetDesignMissionStatus::Unverified => "UNVERIFIED - no source-backed mission registered",
        PresetDesignMissionStatus::NotEvaluated => "NOT EVALUATED",
        PresetDesignMissionStatus::Passed => "PASS",
        PresetDesignMissionStatus::Failed => "FAIL",
    }
}

fn format_planning_cg_status(status: PlanningCgStatus) -> &'static str {
    match status {
        PlanningCgStatus::NotEvaluated => "N/E",
        PlanningCgStatus::WithinPublishedLimits => "WITHIN",
        PlanningCgStatus::ForwardLimitViolation => "FWD FAIL",
        PlanningCgStatus::AftLimitViolation => "AFT FAIL",
        PlanningCgStatus::AftLimitNotPublished => "AFT N/P",
    }
}

fn format_finding(finding: &PhysicalFinding) -> String {
    match (finding.actual, finding.limit) {
        (Some(actual), Some(limit)) if !finding.unit.is_empty() => format!(
            "{} (actual {:.3} {}, limit {:.3} {})",
            finding.message, actual, finding.unit, limit, finding.unit
        ),
        _ => finding.message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_success_cannot_hide_a_physical_failure() {
        assert!(!combined_acceptance(true, false, true));
        assert!(!combined_acceptance(false, true, true));
        assert!(!combined_acceptance(true, true, false));
        assert!(combined_acceptance(true, true, true));
    }

    #[test]
    fn unknown_missions_make_an_otherwise_clean_matrix_incomplete_not_failed() {
        let report = AcceptanceMatrixReport {
            presets: Vec::new(),
            all_executed: true,
            all_passed: false,
            all_physical_passed: true,
            all_design_missions_verified: false,
        };

        let text = format_matrix_report(&report);
        assert!(text.contains("Acceptance Verdict: INCOMPLETE - DESIGN MISSIONS UNVERIFIED"));
    }

    #[test]
    fn unverified_route_findings_do_not_govern_a_preset_design_verdict() {
        let finding = PhysicalFinding {
            code: FindingCode::MissionFuelShortfall,
            severity: FindingSeverity::Error,
            message: "arbitrary route is too long".to_owned(),
            actual: Some(2.0),
            limit: Some(1.0),
            unit: "kg",
        };

        assert!(!finding_governs_preset(&finding));
    }

    #[test]
    fn source_registration_alone_is_not_a_design_mission_verdict() {
        let evidence =
            DesignMissionEvidence::SourceBacked(Box::new(alas_config::DesignMissionReference {
                range_m: 1_000_000.0,
                payload_kg: 10_000.0,
                profile: alas_config::MissionProfileConfig::default(),
                required_reserve_fuel_kg: 2_000.0,
                departure_airport: None,
                arrival_airport: None,
                source: "revision-locked payload-range definition",
            }));

        assert_eq!(
            assess_design_mission_status(&evidence),
            PresetDesignMissionStatus::NotEvaluated
        );
    }

    #[test]
    fn planning_status_labels_do_not_claim_certification() {
        let labels = [
            PlanningCgStatus::NotEvaluated,
            PlanningCgStatus::WithinPublishedLimits,
            PlanningCgStatus::ForwardLimitViolation,
            PlanningCgStatus::AftLimitViolation,
            PlanningCgStatus::AftLimitNotPublished,
        ]
        .map(format_planning_cg_status)
        .join(" ");

        assert!(!labels.to_ascii_lowercase().contains("certif"));
    }
}

