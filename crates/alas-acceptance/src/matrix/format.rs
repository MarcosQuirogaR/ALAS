// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Terminal rendering for the acceptance matrix.

use alas_config::{
    AircraftReferenceData, MissingDesignMissionDatum, PartialMissionEvidenceKind, PublishedRange,
};

use super::*;

/// Formats the acceptance matrix as a tabular terminal report.
pub fn format_matrix_report(matrix: &AcceptanceMatrixReport) -> String {
    let mut out = String::new();
    out.push_str(
        "================================================================================================================================================\n",
    );
    out.push_str(
        "                                      ALAS ACCEPTANCE MATRIX REPORT                                         \n",
    );
    out.push_str(
        "================================================================================================================================================\n",
    );
    out.push_str(&format!(
        "{:<10} | {:>9} | {:>9} | {:>9} | {:>9} | {:>9} | {:>9} | {:>7} | {:>7} | {:>8} | {:>10} | {:>4} | {:>7}\n",
        "Preset",
        "MTOW kg",
        "OEW kg",
        "Closure",
        "Carried",
        "Tank kg",
        "TOW kg",
        "L/D",
        "SM (%)",
        "Model CG",
        "Planning CG",
        "Exec",
        "Physical"
    ));
    out.push_str(
        "-----------+-----------+-----------+-----------+-----------+-----------+-----------+---------+---------+----------+------------+------+---------\n",
    );

    for preset in &matrix.presets {
        let status = if preset.physical_passed {
            "PASS"
        } else {
            "FINDING"
        };
        out.push_str(&format!(
            "{:<10} | {:>9.0} | {:>9.0} | {:>9.0} | {:>9.0} | {:>9.0} | {:>9.0} | {:>7.2} | {:>6.1}% | {:>8} | {:>10} | {:>4} | {:>7}\n",
            preset.name,
            preset.mtow_kg,
            preset.oew_kg,
            preset.mtow_closure_fuel_kg,
            preset.analyzed_carried_fuel_kg,
            preset.usable_fuel_capacity_kg.unwrap_or(f64::NAN),
            preset.analyzed_takeoff_mass_kg,
            preset.cruise_l_over_d,
            preset.model_cg_analyzed_takeoff_static_margin * 100.0,
            if preset.model_cg_envelope_ok { "OK" } else { "FAIL" },
            format_planning_cg_status(preset.public_planning_cg_status),
            if preset.execution_passed { "OK" } else { "FAIL" },
            status
        ));
    }
    out.push_str(
        "\nModel CG assessment (preliminary design model, separate from public planning evidence):\n",
    );
    for preset in &matrix.presets {
        let preference = if preset.model_cg_target_preference_met {
            "at/above preference"
        } else {
            "below preference"
        };
        out.push_str(&format!(
            "- {}: hard constraints {}; minimum loading-state SM {:.3}% vs hard floor {:.3}%; analyzed-takeoff SM {:.3}% vs target preference {:.3}% ({preference})\n",
            preset.name,
            if preset.model_cg_envelope_ok {
                "PASS"
            } else {
                "FAIL"
            },
            preset.model_cg_minimum_loading_static_margin * 100.0,
            preset.model_cg_static_margin_floor * 100.0,
            preset.model_cg_analyzed_takeoff_static_margin * 100.0,
            preset.model_cg_target_static_margin * 100.0,
        ));
    }
    out.push_str("\nCruise force-balance telemetry (selected interactive route; not design-mission validation):\n");
    for preset in &matrix.presets {
        let detail = preset.cruise_equilibrium.as_ref().map_or_else(
            || "NOT EVALUATED".to_owned(),
            |assessment| {
                if !assessment.is_finite() {
                    return "INVALID FORCE RECORD".to_owned();
                }
                let convergence = if assessment.all_segments_converged {
                    "SOLVER CONVERGED"
                } else {
                    "SOLVER NOT CONVERGED"
                };
                format!(
                    "{convergence}; {} point(s), |T-D| {:.1} N, |L-W| {:.1} N, inertial |Fx|/|Fz| {:.1}/{:.1} N, |a| {:.3e} m/s^2",
                    assessment.control_points,
                    assessment.max_abs_thrust_minus_drag_n.unwrap_or(f64::NAN),
                    assessment.max_abs_lift_minus_weight_n.unwrap_or(f64::NAN),
                    assessment.max_abs_longitudinal_residual_n.unwrap_or(f64::NAN),
                    assessment.max_abs_vertical_residual_n.unwrap_or(f64::NAN),
                    assessment.max_abs_residual_acceleration_m_s2.unwrap_or(f64::NAN),
                )
            },
        );
        out.push_str(&format!("- {}: {detail}\n", preset.name));
    }
    out.push_str("\nTank-limited load cases:\n");
    let mut tank_limited_count = 0;
    for preset in &matrix.presets {
        if preset.mtow_shortfall_kg.is_finite() && preset.mtow_shortfall_kg > 0.0 {
            tank_limited_count += 1;
            // The shortfall describes the tank bound, so the line names that
            // bound explicitly; the flown takeoff mass can sit well below it
            // when the fuel policy, not the tanks, sized the load.
            out.push_str(&format!(
                "- {}: the tanks admit a takeoff mass of {:.3} kg, {:.3} kg below the {:.3} kg MTOW limit; flown TOW {:.3} kg, carried fuel {:.3} kg, closure remainder {:.3} kg, capacity evidence {}\n",
                preset.name,
                preset.mtow_kg - preset.mtow_shortfall_kg,
                preset.mtow_shortfall_kg,
                preset.mtow_kg,
                preset.analyzed_takeoff_mass_kg,
                preset.analyzed_carried_fuel_kg,
                preset.mtow_closure_fuel_kg,
                format_fuel_capacity_evidence(preset.fuel_capacity_evidence),
            ));
        }
    }
    if tank_limited_count == 0 {
        out.push_str("- None\n");
    }
    out.push_str("\nFindings:\n");
    for preset in matrix
        .presets
        .iter()
        .filter(|preset| !preset.physical_passed)
    {
        let reasons = preset
            .physical_findings
            .iter()
            .filter(|finding| finding_governs_preset(finding))
            .map(format_finding)
            .collect::<Vec<_>>();
        let detail = if reasons.is_empty() {
            "evaluation did not produce a physical report".to_owned()
        } else {
            reasons.join("; ")
        };
        out.push_str(&format!("- {}: {}\n", preset.name, detail));
    }
    out.push_str("\nDesign mission evidence:\n");
    for preset in &matrix.presets {
        out.push_str(&format!(
            "- {}: {}\n",
            preset.name,
            format_design_mission_status(preset.design_mission_status)
        ));
    }
    out.push_str(
        "Partial mission evidence (source material only; never a design-mission verdict):\n",
    );
    for preset in &matrix.presets {
        let detail = presets::get(&preset.name)
            .ok()
            .map(|registered| format_partial_mission_evidence(&registered.reference))
            .unwrap_or_else(|| "preset source record unavailable".to_owned());
        out.push_str(&format!("- {}: {detail}\n", preset.name));
    }
    out.push_str("\nInteractive route diagnostics (not preset design-mission validation):\n");
    for preset in &matrix.presets {
        let route_findings = preset
            .physical_findings
            .iter()
            .filter(|finding| is_mission_finding(finding.code))
            .map(format_finding)
            .collect::<Vec<_>>();
        if !route_findings.is_empty() {
            out.push_str(&format!(
                "- {}: {}\n",
                preset.name,
                route_findings.join("; ")
            ));
        }
    }
    out.push_str(
        "================================================================================================================================================\n",
    );
    out.push_str(&format!(
        "Execution Verdict: {}\n",
        if matrix.all_executed {
            "ALL PRESETS EXECUTED"
        } else {
            "SOME PRESETS FAILED TO EXECUTE"
        }
    ));
    let physical_findings = matrix
        .presets
        .iter()
        .filter(|preset| !preset.physical_passed)
        .count();
    if matrix.all_physical_passed {
        out.push_str("Physical Verdict: ALL PRESETS PASS ROUTE-INDEPENDENT IMPLEMENTED CHECKS\n");
    } else {
        out.push_str(&format!(
            "Physical Verdict: {physical_findings} preset finding(s) require investigation\n"
        ));
    }
    let acceptance_verdict = if matrix.all_passed {
        "PASSED"
    } else if matrix.all_executed
        && matrix.all_physical_passed
        && !matrix.all_design_missions_verified
    {
        "INCOMPLETE - DESIGN MISSIONS UNVERIFIED"
    } else if !matrix.all_design_missions_verified {
        "NOT PASSED - DESIGN MISSIONS ALSO UNVERIFIED"
    } else {
        "NOT PASSED"
    };
    out.push_str(&format!("Acceptance Verdict: {acceptance_verdict}\n"));
    out
}

