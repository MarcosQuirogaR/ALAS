// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The take-off field of a judgement on every ground plane it must take.
//!
//! A built airplane's lift-off lift ([`crate::envelope::liftoff_lifts`]) is
//! one per ground plane `alas_mass::stations::ground_plane_bases` names: the
//! declared plane, or both ends of the documented clearance sample when the
//! clearance is undeclared. The field is flown whole on each plane (`VMU`, the
//! rotation, the wing heights of the roll and the air legs all from that
//! plane) and the longer field is kept with its own screen point: the ends
//! are two airframes, never channels mixed from different ends.
//!
//! The nominal plane between the ends is not flown. It cannot exceed the
//! longer end when `TOFL` is quasi-convex in the clearance over the sample
//! (no interior maximum). That is a condition, not a theorem: as the gear
//! lengthens the tail-strike attitude rises, so a geometry-limited `VMU`
//! and the lift-off speed fall (convexly: `VMU ~ CL^-1/2` of a lift linear
//! in a concave attitude), while the wing rises out of ground effect and
//! the roll and air-leg drag grow. The residual tests check the condition
//! on the undeclared A320-200 and E195-E2 within a stated tolerance.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;

use super::{takeoff_field_outcome, ResidualInputs, ScreenPoint};
use crate::envelope::{liftoff_lifts, LiftOffLift};
use crate::mdo::types::{ConstraintFamily, ConstraintResidual};
use crate::mdo::ResidualRole;

/// Id of the Diagnostic row of a take-off field without its evidence.
const EVIDENCE_GAP: &str = "takeoff_field_evidence_gap";

/// The Diagnostic row of a take-off field the evidence cannot bind.
pub(super) fn evidence_gap() -> ConstraintResidual {
    ConstraintResidual::direct(
        EVIDENCE_GAP,
        ConstraintFamily::Performance,
        1.0,
        0.0,
        "bool",
        1.0,
        1.0,
        ResidualRole::Diagnostic,
    )
}

/// The field is the integration (a jet with its speeds and evidence, not
/// the legacy correlation); otherwise the correlation, which has no rotation.
pub(super) fn flies_physical(inputs: &ResidualInputs<'_>) -> bool {
    !inputs.perf.legacy_field_correlations
        && inputs.speeds.is_some()
        && inputs.engine_out.is_some_and(|model| !model.is_turboprop())
}

/// A residual row and the screen point of its field.
type FieldOutcome = (ConstraintResidual, Option<ScreenPoint>);

/// The `takeoff_field` residual and the screen point of `plane`
/// ([`takeoff_field_outcome`]) flown on every ground plane of
/// [`liftoff_lifts`], the longer field kept ([`longer_field`]); once without
/// a lift-off schedule when no plane builds the lift or the field is the
/// correlation. A lift built on some planes only is the evidence gap.
pub(in crate::mdo::residuals_performance) fn field_on_planes(
    inputs: &ResidualInputs<'_>,
    plane: &Airplane,
    config: &AlasConfig,
) -> FieldOutcome {
    if !flies_physical(inputs) {
        return takeoff_field_outcome(inputs, None);
    }
    match liftoff_lifts(plane, config) {
        Err(reason) => (evidence_gap().with_detail(reason), None),
        Ok(lifts) => lifts
            .into_iter()
            .map(|lift| takeoff_field_outcome(inputs, Some(lift)))
            .reduce(longer_field)
            .unwrap_or_else(|| takeoff_field_outcome(inputs, None)),
    }
}

/// The longer of two fields of one judgement: an evidence-gap row wins (a
/// plane without the evidence leaves the judgement without it), then the
/// longer field, a field the physics cannot complete counting as the
/// longest; each kept with its own screen point.
pub(super) fn longer_field(first: FieldOutcome, second: FieldOutcome) -> FieldOutcome {
    if first.0.id == EVIDENCE_GAP {
        return first;
    }
    if second.0.id == EVIDENCE_GAP {
        return second;
    }
    // `TOFL - TODA` against the same runway orders the two lengths.
    let length = |row: &ConstraintResidual| {
        if row.raw_residual.is_finite() {
            row.raw_residual
        } else {
            f64::INFINITY
        }
    };
    if length(&second.0) > length(&first.0) {
        second
    } else {
        first
    }
}

/// Fly `fly` on every ground plane of [`liftoff_lifts`] for `plane` and keep
/// the result of the longest `length` (a non-finite length counts as the
/// longest), as the optimiser residual does; once without a lift-off
/// schedule when there is no airplane or no plane builds the lift.
///
/// # Errors
///
/// The take-off field evidence gap of a lift built on some planes only, or
/// the first error of `fly`.
pub fn longest_on_ground_planes<R>(
    plane: Option<&Airplane>,
    config: &AlasConfig,
    mut fly: impl FnMut(Option<LiftOffLift>) -> Result<R, String>,
    length: impl Fn(&R) -> f64,
) -> Result<R, String> {
    let lifts = match plane {
        Some(plane) => liftoff_lifts(plane, config)?,
        None => Vec::new(),
    };
    let key = |field: &R| {
        let l = length(field);
        if l.is_finite() {
            l
        } else {
            f64::INFINITY
        }
    };
    let mut longest: Option<R> = None;
    for lift in lifts {
        let field = fly(Some(lift))?;
        if longest.as_ref().is_none_or(|kept| key(&field) > key(kept)) {
            longest = Some(field);
        }
    }
    match longest {
        Some(field) => Ok(field),
        None => fly(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(tofl_m: f64) -> FieldOutcome {
        let row = ConstraintResidual::scaled(
            "takeoff_field",
            ConstraintFamily::Performance,
            2_000.0,
            tofl_m,
            "m",
            tofl_m - 2_000.0,
            ResidualRole::Constraint,
        );
        (row, None)
    }

    #[test]
    fn the_longer_field_is_kept_and_a_gap_wins() {
        assert_eq!(
            longer_field(field(1_800.0), field(1_900.0)).0.limit,
            1_900.0
        );
        assert_eq!(
            longer_field(field(1_900.0), field(1_800.0)).0.limit,
            1_900.0
        );
        let failed = field(f64::NAN);
        assert!(longer_field(field(1_900.0), failed).0.limit.is_nan());
        let gap = (evidence_gap(), None);
        assert_eq!(longer_field(gap.clone(), field(1_900.0)).0.id, EVIDENCE_GAP);
        assert_eq!(longer_field(field(1_900.0), gap).0.id, EVIDENCE_GAP);
    }
}
