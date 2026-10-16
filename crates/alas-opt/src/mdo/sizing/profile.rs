// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Event counters of one sizing closure for `alas_math::stage_profile`
//! (`ALAS_EVAL_PROFILE=1`); reads finished results only and changes none.

use alas_math::stage_profile::{count, Counter};

use super::super::mda::MdaClosure;
use super::super::mission_model::SegmentMissionModel;

/// Count the closure, its outer passes and re-trims, and the mission work
/// its model performed.
pub(super) fn record_closure(closure: &MdaClosure, model: &SegmentMissionModel) {
    count(Counter::Closures, 1);
    count(Counter::ClosurePasses, closure.sizing_iterations as u64);
    count(Counter::Retrims, closure.retrim_count as u64);
    count(Counter::TripFlights, model.work().map_or(0, |work| work.0));
    count(Counter::DeckEvals, model.propulsion.evaluation_count());
}
