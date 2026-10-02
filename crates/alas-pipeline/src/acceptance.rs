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
/// fidelity, the search's own finalist included. The refinement budget
/// reserves evaluations and time for them (`alas_opt::verification_reserve`),
/// so the loop runs inside the declared budget rather than after it.
pub use alas_opt::MAX_VERIFIED_CANDIDATES;

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
    /// The violated hard relative balance guards of the reported analysis
    /// (`usable_cg_range_vs_nominal`, `tail_scrape_vs_nominal`, or
    /// `relative_balance_nominal_unavailable`), measured against the
    /// registered aircraft at the same reporting fidelity
    /// ([`ReportingNominal`]). Empty when no guard applies.
    pub relative_balance: Vec<alas_opt::ConstraintResidual>,
}

/// The registered aircraft a reference adaptation redesigns, re-evaluated
/// once per run at reporting fidelity, exactly as a finalist is.
///
/// The in-loop relative balance guard compares each candidate with the
/// registered aircraft on the in-loop mesh. The reported mesh moves the
/// neutral point, and with it the usable CG range, of both aircraft; a
/// finalist is therefore held to the same `min(requirement, nominal)` limit
/// against the nominal on the reported mesh, never against the in-loop one.
/// The evaluation also serves as the reporting baseline when the run's
/// baseline is the registered design vector, so it costs no extra analysis
/// in that case.
pub struct ReportingNominal {
    design: DesignVector,
    evaluation: Result<(FinalistVerification, Vec<&'static str>), String>,
}

impl ReportingNominal {
    /// The nominal of `config` at reporting fidelity, or `None` when no
    /// finalist can be rejected by the guard: outside a reference adaptation
    /// or under a Balance policy that is not hard.
    ///
    /// # Errors
    ///
    /// Only a cancellation, as the replay's own message. An analysis failure
    /// is kept as an unavailable nominal, which fails the hard guard.
    pub fn evaluate(
        config: &AlasConfig,
        route: Option<&AcceptanceRoute>,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Option<Result<Self, String>> {
        if config.optimizer.design_space.mode != alas_config::DesignMode::ReferenceAdaptation
            || config.optimizer.objective.balance_constraints != alas_config::ConstraintPolicy::Hard
        {
            return None;
        }
        let design = match alas_config::presets::get(&config.preset) {
            Ok(preset) => preset.design_vector,
            Err(error) => {
                return Some(Ok(Self {
                    design: DesignVector::default(),
                    evaluation: Err(format!("registered design: {error}")),
                }))
            }
        };
        let evaluation = reporting_fidelity(config, &design, route, cancel, false);
        match evaluation {
            Err(error) if error.starts_with("Cancelled safely") => Some(Err(error)),
            evaluation => {
                if let Err(error) = &evaluation {
                    tracing::warn!(%error, "the registered aircraft could not be re-evaluated at reporting fidelity");
                }
                Some(Ok(Self { design, evaluation }))
            }
        }
    }

    /// The registered design vector that was evaluated.
    pub fn design(&self) -> &DesignVector {
        &self.design
    }

    /// The nominal's model CG envelope on the reported analysis, `None` when
    /// it could not be evaluated.
    fn model_cg(&self) -> Option<&alas_opt::ModelCgEnvelopeAssessment> {
        self.evaluation
            .as_ref()
            .ok()
            .and_then(|(verification, _)| verification.feasibility.model_cg.as_ref())
    }
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
        rejected.extend(
            self.relative_balance
                .iter()
                .map(|r| format!("hard:{}", r.id)),
        );
        rejected
    }

