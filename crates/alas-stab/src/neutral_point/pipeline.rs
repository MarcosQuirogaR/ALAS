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

use super::surfaces::{combine, goethert_stretch, per_surface_contributions};
use super::{fuselage, nacelles, FuselageTerms, NacelleTerm, NpDiagnostics};
use crate::trim::{hstab, main_wing, resolution, AC_CHORD_FRACTION, HSTAB_NAME, MAIN_WING_NAME};

/// Beta floor shared with [`goethert_stretch`], kept here too so
/// `1.0 - mach*mach` is never evaluated at `mach >= 1.0` (this module has no
/// transonic/supersonic model).
const MAX_MACH_FOR_BETA: f64 = 0.99;

pub(crate) fn beta_for_mach(mach: f64) -> f64 {
    let m = mach.abs().min(MAX_MACH_FOR_BETA);
    (1.0 - m * m).sqrt()
}

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
/// [`clean_np_at_condition`]). `eta_tail` and `tail_deps_extra` set the F1
/// tail weighting (`tail_deps_extra` is the high-lift downwash increment,
/// `0.0` for the clean case); `include_airframe_terms` gates the F2/F3
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
    tail_deps_extra: f64,
    include_airframe_terms: bool,
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
        tail_deps_extra,
        include_airframe_terms,
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
        analysis.tail_efficiency,
        0.0,
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
pub(crate) fn corrected_np_from_solved_probes(
    airplane: &Airplane,
    condition: &ProbeCondition,
    lo: &alas_aero::vlm::VlmResult,
    hi: &alas_aero::vlm::VlmResult,
    eta_tail: f64,
    tail_deps_extra: f64,
    include_airframe_terms: bool,
) -> Result<(f64, NpDiagnostics), VlmError> {
    let shared = np_shared_terms(airplane, condition, lo, hi)?;
    Ok(np_from_shared_terms(
        airplane,
        &shared,
        eta_tail,
        tail_deps_extra,
        include_airframe_terms,
    ))
}

/// Every intermediate [`corrected_np_from_solved_probes`] term that does not
/// depend on the tail downwash increment: the per-surface contributions, the
/// Mach-consistent total lift-curve slope, and the VLM-implied downwash
/// gradient at the tail (which itself pays for an isolated-tail solve on a
/// one-wing subset mesh). [`neutral_point_conditions`]'s low-speed and
/// high-lift conditions probe the same stretched geometry at the same
/// Mach/altitude and differ only in `tail_deps_extra`, so this is the part
/// they share.
///
/// [`neutral_point_conditions`]: super::neutral_point_conditions
struct NpSharedTerms {
    contributions: Vec<super::SurfaceContribution>,
    main_wing_index: usize,
    tail_wing_index: Option<usize>,
    cl_alpha_total: f64,
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
        deps_dalpha_vlm,
    })
}

/// The tail-deps_extra-dependent half of [`corrected_np_from_solved_probes`]:
/// the tail-weighted combine and, when asked, the fuselage/nacelle terms
/// (themselves independent of `tail_deps_extra`, but gated the same way the
/// undivided function gated them).
fn np_from_shared_terms(
    airplane: &Airplane,
    shared: &NpSharedTerms,
    eta_tail: f64,
    tail_deps_extra: f64,
    include_airframe_terms: bool,
) -> (f64, NpDiagnostics) {
    let x_np_surfaces = combine(
        &shared.contributions,
        shared.tail_wing_index,
        eta_tail,
        tail_deps_extra,
    )
    .unwrap_or(airplane.xyz_ref[0]);
    let x_wing_alone_ac = shared
        .contributions
        .get(shared.main_wing_index)
        .map(|c| c.x_ac)
        .unwrap_or(airplane.xyz_ref[0]);

    let (fuselage_shift, nacelle_shift, fuselage_diag, nacelle_diag) = if include_airframe_terms {
        airframe_terms(airplane, shared.cl_alpha_total, shared.deps_dalpha_vlm)
    } else {
        zero_airframe_terms(shared.deps_dalpha_vlm)
    };

    let x_np = x_np_surfaces + fuselage_shift + nacelle_shift;
    (
        x_np,
        NpDiagnostics {
            x_wing_alone_ac,
            x_np_surfaces,
            cl_alpha_total: shared.cl_alpha_total,
            deps_dalpha_vlm: shared.deps_dalpha_vlm,
            fuselage: fuselage_diag,
            nacelle_terms: nacelle_diag,
            nacelle_shift_m: nacelle_shift,
        },
    )
}

