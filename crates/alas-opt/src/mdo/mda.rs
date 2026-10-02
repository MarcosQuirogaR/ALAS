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
//! vortex-lattice trim and its drag table, is re-run only when the centre of
//! gravity has moved by more than `retrim_cg_tolerance_pct_mac` percent of
//! the mean aerodynamic chord; the table spans the lift range, so a mass
//! change alone needs no new trim. A zero tolerance re-trims on any centre
//! of gravity change. The shift that was accepted without a re-trim is
//! reported as `MdaClosure::cg_shift_pct_mac`. Each pass flies its trips on
//! a plan frozen at the pass's takeoff mass, and the dispatch iteration
//! starts from that mass.
//!
//! `MtowSizing::FixedRequirement` takes one pass: the takeoff mass is the
//! requirement and the mission must fit under it. `MtowSizing::SizedByMission`
//! and `MtowSizing::Unconstrained` both iterate this same fixed point,
//! starting from the requirement value as the first pass's takeoff mass, but
//! differ in what stays bound to that requirement afterwards:
//! `SizedByMission` keeps it as the dispatch ceiling and as the Aitken
//! extrapolation's admissibility bound on every pass, while `Unconstrained`
//! drops both after the seed, so the requirement never reappears as a limit
//! anywhere in the closure, only the freely converging dispatch mass does.
//! `MtowBand` seeds at its target and clamps at the band's upper edge;
//! `PayloadAdjusted` seeds at the requirement and has no ceiling. Every
//! number comes from `alas_config::MtowPlan`.

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, MtowPlan};
use alas_geom::aircraft::airplane::Airplane;

use alas_mass::breakdown::{MassBreakdown, MassCoordinates, PayloadLayoutSummary};
use alas_mass::dispatch::{
    solve_dispatch_with_initial_guess, DispatchLimits, DispatchSolution, DispatchStatus,
};
use alas_mass::fuel_plan::FuelModelError;
use alas_payload::build::build_payload_layout;
use alas_payload::oew::oew_and_cg;

use super::build::mass_analysis_with_structural_feedback;
use super::mission_model::{
    FreezeError, FrozenMissionPlan, SegmentMissionModel, MISSION_STEP_UNCONVERGED,
    SIZING_BUDGET_EXHAUSTED,
};
use super::trim::{trim_and_polar, TrimmedPolar};
use super::types::CandidateFailure;

mod wing_box;
use alas_mass::wingbox_feedback::ReferenceWingMass;
pub(crate) use wing_box::PassWingBox;

/// What the loop reads but never changes.
pub(crate) struct MdaContext<'a> {
    /// The candidate's configuration (payload load case already applied).
    pub config: &'a AlasConfig,
    /// The design vector, for the aerodynamic analysis.
    pub dv: &'a DesignVector,
    /// Shared segment-integrated model with the candidate polar and engine
    /// terms filled in.
    pub model: SegmentMissionModel,
    /// Design-mission still-air distance, m.
    pub range_m: f64,
    /// The resolved takeoff-mass sizing plan: seed, ceilings, pass budget.
    pub plan: MtowPlan,
    /// Payload the dispatch closure flies instead of the laid-out load case,
    /// kg, when the plan closes on a design payload.
    pub dispatch_payload_kg: Option<f64>,
    /// Usable tank capacity, kg, when the tank arrangement resolved.
    pub tank_capacity_kg: Option<f64>,
    /// Whether the loop may re-trim; false when the polar was supplied by
    /// an external solver and must be held fixed.
    pub retrim_allowed: bool,
    /// Frozen empirical reference wing inventory for reference adaptation or
    /// the baseline sandbox. Clean-sheet runs leave this absent.
    pub structural_reference: Option<ReferenceWingMass>,
    /// Structural wing reconciliation at the initial mass pass.
    pub structural_feedback: alas_mass::wingbox_feedback::WingboxFeedback,
    /// Source and current-mass inventory completeness at the initial pass.
    pub structural_inventory_complete: bool,
    /// The primary box of the latest pass, reused by a pass whose sizing
    /// inputs are unchanged.
    pub wing_box: PassWingBox,
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
    /// Whether the outer takeoff-mass iteration settled within tolerance
    /// (always, for a plan that does not iterate), whatever limit the
    /// dispatch met: `sizing_closed` also requires a converged dispatch.
    pub takeoff_mass_settled: bool,
    /// Structural wing reconciliation at the final mass pass.
    pub structural_feedback: alas_mass::wingbox_feedback::WingboxFeedback,
    /// Source and current-mass inventory completeness at the final pass.
    pub structural_inventory_complete: bool,
    /// Trip plan in force on the last pass, after any re-freeze.
    pub frozen_plan: Option<FrozenMissionPlan>,
    /// Trip plans frozen, one per pass that could make one.
    pub plan_freezes: u32,
}

