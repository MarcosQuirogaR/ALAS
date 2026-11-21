// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What a completed run may be *presented* as.
//!
//! A pipeline result is three separate claims that have been reported as one:
//! that the run executed, that the analyzed aircraft satisfies its own hard
//! physical limits, and that a source-defined design mission was flown and
//! met. Execution is not feasibility, and feasibility against a model's own
//! configured limits is not validation against a real aeroplane.
//!
//! This module produces the one label that may be attached to a delivered
//! result and the explicit list of reasons it is not stronger. Two rules
//! govern it:
//!
//! 1. A **hard-infeasible candidate may still be an optimization incumbent**
//!    (the search ranks infeasible designs rather than discarding them, so
//!    the best of a generation can be infeasible), but it is never an
//!    accepted feasible success. [`DeliveryVerdict::DiagnosticIncumbent`] is
//!    that state, named rather than rounded up.
//! 2. A **calibrated or reference preset** carries a stronger claim than a
//!    clean-sheet design (it asserts something about a real, certified
//!    aeroplane), so it needs source-backed design-mission evidence before it
//!    may be called accepted. A clean-sheet design makes no such claim and is
//!    judged on its own physical feasibility, with the absence of mission
//!    evidence recorded as an advisory rather than silently dropped.
//!
//! Nothing here can promote a result: every constructor of
//! [`DeliveryVerdict::AcceptedFeasible`] runs through
//! [`DeliveryClassification::classify`], which reaches it only with an empty
//! blocker list.

use alas_config::{
    presets, AlasConfig, DesignMissionEvidence, DesignVector, MissingDesignMissionDatum,
};
use alas_opt::AftCgLimitGovernance;

use super::{FeasibilityReport, FindingCode, FindingSeverity};

/// How the analyzed design relates to a registered aircraft.
///
/// The distinction is decided by comparing the analyzed design vector with the
/// registered one, exactly as
/// [`super::structural_mass`] decides whether a published weight limit still
/// applies: a modified design may carry a preset's name while being a
/// different aeroplane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesignProvenance {
    /// A registered real aircraft, unmodified: the result is a claim about a
    /// certified type and is held to the stronger bar.
    CalibratedReference {
        /// Registered preset name.
        preset: String,
    },
    /// A registered aircraft that is not a real one: the notional AVE
    /// reference, whose envelope is a design requirement rather than a
    /// published limit. Held to the same mission bar as a calibrated preset,
    /// because it is still delivered as a named reference aircraft.
    NotionalReference {
        /// Registered preset name.
        preset: String,
    },
    /// A reference adaptation that moved the registered aircraft's design
    /// vector: a modified derivative of `preset`, which asserts nothing about
    /// the real aeroplane.
    AdaptedReference {
        /// Registered preset the design was adapted from.
        preset: String,
    },
    /// A design vector that differs from any registered one: a clean-sheet or
    /// optimized result, which asserts nothing about a real aeroplane.
    CleanSheet {
        /// The registered aircraft the design started from, when it had one.
        derived_from: Option<String>,
    },
}

impl DesignProvenance {
    /// Decide the provenance of one analyzed design.
    #[must_use]
    pub fn of(config: &AlasConfig, design: &DesignVector) -> Self {
        let Ok(preset) = presets::get(&config.preset) else {
            return Self::CleanSheet { derived_from: None };
        };
        if !design.same_geometry(&preset.design_vector) {
            if config.optimizer.design_space.mode == alas_config::DesignMode::ReferenceAdaptation {
                return Self::AdaptedReference {
                    preset: preset.name.to_owned(),
                };
            }
            return Self::CleanSheet {
                derived_from: Some(preset.name.to_owned()),
            };
        }
        // AVE is the one registered entry that is not a real aeroplane; its
        // CG envelope is registered as a design requirement rather than as
        // published or AFM-delegated evidence, which is the typed way to ask.
        if preset.reference.cg_evidence == alas_config::CgEnvelopeEvidence::DesignRequirement {
            return Self::NotionalReference {
                preset: preset.name.to_owned(),
            };
        }
        Self::CalibratedReference {
            preset: preset.name.to_owned(),
        }
    }

