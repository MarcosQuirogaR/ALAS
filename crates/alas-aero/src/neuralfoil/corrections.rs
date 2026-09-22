// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from native aerodynamic model/geometry/airfoil/kulfan_airfoil.py
// (KulfanAirfoil.get_aero_from_neuralfoil, the part after the network call)
// and native aerodynamic model/aerodynamics/aero_2D/airfoil_polar_functions.py
// (airfoil_coefficients_post_stall), with Cf_flat_plate from
// native aerodynamic model/library/aerodynamics/viscous.py.
// Upstream: native aerodynamic model 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! What the network cannot see: the flow separating, and the flow going
//! transonic.
//!
//! NeuralFoil is trained on incompressible, attached, XFoil-converged cases.
//! This program flies a transport aircraft at Mach 0.78 and screens sections
//! across a twenty-degree alpha sweep, so both of the surrogate's boundaries
//! are crossed routinely. native aerodynamic model handles each by blending the network's
//! answer against a model that stays sensible where the network does not, and
//! all of that lives here.
//!
//! # Past the stall
//!
//! A separated airfoil is a flat plate with rounded corners, and there is a
//! closed-form 360-degree model for one. It is blended in on
//! `softmax(alpha - 20, -20 - alpha) / 3`, which is near zero through the
//! attached range and grows past twenty degrees either way. The drag blends
//! *logarithmically*: the attached and separated values are two orders
//! apart, and a linear blend between them would be dominated by the larger
//! one long before the flow was actually separated.
//!
//! The separated model is `airfoil_coefficients_post_stall`, and it reads its
//! `airfoil` argument nowhere: its six coefficients are NACA 0012's, hard
//! coded, with the shape-dependent version commented out upstream and marked
//! TODO. Reproduced faithfully: the function here takes only an angle,
//! rather than carrying a parameter it would not read, and recorded in
//! `docs/PORTING.md`.
//!
//! # Past the critical Mach number
//!
//! The peak suction the network reports fixes the Mach number at which the
//! flow first goes sonic somewhere, through a symbolic-regression fit
//! upstream derived from the Laitone rule and the sonic-`Cp` relation. Drag
//! divergence follows from the Korn equation, lift and moment take a
//! Prandtl-Glauert amplification, and the wave drag is a four-branch schedule,
//! nothing below the critical Mach number, a quartic rise to drag
//! divergence, a cosine-Hermite patch carrying it to Mach 1.1, and a blend
//! above. Past drag divergence the aerodynamic centre also walks back toward
//! mid-chord, which the moment picks up as a further shift.
//!
//! # A translation detail worth stating
//!
//! Upstream writes the wave-drag schedule as nested `np.where`, which
//! evaluates every branch and then selects; this writes it as `if`/`else`,
//! which evaluates one. That is not a behaviour change here, and it was
//! checked rather than assumed: every branch is finite for every Mach number
//! and thickness this can be handed, so no branch that `np.where` discards
//! was discarding an infinity or a NaN that would have propagated.

use std::f64::consts::PI;

use super::network::{BoundaryLayer, NetworkAero, BL_STATIONS};
use super::soft;

/// Where the post-stall blend is centred, in degrees. Set high because
/// NeuralFoil extrapolates past stall better than the model replacing it.
const ALPHA_STALL_POSITIVE: f64 = 20.0;
/// The negative-incidence counterpart of [`ALPHA_STALL_POSITIVE`].
const ALPHA_STALL_NEGATIVE: f64 = -20.0;

/// A section's aerodynamics with compressibility and post-stall behaviour
/// applied: the whole of `KulfanAirfoil.get_aero_from_neuralfoil`'s output.
#[derive(Debug, Clone, PartialEq)]
pub struct Aero {
    /// How far inside its training data the underlying network query sat.
    pub analysis_confidence: f64,
    /// Section lift coefficient.
    pub cl: f64,
    /// Section drag coefficient, including wave drag.
    pub cd: f64,
    /// Section pitching-moment coefficient about the quarter chord.
    pub cm: f64,
    /// Peak suction coefficient, Prandtl-Glauert corrected.
    pub cpmin: f64,
    /// Upper-surface transition location, as a fraction of chord.
    pub top_xtr: f64,
    /// Lower-surface transition location, as a fraction of chord.
    pub bot_xtr: f64,
    /// The Mach number at which the flow first goes sonic somewhere.
    pub mach_crit: f64,
    /// The drag-divergence Mach number, from the Korn equation.
    pub mach_dd: f64,
    /// Peak suction coefficient before the compressibility correction.
    pub cpmin_0: f64,
    /// The upper surface's boundary layer, passed through from the network.
    pub upper: BoundaryLayer,
    /// The lower surface's boundary layer, passed through from the network.
    pub lower: BoundaryLayer,
}