    /// The rejecting findings' own messages, for a caller that must say why.
    pub fn rejection_messages(&self) -> Vec<String> {
        let findings = self
            .feasibility
            .findings
            .iter()
            .filter(|finding| finding.severity == FindingSeverity::Error)
            .map(|finding| finding.message.clone());
        let guards = self.relative_balance.iter().map(|r| {
            format!(
                "{} at reporting fidelity: {:.3} {} against the limit {:.3} {}, the lower of \
                 the requirement and the registered aircraft on the same mesh",
                r.id, r.actual, r.unit, r.limit, r.unit
            )
        });
        findings.chain(guards).collect()
    }
}

/// Re-evaluate one candidate at reporting fidelity and assess it.
///
/// The three steps are the application's own: replay the typed candidate
/// assessment to recover the takeoff mass the coupled sizing closed at, run
/// the reported analysis bound to that mass, then fly the mission and assess
/// physical feasibility exactly as stages 5 and 6 do, then hold the reported
/// analysis to the relative balance guard against `nominal`, the registered
/// aircraft at the same fidelity, when one applies. The coupled replay
/// observes `cancel`.
///
/// # Errors
///
/// The replay, analysis or mission failure, as a description. A candidate
/// that is not hard-feasible on replay is an error rather than a rejection:
/// the search must not have offered it.
pub fn verify_finalist_cancellable(
    config: &AlasConfig,
    design: &DesignVector,
    route: Option<&AcceptanceRoute>,
    nominal: Option<&ReportingNominal>,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<FinalistVerification, String> {
    let (mut verification, _) = reporting_fidelity(config, design, route, cancel, true)?;
    hold_to_nominal(&mut verification, config, nominal);
    Ok(verification)
}

/// Record the violated hard relative balance guards of `verification`'s
/// reported analysis against `nominal`. A candidate without a model CG
/// assessment already carries the error finding that rejects it.
fn hold_to_nominal(
    verification: &mut FinalistVerification,
    config: &AlasConfig,
    nominal: Option<&ReportingNominal>,
) {
    let (Some(nominal), Some(candidate)) = (nominal, verification.feasibility.model_cg.as_ref())
    else {
        return;
    };
    verification.relative_balance =
        alas_opt::reporting_relative_balance(candidate, nominal.model_cg(), config)
            .into_iter()
            .filter(|r| r.policy == alas_config::ConstraintPolicy::Hard && r.violated())
            .collect();
}

/// Re-evaluate the unmodified `baseline` exactly as a finalist is and
/// compare its native-mission trip fuel (completed missions only) with
/// `delivered`'s: the same-model delta of what the search delivered. The
/// baseline need not be feasible; its replayed hard violations join its
/// rejecting findings. `None` when the baseline cannot be analysed at all.
/// A `nominal` evaluated on the same vector is reused rather than re-run.
pub fn compare_with_baseline(
    config: &AlasConfig,
    baseline: &DesignVector,
    delivered: &FinalistVerification,
    route: Option<&AcceptanceRoute>,
    nominal: Option<&ReportingNominal>,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Option<alas_opt::ReportingBaseline> {
    let trip_fuel = |verification: &FinalistVerification| {
        verification
            .mission
            .as_ref()
            .and_then(alas_mission::MissionResult::completed_summary)
            .map(|summary| summary.trip_fuel_kg)
    };
    let evaluated;
    let evaluation = match nominal.filter(|nominal| nominal.design == *baseline) {
        Some(nominal) => nominal.evaluation.as_ref().map(|(v, ids)| (v, ids.clone())),
        None => {
            evaluated = reporting_fidelity(config, baseline, route, cancel, false);
            evaluated.as_ref().map(|(v, ids)| (v, ids.clone()))
        }
    };
    match evaluation {
        Ok((verification, violated)) => {
            let mut rejected_by = verification.rejected_by();
            rejected_by.extend(violated.into_iter().map(|id| format!("hard:{id}")));
            Some(alas_opt::ReportingBaseline::new(
                rejected_by.is_empty(),
                rejected_by,
                trip_fuel(verification),
                trip_fuel(delivered),
            ))
        }
        Err(error) => {
            tracing::warn!(%error, "the baseline could not be re-evaluated at reporting fidelity");
            None
        }
    }
}

/// Write the aircraft a candidate assessment evaluated into `config` and
/// return its design vector.
///
/// The assessed aircraft is not the vector a caller supplied: a clean-sheet
/// design space derives the fuselage coordinate from the cabin load case, and
/// a reference adaptation solves the tail scales that hold the registered
/// tail volume coefficients (`ResolvedProductState::design` and
/// `::tail_sizing`). The fin scale lives in the empennage configuration, not
/// in the vector, so a rebuild from the vector alone draws the unsized fin.
/// Every reporting-fidelity rebuild goes through here so the report, the
/// mission and the feasibility verdict describe the aircraft the search
/// scored.
pub(crate) fn apply_assessed_aircraft(
    resolved: &alas_opt::ResolvedProductState,
    config: &mut AlasConfig,
) -> DesignVector {
    let mut design = resolved.design;
    resolved
        .tail_sizing
        .apply_to(&mut config.geometry.empennage, &mut design);
    design
}

/// The reporting-fidelity re-evaluation, and the hard residuals the replay
/// found violated. With `require_hard_feasible` a violation is an error.
fn reporting_fidelity(
    config: &AlasConfig,
    design: &DesignVector,
    route: Option<&AcceptanceRoute>,
    cancel: Option<&std::sync::atomic::AtomicBool>,
    require_hard_feasible: bool,
) -> Result<(FinalistVerification, Vec<&'static str>), String> {
    let assessment = alas_opt::assess_product_candidate_cancellable(config, design, cancel)
        .map_err(|error| {
            if error == "cancelled" {
                "Cancelled safely: finalist replay interrupted".to_owned()
            } else {
                format!("finalist replay failed: {error}")
            }
        })?;
    let violated = assessment.violated_hard_ids();
    if require_hard_feasible && !assessment.hard_feasible {
        let violations = violated.join(", ");
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
    // The aircraft the gate above passed, not the vector it was handed: the
    // resolved design (a clean-sheet space derives the fuselage from the
    // cabin) with its solved tail sizing, so the report, mission and
    // feasibility all describe the assessed aircraft.
    let mut sized_config = config.clone();
    let sized_design = apply_assessed_aircraft(&assessment.resolved, &mut sized_config);
    let report = FullAnalysis::new(sized_config.clone())
        .run_sized_candidate(&sized_design, &assessment.sized)
        .map_err(|error| format!("reporting-fidelity analysis failed: {error}"))?;

    let (mission, load_case) = match (sized_config.mission.enabled, route) {
        (true, Some(planned)) => {
            let (mission, load_case) = mission_stage::evaluate(
                &sized_config,
                &report,
                &planned.origin,
                &planned.destination,
                planned.distance_m,
            )
            .map_err(|error| format!("reporting-fidelity mission failed: {error}"))?;
            (mission, Some(load_case))
        }
        _ => (None, None),
    };

    let feasibility = assess_physical_feasibility_with_load_case(
        &sized_config,
        &sized_design,
        &report,
        mission.as_ref(),
        load_case.as_ref(),
    );
    Ok((
        FinalistVerification {
            design: *design,
            report,
            mission,
            load_case,
            feasibility,
            relaxed_limits,
            relative_balance: Vec::new(),
        },
        violated,
    ))
}

// A test asserts on values it built here, so a failed expect is the
// assertion failing rather than a library invariant being broken.
#[allow(clippy::expect_used)]
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

    /// `(V_H, V_V)` of a built aircraft.
    fn volumes(plane: &alas_geom::aircraft::airplane::Airplane) -> (f64, f64) {
        match alas_stab::trim::tail_volume_coefficients(plane) {
            (Some(vh), Some(vv)) => (vh, vv),
            other => panic!("tails missing: {other:?}"),
        }
    }

    #[test]
    fn the_verified_aircraft_carries_the_solved_tail_of_a_moved_wing() {
        let preset = "A320-200";
        let registered = alas_config::presets::get(preset).expect("registered preset");
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": preset,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .expect("valid configuration");
        let nominal_plane =
            alas_geom::builder::AircraftBuilder::new(Some(registered.geometry.clone()))
                .build(Some(&registered.design_vector), false)
                .expect("nominal builds");
        let (nominal_h, nominal_v) = volumes(&nominal_plane);

        let mut moved = registered.design_vector;
        moved.span_m *= 0.95;
        for chord in [
            &mut moved.root_chord_m,
            &mut moved.break_chord_m,
            &mut moved.tip_chord_m,
        ] {
            *chord *= 1.07;
        }
        let (verification, _) =
            reporting_fidelity(&config, &moved, None, None, false).expect("analysable");
        let verified = &verification.report.airplane;

        let assessment =
            alas_opt::assess_product_candidate(&config, &moved).expect("assessable candidate");
        let mut assessed_config = config.clone();
        let assessed_design = apply_assessed_aircraft(&assessment.resolved, &mut assessed_config);
        let assessed = alas_geom::builder::AircraftBuilder::new(Some(assessed_config.geometry))
            .build(Some(&assessed_design), false)
            .expect("assessed aircraft builds");
        let fin_area =
            |plane: &alas_geom::aircraft::airplane::Airplane| plane.wings[2].unfolded_area();
        assert!((fin_area(verified) / fin_area(&assessed) - 1.0).abs() < 1e-9);

        let (vh, vv) = volumes(verified);
        assert!(
            (vh / nominal_h - 1.0).abs() < 1e-6,
            "V_H {vh} vs {nominal_h}"
        );
        assert!(
            (vv / nominal_v - 1.0).abs() < 1e-6,
            "V_V {vv} vs {nominal_v}"
        );

        // The same vector without the solved fin ratio is a different fin:
        // the property above is not satisfied by construction.
        let unsized_fin = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&assessment.resolved.design), false)
            .expect("unsized aircraft builds");
        assert!((volumes(&unsized_fin).1 / nominal_v - 1.0).abs() > 1e-3);
    }

    /// The reporting-fidelity guard holds a finalist to the registered
    /// aircraft on the reported mesh: the registered aircraft itself passes,
    /// a finalist whose reported usable CG range lies below the reported
    /// nominal is rejected (and the ladder moves on), and an unavailable
    /// nominal rejects under the hard Balance policy.
    #[test]
    fn a_finalist_below_the_reporting_fidelity_nominal_cg_range_is_rejected() {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": "A320-200",
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .expect("valid configuration");
        assert_eq!(
            config.optimizer.objective.balance_constraints,
            alas_config::ConstraintPolicy::Hard
        );
        let nominal = ReportingNominal::evaluate(&config, None, None)
            .expect("a hard reference adaptation has a reporting nominal")
            .expect("not cancelled");
        let (own, _) = nominal.evaluation.as_ref().expect("the nominal analyses");
        let range = |verification: &FinalistVerification| {
            verification
                .feasibility
                .model_cg
                .as_ref()
                .expect("model CG assessed")
                .loading_states
                .iter()
                .flat_map(|state| &state.constraints)
                .filter(|c| c.constraint == alas_opt::ModelCgConstraint::MinimumUsableCgRange)
                .max_by(|a, b| a.normalized_exceedance.total_cmp(&b.normalized_exceedance))
                .copied()
                .expect("usable CG range assessed")
        };
        let worst = range(own);
        assert!(
            worst.actual < worst.limit,
            "the registered A320 meets the requirement on the reported mesh"
        );

        // The registered aircraft against itself: the guard passes.
        let mut finalist = own.clone();
        hold_to_nominal(&mut finalist, &config, Some(&nominal));
        assert!(finalist.relative_balance.is_empty());

        // A registered aircraft with a wider reported range, still short of
        // the requirement: the same finalist now degrades it and is rejected.
        let mut wider = own.clone();
        for state in &mut wider
            .feasibility
            .model_cg
            .as_mut()
            .expect("model CG")
            .loading_states
        {
            for c in &mut state.constraints {
                if c.constraint == alas_opt::ModelCgConstraint::MinimumUsableCgRange {
                    c.actual += 0.5 * (worst.limit - worst.actual);
                }
            }
        }
        let wider = ReportingNominal {
            design: nominal.design,
            evaluation: Ok((wider, Vec::new())),
        };
        hold_to_nominal(&mut finalist, &config, Some(&wider));
        let guard = finalist
            .relative_balance
            .iter()
            .find(|r| r.id == "usable_cg_range_vs_nominal")
            .expect("the range guard is violated");
        assert!((guard.actual - worst.actual).abs() < 1e-12);
        assert!((guard.limit - (worst.actual + 0.5 * (worst.limit - worst.actual))).abs() < 1e-9);
        assert!(!finalist.accepted());
        assert!(finalist
            .rejected_by()
            .contains(&"hard:usable_cg_range_vs_nominal".to_owned()));
        assert!(finalist
            .rejection_messages()
            .iter()
            .any(|m| m.starts_with("usable_cg_range_vs_nominal at reporting fidelity")));

        // No nominal at reporting fidelity: the hard guard fails closed.
        let unavailable = ReportingNominal {
            design: nominal.design,
            evaluation: Err("analysis failed".to_owned()),
        };
        let mut finalist = own.clone();
        hold_to_nominal(&mut finalist, &config, Some(&unavailable));
        assert!(finalist
            .rejected_by()
            .contains(&"hard:relative_balance_nominal_unavailable".to_owned()));

        // A diagnostic Balance policy has no reporting nominal to evaluate.
        let mut diagnostic = config.clone();
        diagnostic.optimizer.objective.balance_constraints =
            alas_config::ConstraintPolicy::Diagnostic;
        assert!(ReportingNominal::evaluate(&diagnostic, None, None).is_none());
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
