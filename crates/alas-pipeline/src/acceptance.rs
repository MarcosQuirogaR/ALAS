// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The reporting-fidelity re-evaluation a finalist has to survive before the
//! run may call it a delivered aircraft.
//!
//! The search and the published analysis are two different models of the same
//! aeroplane. The search ranks candidates on the in-loop panel mesh
//! (`analysis.chordwise_resolution`) and closes its own analytic dispatch
//! fixed point; the report re-solves the winner on the finer reported mesh
//! (`analysis.fine_chordwise_resolution`) and flies the route with the native
//! segment mission and the fuel policy. Neither is the other's approximation
//! error: the chordwise convergence is first-order in panel count, so a
//! supercritical wing's trimmed body attitude moves by about the coarse
//! mesh's own error, and the analytic dispatch closure can converge to a
//! takeoff mass the native mission then cannot fly.
//!
//! Without this loop a run can report the search as `converged` while the
//! feasibility stage reports the delivered aircraft INFEASIBLE, two
//! contradictory statements from the same run.
//!
//! What this module does is deliberately narrow. It re-evaluates a candidate
//! exactly the way the application's own stages 3, 5 and 6 do (same
//! analysis, same mission, same feasibility assessment, same limits) and
//! reports which findings rejected it. It relaxes nothing: every hard
//! residual, every limit and every finding stays exactly where it was, and a
//! design that is rejected here is rejected, not adjusted until it passes.

use alas_config::airports::Airport;
use alas_config::{AlasConfig, DesignVector};
use alas_mission::MissionResult;

use crate::feasibility::{
    assess_physical_feasibility_with_load_case, FeasibilityReport, FindingSeverity,
};
use crate::full_analysis::{AnalysisReport, FullAnalysis};
use crate::mission_stage::{self, SelectedLoadCase};

/// How many candidates the acceptance loop may re-evaluate at reporting
/// fidelity, the search's own finalist included.
///
/// One re-evaluation is a full reported analysis plus a flown mission, about
/// one to three seconds on the registered presets: the same order as a
/// handful of search evaluations, and two orders below the search itself. The
/// cap exists so a design space whose whole feasible region fails at
/// reporting fidelity is reported as such promptly rather than walked
/// candidate by candidate: eight is enough to clear a finalist rejected by a
/// mesh-error-sized attitude shift, and small enough that the stage stays
/// inside its runtime budget when nothing can be cleared.
pub const MAX_VERIFIED_CANDIDATES: usize = 8;

/// The route a finalist's mission is flown over.
///
/// Resolved once per run, before the search starts, because it depends on the
/// configuration and not on the design. Passing it in is what lets the
/// acceptance loop fly the same route the pipeline's own mission stage will.
///
/// The two airports are carried resolved rather than read back out of the
/// route. A planned route does not always carry its endpoint records - a
/// great-circle plan for a configured city pair carries the geometry and
/// leaves `origin_airport`/`dest_airport` empty - and the mission stage has
/// always fallen back to the configured airports in that case. Resolving once
/// here, with that same fallback, is what keeps the acceptance check and the
/// published mission on one route instead of letting the check fail on a run
/// the mission flies perfectly well.
#[derive(Debug, Clone, PartialEq)]
pub struct AcceptanceRoute {
    /// Departure field the mission starts from.
    pub origin: Airport,
    /// Arrival field the mission ends at.
    pub destination: Airport,
    /// Planned still-air route distance, m.
    pub distance_m: f64,
}

/// One candidate's reporting-fidelity outcome.
#[derive(Debug, Clone)]
pub struct FinalistVerification {
    /// The candidate that was re-evaluated.
    pub design: DesignVector,
    /// The reported analysis, bound to the candidate's own closed takeoff
    /// mass rather than to the configured ceiling.
    pub report: AnalysisReport,
    /// Flown mission telemetry, when the run has a mission and a route.
    pub mission: Option<MissionResult>,
    /// The load case the mission was flown at.
    pub load_case: Option<SelectedLoadCase>,
    /// The feasibility assessment of the delivered aircraft.
    pub feasibility: FeasibilityReport,
    /// Hard limits this candidate met only through the controlled-relaxation
    /// policy, prefixed `relaxed:`. Empty under the shipped strict policy.
    ///
    /// A relaxed candidate is admissible for the *search*, and it is never
    /// acceptable as a delivered aircraft: the acceptance gate exists to say
    /// whether the application considers the design feasible, and a relaxed
    /// design is by definition one it does not. Carrying the identifiers here
    /// is what keeps that distinction visible instead of silent.
    pub relaxed_limits: Vec<String>,
}

/// Identifiers of the error-severity findings in a feasibility report, in
/// report order.
///
/// Acceptance is the absence of one. Warnings stay warnings: they are
/// diagnostics about the evidence behind a check, not statements that the
/// aircraft cannot fly the case, and promoting them here would reject designs
/// the application itself reports as feasible.
pub fn rejecting_finding_ids(feasibility: &FeasibilityReport) -> Vec<String> {
    feasibility
        .findings
        .iter()
        .filter(|finding| finding.severity == FindingSeverity::Error)
        .map(|finding| finding.code.as_str().to_owned())
        .collect()
}

