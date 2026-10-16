// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Workflow sections, outcomes and execution states for the UAV page.

use alas_uav::optimizer::{CompetitionOptimum, OptimizationProgress};

/// The completed competition search and the preset it was run against.
#[derive(Debug, Clone)]
pub struct CompetitionResult {
    /// Preset identifier, for example `acc-2026`.
    pub preset_id: String,
    /// Preset display name.
    pub preset_name: String,
    /// The search output, including the best candidate and its artefacts.
    pub optimum: CompetitionOptimum,
}

/// The current task-oriented section of the UAV workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UavSection {
    /// Competition preset, search budget, and mission objectives.
    #[default]
    Mission,
    /// Topology selection, geometry search, and preliminary model assumptions.
    Airframe,
    /// Latest competition search result.
    Results,
}

impl UavSection {
    /// Short label suitable for the workflow tabs.
    pub const fn short_title(self) -> &'static str {
        match self {
            Self::Mission => "Mission",
            Self::Airframe => "Airframe",
            Self::Results => "Results",
        }
    }

    /// Stable title matching the desktop translation catalog.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Mission => "Competition and mission objectives",
            Self::Airframe => "Geometry search and preliminary model",
            Self::Results => "UAV result",
        }
    }

    /// Explain the purpose of a section before its inputs are shown.
    pub const fn description(self) -> &'static str {
        match self {
            Self::Mission => "Choose the competition preset, the search budget and seed, and the mission objectives and payload the aircraft must carry.",
            Self::Airframe => "Choose a design convention, then set the geometry and preliminary-model bounds.",
            Self::Results => "Review the best candidate: score, chosen components, geometry, stability, trim, spar margins and the gates that bind.",
        }
    }

    /// Ordered sections reflecting the evidence dependency of a run.
    pub const ALL: [Self; 3] = [Self::Mission, Self::Airframe, Self::Results];
}

/// Result of the most recent completed search.
#[derive(Debug, Clone, Default)]
pub enum UavWorkflowOutcome {
    /// Inputs have not been evaluated.
    #[default]
    NotRun,
    /// A competition search completed; its best candidate may still be infeasible.
    Competition(Box<CompetitionResult>),
}

/// Transient worker state kept separate from the last completed outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum UavExecutionStatus {
    /// No search has been requested in this app session.
    Idle,
    /// A background worker is evaluating deterministic candidates.
    Running(OptimizationProgress),
    /// Cancellation has been requested and will be observed between candidates.
    CancelRequested(OptimizationProgress),
    /// A search completed.
    Completed,
    /// A worker stopped cooperatively; the last completed outcome is retained.
    Cancelled {
        /// Candidates completed before the worker observed cancellation.
        evaluated_candidates: usize,
    },
    /// Input validation or worker execution failed; prior output is retained.
    Failed(String),
}
