// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The VLM probe/mesh plumbing behind [`super::neutral_point_conditions`] and
//! `crate::trim`'s product paths: bundling the operating condition
//! ([`ProbeCondition`]), the per-surface/fuselage/nacelle assembly
//! ([`corrected_np_from_probes`], [`corrected_np_from_solved_probes`]), and
//! the Goethert-stretched, stated-Mach evaluation ([`clean_np_at_condition`]).
//! Split out of `super` purely to keep each file under the workspace's
//! per-module line budget; see that module's doc for the physics.

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::{VlmError, VlmSystem};
use alas_atmo::Atmosphere;
use alas_config::analysis::AnalysisConfig;
use alas_geom::aircraft::airplane::Airplane;

use super::surfaces::{
    beta_for_mach, combine_scaled, goethert_stretch, per_surface_contributions, tail_slope_loss,
};
use super::NpDiagnostics;
use crate::trim::{resolution, HSTAB_NAME, MAIN_WING_NAME};

mod aeroelastic;
mod airframe;
pub(super) mod high_lift;
pub(crate) use aeroelastic::elastic_np_shift;
pub(crate) use high_lift::HighLiftScaling;
pub(crate) mod compressible;
use airframe::{airframe_terms, zero_airframe_terms};

/// The mesh resolution and two-alpha operating condition every VLM probe in
/// this module is taken at: bundled so call sites (here and in
/// `crate::trim`) pass one value instead of six positional ones.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProbeCondition {
    pub(crate) spanwise_resolution: usize,
    pub(crate) chordwise_resolution: usize,
    pub(crate) atmosphere: Atmosphere,
    pub(crate) velocity: f64,
    pub(crate) alpha_lo_deg: f64,
    pub(crate) alpha_hi_deg: f64,
}

impl ProbeCondition {
    fn d_alpha_rad(&self) -> f64 {
        (self.alpha_hi_deg - self.alpha_lo_deg).to_radians()
    }
}

/// The corrected physical neutral point (F1-F3) at `condition`: no Mach
/// stretch is applied here (the caller is responsible for that, see
/// [`clean_np_at_condition`]). `eta_tail` and `wing_slope_scale` set the F1
/// tail weighting (`wing_slope_scale` is the factor by which extended flaps and
/// slats raise the main wing's lift slope, `1.0` for the clean case);
/// `include_airframe_terms` gates the F2/F3
/// fuselage and nacelle corrections (matching `AnalysisConfig::include_fuselage_stability`'s
/// existing role).
///
/// # Errors
///
/// See [`VlmError`].
fn corrected_np_from_probes(
    airplane: &Airplane,
    condition: &ProbeCondition,
    eta_tail: f64,
    high_lift: HighLiftScaling,
    include_airframe_terms: bool,
    body_lift: bool,
) -> Result<(f64, NpDiagnostics), VlmError> {
    let system = VlmSystem::assemble(
        airplane,
        condition.spanwise_resolution,
        condition.chordwise_resolution,
    )?;
    let op = |alpha_deg| {
        OperatingPoint::new(
            condition.atmosphere,
            condition.velocity,
            alpha_deg,
            0.0,
            0.0,
            0.0,
            0.0,
        )
    };
    let lo = system.solve(&op(condition.alpha_lo_deg))?;
    let hi = system.solve(&op(condition.alpha_hi_deg))?;
    corrected_np_from_solved_probes(
        airplane,
        condition,
        &lo,
        &hi,
        eta_tail,
        high_lift,
        include_airframe_terms,
        body_lift,
    )
}

/// The product-path neutral point from two already-solved probes at
/// `analysis`'s own condition: what `crate::trim`'s
/// non-`reference_compatibility` branches call.
///
/// # Errors
///
/// See [`VlmError`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn product_neutral_point(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    atmosphere: Atmosphere,
    velocity: f64,
    alpha_lo_deg: f64,
    alpha_hi_deg: f64,
    lo: &alas_aero::vlm::VlmResult,
    hi: &alas_aero::vlm::VlmResult,
) -> Result<f64, VlmError> {
    let condition = ProbeCondition {
        spanwise_resolution: resolution(analysis.spanwise_resolution),
        chordwise_resolution: resolution(analysis.chordwise_resolution),
        atmosphere,
        velocity,
        alpha_lo_deg,
        alpha_hi_deg,
    };
    corrected_np_from_solved_probes(
        airplane,
        &condition,
        lo,
        hi,
        analysis.tail_efficiency_or(airplane.horizontal_tail_dynamic_pressure_ratio()),
        HighLiftScaling::NONE,
        analysis.include_fuselage_stability,
        analysis.include_fuselage_stability,
    )
    .map(|(x_np, _)| x_np)
}

