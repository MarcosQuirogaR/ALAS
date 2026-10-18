// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Typed mass-bracket recovery; cancellation is never bracketable.
use super::*;

/// Whether `error` is a mass-dependent rating/energy shortfall that a
/// *different takeoff mass* can plausibly resolve, as opposed to a route,
/// polar/validity, or invalid-input failure that no mass in the bracket
/// fixes.
///
/// Only [`FuelModelError::NotConverged`] is ever bracketable, and only when
/// its message names a rating or energy deficit; the enum's other typed
/// variants (`RouteTooShort`, `InvalidDistance`, `MassOutOfRange`,
/// `InvalidModel`) are never mass-dependent in this sense and are excluded
/// outright. Within `NotConverged`, a `"polar validity"` or `"speed schedule
/// not attained"` rejection is excluded too: those are validity/schedule
/// breaches at the flown condition, not a power margin the search should
/// paper over by quietly flying a different mass.
pub(super) fn is_mass_bracketable(error: &FuelModelError) -> bool {
    match error {
        FuelModelError::NotConverged(reason) => {
            (reason.contains("deficit") || reason.contains("rating"))
                && !reason.contains("polar validity")
                && !reason.contains("speed schedule not attained")
        }
        FuelModelError::Cancelled
        | FuelModelError::RouteTooShort { .. }
        | FuelModelError::InvalidDistance { .. }
        | FuelModelError::MassOutOfRange { .. }
        | FuelModelError::InvalidModel(_) => false,
    }
}

/// Evaluate the plan at `candidate_kg`, or, if the burn model fails there
/// with a mass-bracketable error (see [`is_mass_bracketable`]), at the
/// highest mass between `zero_fuel_mass_kg` and `candidate_kg` where it
/// succeeds, by bisection.
///
/// This never invents a feasible point beyond what the model itself
/// supports, and never bisects past a non-mass-dependent failure: it only
/// stops a rating/energy shortfall strictly above a feasible root (an
/// overshoot during the Picard iteration, or a seed placed in an
/// unevaluable region) from blocking that root. A route, polar/validity, or
/// invalid-input failure (at the candidate or anywhere the bisection
/// probes) is returned immediately as that typed failure, not silently
/// treated as "too heavy, try lighter".
///
/// The bisection assumes the evaluable set is a single contiguous region
/// reaching down from `zero_fuel_mass_kg` (a heavier aircraft needs no less
/// power margin than a lighter one at the same condition, so a mass-
/// dependent deficit above some threshold does not reappear below it). It
/// does not sample every mass in `[zero_fuel_mass_kg, candidate_kg]`, so a
/// failure at both endpoints is read *under that assumption* (not as an
/// exhaustive proof the model is unevaluable at every point in between)
/// and reported as such.
#[derive(Debug)]
pub(super) enum DispatchSolveError {
    Cancelled,
    Failed(String),
}
impl From<String> for DispatchSolveError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}
impl From<FuelModelError> for DispatchSolveError {
    fn from(error: FuelModelError) -> Self {
        match error {
            FuelModelError::Cancelled => Self::Cancelled,
            other => Self::Failed(other.to_string()),
        }
    }
}

