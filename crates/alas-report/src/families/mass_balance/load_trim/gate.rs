// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The run's own CG-gate verdict for a numbered loading state.
//!
//! The sheet does not judge a state itself: a gated state carries the
//! model assessment's governing limits, CG and constraint verdicts, so a
//! ring on the chart always agrees with the run's feasibility verdict. A
//! state the gate does not evaluate (an intermediate loading step) carries
//! no verdict.

use alas_opt::envelope::PhaseLimits;
use alas_opt::{ModelCgLoadingAssessment, ModelCgLoadingState};

/// The flight phase whose limits govern a gated state, as the gate scopes
/// them ([`PhaseLimits::for_state`]); it selects the limit-set line style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatePhase {
    /// Ground mechanisms only.
    Ground,
    /// Rotation forward, static-margin floor aft.
    Takeoff,
    /// Landing trim forward, static-margin floor aft, en route.
    Flight,
    /// Landing trim forward, static-margin floor aft, on approach.
    Landing,
}

impl GatePhase {
    /// The phase of a gated loading state, mirroring [`PhaseLimits::for_state`].
    #[must_use]
    pub const fn of(state: ModelCgLoadingState) -> Self {
        match state {
            ModelCgLoadingState::OperatingEmpty => Self::Ground,
            ModelCgLoadingState::AnalyzedZeroFuel
            | ModelCgLoadingState::OperationalMidMission
            | ModelCgLoadingState::OperationalReserve => Self::Flight,
            ModelCgLoadingState::AnalyzedTakeoff => Self::Takeoff,
            ModelCgLoadingState::AnalyzedLanding => Self::Landing,
        }
    }

    /// The gate's mechanism set for this phase.
    #[must_use]
    pub const fn limits(self) -> PhaseLimits {
        match self {
            Self::Ground => PhaseLimits::GROUND,
            Self::Takeoff => PhaseLimits::TAKEOFF,
            Self::Flight => PhaseLimits::FLIGHT,
            Self::Landing => PhaseLimits::LANDING,
        }
    }
}

/// The CG gate's verdict for one numbered state, all in %MAC.
#[derive(Debug, Clone, PartialEq)]
pub struct StepGate {
    /// The gated loading state.
    pub state: ModelCgLoadingState,
    /// The phase whose limits govern it.
    pub phase: GatePhase,
    /// The gate's CG for this state.
    pub cg_pct_mac: f64,
    /// Governing forward and aft limits, scoped to the phase.
    pub fwd_pct_mac: f64,
    /// See [`Self::fwd_pct_mac`].
    pub aft_pct_mac: f64,
    /// Whether the gate failed the state: a violated hard constraint. A
    /// violated diagnostic constraint is a warning in the run's verdict and
    /// does not fail it.
    pub violated: bool,
}

impl StepGate {
    /// The verdict the model assessment recorded for `assessment`.
    #[must_use]
    pub fn from_assessment(assessment: &ModelCgLoadingAssessment) -> Self {
        Self {
            state: assessment.state,
            phase: GatePhase::of(assessment.state),
            cg_pct_mac: assessment.cg_pct_mac,
            fwd_pct_mac: assessment.physical_limits.fwd_limit_pct_mac,
            aft_pct_mac: assessment.physical_limits.aft_limit_pct_mac,
            violated: assessment
                .constraints
                .iter()
                .any(|c| c.violated && !c.constraint.is_diagnostic()),
        }
    }

    /// `min(CG - forward, aft - CG)`: the CG's distance inside its limits.
    /// The verdict also covers gear-load constraints, so a state can fail
    /// with a positive margin.
    #[must_use]
    pub fn margin_pct_mac(&self) -> f64 {
        (self.cg_pct_mac - self.fwd_pct_mac).min(self.aft_pct_mac - self.cg_pct_mac)
    }
}
