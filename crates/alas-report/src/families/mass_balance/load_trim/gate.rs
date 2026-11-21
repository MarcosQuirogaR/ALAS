// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The run's own CG-gate verdict for a numbered loading state.
//!
//! The sheet does not judge a state itself: a gated state carries the
//! model assessment's governing limits, CG and constraint verdicts, so a
//! ring on the chart always agrees with the run's feasibility verdict. A
//! state the gate does not evaluate (an intermediate loading step) carries
//! no verdict.

use alas_opt::envelope::{ForwardLimitGovernance, PhaseLimits};
use alas_opt::{ModelCgLoadingAssessment, ModelCgLoadingState};

use super::{LoadStep, Mechanism};

/// The flight phase whose limits govern a gated state, as the gate scopes
/// them ([`PhaseLimits::for_state_at_mass`]); it selects the limit set and
/// the envelope a gated row is read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatePhase {
    /// Ground mechanisms only.
    Ground,
    /// Rotation forward, static-margin floor aft.
    Takeoff,
    /// Landing trim forward, static-margin floor aft, en route at or below
    /// the maximum landing mass.
    Flight,
    /// Landing trim forward, static-margin floor aft, on approach.
    Landing,
    /// En route above the maximum landing mass: the state cannot land, so no
    /// landing trim; static-margin floor aft. Its row keeps the gate's own
    /// limits (no landing or zero-fuel envelope is read for it) and the table
    /// names the forward limit the gate found ([`StepGate::fwd_governance`]).
    EnRouteAboveMlw,
}

impl GatePhase {
    /// The phase of a loading state at or below the maximum landing mass,
    /// mirroring [`PhaseLimits::for_state`].
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

    /// The phase the gate scoped `assessment` to, read from the assessment's
    /// own scope (its phase at its mass against the maximum landing mass).
    #[must_use]
    pub fn of_assessment(assessment: &ModelCgLoadingAssessment) -> Self {
        if assessment.scope == PhaseLimits::EN_ROUTE_ABOVE_MLW {
            Self::EnRouteAboveMlw
        } else {
            Self::of(assessment.state)
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
            Self::EnRouteAboveMlw => PhaseLimits::EN_ROUTE_ABOVE_MLW,
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
    /// The mechanism the gate found governing the forward limit.
    pub fwd_governance: ForwardLimitGovernance,
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
            phase: GatePhase::of_assessment(assessment),
            cg_pct_mac: assessment.cg_pct_mac,
            fwd_pct_mac: assessment.physical_limits.fwd_limit_pct_mac,
            aft_pct_mac: assessment.physical_limits.aft_limit_pct_mac,
            fwd_governance: assessment.physical_limits.fwd_limit_governance,
            violated: assessment
                .constraints
                .iter()
                .any(|c| c.violated && !c.is_diagnostic()),
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

/// Footnote of the weight and balance table: the empty weight is judged by
/// the ground limits.
pub(super) const TABLE_NOTE: &str = "DOW: ground limits; others: flight limits";
/// Footnote lead of the states gated en route above the maximum landing mass,
/// followed by the label of the mechanism that bounds them forward.
pub(super) const NOTE_ABOVE_MLW: &str = "Above MLW, fwd limit:";

/// The table's footnotes, each a fixed lead and an optional mechanism label:
/// the ground-limit note when the empty weight is gated, and one for the
/// states gated above the maximum landing mass naming the forward limit the
/// gate found at the heaviest of them. The name is read from that verdict's
/// governance, so a new in-flight forward limit is named, not assumed.
pub(super) fn table_notes(steps: &[LoadStep]) -> Vec<(&'static str, Option<&'static str>)> {
    let mut notes = Vec::new();
    if steps
        .first()
        .is_some_and(|s| s.state == "DOW" && s.gate.is_some())
    {
        notes.push((TABLE_NOTE, None));
    }
    let heaviest_above = steps
        .iter()
        .filter(|s| {
            s.gate
                .as_ref()
                .is_some_and(|g| g.phase == GatePhase::EnRouteAboveMlw)
        })
        .max_by(|a, b| a.mass_kg.total_cmp(&b.mass_kg))
        .and_then(|s| s.gate.as_ref());
    if let Some(gate) = heaviest_above {
        let bound = Mechanism::of_forward_governance(gate.fwd_governance);
        notes.push((NOTE_ABOVE_MLW, Some(bound.label())));
    }
    notes
}