/// [`corrected_np_from_probes`], taking two already-solved VLM probes
/// instead of solving them: for a caller (`crate::trim`) that already paid
/// for the same two-alpha solve and would otherwise duplicate it.
///
/// # Errors
///
/// See [`VlmError`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn corrected_np_from_solved_probes(
    airplane: &Airplane,
    condition: &ProbeCondition,
    lo: &alas_aero::vlm::VlmResult,
    hi: &alas_aero::vlm::VlmResult,
    eta_tail: f64,
    high_lift: HighLiftScaling,
    include_airframe_terms: bool,
    body_lift: bool,
) -> Result<(f64, NpDiagnostics), VlmError> {
    let shared = np_shared_terms(airplane, condition, lo, hi)?;
    Ok(np_from_shared_terms(
        airplane,
        &shared,
        eta_tail,
        high_lift,
        include_airframe_terms,
        body_lift,
    ))
}

/// Every intermediate [`corrected_np_from_solved_probes`] term that does not
/// depend on the tail downwash increment: the per-surface contributions, the
/// Mach-consistent total lift-curve slope, and the VLM-implied downwash
/// gradient at the tail (which itself pays for an isolated-tail solve on a
/// one-wing subset mesh). [`neutral_point_conditions`]'s low-speed and
/// high-lift conditions probe the same stretched geometry at the same
/// Mach/altitude and differ only in `wing_slope_scale`, so this is the part
/// they share.
///
/// [`neutral_point_conditions`]: super::neutral_point_conditions
struct NpSharedTerms {
    contributions: Vec<super::SurfaceContribution>,
    main_wing_index: usize,
    tail_wing_index: Option<usize>,
    cl_alpha_total: f64,
    wing_lift_fraction: f64,
    deps_dalpha_vlm: f64,
}

/// The tail-deps_extra-independent half of [`corrected_np_from_solved_probes`].
///
/// # Errors
///
/// See [`VlmError`]: the isolated-tail subset mesh failing to assemble or
/// solve.
fn np_shared_terms(
    airplane: &Airplane,
    condition: &ProbeCondition,
    lo: &alas_aero::vlm::VlmResult,
    hi: &alas_aero::vlm::VlmResult,
) -> Result<NpSharedTerms, VlmError> {
    let spanwise_resolution = condition.spanwise_resolution;
    let chordwise_resolution = condition.chordwise_resolution;
    let d_alpha = condition.d_alpha_rad();
    let contributions = per_surface_contributions(airplane, lo, hi);
    let d_lift_sum: f64 = contributions.iter().map(|c| c.d_lift).sum();
    let cl_alpha_total = if d_alpha.abs() > 1e-9 {
        (hi.cl_lift - lo.cl_lift) / d_alpha
    } else {
        f64::NAN
    };

    let main_wing_index = airplane
        .wings
        .iter()
        .position(|w| w.name == MAIN_WING_NAME)
        .unwrap_or(0);
    let tail_wing_index = airplane.wings.iter().position(|w| w.name == HSTAB_NAME);
    let wing_lift_fraction = if d_lift_sum.abs() > 1e-9 {
        contributions[main_wing_index].d_lift / d_lift_sum
    } else {
        1.0
    };

    let deps_dalpha_vlm = match tail_wing_index {
        Some(idx) if d_lift_sum.abs() > 1e-9 && cl_alpha_total.is_finite() => {
            let a_t_in_presence = cl_alpha_total * contributions[idx].d_lift / d_lift_sum;
            let mut subset = airplane.clone();
            subset.wings = vec![airplane.wings[idx].clone()];
            let iso_system =
                VlmSystem::assemble(&subset, spanwise_resolution, chordwise_resolution)?;
            let op = |alpha_deg| {
                OperatingPoint::new(
                    condition.atmosphere,
                    condition.velocity,
                    alpha_deg,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                )
            };
            let iso_lo = iso_system.solve(&op(condition.alpha_lo_deg))?;
            let iso_hi = iso_system.solve(&op(condition.alpha_hi_deg))?;
            let isolated = (iso_hi.cl_lift - iso_lo.cl_lift) / d_alpha;
            if isolated.abs() > 1e-9 {
                (1.0 - a_t_in_presence / isolated).clamp(0.0, 0.9)
            } else {
                0.0
            }
        }
        _ => 0.0,
    };

    Ok(NpSharedTerms {
        contributions,
        main_wing_index,
        tail_wing_index,
        cl_alpha_total,
        wing_lift_fraction,
        deps_dalpha_vlm,
    })
}