/// Apply the post-stall blend and the compressibility corrections to one
/// network answer.
///
/// `alpha_deg` must already be wrapped into `[-180, 180)`; the caller does
/// that once, before the network sees it, because the network's own
/// `sin(2 alpha)` feature depends on it too.
pub(super) fn apply(
    network: NetworkAero,
    t_over_c: f64,
    alpha_deg: f64,
    reynolds: f64,
    mach: f64,
) -> Aero {
    let sin_alpha = alpha_deg.to_radians().sin();
    let cos_alpha = alpha_deg.to_radians().cos();

    let mut cl = network.cl;
    let mut cd = network.cd;
    let mut cm = network.cm;
    let mut top_xtr = network.top_xtr;
    let mut bot_xtr = network.bot_xtr;
    let mut cpmin_0 = peak_suction(&network);

    // The 360-degree extension. `include_360_deg_effects` defaults to true
    // and neither reached call site overrides it, so there is no branch here:
    // the false path is not translated.
    let (cl_separated, cd_separated, cm_separated) = post_stall(alpha_deg);
    let is_separated = soft::softmax(
        &[
            alpha_deg - ALPHA_STALL_POSITIVE,
            ALPHA_STALL_NEGATIVE - alpha_deg,
        ],
        1.0,
    ) / 3.0;

    cl = soft::blend(is_separated, cl_separated, cl);
    cd = soft::blend(
        is_separated,
        (cd_separated + turbulent_flat_plate_friction(reynolds)).ln(),
        cd.ln(),
    )
    .exp();
    cm = soft::blend(is_separated, cm_separated, cm);
    // A rough fit to Shademan and Naghib-Lahouti's inclined-flat-plate data,
    // as upstream's comment records.
    cpmin_0 = soft::blend(is_separated, -1.0 - 0.5 * sin_alpha * sin_alpha, cpmin_0);
    let separated_transition = (10.0 * sin_alpha).tanh();
    top_xtr = soft::blend(is_separated, 0.5 - 0.5 * separated_transition, top_xtr);
    bot_xtr = soft::blend(is_separated, 0.5 + 0.5 * separated_transition, bot_xtr);

    // Suction can only be suction here: a positive peak `Cp` would put the
    // critical-Mach fit's `(-Cpmin_0)**0.67` on the wrong side of zero.
    cpmin_0 = soft::softmin(&[cpmin_0, 0.0], 0.001);
    let mach_crit = critical_mach(cpmin_0);
    // W. H. Mason's form of the Korn equation.
    let mach_dd = mach_crit + (0.1_f64 / 320.0).powf(1.0 / 3.0);

    let beta_squared_ideal = 1.0 - mach * mach;
    // Softened so that the Prandtl-Glauert singularity at Mach 1 becomes a
    // finite peak; the softness is empirically tuned upstream.
    let beta = soft::softmax(&[beta_squared_ideal, -beta_squared_ideal], 0.5).sqrt();
    cl /= beta;
    cm /= beta;
    let cpmin = cpmin_0 / beta;

    // Buffet, tuned to RANS data, and the drop in lift-curve slope from
    // 2 pi to 4 as the flow goes supersonic.
    let buffet = soft::blend(
        50.0 * (mach - (mach_dd + 0.04)),
        soft::blend((mach - 1.0) / 0.1, 1.0, 0.5),
        1.0,
    );
    let supersonic_slope_ratio = soft::blend((mach - 1.0) / 0.1, 4.0 / (2.0 * PI), 1.0);
    cl = cl * buffet * supersonic_slope_ratio;

    cd += wave_drag(mach, mach_crit, mach_dd, t_over_c);

    // Mach tuck: past drag divergence, or once separated, the aerodynamic
    // centre walks back toward mid-chord.
    let centre_shift = soft::softmax(&[is_separated, (mach - (mach_dd + 0.06)) / 0.06], 0.1);
    cm += soft::blend(
        centre_shift,
        -0.25 * cos_alpha * cl - 0.25 * sin_alpha * cd,
        0.0,
    );

    Aero {
        analysis_confidence: network.analysis_confidence,
        cl,
        cd,
        cm,
        cpmin,
        top_xtr,
        bot_xtr,
        mach_crit,
        mach_dd,
        cpmin_0,
        upper: network.upper,
        lower: network.lower,
    }
}

