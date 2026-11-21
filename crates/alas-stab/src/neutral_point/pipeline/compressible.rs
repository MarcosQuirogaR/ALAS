// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The compressible neutral point from probes already solved on the
//! Goethert-stretched lattice: what the compressible trim reads, so that its
//! static margin and its lift/moment slopes come from one stretched solve.

use alas_aero::vlm::{VlmError, VlmResult};
use alas_atmo::Atmosphere;
use alas_config::analysis::AnalysisConfig;
use alas_geom::aircraft::airplane::Airplane;

use super::{
    airframe_terms, corrected_np_from_solved_probes, resolution, zero_airframe_terms,
    HighLiftScaling, ProbeCondition,
};

/// [`super::clean_np_at_condition`] for two alpha probes `lo` and `hi` that
/// the caller already solved on `stretched = goethert_stretch(airplane, beta)`
/// at `velocity` and `atmosphere`: the wing and tail stations come from the
/// stretched probes and scale back by `beta`; the fuselage and nacelle terms
/// are formed on the physical `airplane` with the stretched, Mach-consistent
/// lift slope, exactly as the stated-Mach neutral point does.
///
/// Returns `(x_np, airframe_shift_m, body_lift)`: the neutral point, geometry
/// axes, m, the part of its station that the fuselage and nacelle terms moved
/// (negative = forward, destabilising; `0.0` when
/// `include_fuselage_stability` is off), and the wing-body interference lift
/// `(K_I - 1) a_w` per rad with its station (geometry axes, m), `(0.0, 0.0)`
/// when not applied, so the trim can carry the same airframe lift and moment
/// slope (STAB-02).
///
/// # Errors
///
/// See [`VlmError`] (the isolated-tail subset solve).
#[allow(clippy::too_many_arguments)]
pub(crate) fn compressible_np_from_stretched_probes(
    airplane: &Airplane,
    stretched: &Airplane,
    analysis: &AnalysisConfig,
    atmosphere: Atmosphere,
    velocity: f64,
    beta: f64,
    alpha_lo_deg: f64,
    alpha_hi_deg: f64,
    lo: &VlmResult,
    hi: &VlmResult,
) -> Result<(f64, f64, (f64, f64)), VlmError> {
    let condition = ProbeCondition {
        spanwise_resolution: resolution(analysis.spanwise_resolution),
        chordwise_resolution: resolution(analysis.chordwise_resolution),
        atmosphere,
        velocity,
        alpha_lo_deg,
        alpha_hi_deg,
    };
    let (x_np_stretched, diag) = corrected_np_from_solved_probes(
        stretched,
        &condition,
        lo,
        hi,
        analysis.tail_efficiency_or(stretched.horizontal_tail_dynamic_pressure_ratio()),
        HighLiftScaling::NONE,
        false,
        analysis.include_fuselage_stability,
    )?;
    let (fuselage_shift, nacelle_shift, _, _) = if analysis.include_fuselage_stability {
        airframe_terms(
            airplane,
            diag.cl_alpha_total,
            diag.wing_lift_fraction,
            diag.deps_dalpha_vlm,
        )
    } else {
        zero_airframe_terms(diag.deps_dalpha_vlm)
    };
    // The stretched solve is in stretched axes: back to geometry axes.
    let airframe_shift = fuselage_shift + nacelle_shift;
    // `a_w + extra = f' T` and `a_w + extra = K_I a_w`, so `extra = (K_I - 1)/K_I f' T`.
    let k_i = diag.wing_body_lift_factor;
    let body_lift = if (k_i - 1.0).abs() > 1e-12 && diag.x_wing_body_lift_m.is_finite() {
        (
            (k_i - 1.0) / k_i * diag.wing_lift_fraction * diag.cl_alpha_total,
            diag.x_wing_body_lift_m * beta,
        )
    } else {
        (0.0, 0.0)
    };
    Ok((
        x_np_stretched * beta + airframe_shift,
        airframe_shift,
        body_lift,
    ))
}
