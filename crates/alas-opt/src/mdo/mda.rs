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
//! requirement and the mission must fit under it. `MtowSizing::SizedByMission`
//! and `MtowSizing::Unconstrained` both iterate this same fixed point,
//! starting from the requirement value as the first pass's takeoff mass, but
//! differ in what stays bound to that requirement afterwards:
//! `SizedByMission` keeps it as the dispatch ceiling and as the Aitken
//! extrapolation's admissibility bound on every pass, while `Unconstrained`
//! drops both after the seed, so the requirement never reappears as a limit
//! anywhere in the closure, only the freely converging dispatch mass does.

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, MassSizingBasis, MtowSizing};
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::breakdown::{MassBreakdown, MassCoordinates, PayloadLayoutSummary};
use alas_mass::dispatch::{solve_dispatch, DispatchLimits, DispatchSolution, DispatchStatus};
use alas_payload::build::build_payload_layout;
use alas_payload::oew::oew_and_cg;

use super::build::mass_analysis_with_structural_feedback;
use super::mission_model::SegmentMissionModel;
use super::trim::{trim_and_polar, TrimmedPolar};
use super::types::CandidateFailure;
use alas_mass::wingbox_feedback::ReferenceWingMass;

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
    /// Structural wing reconciliation at the final mass pass.
    pub structural_feedback: alas_mass::wingbox_feedback::WingboxFeedback,
}

fn mass_coordinates_failure() -> CandidateFailure {
    CandidateFailure {
        reason: "mass_coordinates",
    }
}