    /// Whether this provenance claims something about a named reference
    /// aircraft and therefore needs source-backed mission evidence.
    #[must_use]
    pub const fn requires_source_backed_mission(&self) -> bool {
        matches!(
            self,
            Self::CalibratedReference { .. } | Self::NotionalReference { .. }
        )
    }

    /// Stable report label.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::CalibratedReference { .. } => "calibrated reference preset",
            Self::NotionalReference { .. } => "notional reference preset",
            Self::AdaptedReference { .. } => "adapted reference result",
            Self::CleanSheet { .. } => "clean-sheet result",
        }
    }
}

mod completion;
pub use completion::RunCompletion;

/// One reason a result may not be presented as an accepted feasible success.
#[derive(Debug, Clone, PartialEq)]
pub enum DeliveryBlocker {
    /// An error-severity physical finding.
    PhysicalInfeasibility {
        /// Machine-stable identifier of the failed check.
        code: FindingCode,
        /// The finding's own message.
        message: String,
    },
    /// A reference aircraft with no source-backed design mission.
    DesignMissionUnverified {
        /// The contract data no registered source supplies.
        missing: Vec<MissingDesignMissionDatum>,
    },
    /// The run did not complete.
    RunIncomplete {
        /// How it ended.
        completion: RunCompletion,
    },
    /// The item ledger and the lumped model place the takeoff centre of
    /// gravity further apart than the reporting band.
    ///
    /// This blocks delivery even though the underlying finding is a warning:
    /// the hard model CG and gear constraints are evaluated on the lumped
    /// coordinates while the published balance statement is the ledger's, so
    /// while the two disagree the feasibility verdict is not a statement
    /// about the aircraft whose weight and balance is delivered beside it.
    /// It is the AVE case in the eight-preset baseline, at 28.2 against 21.5
    /// percent MAC.
    MassCoordinatePathsDisagree {
        /// The reporting message, which carries both percentages.
        message: String,
    },
}

impl DeliveryBlocker {
    /// One sentence a report can print without further formatting.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::PhysicalInfeasibility { code, message } => {
                format!("physical: {} ({message})", code.as_str())
            }
            Self::DesignMissionUnverified { missing } => {
                if missing.is_empty() {
                    "design mission: no source-backed mission is registered".to_owned()
                } else {
                    format!(
                        "design mission: unverified; no registered source supplies {}",
                        missing
                            .iter()
                            .map(|datum| alas_config::datum_label(*datum))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
            Self::MassCoordinatePathsDisagree { message } => {
                format!("mass coordinates: {message}")
            }
            Self::RunIncomplete { completion } => match completion {
                RunCompletion::Completed => "run: complete".to_owned(),
                RunCompletion::NotConverged { detail } => {
                    format!("run: did not converge ({detail})")
                }
                RunCompletion::Cancelled { detail } => format!("run: cancelled ({detail})"),
                RunCompletion::InProgress => {
                    "run: still in progress; feasibility is not assessed yet".to_owned()
                }
            },
        }
    }
}

/// What a completed result may be presented as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryVerdict {
    /// Every blocker is closed: the analyzed aircraft satisfies its own hard
    /// limits, the run completed, and any claim about a real aeroplane is
    /// backed by a registered design mission.
    AcceptedFeasible,
    /// Physically infeasible, or produced by a run that was cancelled or is
    /// still running.
    /// Usable as an optimization incumbent and as a diagnostic; never as a
    /// success.
    DiagnosticIncumbent,
    /// Physically feasible and complete, but a claim about a reference
    /// aircraft that no source-backed mission supports.
    NotAcceptedUnverifiedMission,
}

impl DeliveryVerdict {
    /// Stable report label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AcceptedFeasible => "ACCEPTED - FEASIBLE",
            Self::DiagnosticIncumbent => "NOT ACCEPTED - DIAGNOSTIC INCUMBENT ONLY",
            Self::NotAcceptedUnverifiedMission => "NOT ACCEPTED - DESIGN MISSION UNVERIFIED",
        }
    }

    /// Whether this verdict may be reported as a success anywhere.
    #[must_use]
    pub const fn is_success(self) -> bool {
        matches!(self, Self::AcceptedFeasible)
    }
}