/// The tail-deps_extra-dependent half of [`corrected_np_from_solved_probes`]:
/// the tail-weighted combine and, when asked, the fuselage/nacelle terms
/// (themselves independent of `wing_slope_scale`, but gated the same way the
/// undivided function gated them).
fn np_from_shared_terms(
    airplane: &Airplane,
    shared: &NpSharedTerms,
    eta_tail: f64,
    high_lift: HighLiftScaling,
    include_airframe_terms: bool,
    body_lift: bool,
) -> (f64, NpDiagnostics) {
    // A lattice that already carries the tail's effectiveness as the
    // surface's dynamic-pressure ratio (`Wing::dynamic_pressure_ratio`, set by
    // the product pipeline) has applied it to the tail's lift: weight the
    // tail by the remainder only, so the efficiency counts once.
    let eta_tail = eta_tail
        / airplane
            .horizontal_tail_dynamic_pressure_ratio()
            .max(1.0e-6);
    // High-lift condition: extended flaps (and slats, if declared) add wing area
    // and so raise the wing slope by `r = 1 + delta_flap + delta_slat` (Raymer
    // p. 413); the added lift acts at the quarter chord of the extended section,
    // which moves the wing's aerodynamic centre. The downwash gradient at the
    // tail keeps its clean value: USAF DATCOM (1978, Sec. 4.4.1, "Downwash Due
    // to Flap Deflection", pp. 4.4.1-2 and 4.4.1-8/-9, Fig. 4.4.1-72) gives
    // the flap's effect on the downwash as an angle increment
    // `Delta eps = f(h_H/(b/2), flap type) Delta C_L / (A b_f/(b/2))`,
    // proportional to the flap lift increment at small angles of attack, a
    // trim term and not a change of `d eps / d alpha`; its gradient method
    // has no flap term. (Scaling the gradient with the flapped wing slope,
    // Torenbeek's clean eq. E-52 extrapolated, is not what either source
    // states.)
    let wing_slope_scale = high_lift.slope_scale();
    let eps = shared.deps_dalpha_vlm;
    let eps_scaled = eps;
    let tail_loss = tail_slope_loss(eps_scaled - eps, eps);
    let wing_fraction = shared.wing_lift_fraction.clamp(0.0, 1.0);
    let tail_slope_factor = 1.0 - tail_loss;
    let total_scale = wing_slope_scale * wing_fraction + tail_slope_factor * (1.0 - wing_fraction);
    let mut cl_alpha_total = shared.cl_alpha_total * total_scale;
    let mut wing_lift_fraction = if total_scale > 1e-9 {
        wing_slope_scale * wing_fraction / total_scale
    } else {
        wing_fraction
    };
    let x_wing_alone_ac = shared
        .contributions
        .get(shared.main_wing_index)
        .map(|c| c.x_ac)
        .unwrap_or(airplane.xyz_ref[0]);
    let wing_ac_shift = high_lift.ac_shift(x_wing_alone_ac);
    let x_np_surfaces = combine_scaled(
        &shared.contributions,
        Some(shared.main_wing_index),
        wing_slope_scale,
        wing_ac_shift,
        shared.tail_wing_index,
        eta_tail,
        tail_loss,
    )
    .unwrap_or(airplane.xyz_ref[0]);

    // Wing-body lift interference, Torenbeek 1982 E-33/E-34 (p. 479): the
    // body carries `(K_I - 1) a_w` of extra lift at the exposed root
    // mid-chord. It joins the total slope and the wing-body share, and moves
    // the surface neutral point to the lift-weighted station.
    let mut x_np_surfaces = x_np_surfaces;
    let mut wing_body_lift_factor = 1.0;
    let mut x_wing_body_lift_m = f64::NAN;
    if body_lift {
        let a_w = cl_alpha_total * wing_lift_fraction;
        if let Some((k_i, x_extra)) = airframe::wing_body_lift_interference(airplane, a_w) {
            let extra = (k_i - 1.0) * a_w;
            let total = cl_alpha_total + extra;
            if total.abs() > 1e-9 {
                x_np_surfaces = (cl_alpha_total * x_np_surfaces + extra * x_extra) / total;
                wing_lift_fraction = (a_w + extra) / total;
                cl_alpha_total = total;
                wing_body_lift_factor = k_i;
                x_wing_body_lift_m = x_extra;
            }
        }
    }

    let (fuselage_shift, nacelle_shift, fuselage_diag, nacelle_diag) = if include_airframe_terms {
        airframe_terms(airplane, cl_alpha_total, wing_lift_fraction, eps_scaled)
    } else {
        zero_airframe_terms(eps_scaled)
    };

    let x_np = x_np_surfaces + fuselage_shift + nacelle_shift;
    (
        x_np,
        NpDiagnostics {
            x_wing_alone_ac,
            x_np_surfaces,
            cl_alpha_total,
            wing_lift_fraction,
            deps_dalpha_vlm: eps_scaled,
            fuselage: fuselage_diag,
            nacelle_terms: nacelle_diag,
            nacelle_shift_m: nacelle_shift,
            wing_body_lift_factor,
            x_wing_body_lift_m,
        },
    )
}

