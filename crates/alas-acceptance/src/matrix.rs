// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Multi-preset acceptance matrix and cross-disciplinary verification.
//!
//! The matrix reports end-to-end execution separately from the physical
//! findings produced for every published aircraft configuration. Passing the
//! implemented checks is preliminary-design evidence, not aircraft
//! certification.

mod evaluate;
mod format;
mod model_audit;
mod preset_runs;

pub use evaluate::{evaluate_preset, generate_scenes_for_preset};
pub use format::{format_matrix_json, format_matrix_report};
pub use model_audit::PresetModelAudit;

use alas_config::presets;
use alas_config::DesignMissionEvidence;
use alas_pipeline::{
    CruiseEquilibriumAssessment, FindingCode, FuelCapacityEvidence, PhysicalFinding,
    PlanningCgStatus,
};

/// Evidence-aware result of validating a preset's design mission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetDesignMissionStatus {
    /// The registered preset has no complete, revision-locked mission source.
    Unverified,
    /// Source evidence exists, but the exact design mission was not evaluated.
    NotEvaluated,
    /// The source-backed design mission passed its physical checks.
    Passed,
    /// The source-backed design mission failed at least one physical check.
    Failed,
}

/// Metric summary from evaluating a single preset through the acceptance matrix.
#[derive(Debug, Clone)]
pub struct PresetAcceptanceResult {
    /// Aircraft preset identifier.
    pub name: String,
    /// Whether the geometry builder succeeded.
    pub geometry_valid: bool,
    /// Maximum takeoff weight in kilograms.
    pub mtow_kg: f64,
    /// Operating empty weight in kilograms.
    pub oew_kg: f64,
    /// MTOW mass-closure remainder assigned to fuel, in kilograms.
    pub mtow_closure_fuel_kg: f64,
    /// Established usable fuel capacity, in kilograms.
    pub usable_fuel_capacity_kg: Option<f64>,
    /// Fuel actually carried by the analyzed load case, in kilograms.
    pub analyzed_carried_fuel_kg: f64,
    /// Fuel the dispatched route loads when it differs from the analyzed
    /// (Hard-MTOW design) load, in kilograms.
    pub flown_carried_fuel_kg: Option<f64>,
    /// Takeoff mass actually evaluated by the mission, in kilograms.
    pub analyzed_takeoff_mass_kg: f64,
    /// Amount by which the analyzed load case lies below MTOW, in kilograms.
    pub mtow_shortfall_kg: f64,
    /// Design payload in kilograms.
    pub payload_kg: f64,
    /// Lift-to-drag ratio at cruise design point.
    pub cruise_l_over_d: f64,
    /// Static margin as a fraction of mean aerodynamic chord.
    pub static_margin: f64,
    /// Neutral point longitudinal position in metres.
    pub neutral_point_x: f64,
    /// Design cruise Mach number.
    pub cruise_mach: f64,
    /// Sized wingbox structural mass in kilograms.
    pub wingbox_mass_kg: f64,
    /// Number of report figure scenes generated.
    pub figure_scenes_count: usize,
    /// Whether the public pipeline evaluated the selected preset design.
    pub public_design_matches_preset: bool,
    /// Whether every hard model-derived CG and gear-load constraint passed.
    ///
    /// This is a preliminary design-model result, not certification evidence.
    pub model_cg_envelope_ok: bool,
    /// Hard longitudinal-stability floor applied by the model.
    pub model_cg_static_margin_floor: f64,
    /// Static margin at the fuel-capped takeoff point assessed by the model.
    pub model_cg_analyzed_takeoff_static_margin: f64,
    /// Lowest static margin across OEW and the analyzed ZFW/TOW cases.
    pub model_cg_minimum_loading_static_margin: f64,
    /// Non-governing optimizer preference for takeoff static margin.
    pub model_cg_target_static_margin: f64,
    /// Whether analyzed-takeoff static margin reaches the preference.
    pub model_cg_target_preference_met: bool,
    /// Typed result of the manufacturer public planning-envelope comparison.
    ///
    /// `NotEvaluated` does not imply compliance; the aircraft WBM remains the
    /// operational authority for a real aircraft.
    pub public_planning_cg_status: PlanningCgStatus,
    /// Analyzed CG normalized by the built aerodynamic model's MAC frame.
    pub model_cg_pct_mac: f64,
    /// Analyzed CG normalized by the registered manufacturer planning frame.
    pub public_planning_cg_pct_mac: Option<f64>,
    /// Residual pitching-moment coefficient from a finite cruise trim result.
    pub trim_cm_residual: Option<f64>,
    /// Whether every segment of the selected interactive route converged.
    pub mission_converged: bool,
    /// Whether the selected interactive route fits the modeled available fuel.
    pub mission_fuel_within_available: bool,
    /// Solved cruise force-balance record from the selected interactive route.
    ///
    /// This is route telemetry, not design-mission evidence.
    pub cruise_equilibrium: Option<CruiseEquilibriumAssessment>,
    /// Provenance of the usable tank capacity applied to this load case.
    pub fuel_capacity_evidence: FuelCapacityEvidence,
    /// Evidence-aware design-mission verdict for this aircraft preset.
    pub design_mission_status: PresetDesignMissionStatus,
    /// Whether the built reference area respects the preset's configured cap.
    pub wing_area_within_limit: bool,
    /// End-to-end execution and finite-output verdict.
    pub execution_passed: bool,
    /// Physical-feasibility verdict from the checks currently implemented.
    pub physical_passed: bool,
    /// Typed physical findings published by the product pipeline.
    pub physical_findings: Vec<PhysicalFinding>,
    /// Detailed model outputs retained for public-data correlation work.
    pub model_audit: PresetModelAudit,
}

/// Aggregated acceptance matrix results across all aircraft presets.
#[derive(Debug, Clone)]
pub struct AcceptanceMatrixReport {
    /// Results for each evaluated aircraft preset.
    pub presets: Vec<PresetAcceptanceResult>,
    /// Whether every preset completed the execution checks.
    pub all_executed: bool,
    /// Whether execution and physical validity both passed for every preset.
    pub all_passed: bool,
    /// Whether every preset passed the implemented physical checks.
    pub all_physical_passed: bool,
    /// Whether every preset has a source-backed, evaluated design mission.
    pub all_design_missions_verified: bool,
}

/// Run the acceptance suite across every registered aircraft preset.
pub fn run_acceptance_matrix() -> AcceptanceMatrixReport {
    let results = preset_runs::evaluate_presets(&presets::available());

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
    use alas_pipeline::FindingSeverity;

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