/// The delivered label, its provenance, and everything standing between them.
#[derive(Debug, Clone, PartialEq)]
pub struct DeliveryClassification {
    /// What kind of result this is.
    pub provenance: DesignProvenance,
    /// The one label that may be attached to it.
    pub verdict: DeliveryVerdict,
    /// Every reason the verdict is not [`DeliveryVerdict::AcceptedFeasible`].
    pub blockers: Vec<DeliveryBlocker>,
    /// Conditions that do not block delivery but must travel with it.
    pub advisories: Vec<String>,
}

impl DeliveryClassification {
    /// Classify one completed result.
    ///
    /// The verdict is derived from `blockers`, never asserted independently,
    /// so a future blocker cannot be added without also affecting the label.
    #[must_use]
    pub fn classify(
        config: &AlasConfig,
        design: &DesignVector,
        report: &FeasibilityReport,
        completion: &RunCompletion,
    ) -> Self {
        Self::classify_with_mission_gap(
            config,
            design,
            report,
            completion,
            mission_evidence_gap(config),
        )
    }

    /// [`Self::classify`] with the preset's mission gap supplied, so the rule
    /// can be exercised against a preset state the registry does not hold.
    fn classify_with_mission_gap(
        config: &AlasConfig,
        design: &DesignVector,
        report: &FeasibilityReport,
        completion: &RunCompletion,
        mission_gap: Option<Vec<MissingDesignMissionDatum>>,
    ) -> Self {
        let provenance = DesignProvenance::of(config, design);
        let mut blockers = Vec::new();
        let mut advisories_early = Vec::new();

        for finding in &report.findings {
            if finding.severity == FindingSeverity::Error {
                blockers.push(DeliveryBlocker::PhysicalInfeasibility {
                    code: finding.code,
                    message: finding.message.clone(),
                });
            } else if finding.code == FindingCode::MassModelDisagreement {
                blockers.push(DeliveryBlocker::MassCoordinatePathsDisagree {
                    message: finding.message.clone(),
                });
            }
        }
        // A search that stopped on its clock or its rejection cap before its
        // candidate plan ended still delivers a design the reporting analysis
        // verified: convergence is not a delivery criterion (the search's
        // convergence indicator reports how far its best was still moving),
        // so the early stop travels as an advisory. A cancelled or still
        // running search delivers nothing verified and blocks.
        match completion {
            RunCompletion::Completed => {}
            RunCompletion::NotConverged { detail } => advisories_early.push(format!(
                "the search stopped before its candidate plan ended ({detail}); the delivered design is its best verified at reporting fidelity"
            )),
            RunCompletion::Cancelled { .. } | RunCompletion::InProgress => {
                blockers.push(DeliveryBlocker::RunIncomplete {
                    completion: completion.clone(),
                });
            }
        }

        let mission_unverified = mission_gap.is_some();
        if let Some(missing) = mission_gap {
            if provenance.requires_source_backed_mission() {
                blockers.push(DeliveryBlocker::DesignMissionUnverified { missing });
            }
        }

        let mut advisories = advisories_early;
        if mission_unverified && !provenance.requires_source_backed_mission() {
            advisories.push(
                "no source-backed design mission is registered for the aircraft this design was \
                 derived from; the result is a physical-feasibility statement only"
                    .to_owned(),
            );
        }
        if let Some(model_cg) = &report.model_cg {
            if let AftCgLimitGovernance::GroundMinimumNoseLoad { margin_pct_mac } =
                model_cg.aft_limit_governance
            {
                advisories.push(format!(
                    "the model CG envelope's aft boundary is the aerodynamic one at {:.3} % MAC \
                     while the ground minimum-nose-load boundary is {:.3} % MAC, {:.3} % MAC \
                     further forward (effective main gear {:.3} % MAC): the envelope admits a \
                     band of centres of gravity this gear cannot carry, so a nose-load finding on \
                     this aircraft is a symptom of the aft boundary, not of the loading state",
                    model_cg.aerodynamic_aft_limit_pct_mac,
                    model_cg.ground_aft_limit_pct_mac,
                    margin_pct_mac,
                    model_cg.main_gear_station_pct_mac,
                ));
            }
        }

        let verdict = if blockers.iter().any(|blocker| {
            matches!(
                blocker,
                DeliveryBlocker::PhysicalInfeasibility { .. }
                    | DeliveryBlocker::RunIncomplete { .. }
                    | DeliveryBlocker::MassCoordinatePathsDisagree { .. }
            )
        }) {
            DeliveryVerdict::DiagnosticIncumbent
        } else if blockers.is_empty() {
            DeliveryVerdict::AcceptedFeasible
        } else {
            DeliveryVerdict::NotAcceptedUnverifiedMission
        };

        Self {
            provenance,
            verdict,
            blockers,
            advisories,
        }
    }

