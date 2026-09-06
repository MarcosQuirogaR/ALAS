// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The candidate's geometry, mass and trimmed aerodynamic operating point --
//! the part of the legacy evaluation that does not depend on the takeoff
//! mass, so it is built exactly once per candidate.
//!
//! This reuses the same public crate calls, in the same order, as
//! `crate::objective_evaluate`'s legacy path: geometry build, candidate
//! payload load case, a two-pass mass analysis with the payload layout
//! summary, the cruise stall guard, and `stability_and_trim` followed by
//! `AeroAnalysis::trimmed_performance`. A design vector that fails any of
//! these is not a physically evaluable aircraft, independent of which
//! mission quantity the search is minimising.

use alas_aero::analysis::{AeroAnalysis, TrimPoint};
use alas_atmo::Atmosphere;
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    run_mass_analysis_with_model_checked_product_with_gear, MassBreakdown, MassCoordinateModel,
    MassCoordinates, PayloadLayoutSummary,
};
use alas_payload::build::build_payload_layout;
use alas_payload::oew::oew_and_cg;
use alas_stab::trim::stability_and_trim;

use crate::objective::apply_candidate_payload_load_case;

use super::types::CandidateFailure;

/// The trimmed drag-polar terms a Breguet model is built from, alongside the
/// history-facing angle labels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TrimmedPolar {
    /// Zero-lift drag coefficient at the trimmed cruise point.
    pub cd0: f64,
    /// Induced-drag factor `k` implied by the trimmed point, guarded to a
    /// positive finite value.
    pub induced_factor_k: f64,
    /// Trimmed lift-to-drag ratio.
    pub lift_to_drag: f64,
    /// Compressibility-corrected reporting angle of attack, degrees.
    pub alpha_deg: f64,
    /// Trimmed horizontal-stabilizer incidence, degrees.
    pub incidence_deg: f64,
    /// Neutral-point station in geometry axes, m.
    pub x_np: f64,
}

/// Floor applied to a degenerate induced-drag factor so the Breguet model
/// never divides by a zero or negative `k`. Physically `k` is always
/// positive for a lifting wing; this only guards a pathological polar fit.
const MIN_INDUCED_FACTOR: f64 = 1.0e-4;

/// Failure with the `geometry_build` reason, for every early exit below.
fn geometry_build_failure() -> CandidateFailure {
    CandidateFailure {
        reason: "geometry_build",
    }
}

/// Build the candidate's geometry and payload-recomputed configuration.
pub(crate) fn build_geometry(
    config: &AlasConfig,
    x: &[f64],
) -> Result<(AlasConfig, DesignVector, Airplane), CandidateFailure> {
    let dv = DesignVector::from_array(x).map_err(|_| geometry_build_failure())?;
    let mut candidate_config = config.clone();
    if apply_candidate_payload_load_case(&mut candidate_config, &dv).is_err() {
        return Err(geometry_build_failure());
    }
    let builder = AircraftBuilder::new(Some(candidate_config.geometry.clone()));
    let plane = builder
        .build(Some(&dv), false)
        .map_err(|_| geometry_build_failure())?;
    if plane.s_ref <= 0.0 || plane.c_ref <= 0.0 {
        return Err(geometry_build_failure());
    }
    Ok((candidate_config, dv, plane))
}

/// Run the two-pass mass analysis at `config.requirements.mtow_kg`, the
/// ceiling every sizing pass starts from, and return the masses, coordinates,
/// physical CG and the payload-layout summary later passes reuse.
pub(crate) fn first_mass_pass(
    config: &AlasConfig,
    plane: &Airplane,
) -> Result<
    (
        MassBreakdown,
        MassCoordinates,
        [f64; 3],
        PayloadLayoutSummary,
    ),
    CandidateFailure,
