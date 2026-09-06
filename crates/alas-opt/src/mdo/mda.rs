// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The multidisciplinary sizing loop: mass and centre of gravity at the
//! current takeoff mass, trim and drag polar at that mass and centre of
//! gravity, mission fuel at that polar, and the takeoff mass those imply,
//! repeated until the design weights stop changing.
//!
//! This is the converged-weights loop every sizing environment runs around
//! its disciplines (FLOPS, FAST-OAD, TASOPT, NASA Ames' Faber): a
//! Gauss-Seidel fixed point on the takeoff mass, accelerated here with
//! Aitken's delta-squared extrapolation because the map contracts by a
//! roughly constant factor per pass. The expensive discipline, the
//! vortex-lattice trim, is re-run only when the centre of gravity has moved
//! by more than the configured fraction of the mean chord since the last
//! trim; a zero tolerance keeps the single trim at the takeoff-mass ceiling.
//!
//! `MtowSizing::FixedRequirement` takes one pass: the takeoff mass is the
//! requirement and the mission must fit under it.

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, MtowSizing};
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::breakdown::{
    run_mass_analysis_with_model_checked_product_with_gear, MassBreakdown, MassCoordinateModel,
    MassCoordinates, PayloadLayoutSummary,
};
use alas_mass::breguet::BreguetFuelModel;
use alas_mass::dispatch::{solve_dispatch, DispatchLimits, DispatchSolution, DispatchStatus};
use alas_payload::oew::oew_and_cg;

use super::build::{trim_and_polar, TrimmedPolar};
use super::types::CandidateFailure;

/// What the loop reads but never changes.
pub(crate) struct MdaContext<'a> {
    /// The candidate's configuration (payload load case already applied).
    pub config: &'a AlasConfig,
    /// The design vector, for the aerodynamic analysis.
    pub dv: &'a DesignVector,
    /// Payload layout the mass analysis places.
    pub summary: &'a PayloadLayoutSummary,
    /// Breguet model with every term except the polar filled in.
    pub model: BreguetFuelModel,
    /// Design-mission still-air distance, m.
    pub range_m: f64,
    /// Usable tank capacity, kg, when the tank arrangement resolved.
    pub tank_capacity_kg: Option<f64>,
    /// Whether the loop may re-trim; false when the polar was supplied by
    /// an external solver and must be held fixed.
    pub retrim_allowed: bool,
}

/// The coupled state at one pass.
pub(crate) struct MdaState {
    pub masses: MassBreakdown,
    pub coords: MassCoordinates,
    pub cg: [f64; 3],
    pub polar: TrimmedPolar,
}

/// The converged (or budget-exhausted) loop.
pub(crate) struct MdaClosure {
    pub dispatch: DispatchSolution,
    pub state: MdaState,
    /// Outer passes taken, including the first.
    pub sizing_iterations: usize,
    /// Trim solves after the first.
    pub retrim_count: usize,
    /// Centre-of-gravity shift, percent MAC, between the last trim and the
    /// converged state.
    pub cg_shift_pct_mac: f64,
    /// Whether the takeoff mass settled within tolerance and the dispatch
    /// model never failed.
    pub sizing_closed: bool,
}

fn mass_coordinates_failure() -> CandidateFailure {
    CandidateFailure {
        reason: "mass_coordinates",
    }
}

fn is_model_failed(status: &DispatchStatus) -> bool {
    matches!(status, DispatchStatus::ModelFailed(_))
}

/// Aitken delta-squared extrapolation of the fixed-point iterates
/// `x0 -> x1 -> x2`, or `None` when the denominator vanishes or the
/// estimate leaves `(0, ceiling]`.
fn aitken(x0: f64, x1: f64, x2: f64, ceiling: f64) -> Option<f64> {
    let denominator = (x2 - x1) - (x1 - x0);
    if denominator.abs() < 1e-9 {
        return None;
    }
    let estimate = x2 - (x2 - x1) * (x2 - x1) / denominator;
    (estimate.is_finite() && estimate > 0.0 && estimate <= ceiling).then_some(estimate)
}