pub(super) fn evaluate_bracketed(
    plans: &mut PlanEvaluator<'_>,
    zero_fuel_mass_kg: f64,
    candidate_kg: f64,
) -> Result<(f64, FuelPlan), DispatchSolveError> {
    if candidate_kg <= zero_fuel_mass_kg {
        return plans
            .evaluate(candidate_kg)
            .map(|plan| (candidate_kg, plan))
            .map_err(DispatchSolveError::from);
    }
    let candidate_error = match plans.evaluate(candidate_kg) {
        Ok(plan) => return Ok((candidate_kg, plan)),
        Err(error) => error,
    };
    if !is_mass_bracketable(&candidate_error) {
        return Err(candidate_error.into());
    }
    if let Some(found) = bisect_on_inferred_outcomes(plans, zero_fuel_mass_kg, candidate_kg)? {
        return Ok(found);
    }
    let mut low = zero_fuel_mass_kg;
    let mut low_plan = plans.evaluate(low).map_err(|error| {
        if matches!(error, FuelModelError::Cancelled) { return DispatchSolveError::Cancelled; }
        DispatchSolveError::Failed(format!(
            "burn model failed at both ends of the {low:.1}-{candidate_kg:.1} kg search bracket ({error} at the zero-fuel mass, {candidate_error} at {candidate_kg:.1} kg); assuming a single contiguous evaluable region reaching down from the zero-fuel mass, this bracket has none, though every intermediate mass was not sampled"
        ))
    })?;
    let mut high = candidate_kg;
    for _ in 0..EVALUATION_BRACKET_ITERATIONS {
        if high - low <= EVALUATION_BRACKET_RESOLUTION_KG {
            break;
        }
        let mid = 0.5 * (low + high);
        match plans.evaluate(mid) {
            Ok(plan) => {
                low = mid;
                low_plan = plan;
            }
            Err(error) if is_mass_bracketable(&error) => high = mid,
            Err(error) => return Err(error.into()),
        }
    }
    Ok((low, low_plan))
}

/// The bisection of [`evaluate_bracketed`], flown without pricing a mass whose
/// outcome the masses already priced settle.
///
/// The bisection always starts from the whole bracket `[zero_fuel_mass_kg,
/// candidate_kg]`, so successive overshoots of the Picard iteration each walk
/// down to the same edge of the evaluable region through new midpoints, a
/// few dozen mission pricings per iteration for an edge already known to the
/// resolution. Here every midpoint the priced masses settle (see
/// [`PlanEvaluator::inferred`]) is read from them and only the midpoints
/// inside the still-unknown interval, and the mass finally returned, are
/// priced. The midpoints, their outcomes and the returned mass and plan are
/// those of the full bisection whenever the evaluable region is contiguous,
/// which is the assumption the bisection itself makes.
///
/// `None` when an inferred success proves unevaluable when priced (the region
/// is not contiguous after all): the caller then runs the full bisection,
/// which prices every midpoint.
fn bisect_on_inferred_outcomes(
    plans: &mut PlanEvaluator<'_>,
    zero_fuel_mass_kg: f64,
    candidate_kg: f64,
) -> Result<Option<(f64, FuelPlan)>, DispatchSolveError> {
    let mut low = zero_fuel_mass_kg;
    // Priced at `low`, when it has been; an inferred success has no plan yet.
    let mut low_plan: Option<FuelPlan> = None;
    let mut high = candidate_kg;
    if plans.inferred(low) != Some(Inferred::Evaluable) {
        // Not settled: price it exactly as the full bisection does, so a
        // failure at both ends reports the same way.
        return Ok(None);
    }
    for _ in 0..EVALUATION_BRACKET_ITERATIONS {
        if high - low <= EVALUATION_BRACKET_RESOLUTION_KG {
            break;
        }
        let mid = 0.5 * (low + high);
        let evaluable = match plans.inferred(mid) {
            Some(Inferred::Evaluable) => {
                low_plan = None;
                true
            }
            Some(Inferred::Unevaluable) => false,
            None => match plans.evaluate(mid) {
                Ok(plan) => {
                    low_plan = Some(plan);
                    true
                }
                Err(error) if is_mass_bracketable(&error) => false,
                Err(error) => return Err(error.into()),
            },
        };
        if evaluable {
            low = mid;
        } else {
            high = mid;
        }
    }
    let plan = match low_plan {
        Some(plan) => plan,
        None => match plans.evaluate(low) {
            Ok(plan) => plan,
            Err(FuelModelError::Cancelled) => return Err(DispatchSolveError::Cancelled),
            Err(_) => return Ok(None),
        },
    };
    Ok(Some((low, plan)))
}