/// The declared-requirement dispatch ceiling `MtowSizing::Unconstrained`
/// substitutes with, so far above any physically credible transport takeoff
/// mass that [`alas_mass::dispatch::solve_dispatch`]'s own `.min(mtow_kg)`
/// clamp and its `required_unclamped_kg > mtow_kg + tolerance_kg` boundary
/// check are both inert: the free-converged mass this closure ever produces
/// is not observed anywhere near it.
///
/// This cannot be `f64::INFINITY`, even though the outer loop's own Aitken
/// admissibility bound (below) can and does use it: `solve_dispatch`'s
/// `validate_inputs` requires `limits.mtow_kg.is_finite()` and reports an
/// infinite MTOW as a `DispatchStatus::ModelFailed` input error before the
/// Picard loop ever runs, which would fail every Unconstrained pass outright.
const UNCONSTRAINED_DISPATCH_MTOW_KG: f64 = 1.0e9;

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
    state: &mut MdaState,
    model: &mut SegmentMissionModel,
    structural_feedback: &mut alas_mass::wingbox_feedback::WingboxFeedback,
    cg_at_trim: &mut f64,
    retrim_count: &mut usize,
) -> Result<(), CandidateFailure> {
    let config = context.config;
    // The closure mass is what the fuel remainder, the payload layout and
    // the trim read. Whether the *components* follow it is the design-mode
    // question `AlasConfig::at_closure_mass` answers: a registered aircraft
    // (`BaselineSandbox`, `ReferenceAdaptation`) keeps its declared design
    // gross and landing masses, so a mission-only change cannot re-size its
    // structure; a clean-sheet design couples, so every pass re-evaluates
    // the components at the current iterate.
    let pass_config = config.at_closure_mass(tow_k);
    let (lumped_masses, lumped_coords, ..) = mass_analysis_with_structural_feedback(
        &pass_config,
        context.dv,
        plane,
        None,
        context.structural_reference,
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
    let (masses, coords, cg, feedback, _, _) = mass_analysis_with_structural_feedback(
        &pass_config,
        context.dv,
        plane,
        Some(&summary),
        context.structural_reference,
    )?;
    *structural_feedback = feedback;
    state.masses = masses;
    state.coords = coords;
    state.cg = cg;
    // The mission model is coupled to the trimmed polar. Re-trim on every
    // refreshed mass state so a CG change cannot leave fuel sizing on a stale
    // induced-drag model.
    if context.retrim_allowed {
        state.polar = trim_and_polar(config, plane, cg[0], context.dv, tow_k)?;
        *cg_at_trim = cg[0];
        *retrim_count += 1;
        model.cd0 = state.polar.cd0;
        model.induced_factor_k = state.polar.induced_factor_k;
        model.wave_drag_cd = state.polar.wave_drag_cd;
        model.validate().map_err(|_| CandidateFailure {
            reason: "trim_solve",
        })?;
    }
    Ok(())
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
    // The declared requirement seeds the first pass under every mode (see
    // the module doc comment). `FixedRequirement` takes exactly that one
    // pass; `SizedByMission` and `Unconstrained` both iterate it, so both
    // get the configured pass budget instead of one.
    let ceiling = config.requirements.mtow_kg;
    let unconstrained = objective.mtow_sizing == MtowSizing::Unconstrained;
    let sizing_iterates = objective.mtow_sizing != MtowSizing::FixedRequirement;
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
    // to, so every pass (under `FixedRequirement`, `SizedByMission` and
    // `Unconstrained` alike) uses the sizing basis's own
    // `design_landing_mass_kg`. Only a coupled clean-sheet closure recomputes
    // the limit from the current takeoff-mass iterate, because there the
    // landing mass is defined as a fraction of whatever the design converges
    // to.
    let basis = config.mass_sizing_basis();
    // The dispatch-loop MTOW clamp: the declared ceiling for the two
    // ceiling-bound modes, unchanged, and a sentinel so far above any
    // credible transport mass that it never binds for `Unconstrained` (see
    // its doc comment for why this cannot be `f64::INFINITY`).
    let dispatch_mtow_kg = if unconstrained {
        UNCONSTRAINED_DISPATCH_MTOW_KG
    } else {
        ceiling
    };
    // The Aitken extrapolation's admissible-estimate upper bound: the same
    // declared ceiling for the two ceiling-bound modes, and no bound at all
    // for `Unconstrained`, where only the finite/positive checks in
    // `aitken` still guard against genuine divergence.
    let aitken_ceiling = if unconstrained {
        f64::INFINITY
    } else {
        ceiling
    };
    let tolerance_kg = objective.sizing_tolerance_kg;
    // The report and the search both consume the converged state, but the
    // dispatch map returns the *next* takeoff mass. A final state refresh below
    // synchronizes the shared ledger when the configured outer tolerance is
    // met without changing the user's convergence criterion.
    let ledger_sync_tolerance_kg = tolerance_kg.min(1.0e-6);
    let mac = plane.c_ref.max(1e-9);

    let mut state = initial;
    let mut cg_at_trim = state.cg[0];
    let mut tow_k = ceiling;
    let mut iterates: Vec<f64> = vec![tow_k];
    let mut model = context.model.clone();
    let mut outcome: Option<(DispatchSolution, bool)> = None;
    let mut sizing_iterations = 0;
    let mut retrim_count = 0;
    let mut structural_feedback = context.structural_feedback;

    for pass in 0..max_passes {
        sizing_iterations = pass + 1;
        if pass > 0 {
            evaluate_state_at_tow(
                context,
                plane,
                tow_k,
                &mut state,
                &mut model,
                &mut structural_feedback,
                &mut cg_at_trim,
                &mut retrim_count,
            )?;
        }
        let (oew_k, _) = oew_and_cg(&state.masses, &state.coords);
        let mlw_kg = match basis {
            MassSizingBasis::FixedAircraft {
                design_landing_mass_kg,
                ..
            } => design_landing_mass_kg,
            MassSizingBasis::Coupled => config.landing_mass_limit_kg(tow_k),
        };
        let limits = DispatchLimits {
            mtow_kg: dispatch_mtow_kg,
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
        let dispatch_converged = matches!(solution.status, DispatchStatus::Converged);
        if !sizing_iterates {
            outcome = Some((solution, dispatch_converged));
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
            evaluate_state_at_tow(
                context,
                plane,
                next_tow,
                &mut state,
                &mut model,
                &mut structural_feedback,
                &mut cg_at_trim,
                &mut retrim_count,
            )?;
        }
        outcome = Some((solution, configured_done && dispatch_converged));
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
        structural_feedback,
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