/// Run the loop from `initial`, which was evaluated at the takeoff-mass
/// ceiling. `plane` is re-trimmed in place when the centre of gravity moves.
pub(crate) fn converge(
    context: &MdaContext<'_>,
    plane: &mut Airplane,
    initial: MdaState,
) -> Result<MdaClosure, CandidateFailure> {
    let config = context.config;
    let objective = &config.optimizer.objective;
    let ceiling = config.requirements.mtow_kg;
    let mlw_kg = ceiling * config.mass_model.mlw_fraction_mtow;
    let sized_by_mission = objective.mtow_sizing == MtowSizing::SizedByMission;
    let max_passes = if sized_by_mission {
        objective.sizing_max_iterations.max(1) as usize
    } else {
        1
    };
    let tolerance_kg = objective.sizing_tolerance_kg;
    let retrim_tolerance = objective.retrim_cg_tolerance_pct_mac;
    let mac = plane.c_ref.max(1e-9);

    let mut state = initial;
    let mut cg_at_trim = state.cg[0];
    let mut tow_k = ceiling;
    let mut iterates: Vec<f64> = vec![tow_k];
    let mut model = context.model;
    let mut outcome: Option<(DispatchSolution, bool)> = None;
    let mut sizing_iterations = 0;
    let mut retrim_count = 0;

    for pass in 0..max_passes {
        sizing_iterations = pass + 1;
        if pass > 0 {
            let mut pass_requirements = config.requirements.clone();
            pass_requirements.mtow_kg = tow_k;
            let (masses, coords, cg) = run_mass_analysis_with_model_checked_product_with_gear(
                plane,
                &pass_requirements,
                &config.geometry,
                &config.cabin,
                &config.control_surfaces,
                Some(&config.mass_model),
                Some(context.summary),
                MassCoordinateModel::StructuralWingbox(&config.structures),
                &config.landing_gear,
            )
            .map_err(|_| mass_coordinates_failure())?;
            state.masses = masses;
            state.coords = coords;
            state.cg = cg;
            let shift_pct = (cg[0] - cg_at_trim).abs() / mac * 100.0;
            if context.retrim_allowed && retrim_tolerance > 0.0 && shift_pct > retrim_tolerance {
                state.polar = trim_and_polar(config, plane, cg[0], context.dv, tow_k)?;
                cg_at_trim = cg[0];
                retrim_count += 1;
                model.cd0 = state.polar.cd0;
                model.induced_factor_k = state.polar.induced_factor_k;
                model.validate().map_err(|_| CandidateFailure {
                    reason: "trim_solve",
                })?;
            }
        }
        let (oew_k, _) = oew_and_cg(&state.masses, &state.coords);
        let limits = DispatchLimits {
            mtow_kg: ceiling,
            mzfw_kg: None,
            mlw_kg: Some(mlw_kg),
            usable_capacity_kg: context.tank_capacity_kg,
        };
        let solution = solve_dispatch(
            oew_k + state.masses.payload,
            context.range_m,
            &config.fuel_policy,
            &model,
            &limits,
            max_passes.max(objective.sizing_max_iterations.max(1) as usize),
            tolerance_kg,
        );
        let model_failed = is_model_failed(&solution.status);
        if !sized_by_mission {
            outcome = Some((solution, !model_failed));
            break;
        }
        let next_tow = solution.takeoff_mass_kg;
        let delta_kg = (next_tow - tow_k).abs();
        let done = delta_kg < tolerance_kg;
        outcome = Some((solution, done && !model_failed));
        if done {
            break;
        }
        iterates.push(next_tow);
        // Every third iterate is extrapolated from the two plain passes
        // before it; the plain iterate stands when the extrapolation is
        // not admissible.
        tow_k = match iterates.as_slice() {
            [.., x0, x1, x2] if iterates.len() % 3 == 0 => {
                aitken(*x0, *x1, *x2, ceiling).unwrap_or(next_tow)
            }
            _ => next_tow,
        };
    }

    let Some((dispatch, sizing_closed)) = outcome else {
        // `max_passes` is at least one, so the loop always assigns
        // `outcome`; this arm only keeps the function panic-free.
        return Err(mass_coordinates_failure());
    };
    let cg_shift_pct_mac = (state.cg[0] - cg_at_trim).abs() / mac * 100.0;
    Ok(MdaClosure {
        dispatch,
        state,
        sizing_iterations,
        retrim_count,
        cg_shift_pct_mac,
        sizing_closed,
    })
}

#[cfg(test)]
mod tests {
    use super::aitken;

    #[test]
    fn aitken_reaches_the_fixed_point_of_a_linear_contraction_in_one_step() {
        // x -> 0.25 x + 75 has the fixed point 100; three iterates from 0
        // extrapolate to it exactly.
        let g = |x: f64| 0.25 * x + 75.0;
        let x0 = 0.0;
        let x1 = g(x0);
        let x2 = g(x1);
        let estimate = aitken(x0, x1, x2, 1_000.0).unwrap_or_else(|| panic!("admissible"));
        assert!((estimate - 100.0).abs() < 1e-9);
    }

    #[test]
    fn a_stalled_or_out_of_range_extrapolation_is_declined() {
        assert!(aitken(1.0, 1.0, 1.0, 10.0).is_none());
        assert!(aitken(0.0, 10.0, 15.0, 12.0).is_none());
    }
}