/// The peak suction coefficient, softly minimized over both surfaces.
///
/// `Cp = 1 - (ue/vinf)^2` at every reported boundary-layer station; upstream
/// notes that the network has a `Cpmin` channel and takes this instead. The
/// station order is upstream's (all 32 upper stations, then all 32 lower)
/// because the softmin's sum runs over the list in the order it is given.
fn peak_suction(network: &NetworkAero) -> f64 {
    let mut pressures = Vec::with_capacity(2 * BL_STATIONS);
    for &edge_velocity in &network.upper.ue_over_vinf {
        pressures.push(1.0 - edge_velocity * edge_velocity);
    }
    for &edge_velocity in &network.lower.ue_over_vinf {
        pressures.push(1.0 - edge_velocity * edge_velocity);
    }
    soft::softmin(&pressures, 0.01)
}

/// The Mach number at which the peak suction first reaches sonic conditions.
///
/// A symbolic-regression fit to the Laitone-rule/`Cp_sonic` relation, which
/// has no closed-form inverse. The coefficients are upstream's, from
/// `native aerodynamic model/studies/MachFitting/CriticalMach/`.
fn critical_mach(cpmin_0: f64) -> f64 {
    (1.011_571_026_701_678 - cpmin_0
        + 0.658_243_135_100_719_5 * (-cpmin_0).powf(0.672_478_943_984_034_3))
    .powf(-0.550_467_703_835_871_1)
}

/// The wave-drag schedule: four branches on the Mach number.
fn wave_drag(mach: f64, mach_crit: f64, mach_dd: f64, t_over_c: f64) -> f64 {
    if mach < mach_crit {
        return 0.0;
    }
    if mach < mach_dd {
        // `powf` rather than `powi`: NumPy's power loop has fast paths only
        // for exponents -1, 0, 0.5, 1 and 2, so a fourth power goes through
        // the platform `pow` and repeated squaring does not always land on
        // the same last bit.
        return 80.0 * (mach - mach_crit).powf(4.0);
    }
    if mach < 1.1 {
        return soft::cosine_hermite_patch(
            mach,
            mach_dd,
            1.1,
            // The quartic's value and slope where drag divergence leaves off.
            80.0 * (0.1_f64 / 320.0).powf(4.0 / 3.0),
            0.8 * t_over_c,
            0.1,
            -0.8 * t_over_c * 8.0,
        );
    }
    soft::blend(
        8.0 * 2.0 * (mach - 1.1) / (1.2 - 0.8),
        0.8 * 0.8 * t_over_c,
        1.2 * 0.8 * t_over_c,
    )
}

/// Truong's 360-degree separated-airfoil model, as
/// `airfoil_coefficients_post_stall` implements it.
///
/// Returns `(CL, CD, CM)`. `CM` is identically zero: upstream leaves it as a
/// TODO, and the coefficients above it are NACA 0012's regardless of the
/// section handed in, which is why this takes an angle and no airfoil.
///
/// Reference: Truong, "An analytical model for airfoil aerodynamic
/// characteristics over the entire 360deg angle of attack range", J.
/// Renewable Sustainable Energy, 2020, doi:10.1063/1.5126055.
fn post_stall(alpha_deg: f64) -> (f64, f64, f64) {
    /// Flat-plate normal-force coefficient at 90 degrees, for NACA 0012.
    const CD90_0: f64 = 2.08;
    const PN2: f64 = 8.36e-2;
    const PN3: f64 = 4.06e-1;
    const PT1: f64 = 9.00e-2;
    const PT2: f64 = -1.78e-1;
    const PT3: f64 = -2.98e-1;

    let sin_alpha = alpha_deg.to_radians().sin();
    let cos_alpha = alpha_deg.to_radians().cos();

    let cd90 = CD90_0 + PN2 * cos_alpha + PN3 * cos_alpha * cos_alpha;
    let normal = cd90 * sin_alpha;
    let tangential = (PT1 + PT2 * cos_alpha + PT3 * cos_alpha.powf(3.0)) * sin_alpha * sin_alpha;

    (
        normal * cos_alpha + tangential * sin_alpha,
        normal * sin_alpha - tangential * cos_alpha,
        0.0,
    )
}