/// [`clean_np_at_condition`] evaluated at the same Mach/altitude for two
/// `tail_deps_extra` values at once, reusing one stretched-mesh assembly,
/// one pair of alpha probes, the isolated-tail downwash solve and the
/// fuselage/nacelle terms: none of those depend on `tail_deps_extra`, only
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
    tail_deps_extra_a: f64,
    tail_deps_extra_b: f64,
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

    let finish = |tail_deps_extra: f64| -> (f64, NpDiagnostics) {
        let (x_np_stretched, mut diag) = np_from_shared_terms(
            &stretched,
            &shared,
            analysis.tail_efficiency,
            tail_deps_extra,
            false,
        );
        // The stretched solve gave the wing/tail contribution and the
        // Mach-consistent slopes; rescale the wing/tail stations back to
        // physical geometry axes, then add the airframe terms on the
        // physical airplane, matching `clean_np_at_condition`.
        diag.x_np_surfaces = x_np_stretched * beta;
        diag.x_wing_alone_ac *= beta;

        let (fuselage_shift, nacelle_shift, fuselage_diag, nacelle_diag) =
            if analysis.include_fuselage_stability {
                airframe_terms(airplane, diag.cl_alpha_total, diag.deps_dalpha_vlm)
            } else {
                zero_airframe_terms(diag.deps_dalpha_vlm)
            };
        diag.fuselage = fuselage_diag;
        diag.nacelle_terms = nacelle_diag;
        diag.nacelle_shift_m = nacelle_shift;

        (diag.x_np_surfaces + fuselage_shift + nacelle_shift, diag)
    };

    Ok((finish(tail_deps_extra_a), finish(tail_deps_extra_b)))
}

/// The F2/F3 fuselage and nacelle terms for `airplane`'s own (physical)
/// geometry, given the Mach-consistent `cl_alpha_total` and the VLM-implied
/// tail downwash `deps_dalpha_vlm`: `(fuselage_shift_m, nacelle_shift_m,
/// FuselageTerms, Vec<NacelleTerm>)`. `0.0`/empty when `cl_alpha_total` is
/// too small a slope to normalise by.
fn airframe_terms(
    airplane: &Airplane,
    cl_alpha_total: f64,
    deps_dalpha_vlm: f64,
) -> (f64, f64, FuselageTerms, Vec<NacelleTerm>) {
    if !cl_alpha_total.is_finite() || cl_alpha_total.abs() < 0.1 {
        return zero_airframe_terms(deps_dalpha_vlm);
    }
    let Some(wing) = main_wing(airplane).filter(|wing| !wing.xsecs.is_empty()) else {
        return zero_airframe_terms(deps_dalpha_vlm);
    };
    let x_le_root = wing.xsecs[0].xyz_le[0];
    let x_te_root = x_le_root + wing.xsecs[0].chord;
    let x_wing_ac_root = x_le_root + AC_CHORD_FRACTION * wing.xsecs[0].chord;
    let x_tail_ac = match hstab(airplane) {
        Some(h) => h.aerodynamic_center(AC_CHORD_FRACTION)[0],
        None => x_te_root + 3.0 * airplane.c_ref.max(0.1),
    };
    let fus = fuselage::fuselage_terms(
        airplane,
        cl_alpha_total,
        deps_dalpha_vlm,
        x_le_root,
        x_te_root,
        x_tail_ac,
        wing.mean_sweep_angle(AC_CHORD_FRACTION).to_radians(),
        wing.taper_ratio(),
    );
    let (nac_shift, nac_terms) =
        nacelles::nacelle_terms(airplane, cl_alpha_total, wing, x_wing_ac_root, x_te_root);
    (
        fus.multhopp_shift_m + fus.torenbeek_d2_shift_m,
        nac_shift,
        fus,
        nac_terms,
    )
}

/// The all-zero [`airframe_terms`] result, for a caller that has disabled
/// the fuselage/nacelle correction (`AnalysisConfig::include_fuselage_stability`)
/// or has too degenerate a lift slope to normalise by.
fn zero_airframe_terms(deps_dalpha_vlm: f64) -> (f64, f64, FuselageTerms, Vec<NacelleTerm>) {
    (
        0.0,
        0.0,
        FuselageTerms {
            multhopp_shift_m: 0.0,
            torenbeek_d2_shift_m: 0.0,
            torenbeek_d1_shift_m: 0.0,
            gilruth_shift_m: 0.0,
            deps_dalpha: deps_dalpha_vlm,
        },
        Vec::new(),
    )
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
    tail_deps_extra: f64,
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
        analysis.tail_efficiency,
        tail_deps_extra,
        false,
    )?;
    // The stretched solve gave the wing/tail contribution and the
    // Mach-consistent slopes; rescale the wing/tail stations back to
    // physical geometry axes, then add the airframe terms on the physical
    // airplane.
    diag.x_np_surfaces = x_np_stretched * beta;
    diag.x_wing_alone_ac *= beta;

    let (fuselage_shift, nacelle_shift, fuselage_diag, nacelle_diag) =
        if analysis.include_fuselage_stability {
            airframe_terms(airplane, diag.cl_alpha_total, diag.deps_dalpha_vlm)
        } else {
            zero_airframe_terms(diag.deps_dalpha_vlm)
        };
    diag.fuselage = fuselage_diag;
    diag.nacelle_terms = nacelle_diag;
    diag.nacelle_shift_m = nacelle_shift;

    Ok((diag.x_np_surfaces + fuselage_shift + nacelle_shift, diag))
}