/// [`clean_np_at_condition`] evaluated at the same Mach/altitude for two
/// `wing_slope_scale` values at once, reusing one stretched-mesh assembly,
/// one pair of alpha probes, the isolated-tail downwash solve and the
/// fuselage/nacelle terms: none of those depend on `wing_slope_scale`, only
/// the final tail-weighted combine does (see [`np_shared_terms`]).
/// [`neutral_point_conditions`]'s low-speed and high-lift conditions are
/// exactly this: the same condition, differing only in the high-lift
/// downwash increment. Fixed to two values (rather than a slice) because
/// that is every caller today and it keeps the return type a plain pair
/// instead of a `Vec` a caller must length-check.
///
/// # Errors
///
/// See [`VlmError`].
///
/// [`neutral_point_conditions`]: super::neutral_point_conditions
type NpResultPair = ((f64, NpDiagnostics), (f64, NpDiagnostics));

pub(crate) fn clean_np_at_conditions_sharing_probe(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    mach: f64,
    altitude_m: f64,
    high_lift_a: HighLiftScaling,
    high_lift_b: HighLiftScaling,
) -> Result<NpResultPair, VlmError> {
    let atmosphere = Atmosphere::new(altitude_m);
    let beta = beta_for_mach(mach);
    let condition = ProbeCondition {
        spanwise_resolution: resolution(analysis.spanwise_resolution),
        chordwise_resolution: resolution(analysis.chordwise_resolution),
        atmosphere,
        velocity: mach * atmosphere.speed_of_sound(),
        alpha_lo_deg: analysis.autobalance_alpha_low_deg,
        alpha_hi_deg: analysis.autobalance_alpha_high_deg,
    };
    let stretched = goethert_stretch(airplane, beta);
    let system = VlmSystem::assemble(
        &stretched,
        condition.spanwise_resolution,
        condition.chordwise_resolution,
    )?;
    let op = |alpha_deg| {
        OperatingPoint::new(
            condition.atmosphere,
            condition.velocity,
            alpha_deg,
            0.0,
            0.0,
            0.0,
            0.0,
        )
    };
    let lo = system.solve(&op(condition.alpha_lo_deg))?;
    let hi = system.solve(&op(condition.alpha_hi_deg))?;
    let shared = np_shared_terms(&stretched, &condition, &lo, &hi)?;

    let finish = |high_lift: HighLiftScaling| -> (f64, NpDiagnostics) {
        let (x_np_stretched, mut diag) = np_from_shared_terms(
            &stretched,
            &shared,
            analysis.tail_efficiency_or(stretched.horizontal_tail_dynamic_pressure_ratio()),
            high_lift.stretched(beta),
            false,
            analysis.include_fuselage_stability,
        );
        // The stretched solve gave the wing/tail contribution and the
        // Mach-consistent slopes; rescale the wing/tail stations back to
        // physical geometry axes, then add the airframe terms on the
        // physical airplane, matching `clean_np_at_condition`.
        diag.x_np_surfaces = x_np_stretched * beta;
        diag.x_wing_alone_ac *= beta;
        diag.x_wing_body_lift_m *= beta;

        let (fuselage_shift, nacelle_shift, fuselage_diag, nacelle_diag) =
            if analysis.include_fuselage_stability {
                airframe_terms(
                    airplane,
                    diag.cl_alpha_total,
                    diag.wing_lift_fraction,
                    diag.deps_dalpha_vlm,
                )
            } else {
                zero_airframe_terms(diag.deps_dalpha_vlm)
            };
        diag.fuselage = fuselage_diag;
        diag.nacelle_terms = nacelle_diag;
        diag.nacelle_shift_m = nacelle_shift;

        (diag.x_np_surfaces + fuselage_shift + nacelle_shift, diag)
    };

    Ok((finish(high_lift_a), finish(high_lift_b)))
}