/// Mean skin-friction coefficient over a smooth flat plate, turbulent
/// throughout: `Cf_flat_plate(Re_L, method="turbulent")`.
///
/// Cengel and Cimbala, "Fluid Mechanics: Fundamentals and Applications",
/// Table 10-4. It is added to the separated drag so that a fully stalled
/// section still carries its friction.
fn turbulent_flat_plate_friction(reynolds: f64) -> f64 {
    0.074 / reynolds.abs().powf(0.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network(cl: f64, cd: f64, cm: f64, peak_ue: f64) -> NetworkAero {
        let layer = |ue: f64| BoundaryLayer {
            theta: [1e-4; BL_STATIONS],
            shape_factor: [2.5; BL_STATIONS],
            ue_over_vinf: [ue; BL_STATIONS],
        };
        NetworkAero {
            analysis_confidence: 0.9,
            cl,
            cd,
            cm,
            top_xtr: 0.4,
            bot_xtr: 0.9,
            upper: layer(peak_ue),
            lower: layer(-0.9),
        }
    }

    fn attached() -> NetworkAero {
        network(0.6, 0.006, -0.05, 1.3)
    }

    #[test]
    fn an_incompressible_attached_case_keeps_the_networks_drag_and_nearly_its_lift() {
        let corrected = apply(attached(), 0.12, 2.0, 1e6, 0.0);
        assert!((corrected.cd - 0.006).abs() < 1e-6);
        assert!((corrected.cl - 0.6).abs() < 0.01);
        assert!((corrected.cm + 0.05).abs() < 0.01);
    }

    #[test]
    fn the_softened_prandtl_glauert_factor_bites_even_at_zero_mach() {
        // Faithful translation, recorded because it looks like a bug: `beta`
        // is a *softmax* of `1 - M^2` against its negative, which at M = 0 is
        // `softmax(1, -1, softness=0.5) = 1.00907` rather than 1, so lift and
        // moment come back 0.45% high with no compressibility present at all.
        // The softening is what keeps `beta` finite through Mach 1, and it
        // does not switch itself off away from there.
        let corrected = apply(attached(), 0.12, 2.0, 1e6, 0.0);
        let beta = soft::softmax(&[1.0, -1.0], 0.5).sqrt();
        assert!((beta - 1.004_524).abs() < 1e-5, "beta = {beta}");
        // Not exact: at two degrees the post-stall blend still mixes in six
        // parts per million of the separated model, which is the smooth
        // blend's price for having no switch to be on the wrong side of.
        assert!((corrected.cl - 0.6 / beta).abs() < 1e-4);
    }

    #[test]
    fn there_is_no_wave_drag_below_the_critical_mach_number() {
        let corrected = apply(attached(), 0.12, 2.0, 1e6, 0.2);
        assert!(corrected.mach_crit > 0.2);
        assert!((corrected.cd - 0.006).abs() < 1e-6);
    }

    #[test]
    fn drag_divergence_sits_one_korn_step_above_the_critical_mach_number() {
        let corrected = apply(attached(), 0.12, 2.0, 1e6, 0.2);
        let step = (0.1_f64 / 320.0).powf(1.0 / 3.0);
        assert!((corrected.mach_dd - corrected.mach_crit - step).abs() < 1e-15);
        assert!((step - 0.067_86).abs() < 1e-4);
    }

    #[test]
    fn the_wave_drag_schedule_is_continuous_across_all_four_branches() {
        // Each boundary is where a port that misread one branch's endpoints
        // shows up, and the schedule is built to be continuous through all
        // three of them.
        let (crit, divergence, thickness) = (0.65, 0.65 + 0.067_86, 0.12);
        for boundary in [crit, divergence, 1.1] {
            let step = 1e-7;
            let below = wave_drag(boundary - step, crit, divergence, thickness);
            let above = wave_drag(boundary + step, crit, divergence, thickness);
            assert!(
                (above - below).abs() < 1e-5,
                "wave drag jumps at Mach {boundary}: {below} to {above}"
            );
        }
    }

    #[test]
    fn the_wave_drag_rises_through_the_transonic_range_and_stays_bounded_above_it() {
        // Not monotonic, and deliberately so: the patch's slope at Mach 1.1
        // is `-0.8 * t/c * 8`, which is steeply negative, so the schedule
        // peaks below Mach 1.1 and comes back down to the supersonic blend.
        // A port that assumed a monotonic rise would "fix" that.
        let (crit, divergence, thickness) = (0.65, 0.65 + 0.067_86, 0.12);
        let sample = |mach: f64| wave_drag(mach, crit, divergence, thickness);

        assert_eq!(sample(0.6), 0.0);
        assert!(sample(divergence) > sample(crit + 0.01));
        assert!(sample(0.95) > sample(divergence));
        for step in 0..=80 {
            let mach = 0.6 + f64::from(step) * 0.01;
            let drag = sample(mach);
            assert!(drag >= 0.0, "wave drag went negative at Mach {mach}");
            assert!(drag < 1.0, "wave drag ran away at Mach {mach}: {drag}");
        }
    }

    #[test]
    fn a_deeply_stalled_section_reports_the_flat_plate_model_and_not_the_network() {
        // At 60 degrees the blend is fully separated, so the answer should be
        // the closed-form model regardless of what the network said.
        let corrected = apply(network(0.6, 0.006, -0.05, 1.3), 0.12, 60.0, 1e6, 0.1);
        let (cl_separated, cd_separated, _) = post_stall(60.0);
        assert!((corrected.cl - cl_separated).abs() < 1e-3);
        assert!(corrected.cd > 10.0 * cd_separated.min(1.0) * 0.05);
        assert!(corrected.cd > 1.0, "stalled drag was {}", corrected.cd);
    }

    #[test]
    fn the_post_stall_model_is_odd_in_the_angle_of_attack() {
        // A symmetric flat plate: lift reverses with incidence, drag does not.
        for alpha in [5.0, 30.0, 75.0, 120.0] {
            let (cl_up, cd_up, _) = post_stall(alpha);
            let (cl_down, cd_down, _) = post_stall(-alpha);
            assert!((cl_up + cl_down).abs() < 1e-12, "alpha {alpha}");
            assert!((cd_up - cd_down).abs() < 1e-12, "alpha {alpha}");
        }
    }

    #[test]
    fn the_post_stall_model_peaks_near_ninety_degrees() {
        let (_, cd_broadside, _) = post_stall(90.0);
        assert!((cd_broadside - 2.08).abs() < 1e-9);
        let (cl_broadside, _, _) = post_stall(90.0);
        assert!(cl_broadside.abs() < 0.1);
    }

    #[test]
    fn compressibility_amplifies_lift_and_moment_but_not_drag() {
        let low = apply(attached(), 0.12, 2.0, 1e6, 0.05);
        let high = apply(attached(), 0.12, 2.0, 1e6, 0.5);
        assert!(high.cl > low.cl);
        assert!(high.cm.abs() > low.cm.abs());
        assert!((high.cd - low.cd).abs() < 1e-9);
    }

    #[test]
    fn beta_stays_finite_through_mach_one() {
        // The Prandtl-Glauert singularity is softened rather than reached; a
        // port that divided by `sqrt(1 - M^2)` directly would report an
        // infinite lift coefficient here.
        for mach in [0.98, 0.999, 1.0, 1.001, 1.02] {
            let corrected = apply(attached(), 0.12, 2.0, 1e6, mach);
            assert!(corrected.cl.is_finite(), "Mach {mach}");
            assert!(corrected.cd.is_finite(), "Mach {mach}");
        }
    }

    #[test]
    fn a_thicker_section_goes_critical_earlier_and_pays_more_wave_drag() {
        let thin = apply(network(0.6, 0.006, -0.05, 1.15), 0.08, 2.0, 1e6, 0.85);
        let thick = apply(network(0.6, 0.006, -0.05, 1.45), 0.18, 2.0, 1e6, 0.85);
        assert!(thick.mach_crit < thin.mach_crit);
        assert!(thick.cd > thin.cd);
    }

    #[test]
    fn the_turbulent_friction_coefficient_matches_its_published_form() {
        assert!((turbulent_flat_plate_friction(1e6) - 0.074 / 1e6_f64.powf(0.2)).abs() < 1e-18);
        // Negative Reynolds numbers are folded, as upstream's `np.abs` does.
        assert_eq!(
            turbulent_flat_plate_friction(-1e6),
            turbulent_flat_plate_friction(1e6)
        );
    }
}
