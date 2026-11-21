// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The first step of a reporting-fidelity re-evaluation on its own: the
//! replay of the typed candidate assessment with the configured in-loop
//! model.
//!
//! A staged search refines its candidates on a coarser model than the one
//! the application replays them with, so a candidate at a hard limit can be
//! feasible there and not here. The replay is a fraction of the cost of the
//! reported analysis and mission that follow it, so the verification ladder
//! replays several ranked candidates at once first, and spends the reported
//! analyses only on those the replay keeps (`dual_solver::vlm`). The replay
//! is then not repeated: [`verify_replayed`] continues from it.

use alas_config::{AlasConfig, DesignVector};

use super::{finish, AcceptanceRoute, FinalistVerification};

/// A candidate replayed with the configured in-loop model.
pub struct Replayed {
    /// The configuration the candidate is analysed under (the fixed
    /// requirement at its own mass under the MTOW band mode).
    pub(super) config: AlasConfig,
    /// The vector the replay was handed.
    pub(super) design: DesignVector,
    pub(super) assessment: alas_opt::CandidateAssessment,
}

impl Replayed {
    /// Replay `design` under `config`.
    ///
    /// # Errors
    ///
    /// The replay's failure, as a description; a cancellation as
    /// "Cancelled safely: ...".
    pub fn run(
        config: &AlasConfig,
        design: &DesignVector,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<Self, String> {
        Self::run_from(config, design, cancel, (None, None))
    }

    /// [`Self::run`], a clean-sheet engine sizing starting from the factor
    /// of `engine_start` on the selected engine and taking its slope for the
    /// first update (the search's own start, `SearchDiagnostics::
    /// engine_start` and `engine_slope`) instead of the selected engine and
    /// the plain update. The sizing stops in the same tight band either way,
    /// so the start sets only the number of closures.
    ///
    /// # Errors
    ///
    /// As [`Self::run`].
    pub fn run_from(
        config: &AlasConfig,
        design: &DesignVector,
        cancel: Option<&std::sync::atomic::AtomicBool>,
        engine_start: (Option<f64>, Option<f64>),
    ) -> Result<Self, String> {
        // Under the MTOW band mode the candidate is the fixed requirement at
        // its own mass: replay, report, mission and feasibility all read that
        // configuration.
        let config = config.at_band_mass(design.mtow_kg);
        let controls = alas_opt::mdo::SizingControls {
            initial_engine_factor: engine_start.0,
            initial_engine_slope: engine_start.1,
            ..alas_opt::mdo::SizingControls::default()
        };
        let assessment = alas_opt::mdo::assess_product_candidate_with_controls_cancellable(
            &config, design, controls, cancel,
        )
        .map_err(|error| {
            if error == "cancelled" {
                "Cancelled safely: finalist replay interrupted".to_owned()
            } else {
                format!("finalist replay failed: {error}")
            }
        })?;
        Ok(Self {
            config,
            design: *design,
            assessment,
        })
    }

    /// Whether every hard residual of the replay holds.
    pub fn hard_feasible(&self) -> bool {
        self.assessment.hard_feasible
    }

    /// The scalar cost the replay ranks the candidate by (the search's
    /// cost, lower is better), at the configured model: in a clean sheet
    /// with the engine sized for this candidate.
    pub fn cost(&self) -> f64 {
        self.assessment.cost
    }

    /// Identifiers of the hard residuals the replay found violated.
    pub fn violated_hard_ids(&self) -> Vec<&'static str> {
        self.assessment.violated_hard_ids()
    }

    /// The message of a candidate the replay found not hard-feasible.
    pub fn not_hard_feasible(&self) -> String {
        let violations = self.assessment.violated_hard_details().join(", ");
        format!(
            "finalist is not hard-feasible on replay: {}",
            if violations.is_empty() {
                "unidentified hard residual".to_owned()
            } else {
                violations
            }
        )
    }
}

/// The rest of the reporting-fidelity verification of a `replayed`
/// candidate: the reported analysis, the mission and the feasibility
/// assessment, as `verify_finalist_cancellable` does them from the vector
/// with no nominal (the ladder holds the result to the registered aircraft
/// itself, `hold_to_nominal_at_mass`).
///
/// # Errors
///
/// The analysis or mission failure, as a description.
pub fn verify_replayed(
    replayed: Replayed,
    route: Option<&AcceptanceRoute>,
) -> Result<FinalistVerification, String> {
    finish(replayed, route).map(|(verification, _)| verification)
}