    /// Whether this result may be reported as a success.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.verdict.is_success() && self.blockers.is_empty()
    }

    /// The label and every blocker, one per line, for a text report.
    #[must_use]
    pub fn render(&self) -> String {
        let mut lines = vec![format!(
            "Delivery verdict: {} ({})",
            self.verdict.label(),
            self.provenance.label()
        )];
        for blocker in &self.blockers {
            lines.push(format!("  blocker: {}", blocker.message()));
        }
        for advisory in &self.advisories {
            lines.push(format!("  advisory: {advisory}"));
        }
        lines.join("\n")
    }
}

/// The contract data no registered source supplies for the selected preset,
/// or [`None`] when a complete source-backed mission is registered.
///
/// A preset with partial evidence rows reports the union of what those rows
/// still leave missing; a preset with no rows at all reports all four, which
/// is the honest reading of "nothing registered" and not an empty list that
/// could be mistaken for "nothing missing".
fn mission_evidence_gap(config: &AlasConfig) -> Option<Vec<MissingDesignMissionDatum>> {
    presets::get(&config.preset).map_or_else(
        |_| Some(ALL_MISSION_DATA.to_vec()),
        preset_mission_evidence_gap,
    )
}

/// [`mission_evidence_gap`] for one preset.
fn preset_mission_evidence_gap(
    preset: &alas_config::AircraftPreset,
) -> Option<Vec<MissingDesignMissionDatum>> {
    if matches!(
        preset.reference.design_mission_evidence,
        DesignMissionEvidence::SourceBacked(_)
    ) {
        return None;
    }
    // Per-datum provenance: a complete set that clears every promotion bar
    // (tier, exact variant, stated conditions) closes the gap; a partly
    // registered set reports only the data it still refuses.
    let provenance = &preset.reference.mission_provenance;
    if provenance.admits_source_backed_promotion() {
        return None;
    }
    if *provenance != alas_config::DesignMissionProvenanceSet::default() {
        return Some(
            provenance
                .refusals()
                .into_iter()
                .map(alas_config::MissionPromotionRefusal::datum)
                .collect(),
        );
    }
    let rows = &preset.reference.partial_design_mission_evidence;
    if rows.is_empty() {
        return Some(ALL_MISSION_DATA.to_vec());
    }
    let missing: Vec<MissingDesignMissionDatum> = ALL_MISSION_DATA
        .into_iter()
        .filter(|datum| rows.iter().all(|row| row.missing.contains(datum)))
        .collect();
    Some(missing)
}

/// The four contract data, in the order the report prints them.
const ALL_MISSION_DATA: [MissingDesignMissionDatum; 4] = [
    MissingDesignMissionDatum::Range,
    MissingDesignMissionDatum::Payload,
    MissingDesignMissionDatum::Profile,
    MissingDesignMissionDatum::ReserveFuel,
];

