// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! How the run that produced a delivered result ended.

/// How the run that produced the result ended.
///
/// Supplied by the caller because the pipeline result alone cannot say it: a
/// cancelled or budget-exhausted search still returns its incumbent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RunCompletion {
    /// The run ran to its own stated completion.
    #[default]
    Completed,
    /// The run finished without meeting its convergence criterion.
    NotConverged {
        /// What did not converge, in the producer's own words.
        detail: String,
    },
    /// The run was cancelled or stopped by a guard before completing.
    Cancelled {
        /// Why it stopped, in the producer's own words.
        detail: String,
    },
    /// The run is still executing: the result is a live snapshot whose
    /// feasibility has not been assessed yet.
    InProgress,
}

impl RunCompletion {
    /// The completion state of a run from the search that produced its
    /// design, or [`Self::Completed`] when no search ran (a fixed design has
    /// no convergence criterion to miss).
    ///
    /// A search that ran its own candidate plan to its end is
    /// [`Self::Completed`]: a staged search stops on a plan of candidates,
    /// and how far its best design was still moving is reported by its
    /// convergence indicator (`alas_opt::ConvergenceIndicator`), not by
    /// refusing the result. A search stopped by a clock or a rejection cap
    /// before its plan ended is [`Self::NotConverged`], with the search's
    /// termination label as the detail; a search stopped by its caller is
    /// [`Self::Cancelled`].
    #[must_use]
    pub fn of_search(search: Option<&alas_opt::OptimizationResult>) -> Self {
        let Some(search) = search else {
            return Self::Completed;
        };
        if search.was_cancelled() {
            Self::Cancelled {
                detail: format!("search termination: {}", search.termination),
            }
        } else if search.completed_plan()
            || search.termination == alas_opt::REGISTERED_DESIGN_RETAINED
            || search.termination == alas_opt::START_DESIGN_RETAINED
            || search.termination == alas_opt::REPORTING_FIDELITY_FALLBACK
                && search
                    .search_diagnostics
                    .as_ref()
                    .is_some_and(|diagnostics| diagnostics.plan_completed)
        {
            Self::Completed
        } else {
            Self::NotConverged {
                detail: format!("search termination: {}", search.termination),
            }
        }
    }
}
