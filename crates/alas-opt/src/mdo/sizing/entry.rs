// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The thin entry points of the sizing loop and its one shared failure.

use alas_config::AlasConfig;

use super::{run_candidate_cancellable, SizingOutcome};
use crate::mdo::types::{CandidateFailure, SizingControls};

/// A failure with the `mass_coordinates` reason, for the engine-binding
/// lookups beside the mass analysis.
///
/// This is deliberately **not** the seam for a station-placement failure. A
/// candidate whose main-gear station cannot be placed never reaches here: the
/// mass analysis it passes through first
/// (`build::mass_analysis_with_structural_feedback`) already classifies that
/// cause as `main_gear_station_not_measured` and propagates it with `?`, so
/// a search log can separate a missing gear datum from a degenerate geometry.
/// `a_missing_main_gear_datum_survives_the_mdo_sizing_entry_point` pins that.
/// What is bucketed under `mass_coordinates` here is the engine binding
/// beside the mass analysis, for the reason stated at each call site.
pub(super) fn mass_coordinates_failure() -> CandidateFailure {
    CandidateFailure {
        reason: "mass_coordinates",
    }
}

/// Build, size and trim one candidate design vector.
///
/// # Errors
///
/// [`CandidateFailure`] when the geometry, mass, payload layout or trim
/// solve fails: the candidate is not a physically evaluable aircraft.
#[cfg(test)]
pub(crate) fn run_candidate(
    config: &AlasConfig,
    x: &[f64],
) -> Result<SizingOutcome, CandidateFailure> {
    run_candidate_cancellable(config, x, None, false, None, SizingControls::default())
}

/// Run a candidate while preserving a caller-pinned clean-sheet fuselage
/// coordinate.  This is used by fixed desktop/reference reviews; ordinary
/// product optimization keeps the cabin-derived sizing behavior.
pub(crate) fn run_candidate_with_fuselage_policy(
    config: &AlasConfig,
    x: &[f64],
    preserve_explicit_fuselage_length: bool,
) -> Result<SizingOutcome, CandidateFailure> {
    run_candidate_cancellable(
        config,
        x,
        None,
        preserve_explicit_fuselage_length,
        None,
        SizingControls::default(),
    )
}