> {
    let req = &config.requirements;
    let mass_failure = || CandidateFailure {
        reason: "mass_coordinates",
    };
    let initial = run_mass_analysis_with_model_checked_product_with_gear(
        plane,
        req,
        &config.geometry,
        &config.cabin,
        &config.control_surfaces,
        Some(&config.mass_model),
        None,
        MassCoordinateModel::StructuralWingbox(&config.structures),
        &config.landing_gear,
    );
    let (m1, c1, _cg1) = initial.map_err(|_| mass_failure())?;
    let (oew, x_oew) = oew_and_cg(&m1, &c1);
    let payload_layout =
        build_payload_layout(plane, config, oew, x_oew).map_err(|_| CandidateFailure {
            reason: "payload_layout",
        })?;
    let summary = PayloadLayoutSummary {
        total_mass: payload_layout.total_mass,
        cg_x: payload_layout.cg_x,
        cg_y: payload_layout.cg_y,
    };
    let second = run_mass_analysis_with_model_checked_product_with_gear(
        plane,
        req,
        &config.geometry,
        &config.cabin,
        &config.control_surfaces,
        Some(&config.mass_model),
        Some(&summary),
        MassCoordinateModel::StructuralWingbox(&config.structures),
        &config.landing_gear,
    );
    let (masses, coords, cg) = second.map_err(|_| mass_failure())?;
    Ok((masses, coords, cg, summary))
}

/// Trim the candidate at the ceiling loading state's cruise lift coefficient
/// and evaluate the trimmed drag polar exactly once.
pub(crate) fn trim_and_polar(
    config: &AlasConfig,
    plane: &mut Airplane,
    cg_x: f64,
    dv: &DesignVector,
    cruise_mass_kg: f64,
) -> Result<TrimmedPolar, CandidateFailure> {
    let req = &config.requirements;
    let trim_failure = || CandidateFailure {
        reason: "trim_solve",
    };
    plane.xyz_ref[0] = cg_x;

    let atmo = Atmosphere::new(req.cruise_altitude_m);
    let velocity_m_s = req.cruise_mach * atmo.speed_of_sound();
    let dynamic_pressure_pa = 0.5 * atmo.density() * velocity_m_s * velocity_m_s;
    // `W / (q S)` at the mass the loop is sizing, which equals
    // `DesignRequirements::required_cruise_cl` at the takeoff-mass ceiling.
    let cl_target = cruise_mass_kg * req.gravity_m_s2 / (dynamic_pressure_pa * plane.s_ref);

    // A candidate too close to stall to fly the required cruise CL has no
    // physically valid trim point. The mission-sized reason vocabulary is
    // deliberately the same four labels the legacy path uses, so this is
    // folded into `trim_solve` rather than adding a fifth.
    if cl_target > req.max_cruise_cl || cl_target <= 0.0 {
        return Err(trim_failure());
    }

    let trim = stability_and_trim(
        plane,
        &config.analysis,
        cl_target,
        req.cruise_mach,
        req.cruise_altitude_m,
    )
    .map_err(|_| trim_failure())?;

    let aero = AeroAnalysis::new(
        plane,
        dv.sweep_deg,
        Some(config.geometry.clone()),
        Some(config.drag_model.clone()),
        Some(config.analysis.clone()),
    );
    let trim_point = TrimPoint {
        trim_alpha_deg: trim.trim_alpha_deg,
        trim_ih_deg: trim.trim_ih_deg,
        cl_alpha: trim.cl_alpha,
    };
    let perf = aero
        .trimmed_performance(&trim_point, req.cruise_mach, req.cruise_altitude_m)
        .map_err(|_| trim_failure())?;
    let cd0 = aero.parasite_drag(
        req.cruise_mach,
        req.cruise_altitude_m,
        cl_target,
        None,
        None,
    );

    if !perf.l_over_d.is_finite() || perf.l_over_d <= 0.0 {
        return Err(trim_failure());
    }
    let induced_factor_k_raw = (perf.cd - cd0) / (perf.cl * perf.cl);
    let induced_factor_k = if induced_factor_k_raw.is_finite() && induced_factor_k_raw > 0.0 {
        induced_factor_k_raw
    } else {
        MIN_INDUCED_FACTOR
    };

    Ok(TrimmedPolar {
        cd0,
        induced_factor_k,
        lift_to_drag: perf.l_over_d,
        alpha_deg: perf.alpha_deg,
        incidence_deg: perf.incidence_deg,
        x_np: trim.x_np,
    })
}