// Tests build their own configurations and assert on them, so a failed expect
// or panic is the assertion failing rather than a library invariant breaking.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::feasibility::PhysicalFinding;

    fn config(preset: &str) -> AlasConfig {
        AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// The mission gap of the A320-200 with its registered provenance
    /// withdrawn: the state of a preset that has no sourced mission.
    fn unregistered_gap() -> Option<Vec<MissingDesignMissionDatum>> {
        let mut preset = presets::get("A320-200")
            .unwrap_or_else(|error| panic!("{error}"))
            .clone();
        preset.reference.mission_provenance = alas_config::DesignMissionProvenanceSet::default();
        preset.reference.partial_design_mission_evidence.clear();
        preset_mission_evidence_gap(&preset)
    }

    fn registered_design(preset: &str) -> DesignVector {
        presets::get(preset)
            .unwrap_or_else(|error| panic!("{error}"))
            .design_vector
    }

    fn error_finding(code: FindingCode) -> PhysicalFinding {
        PhysicalFinding {
            code,
            severity: FindingSeverity::Error,
            message: "a hard limit was exceeded".to_owned(),
            actual: Some(1.0),
            limit: Some(0.5),
            unit: "",
        }
    }

    /// The rule the release narrative turns on: a calibrated preset that is
    /// physically infeasible is an optimization incumbent and a diagnostic,
    /// and cannot carry a success label under any combination of the other
    /// inputs.
    #[test]
    fn an_infeasible_calibrated_preset_cannot_produce_a_success_label() {
        let config = config("A320-200");
        let design = registered_design("A320-200");
        let report = FeasibilityReport {
            findings: vec![error_finding(FindingCode::MinimumNoseGearLoadViolation)],
            ..FeasibilityReport::default()
        };
        let classification =
            DeliveryClassification::classify(&config, &design, &report, &RunCompletion::Completed);

        assert_eq!(
            classification.provenance,
            DesignProvenance::CalibratedReference {
                preset: "A320-200".to_owned()
            }
        );
        assert_eq!(classification.verdict, DeliveryVerdict::DiagnosticIncumbent);
        assert!(!classification.is_success());
        assert!(!classification.verdict.is_success());
        assert!(classification.blockers.iter().any(|blocker| matches!(
            blocker,
            DeliveryBlocker::PhysicalInfeasibility {
                code: FindingCode::MinimumNoseGearLoadViolation,
                ..
            }
        )));
    }

    /// The second half of the same rule: feasibility alone does not accept a
    /// reference aircraft. A preset with no registered mission provenance is
    /// `Unverified`, so a finding-free A320-200 without its provenance is
    /// still not a success.
    #[test]
    fn a_feasible_but_unverified_calibrated_preset_cannot_produce_a_success_label() {
        let config = config("A320-200");
        let design = registered_design("A320-200");
        let classification = DeliveryClassification::classify_with_mission_gap(
            &config,
            &design,
            &FeasibilityReport::default(),
            &RunCompletion::Completed,
            unregistered_gap(),
        );

        assert_eq!(
            classification.verdict,
            DeliveryVerdict::NotAcceptedUnverifiedMission
        );
        assert!(!classification.is_success());
        assert_eq!(classification.blockers.len(), 1);
        assert!(matches!(
            classification.blockers[0],
            DeliveryBlocker::DesignMissionUnverified { .. }
        ));
    }

    /// A preset whose four mission data each carry a manufacturer-tier,
    /// exact-variant, conditioned source is not blocked on its mission; one
    /// without such rows keeps all four missing.
    #[test]
    fn a_preset_with_four_sourced_mission_data_has_no_mission_blocker() {
        for &name in MISSION_REGISTERED {
            assert!(
                mission_evidence_gap(&config(name)).is_none(),
                "{name} registers a complete sourced mission"
            );
            let classification = DeliveryClassification::classify(
                &config(name),
                &registered_design(name),
                &FeasibilityReport::default(),
                &RunCompletion::Completed,
            );
            assert!(
                classification.is_success(),
                "{name}: {:?}",
                classification.blockers
            );
        }
        let unregistered = unregistered_gap().expect("no provenance, no mission");
        assert_eq!(unregistered.len(), 4);
    }

    /// AVE is not a real aeroplane, but it is still delivered as a named
    /// reference aircraft, so it is held to the same mission bar.
    #[test]
    fn the_notional_reference_aircraft_is_held_to_the_mission_bar_too() {
        let config = config("AVE");
        let design = registered_design("AVE");
        let classification = DeliveryClassification::classify(
            &config,
            &design,
            &FeasibilityReport::default(),
            &RunCompletion::Completed,
        );
        assert_eq!(
            classification.provenance,
            DesignProvenance::NotionalReference {
                preset: "AVE".to_owned()
            }
        );
        assert!(classification.provenance.requires_source_backed_mission());
        // Its design requirement is the registered source of the mission, so
        // the mission blocker is closed; a finding-free AVE is accepted.
        assert!(classification.is_success(), "{:?}", classification.blockers);
    }

    /// A modified design vector is an adapted reference result: it claims nothing
    /// about the A320-200 and is judged on its own feasibility, with the
    /// missing mission evidence carried as an advisory rather than dropped.
    #[test]
    fn a_modified_design_is_an_adapted_result_and_may_be_accepted_on_feasibility() {
        let config = config("A320-200");
        let mut design = registered_design("A320-200");
        design.span_m += 1.0;
        let classification = DeliveryClassification::classify_with_mission_gap(
            &config,
            &design,
            &FeasibilityReport::default(),
            &RunCompletion::Completed,
            unregistered_gap(),
        );

        assert_eq!(
            classification.provenance,
            DesignProvenance::AdaptedReference {
                preset: "A320-200".to_owned()
            }
        );
        assert_eq!(classification.verdict, DeliveryVerdict::AcceptedFeasible);
        assert!(classification.is_success());
        assert!(classification.blockers.is_empty());
        assert_eq!(classification.advisories.len(), 1);
        assert!(classification.advisories[0].contains("physical-feasibility statement only"));
    }

    /// A reference adaptation that moved the vector is labelled a derivative
    /// of its registered aircraft, not a clean sheet; a clean-sheet run that
    /// starts from the same preset is a clean-sheet result.
    #[test]
    fn a_moved_reference_adaptation_is_an_adapted_reference_and_a_clean_sheet_is_not() {
        let mut adapted = config("ATR72-600");
        adapted.optimizer.design_space.mode = alas_config::DesignMode::ReferenceAdaptation;
        let mut design = registered_design("ATR72-600");
        design.span_m += 1.0;
        let provenance = DesignProvenance::of(&adapted, &design);
        assert_eq!(
            provenance,
            DesignProvenance::AdaptedReference {
                preset: "ATR72-600".to_owned()
            }
        );
        assert_eq!(provenance.label(), "adapted reference result");
        assert!(!provenance.requires_source_backed_mission());
        let mut clean = adapted;
        clean.optimizer.design_space.mode = alas_config::DesignMode::CleanSheet;
        assert_eq!(
            DesignProvenance::of(&clean, &design).label(),
            "clean-sheet result"
        );
    }

    /// A cancelled or still running search cannot be accepted even when the
    /// result it returned carries no finding: the incumbent is a snapshot
    /// nobody verified.
    #[test]
    fn a_cancelled_or_running_search_cannot_produce_a_success_label() {
        let config = config("A320-200");
        let mut design = registered_design("A320-200");
        design.span_m += 1.0;
        for completion in [
            RunCompletion::Cancelled {
                detail: "480 s guard".to_owned(),
            },
            RunCompletion::InProgress,
        ] {
            let classification = DeliveryClassification::classify(
                &config,
                &design,
                &FeasibilityReport::default(),
                &completion,
            );
            assert_eq!(
                classification.verdict,
                DeliveryVerdict::DiagnosticIncumbent,
                "{completion:?}"
            );
            assert!(!classification.is_success(), "{completion:?}");
            assert!(classification
                .blockers
                .iter()
                .any(|blocker| matches!(blocker, DeliveryBlocker::RunIncomplete { .. })));
        }
    }

    /// A search stopped on its clock before its plan ended delivers a
    /// feasible verified design: the early stop is an advisory, not a
    /// blocker (convergence is not required).
    #[test]
    fn a_search_stopped_early_with_a_feasible_verified_design_is_delivered() {
        let config = config("A320-200");
        let mut design = registered_design("A320-200");
        design.span_m += 1.0;
        let classification = DeliveryClassification::classify(
            &config,
            &design,
            &FeasibilityReport::default(),
            &RunCompletion::NotConverged {
                detail: "search termination: time_budget".to_owned(),
            },
        );
        assert_eq!(classification.verdict, DeliveryVerdict::AcceptedFeasible);
        assert!(classification.is_success());
        assert!(classification
            .advisories
            .iter()
            .any(|advisory| advisory.contains("time_budget")));
    }

    /// A warning-severity finding is reported but does not block delivery;
    /// only error severity does. Otherwise the OEI evidence-gap warning every
    /// preset carries would silently block every result.
    #[test]
    fn a_warning_finding_does_not_block_delivery() {
        let config = config("A320-200");
        let mut design = registered_design("A320-200");
        design.span_m += 1.0;
        let report = FeasibilityReport {
            findings: vec![PhysicalFinding {
                code: FindingCode::FieldPerformanceUnavailable,
                severity: FindingSeverity::Warning,
                message: "OEI SLS evidence gap".to_owned(),
                actual: None,
                limit: None,
                unit: "",
            }],
            ..FeasibilityReport::default()
        };
        let classification =
            DeliveryClassification::classify(&config, &design, &report, &RunCompletion::Completed);
        assert_eq!(classification.verdict, DeliveryVerdict::AcceptedFeasible);
    }

    #[test]
    fn independent_structural_mass_estimates_do_not_block_delivery() {
        let config = config("A320-200");
        let mut design = registered_design("A320-200");
        design.span_m += 1.0;
        let report = FeasibilityReport {
            findings: vec![PhysicalFinding {
                code: FindingCode::StructuralMassModelDifference,
                severity: FindingSeverity::Warning,
                message: "Independent structural and empirical mass estimates differ".to_owned(),
                actual: Some(27_000.0),
                limit: Some(23_000.0),
                unit: "kg",
            }],
            ..FeasibilityReport::default()
        };
        let classification =
            DeliveryClassification::classify(&config, &design, &report, &RunCompletion::Completed);
        assert_eq!(classification.verdict, DeliveryVerdict::AcceptedFeasible);
        assert!(classification.blockers.is_empty());
        assert_eq!(
            report.findings[0].code.as_str(),
            "structural_mass_model_difference"
        );
    }

    /// The AVE case: the two mass-coordinate paths disagree by 6.6 % MAC, so
    /// the hard constraints and the delivered balance statement are not about
    /// the same centre of gravity. A warning-severity finding is enough to
    /// block delivery here, and it does so even on a clean-sheet design that
    /// carries no mission bar.
    #[test]
    fn disagreeing_mass_coordinate_paths_block_delivery_even_without_an_error_finding() {
        let config = config("AVE");
        let mut design = registered_design("AVE");
        design.span_m += 1.0;
        let report = FeasibilityReport {
            findings: vec![PhysicalFinding {
                code: FindingCode::MassModelDisagreement,
                severity: FindingSeverity::Warning,
                message: "the item ledger places the takeoff centre of gravity at 28.2 percent \
                          MAC and the lumped model at 21.5"
                    .to_owned(),
                actual: Some(28.164),
                limit: Some(21.545),
                unit: "% MAC",
            }],
            ..FeasibilityReport::default()
        };
        let classification =
            DeliveryClassification::classify(&config, &design, &report, &RunCompletion::Completed);

        assert_eq!(
            classification.provenance,
            DesignProvenance::AdaptedReference {
                preset: "AVE".to_owned()
            }
        );
        assert_eq!(classification.verdict, DeliveryVerdict::DiagnosticIncumbent);
        assert!(!classification.is_success());
        assert!(classification
            .blockers
            .iter()
            .any(|blocker| matches!(blocker, DeliveryBlocker::MassCoordinatePathsDisagree { .. })));
        assert!(classification.render().contains("mass coordinates:"));
    }

    /// Every registered preset carries a sourced design mission, and a preset
    /// whose provenance and partial rows are withdrawn reports all four data
    /// missing rather than an empty list.
    #[test]
    fn every_registered_preset_carries_a_mission_and_an_empty_set_reports_all_four() {
        for name in presets::available() {
            assert!(
                MISSION_REGISTERED.contains(&name),
                "{name} has no registered design-mission provenance"
            );
        }
        assert_eq!(unregistered_gap(), Some(ALL_MISSION_DATA.to_vec()));
    }

    /// Presets whose four mission data are registered through
    /// `mission_provenance` (manufacturer charts, the AVE design requirement,
    /// the C295 payload-defined mission, and the regulation or cited standard
    /// for reserves the chart does not print).
    const MISSION_REGISTERED: &[&str] = &[
        "A220-300",
        "A320-200",
        "A340-300",
        "A350-900",
        "A380-800",
        "A400M",
        "ATR72-600",
        "AVE",
        "B747-400",
        "B787-9",
        "C295",
        "C919",
        "DC-10",
        "E195-E2",
        "LJ60",
    ];
}