impl FinalistVerification {
    /// Whether the application accepts this design.
    pub fn accepted(&self) -> bool {
        self.rejected_by().is_empty()
    }

    /// Identifiers of everything that rejects this design: the error-severity
    /// findings, and any limit met only through the relaxation policy.
    pub fn rejected_by(&self) -> Vec<String> {
        let mut rejected = rejecting_finding_ids(&self.feasibility);
        rejected.extend(self.relaxed_limits.iter().cloned());
        rejected
    }

    /// The rejecting findings' own messages, for a caller that must say why.
    pub fn rejection_messages(&self) -> Vec<String> {
        self.feasibility
            .findings
            .iter()
            .filter(|finding| finding.severity == FindingSeverity::Error)
            .map(|finding| finding.message.clone())
            .collect()
    }
}

/// Re-evaluate one candidate at reporting fidelity and assess it.
///
/// The three steps are the application's own: replay the typed candidate
/// assessment to recover the takeoff mass the coupled sizing closed at, run
/// the reported analysis bound to that mass, then fly the mission and assess
/// physical feasibility exactly as stages 5 and 6 do.
///
/// # Errors
///
/// The replay, analysis or mission failure, as a description. A candidate
/// that is not hard-feasible on replay is an error rather than a rejection:
/// the search must not have offered it.
pub fn verify_finalist(
    config: &AlasConfig,
    design: &DesignVector,
    route: Option<&AcceptanceRoute>,
) -> Result<FinalistVerification, String> {
    let assessment = alas_opt::assess_product_candidate(config, design)
        .map_err(|error| format!("finalist replay failed: {error}"))?;
    if !assessment.hard_feasible {
        let violations = assessment.violated_hard_ids().join(", ");
        return Err(format!(
            "finalist is not hard-feasible on replay: {}",
            if violations.is_empty() {
                "unidentified hard residual".to_owned()
            } else {
                violations
            }
        ));
    }
    let relaxed_limits = assessment
        .relaxation
        .relaxed_ids
        .iter()
        .map(|id| format!("relaxed:{id}"))
        .collect();
    // The aircraft the gate above passed, not the vector it was handed: a
    // clean-sheet design space derives the fuselage coordinate from the cabin
    // load case, so a supplied vector that is not a fixed point of that
    // derivation is not the aircraft that was assessed
    // (`alas_opt::ResolvedProductState::design`).
    let report = FullAnalysis::new(config.clone())
        .run_at_sized_takeoff_mass(
            &assessment.resolved.design,
            true,
            assessment.sized.takeoff_mass_kg,
        )
        .map_err(|error| format!("reporting-fidelity analysis failed: {error}"))?;

    let (mission, load_case) = match (config.mission.enabled, route) {
        (true, Some(planned)) => {
            let (mission, load_case) = mission_stage::evaluate(
                config,
                &report,
                &planned.origin,
                &planned.destination,
                planned.distance_m,
            )
            .map_err(|error| format!("reporting-fidelity mission failed: {error}"))?;
            (Some(mission), Some(load_case))
        }
        _ => (None, None),
    };

    let feasibility = assess_physical_feasibility_with_load_case(
        config,
        design,
        &report,
        mission.as_ref(),
        load_case.as_ref(),
    );
    Ok(FinalistVerification {
        design: *design,
        report,
        mission,
        load_case,
        feasibility,
        relaxed_limits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feasibility::{FindingCode, PhysicalFinding};

    fn finding(code: FindingCode, severity: FindingSeverity) -> PhysicalFinding {
        PhysicalFinding {
            code,
            severity,
            message: format!("{} fixture", code.as_str()),
            actual: None,
            limit: None,
            unit: "",
        }
    }

    #[test]
    fn an_error_finding_rejects_and_a_warning_does_not() {
        let mut feasibility = FeasibilityReport::default();
        assert!(rejecting_finding_ids(&feasibility).is_empty());

        feasibility.findings.push(finding(
            FindingCode::FieldPerformanceUnavailable,
            FindingSeverity::Warning,
        ));
        assert!(
            rejecting_finding_ids(&feasibility).is_empty(),
            "a warning is a diagnostic about the evidence, not a rejection"
        );

        feasibility.findings.push(finding(
            FindingCode::InsufficientStaticMargin,
            FindingSeverity::Error,
        ));
        assert_eq!(
            rejecting_finding_ids(&feasibility),
            vec![FindingCode::InsufficientStaticMargin.as_str().to_owned()]
        );
    }

    #[test]
    fn every_rejecting_finding_is_reported_not_only_the_first() {
        let mut feasibility = FeasibilityReport::default();
        feasibility.findings.push(finding(
            FindingCode::ReportedCruiseAttitudeOutsideWindow,
            FindingSeverity::Error,
        ));
        feasibility.findings.push(finding(
            FindingCode::MissionNotConverged,
            FindingSeverity::Error,
        ));
        assert_eq!(rejecting_finding_ids(&feasibility).len(), 2);
    }
}