/// A failure with the `mass_coordinates` reason, for the one arm of
/// [`converge`] that keeps it panic-free.
///
/// This is deliberately **not** the seam for a station-placement failure.
/// Every mass and coordinate result this loop consumes comes from
/// `build::mass_analysis_with_structural_feedback`, which already classifies
/// a missing main-gear station as `main_gear_station_not_measured` and
/// propagates it with `?`; the candidate is rejected there, before the loop
/// body runs. The arm below is reached only if `max_passes` were zero, which
/// the caller's own `max(1)` prevents, so it names no physical cause at all.
fn mass_coordinates_failure() -> CandidateFailure {
    CandidateFailure {
        reason: "mass_coordinates",
    }
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

/// Re-evaluate the shared product state at one outer-loop takeoff mass.
///
/// The initial mass pass and every mission-sized pass use the same two-stage
/// order: establish the lumped operating-empty mass, place the detailed cabin
/// load against it, then close the FLOPS payload/fuel slots on that load. A
/// small final refresh at the dispatch solution is also allowed to make the
/// search state's ledger exactly the one a finalist report replays, without
/// inventing a second mass method or retaining a stale ceiling-mass layout.
// Every argument is one piece of the loop's shared state; bundling them would
// hide which pass-local quantity each re-evaluation updates.
#[allow(clippy::too_many_arguments)]
fn evaluate_state_at_tow(
    context: &MdaContext<'_>,
    plane: &mut Airplane,
    tow_k: f64,
    landing_floor_kg: Option<f64>,
    state: &mut MdaState,
    model: &mut SegmentMissionModel,
    structural_feedback: &mut alas_mass::wingbox_feedback::WingboxFeedback,
    cg_at_trim: &mut f64,
    retrim_count: &mut usize,
) -> Result<bool, CandidateFailure> {
    let config = context.config;
    // The closure mass is what the fuel remainder, the payload layout and
    // the trim read. Whether the *components* follow it is the design-mode
    // question `AlasConfig::at_closure_mass` answers: a registered aircraft
    // (`BaselineSandbox`, `ReferenceAdaptation`) keeps its declared design
    // gross and landing masses, so a mission-only change cannot re-size its
    // structure; a clean-sheet design couples, so every pass re-evaluates
    // the components at the current iterate. The MTOW band and
    // payload-adjusted modes design every structure at the iterate
    // (`AlasConfig::at_sized_closure_mass`).
    let pass_config = config.at_sized_closure_mass_with_landing_floor(tow_k, landing_floor_kg);
    // The lumped and the payload-placed buildups share this pass's configuration,
    // design and aircraft, so they share one sized primary box; a pass whose
    // structural inputs equal the previous pass's reuses that pass's box.
    let mut design_box = context.wing_box.reusable(&pass_config, plane);
    let (lumped_masses, lumped_coords, ..) = mass_analysis_with_structural_feedback(
        &pass_config,
        context.dv,
        plane,
        None,
        context.structural_reference,
        &mut design_box,
    )?;
    let (pass_oew, pass_x_oew) = oew_and_cg(&lumped_masses, &lumped_coords);
    let layout = build_payload_layout(plane, &pass_config, pass_oew, pass_x_oew).map_err(|_| {
        CandidateFailure {
            reason: "payload_layout",
        }
    })?;
    let summary = PayloadLayoutSummary {
        total_mass: layout.total_mass,
        cg_x: layout.cg_x,
        cg_y: layout.cg_y,
    };
    let (masses, coords, cg, feedback, _, inventory) = mass_analysis_with_structural_feedback(
        &pass_config,
        context.dv,
        plane,
        Some(&summary),
        context.structural_reference,
        &mut design_box,
    )?;
    if let Some(sized) = design_box {
        context.wing_box.store(&pass_config, plane, sized);
    }
    *structural_feedback = feedback;
    state.masses = masses;
    state.coords = coords;
    state.cg = cg;
    // The mission model is coupled to the trimmed drag, and the trim is the
    // expensive discipline in this loop: vortex-lattice solves plus the drag
    // table. The table spans the clean lift range at the trim's centre of
    // gravity (`drag_table`), so a mass change, which only moves the lift
    // coefficient along it, needs no new trim; the attitude follows it
    // through the trim Jacobian (`TrimmedPolar::at_cruise_cl`). The centre
    // of gravity changes the tail load and with it the trimmed induced drag,
    // so the loop re-trims when it has moved by more than
    // `retrim_cg_tolerance_pct_mac` percent of the mean aerodynamic chord. A
    // tolerance of zero re-trims on any change.
    //
    // The residual shift that was accepted is reported through
    // `MdaClosure::cg_shift_pct_mac`, so a candidate never claims a polar it
    // did not solve at.
    if context.retrim_allowed {
        let tolerance_pct = config
            .optimizer
            .objective
            .retrim_cg_tolerance_pct_mac
            .max(0.0);
        let mac = plane.c_ref.max(1e-9);
        let cg_shift_pct_mac = (cg[0] - *cg_at_trim).abs() / mac * 100.0;
        if cg_shift_pct_mac > tolerance_pct {
            state.polar = trim_and_polar(config, plane, cg[0], context.dv, tow_k)?;
            *cg_at_trim = cg[0];
            *retrim_count += 1;
            *model = model
                .clone()
                .with_cruise_drag(state.polar.drag.cruise_drag());
        }
    }
    Ok(inventory.is_complete())
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
    let plan = &context.plan;
    // The plan's seed is the first pass's takeoff mass under every mode (see
    // the module doc comment). `FixedRequirement` takes exactly that one
    // pass; every mission-closed mode iterates it with the configured budget.
    let seed = plan.seed_kg;
    let sizing_iterates = plan.iterates;
    let max_passes = if sizing_iterates {
        objective.sizing_max_iterations.max(1) as usize
    } else {
        1
    };
    // The landing-mass limit follows the design-weight basis
    // (`alas_config::MassSizingBasis`), not the `MtowSizing` mode: a fixed
    // aircraft (`BaselineSandbox`, `ReferenceAdaptation`, or any mode with a
    // declared `flops_structure.design_gross_mass_kg` override) is designed
    // to one landing weight regardless of what the dispatch mass converges
    // to, and only a coupled clean-sheet closure recomputes the limit from
    // the current iterate. The two design modes design a registered
    // aircraft's structure at the iterate too, with its declared MLW/MTOW
    // ratio (`AlasConfig::design_landing_mass_at_closure`).
    //
    // The dispatch-loop MTOW clamp and the Aitken extrapolation's
    // admissible-estimate upper bound are the plan's: the declared ceiling
    // for the two original ceiling-bound modes, the band's upper edge for
    // `MtowBand`, and no bound for `Unconstrained` and `PayloadAdjusted`
    // (a finite sentinel for the dispatch solver, which validates a finite
    // MTOW; only the finite/positive checks in `aitken` remain there).
    let dispatch_mtow_kg = plan.dispatch_ceiling_kg;
    let aitken_ceiling = plan.aitken_ceiling_kg;
    let tolerance_kg = objective.sizing_tolerance_kg;
    // The report and the search both consume the converged state, but the
    // dispatch map returns the *next* takeoff mass. A final state refresh below
    // synchronizes the shared ledger when the configured outer tolerance is
    // met without changing the user's convergence criterion.
    let ledger_sync_tolerance_kg = tolerance_kg.min(1.0e-6);
    let mac = plane.c_ref.max(1e-9);

    let mut state = initial;
    let mut cg_at_trim = state.cg[0];
    let mut tow_k = seed;
    let mut iterates: Vec<f64> = vec![tow_k];
    let mut model = context.model.clone();
    let mut outcome: Option<(DispatchSolution, bool)> = None;
    let mut sizing_iterations = 0;
    let mut retrim_count = 0;
    let mut structural_feedback = context.structural_feedback;
    // A fixed requirement keeps the evaluated first-pass inventory. Every
    // refreshed state replaces it with the inventory of that same mass pass.
    let mut structural_inventory_complete = context.structural_inventory_complete;
    // The reserve-covering landing floor of the last dispatch solution, which
    // the next pass designs the gear to (`AlasConfig::design_landing_mass_with_reserve_floor`).
    let mut landing_floor_kg: Option<f64> = None;
    // Every pass flies its trips on one plan frozen at the pass's takeoff
    // mass (`SegmentMissionModel::freeze_plan`): the cruise levels, climb
    // revisions, step climbs and step count are discrete, so letting each
    // Picard mass re-choose them made the closure depend on where the
    // dispatch iteration started and on the integration step count. With
    // the plan a function of the pass mass alone, the fixed point is a
    // property of the aircraft. A plan the aircraft cannot fly at a later
    // mass of the same pass is re-frozen there and the dispatch restarted
    // on it, at most `frozen_plan::MAX_REFREEZES` times.
    let policy = model.profile.cruise_altitude_policy;
    let mut plan_freezes = 0_u32;
    let mut frozen_plan = None;
    let budget_exhausted = || CandidateFailure {
        reason: SIZING_BUDGET_EXHAUSTED,
    };
    let budget_spent = |model: &SegmentMissionModel| match (model.budget(), model.work()) {
        (Some(budget), Some((flights, deck_evals))) => {
            flights > u64::from(budget.max_trip_flights) || deck_evals > budget.max_deck_evals
        }
        _ => false,
    };
    let max_outer_passes = model.budget().map(|budget| budget.max_outer_passes);

    for pass in 0..max_passes {
        model.check_cancelled().map_err(|_| CandidateFailure {
            reason: "cancelled",
        })?;
        if max_outer_passes.is_some_and(|limit| u32::try_from(pass).map_or(true, |p| p >= limit)) {
            return Err(budget_exhausted());
        }
        sizing_iterations = pass + 1;
        if pass > 0 {
            structural_inventory_complete = evaluate_state_at_tow(
                context,
                plane,
                tow_k,
                landing_floor_kg,
                &mut state,
                &mut model,
                &mut structural_feedback,
                &mut cg_at_trim,
                &mut retrim_count,
            )?;
        }
        let (oew_k, _) = oew_and_cg(&state.masses, &state.coords);
        let limits = DispatchLimits {
            mtow_kg: dispatch_mtow_kg,
            mzfw_kg: None,
            mlw_kg: Some(config.design_landing_mass_with_reserve_floor(tow_k, landing_floor_kg)),
            usable_capacity_kg: context.tank_capacity_kg,
        };
        let payload_kg = context.dispatch_payload_kg.unwrap_or(state.masses.payload);
        // Warm start: the dispatch iteration starts from this pass's mass,
        // the previous pass's solution (or its Aitken extrapolation).
        let solve = |pass_model: &SegmentMissionModel| {
            solve_dispatch_with_initial_guess(
                oew_k + payload_kg,
                tow_k,
                context.range_m,
                &config.fuel_policy,
                pass_model,
                &limits,
                max_passes.max(objective.sizing_max_iterations.max(1) as usize),
                tolerance_kg,
            )
        };
        let (pass_model, solution) =
            match model.solve_on_frozen_plans(tow_k, context.range_m, &policy, &solve) {
                Ok(frozen) => {
                    plan_freezes += 1;
                    frozen
                }
                Err(FreezeError::Fuel(FuelModelError::Cancelled)) => {
                    return Err(CandidateFailure {
                        reason: "cancelled",
                    })
                }
                // An under-resolved trip is a numerical failure of the
                // candidate, not a reason to fly the dispatch unfrozen.
                Err(FreezeError::StepUnconverged { .. }) => {
                    return Err(CandidateFailure {
                        reason: MISSION_STEP_UNCONVERGED,
                    })
                }
                Err(_) if budget_spent(&model) => return Err(budget_exhausted()),
                // No plan can be made at this mass (the trip is not flyable
                // there even with level and climb adaptation): the dispatch
                // brackets its own way to a flyable mass, adapting per flight.
                Err(_) => (model.clone(), solve(&model)),
            };
        if matches!(solution.status, DispatchStatus::Cancelled) {
            return Err(CandidateFailure {
                reason: "cancelled",
            });
        }
        if budget_spent(&model) {
            return Err(budget_exhausted());
        }
        frozen_plan = pass_model.frozen_plan();
        landing_floor_kg = super::mtow_modes::landing_floor_kg(&solution);
        if !sizing_iterates {
            outcome = Some((solution, true));
            break;
        }
        let next_tow = solution.takeoff_mass_kg;
        let delta_kg = (next_tow - tow_k).abs();
        let configured_done = delta_kg < tolerance_kg;
        if configured_done && delta_kg >= ledger_sync_tolerance_kg {
            // `solution` is the mass at which the report will replay the
            // finalist. Refresh the search state at that exact mass before
            // returning, so every component and station is quoted from the
            // same pure-FLOPS evaluation even when the configured tolerance
            // is intentionally looser than floating-point report roundoff.
            //
            // The refresh is a mass-ledger sync only. `solution` was priced
            // on `pass_model`'s drag, and that drag is what the candidate
            // carries into its report (`CandidateFuelArtifacts::drag`), so a
            // re-trim here would hand the report a polar the closure never
            // flew and break report trip = closure trip. A re-trim the sync
            // would take is undone; the centre-of-gravity shift it saw is
            // reported through `MdaClosure::cg_shift_pct_mac`.
            let trimmed = (state.polar.clone(), cg_at_trim, retrim_count, plane.xyz_ref);
            structural_inventory_complete = evaluate_state_at_tow(
                context,
                plane,
                next_tow,
                landing_floor_kg,
                &mut state,
                &mut model,
                &mut structural_feedback,
                &mut cg_at_trim,
                &mut retrim_count,
            )?;
            if retrim_count != trimmed.2 {
                (state.polar, cg_at_trim, retrim_count, plane.xyz_ref) = trimmed;
            }
        }
        outcome = Some((solution, configured_done));
        if configured_done {
            break;
        }
        iterates.push(next_tow);
        // Every third iterate is extrapolated from the two plain passes
        // before it; the plain iterate stands when the extrapolation is
        // not admissible.
        tow_k = match iterates.as_slice() {
            [.., x0, x1, x2] if iterates.len() % 3 == 0 => {
                aitken(*x0, *x1, *x2, aitken_ceiling).unwrap_or(next_tow)
            }
            _ => next_tow,
        };
    }

    let Some((dispatch, takeoff_mass_settled)) = outcome else {
        // `max_passes` is at least one, so the loop always assigns
        // `outcome`; this arm only keeps the function panic-free.
        return Err(mass_coordinates_failure());
    };
    let cg_shift_pct_mac = (state.cg[0] - cg_at_trim).abs() / mac * 100.0;
    let sizing_closed =
        takeoff_mass_settled && matches!(dispatch.status, DispatchStatus::Converged);
    Ok(MdaClosure {
        dispatch,
        state,
        sizing_iterations,
        retrim_count,
        cg_shift_pct_mac,
        sizing_closed,
        takeoff_mass_settled,
        structural_feedback,
        structural_inventory_complete,
        frozen_plan,
        plan_freezes,
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