/// Formats the acceptance matrix as a first-hand machine-readable artifact.
pub fn format_matrix_json(matrix: &AcceptanceMatrixReport) -> serde_json::Result<String> {
    let presets = matrix
        .presets
        .iter()
        .map(|preset| {
            let cruise_equilibrium = preset.cruise_equilibrium.as_ref().map_or_else(
                || serde_json::json!({ "status": "not_evaluated" }),
                |assessment| {
                    serde_json::json!({
                        "status": if assessment.is_finite() { "finite" } else { "invalid" },
                        "all_segments_converged": assessment.all_segments_converged,
                        "control_points": assessment.control_points,
                        "max_abs_thrust_minus_drag_n": assessment.max_abs_thrust_minus_drag_n,
                        "max_abs_lift_minus_weight_n": assessment.max_abs_lift_minus_weight_n,
                        "max_abs_longitudinal_residual_n": assessment.max_abs_longitudinal_residual_n,
                        "max_abs_vertical_residual_n": assessment.max_abs_vertical_residual_n,
                        "max_abs_residual_acceleration_m_s2": assessment
                            .max_abs_residual_acceleration_m_s2,
                    })
                },
            );
            let findings = preset
                .physical_findings
                .iter()
                .map(|finding| {
                    serde_json::json!({
                        "code": format!("{:?}", finding.code),
                        "severity": format!("{:?}", finding.severity),
                        "message": finding.message,
                        "actual": finding.actual,
                        "limit": finding.limit,
                        "unit": finding.unit,
                    })
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "name": preset.name,
                "execution_passed": preset.execution_passed,
                "physical_passed": preset.physical_passed,
                "design_mission_status": format_design_mission_status(preset.design_mission_status),
                "mass": {
                    "mtow_kg": preset.mtow_kg,
                    "oew_kg": preset.oew_kg,
                    "payload_kg": preset.payload_kg,
                },
                "fuel_loading": {
                    "mtow_closure_fuel_kg": preset.mtow_closure_fuel_kg,
                    "usable_capacity_kg": preset.usable_fuel_capacity_kg,
                    "usable_capacity_evidence": format_fuel_capacity_evidence(
                        preset.fuel_capacity_evidence,
                    ),
                    "analyzed_carried_fuel_kg": preset.analyzed_carried_fuel_kg,
                    "analyzed_takeoff_mass_kg": preset.analyzed_takeoff_mass_kg,
                    "mtow_shortfall_kg": preset.mtow_shortfall_kg,
                },
                "cruise_equilibrium": cruise_equilibrium,
                "model_audit": preset.model_audit,
                "findings": findings,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string_pretty(&serde_json::json!({
        "status": "preliminary_model_evidence_not_afm_wbm_limits",
        "generated_by": "cargo run --profile test -p alas-acceptance --bin preset_audit -- --output-dir <directory>",
        "scope": "All routes are interactive diagnostics. No preset has a source-backed complete design mission, and AFM/WBM remains controlling where public planning data are absent.",
        "verdicts": {
            "all_executed": matrix.all_executed,
            "all_physical_passed": matrix.all_physical_passed,
            "all_design_missions_verified": matrix.all_design_missions_verified,
            "all_passed": matrix.all_passed,
        },
        "presets": presets,
    }))
}

fn format_fuel_capacity_evidence(evidence: alas_pipeline::FuelCapacityEvidence) -> &'static str {
    match evidence {
        alas_pipeline::FuelCapacityEvidence::PublishedPreset => "published preset",
        alas_pipeline::FuelCapacityEvidence::GeometryEstimate => "geometry estimate",
        alas_pipeline::FuelCapacityEvidence::Unavailable => "unavailable",
    }
}

fn format_partial_mission_evidence(reference: &AircraftReferenceData) -> String {
    if reference.partial_design_mission_evidence.is_empty() {
        return "none registered; complete range, payload, profile, and reserve evidence is absent"
            .to_owned();
    }

    reference
        .partial_design_mission_evidence
        .iter()
        .map(|evidence| {
            let missing = evidence
                .missing
                .iter()
                .map(format_missing_mission_datum)
                .collect::<Vec<_>>()
                .join(", ");
            let range = evidence
                .range
                .map(format_published_range)
                .unwrap_or_else(|| "range not stated".to_owned());
            let payload = evidence
                .payload_kg
                .map(|value| format!("payload {value:.0} kg"))
                .unwrap_or_else(|| "payload not stated".to_owned());
            format!(
                "{}; {range}; {payload}; missing {missing}; source {}",
                format_partial_mission_kind(evidence.kind),
                evidence.source
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn format_partial_mission_kind(kind: PartialMissionEvidenceKind) -> &'static str {
    match kind {
        PartialMissionEvidenceKind::AdvertisedRange => "advertised range",
        PartialMissionEvidenceKind::PayloadRangeChart => "payload-range chart",
        PartialMissionEvidenceKind::CertificationDemonstration => "certification demonstration",
        PartialMissionEvidenceKind::ActualDesignMission => "actual design mission",
    }
}

fn format_missing_mission_datum(datum: &MissingDesignMissionDatum) -> &'static str {
    match datum {
        MissingDesignMissionDatum::Range => "range",
        MissingDesignMissionDatum::Payload => "payload",
        MissingDesignMissionDatum::Profile => "profile",
        MissingDesignMissionDatum::ReserveFuel => "reserve fuel",
    }
}

fn format_published_range(range: PublishedRange) -> String {
    match range {
        PublishedRange::NauticalMiles(value) => format!("range {value:.0} nmi"),
        PublishedRange::Kilometres(value) => format!("range {value:.0} km"),
    }
}