/// [`corrected_np_from_probes`] at a stated Mach/altitude, through the
/// Goethert stretch (F4): the wing/tail mesh is solved stretched, then its
/// station outputs are scaled back by `beta`; the fuselage and nacelle terms
/// (F2/F3) are formed from `airplane`'s own, unstretched geometry, combined
/// with the Mach-consistent `cl_alpha_total`/`deps_dalpha_vlm` the stretched
/// solve produced (see the parent module doc: this is an approximation for
/// the fuselage/nacelle terms, exact only for the lifting-surface mesh under
/// linear subsonic theory).
///
/// # Errors
///
/// See [`VlmError`].
pub(crate) fn clean_np_at_condition(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    mach: f64,
    altitude_m: f64,
    high_lift: HighLiftScaling,
) -> Result<(f64, NpDiagnostics), VlmError> {
    let atmosphere = Atmosphere::new(altitude_m);
    let beta = beta_for_mach(mach);
    let condition = ProbeCondition {
        spanwise_resolution: resolution(analysis.spanwise_resolution),
        chordwise_resolution: resolution(analysis.chordwise_resolution),
        atmosphere,
        velocity: mach * atmosphere.speed_of_sound(),
        alpha_lo_deg: analysis.autobalance_alpha_low_deg,
        alpha_hi_deg: analysis.autobalance_alpha_high_deg,
    };
    let stretched = goethert_stretch(airplane, beta);
    let (x_np_stretched, mut diag) = corrected_np_from_probes(
        &stretched,
        &condition,
        analysis.tail_efficiency_or(stretched.horizontal_tail_dynamic_pressure_ratio()),
        high_lift.stretched(beta),
        false,
        analysis.include_fuselage_stability,
    )?;
    // The stretched solve gave the wing/tail contribution and the
    // Mach-consistent slopes; rescale the wing/tail stations back to
    // physical geometry axes, then add the airframe terms on the physical
    // airplane.
    diag.x_np_surfaces = x_np_stretched * beta;
    diag.x_wing_alone_ac *= beta;
    diag.x_wing_body_lift_m *= beta;

    let (fuselage_shift, nacelle_shift, fuselage_diag, nacelle_diag) =
        if analysis.include_fuselage_stability {
            airframe_terms(
                airplane,
                diag.cl_alpha_total,
                diag.wing_lift_fraction,
                diag.deps_dalpha_vlm,
            )
        } else {
            zero_airframe_terms(diag.deps_dalpha_vlm)
        };
    diag.fuselage = fuselage_diag;
    diag.nacelle_terms = nacelle_diag;
    diag.nacelle_shift_m = nacelle_shift;

    Ok((diag.x_np_surfaces + fuselage_shift + nacelle_shift, diag))
}
