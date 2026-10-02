// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The closed set of reasons a run stops.

use serde::Serialize;

/// Why a run stopped, as one closed set.
///
/// The four outcomes this task has to keep apart - an external wall-clock
/// guard, a cooperative cancellation, an external tool being terminated, and
/// the search's own convergence - are four distinct variants here, so a caller
/// cannot report one as another by reading a free-text field loosely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// The search met its own convergence criterion. The only reason that is
    /// a positive result.
    Converged,
    /// The coupled-analysis budget was exhausted without convergence.
    EvaluationBudget,
    /// The configured iteration or generation count was exhausted without
    /// convergence.
    IterationLimit,
    /// A search stage's wall-clock limit was reached between generations.
    TimeBudget,
    /// The feasible best stopped improving while the population was still
    /// spread: a stop criterion, not convergence.
    Stagnated,
    /// The mesh contracted below its floor without meeting the improvement
    /// criterion.
    MeshLimit,
    /// The search's own internal wall-clock watchdog fired. Not convergence
    /// and not cancellation: a safety limit inside the optimizer.
    Watchdog,
    /// A supervisor set the cooperative cancellation flag and the search
    /// stopped on it.
    Cancelled,
    /// An external wall-clock guard outside the search expired. Distinct from
    /// [`Self::Cancelled`]: the guard is what *requests* cancellation, and a
    /// row that reports this reason is naming the requester.
    ExternalGuardTimeout,
    /// A supervised external solver process was polled out or force-killed.
    ExternalToolTerminated,
    /// The request itself was rejected before any search ran.
    InvalidInput,
    /// A reason the caller could not classify, kept verbatim rather than
    /// mapped onto a neighbour.
    Unclassified,
}

impl StopReason {
    /// Map the optimizer's own `termination` vocabulary onto this set.
    ///
    /// Unknown strings become [`Self::Unclassified`]; they are never folded
    /// into a nearby variant, because "we do not know why it stopped" and "it
    /// converged" must not be able to alias.
    pub fn from_termination(termination: &str) -> Self {
        match termination {
            "converged" => Self::Converged,
            "evaluation_budget" => Self::EvaluationBudget,
            "iteration_limit" => Self::IterationLimit,
            "time_budget" => Self::TimeBudget,
            "stagnated" => Self::Stagnated,
            "mesh_limit" => Self::MeshLimit,
            "watchdog" => Self::Watchdog,
            "cancelled" => Self::Cancelled,
            "invalid_input" => Self::InvalidInput,
            _ => Self::Unclassified,
        }
    }

    /// Whether this reason permits a run to be reported as converged.
    ///
    /// Exactly one variant does.
    pub const fn is_convergence(self) -> bool {
        matches!(self, Self::Converged)
    }
}
