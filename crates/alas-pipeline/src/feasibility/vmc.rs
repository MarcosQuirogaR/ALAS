// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Minimum control speed and crosswind rudder findings of a completed run.
//!
//! The yaw balance, its sources and the way the computed `VMC` enters the
//! take-off speed rule are in `alas_opt::mdo::residuals_vmc` (the optimizer
//! residual and this finding share one evaluation). A registered aircraft is
//! certified, so a computed value above the CS 25.149(c) ceiling is a warning
//! about the fin model; for a clean-sheet design it is an error.

use alas_opt::mdo::{VmcFailure, VmcOutcome};
use alas_perf::performance::yaw_balance::{
    MINIMUM_DIRECTIONAL_STABILITY_PER_RAD, VMC_CEILING_OVER_VSR,
};

use super::{error, warning, FindingCode, PhysicalFinding};

/// Append the ceiling and crosswind findings.
pub(super) fn append(outcome: &VmcOutcome, findings: &mut Vec<PhysicalFinding>) {
    let finding = |code, message: String, actual, limit, unit| {
        if outcome.registered {
            warning(code, message, actual, limit, unit)
        } else {
            error(code, message, actual, limit, unit)
        }
    };
    let ceiling = outcome
        .at_sea_level
        .as_ref()
        .ok()
        .or(outcome.at_airport.as_ref().ok());
    match (ceiling, &outcome.at_sea_level) {
        (Some(assessment), _) if assessment.vmc_over_vsr > VMC_CEILING_OVER_VSR => {
            findings.push(finding(
                FindingCode::MinimumControlSpeedViolation,
                format!(
                    "computed VMCA {:.1} m/s is {:.3} VSR, above the CS 25.149(c) ceiling of {:.2} VSR (rudder at its stop, bank {:.1} deg, sideslip {:.1} deg); the fin and rudder cannot hold the engine-out yawing moment at that speed",
                    assessment.vmca_m_s,
                    assessment.vmc_over_vsr,
                    VMC_CEILING_OVER_VSR,
                    assessment.bank_deg,
                    assessment.sideslip_deg,
                ),
                Some(assessment.vmc_over_vsr),
                Some(VMC_CEILING_OVER_VSR),
                "VMC/VSR",
            ));
        }
        (None, Err(VmcFailure::NotControllable)) => findings.push(finding(
            FindingCode::MinimumControlSpeedViolation,
            "the fin and rudder cannot hold the engine-out yawing moment at any speed".to_owned(),
            None,
            Some(VMC_CEILING_OVER_VSR),
            "VMC/VSR",
        )),
        (None, Err(VmcFailure::Unavailable(reason))) => findings.push(warning(
            FindingCode::MinimumControlSpeedUnavailable,
            format!("minimum control speed was not evaluated, the declared VMC/VSR stays in the speed rule: {reason}"),
            None,
            None,
            "VMC/VSR",
        )),
        _ => {}
    }
    if let Some(check) = outcome.crosswind {
        if check.cn_beta_per_rad.is_finite()
            && check.cn_beta_per_rad < MINIMUM_DIRECTIONAL_STABILITY_PER_RAD
        {
            findings.push(finding(
                FindingCode::DirectionalStabilityViolation,
                format!(
                    "weathercock stability Cn_beta of the fuselage and fin is {:.3} per rad at the approach speed, below the {:.2} per rad minimum: the aircraft is directionally unstable (CS 25.177; Sadraey p. 282; Torenbeek Eq. 9-62)",
                    check.cn_beta_per_rad, MINIMUM_DIRECTIONAL_STABILITY_PER_RAD,
                ),
                Some(check.cn_beta_per_rad),
                Some(MINIMUM_DIRECTIONAL_STABILITY_PER_RAD),
                "1/rad",
            ));
        }
        if check.rudder_deg > check.rudder_limit_deg {
            findings.push(finding(
                FindingCode::CrosswindControlViolation,
                format!(
                    "holding the sideslip of the design crosswind ({:.1} deg at the {:.1} m/s approach speed) needs {:.1} deg of rudder, above the {:.1} deg limit (Torenbeek Eq. 9-68)",
                    check.sideslip_deg, check.approach_speed_m_s, check.rudder_deg, check.rudder_limit_deg,
                ),
                Some(check.rudder_deg),
                Some(check.rudder_limit_deg),
                "deg",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feasibility::FindingSeverity;
    use crate::report_config::evaluate_report_vmc as evaluate;
    use alas_config::AlasConfig;
    use alas_opt::mdo::{CrosswindAssessment, VmcAssessment};
    use alas_perf::performance::yaw_balance::ControlLimit;

    fn assessment(ratio: f64) -> VmcAssessment {
        VmcAssessment {
            vmca_m_s: 70.0 * ratio,
            vsr_m_s: 70.0,
            vmc_over_vsr: ratio,
            vmcg_m_s: Some(72.0),
            rudder_deg: 30.0,
            sideslip_deg: 2.0,
            bank_deg: 3.0,
            limit: ControlLimit::Rudder,
            asymmetric_force_n: 1.0e5,
            fin_area_ratio_for_ceiling: Some(1.3),
        }
    }

    fn outcome(
        at_sea_level: Result<VmcAssessment, VmcFailure>,
        crosswind: Option<CrosswindAssessment>,
        registered: bool,
    ) -> VmcOutcome {
        VmcOutcome {
            at_airport: at_sea_level.clone(),
            at_sea_level,
            crosswind,
            rule_ratio: None,
            capped_to_declared: false,
            registered,
        }
    }

    #[test]
    fn a_value_above_the_ceiling_is_a_warning_for_a_registered_aircraft_and_an_error_otherwise() {
        for (registered, severity) in [
            (true, FindingSeverity::Warning),
            (false, FindingSeverity::Error),
        ] {
            let mut findings = Vec::new();
            append(
                &outcome(Ok(assessment(1.2)), None, registered),
                &mut findings,
            );
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].code, FindingCode::MinimumControlSpeedViolation);
            assert_eq!(findings[0].severity, severity);
            assert_eq!(findings[0].limit, Some(1.13));
        }
        let mut findings = Vec::new();
        append(&outcome(Ok(assessment(1.0)), None, false), &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn an_uncontrollable_fin_is_a_violation_and_a_missing_input_is_only_a_warning() {
        let mut findings = Vec::new();
        append(
            &outcome(Err(VmcFailure::NotControllable), None, false),
            &mut findings,
        );
        assert_eq!(findings[0].severity, FindingSeverity::Error);
        let mut findings = Vec::new();
        append(
            &outcome(
                Err(VmcFailure::Unavailable("no fin".to_owned())),
                None,
                false,
            ),
            &mut findings,
        );
        assert_eq!(
            findings[0].code,
            FindingCode::MinimumControlSpeedUnavailable
        );
        assert_eq!(findings[0].severity, FindingSeverity::Warning);
    }

    #[test]
    fn the_crosswind_finding_fires_only_above_the_rudder_limit() {
        let check = |rudder_deg| CrosswindAssessment {
            approach_speed_m_s: 70.0,
            sideslip_deg: 12.0,
            rudder_deg,
            rudder_limit_deg: 30.0,
            cn_beta_per_rad: 0.3,
        };
        let mut findings = Vec::new();
        append(
            &outcome(Ok(assessment(1.0)), Some(check(20.0)), false),
            &mut findings,
        );
        assert!(findings.is_empty());
        append(
            &outcome(Ok(assessment(1.0)), Some(check(35.0)), false),
            &mut findings,
        );
        assert_eq!(findings[0].code, FindingCode::CrosswindControlViolation);
    }

    #[test]
    fn a_registered_a320_report_carries_no_control_finding_and_a_computed_ratio() {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": "A320-200",
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap_or_else(|error| panic!("A320 config: {error}"));
        let design = alas_config::presets::get("A320-200")
            .unwrap_or_else(|error| panic!("A320 preset: {error}"))
            .design_vector;
        let report = crate::full_analysis::FullAnalysis::new(config.clone())
            .run(&design, true)
            .unwrap_or_else(|error| panic!("A320 report: {error}"));
        let mass_kg = config.requirements.mtow_kg;
        let result = evaluate(
            &config,
            &report,
            mass_kg,
            0.9 * mass_kg,
            report.airplane.s_ref,
        );
        let ratio = result
            .at_airport
            .as_ref()
            .map(|assessment| assessment.vmc_over_vsr)
            .unwrap_or_else(|error| panic!("VMC: {error:?}"));
        assert!((0.6..1.13).contains(&ratio), "{ratio}");
        let mut findings = Vec::new();
        append(&result, &mut findings);
        assert!(findings.is_empty(), "{findings:?}");
    }
}
