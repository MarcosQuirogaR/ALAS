// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fuselage pitching-moment contribution by the Multhopp strip method, plus
//! the Torenbeek swept-wing lift-loss term (`D2`) and two cross-check-only
//! correlations (Torenbeek `D1`, a Gilruth-style fit).
//!
//! The `reference_compatibility` `fuselage_cm_alpha` in `trim` integrates the
//! *whole* body ahead of the wing root trailing edge at free-stream angle
//! (double-counting the wing-root strip, whose lift the VLM already carries),
//! ramps the afterbody factor the wrong way (`1 -> 1-deps`, Multhopp's is
//! `~0 -> 1-deps`), and applies a Munk `(k2-k1)` apparent-mass factor on top
//! of a strip integral that does not call for one. This module is the
//! corrected form: strips split exactly at the wing root leading/trailing edge, the
//! over-wing strip excluded outright, a forebody upwash factor from a
//! closed-form bound-vortex estimate (documented below; not a fitted DATCOM
//! chart digitisation), and an afterbody factor driven by the VLM-implied
//! downwash at the tail rather than the simple `2 CL_alpha / (pi AR)`
//! correlation.
//!
//! # Forebody upwash: a closed-form surrogate, not a chart lookup
//!
//! Multhopp's method calls for a forebody upwash factor read off a
//! wind-tunnel-derived chart (Multhopp 1942; digitised in Roskam Part VI and
//! USAF DATCOM Sec. 5.2.3.1, neither of which is available locally). This module instead estimates the
//! local upwash from the classical induced-velocity field of a single
//! horseshoe vortex representing the wing's bound circulation at its
//! quarter-chord (root chord, full span): `up(x) = CLa * c_r / (4 pi d) *
//! (b/2) / sqrt(d^2 + (b/2)^2)`, `d` the distance ahead of that line. This is
//! the same order of magnitude as the chart (-19.7 %MAC with this upwash
//! vs -14.4 %MAC without, both against a -27.5 %MAC `reference_compatibility`
//! baseline), but it is an estimate, flagged here and in
//! [`FuselageTerms`]; it is not a substitute for the primary source.
//!
//! # What is primary vs cross-check
//!
//! [`fuselage_terms`] returns the Multhopp strip shift and the Torenbeek
//! `D2` swept-wing lift-loss term as the *primary* fuselage correction (both
//! summed into the product neutral point); Torenbeek `D1` (an independent
//! slender-body correlation covering the same physics as the Multhopp strip
//! integral) and the Gilruth-style fit are reported as cross-check bands
//! only, per the fix plan, and are never added into the neutral point.

use std::f64::consts::PI;

use alas_geom::aircraft::airplane::Airplane;

/// Every term [`fuselage_terms`] computes, all in metres of neutral-point
/// shift (positive aft, negative forward), plus the downwash gradient it
/// used.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuselageTerms {
    /// Multhopp strip shift: the primary fuselage correction, forward
    /// (negative) for a conventional tube-and-wing.
    pub multhopp_shift_m: f64,
    /// Torenbeek `D2` (swept-wing body lift-loss): the second primary
    /// correction, summed with [`Self::multhopp_shift_m`].
    pub torenbeek_d2_shift_m: f64,
    /// Torenbeek `D1`: an independent slender-body cross-check of
    /// [`Self::multhopp_shift_m`]'s physics, *not* summed into the neutral
    /// point.
    pub torenbeek_d1_shift_m: f64,
    /// A Gilruth-style `K_f`-correlation cross-check, *not* summed into the
    /// neutral point. Built from a single recalled anchor point (`K_f ~ 0.011/deg` at `x_c4,root/L ~ 0.39`); order-of-magnitude only,
    /// not independently verified against a primary source.
    pub gilruth_shift_m: f64,
    /// The downwash gradient at the tail this call was given (passed
    /// through for the diagnostics struct one layer up).
    pub deps_dalpha: f64,
}

/// `up(x) = CLa c_r / (4 pi d) * (b/2) / sqrt(d^2 + (b/2)^2)`, the forebody
/// upwash surrogate documented in the module doc. `d` is the distance ahead
/// of the wing root quarter-chord; floored at `0.05` m to keep the estimate
/// finite immediately ahead of the wing.
fn bound_vortex_upwash(cl_alpha: f64, root_chord: f64, half_span: f64, d_ahead: f64) -> f64 {
    let d = d_ahead.max(0.05);
    cl_alpha * root_chord / (4.0 * PI * d) * half_span / (d * d + half_span * half_span).sqrt()
}

/// The Multhopp strip shift, Torenbeek `D2`, and the two cross-check terms,
/// for `airplane.fuselages[0]` against the main wing's root leading/trailing
/// edge (`x_le_root`/`x_te_root`) and the tail's aerodynamic centre
/// (`x_tail_ac`, geometry axes; the same fallback the caller already uses
/// when there is no tail applies upstream of this call).
///
/// `cl_alpha_total` is the Mach-consistent total lift-curve slope [1/rad];
/// `deps_dalpha` is the VLM-implied downwash gradient at the tail (`1 -
/// a_t,in_presence / a_t,isolated`); `sweep_c4_rad` and `taper` are the main
/// wing's own quarter-chord sweep and taper ratio (Torenbeek `D2`'s two
/// planform inputs). Returns all-zero terms when the airplane has no
/// fuselage or fewer than two cross-sections to integrate over (nothing to
/// shift, matching `fuselage_cm_alpha`'s existing guard).
#[allow(clippy::too_many_arguments)]
pub fn fuselage_terms(
    airplane: &Airplane,
    cl_alpha_total: f64,
    deps_dalpha: f64,
    x_le_root: f64,
    x_te_root: f64,
    x_tail_ac: f64,
    sweep_c4_rad: f64,
    taper: f64,
) -> FuselageTerms {
    let zero = FuselageTerms {
        multhopp_shift_m: 0.0,
        torenbeek_d2_shift_m: 0.0,
        torenbeek_d1_shift_m: 0.0,
        gilruth_shift_m: 0.0,
        deps_dalpha,
    };
    let Some(fus) = airplane.fuselages.first() else {
        return zero;
    };
    if fus.xsecs.len() < 2 || cl_alpha_total.abs() < 1e-9 {
        return zero;
    }

    let mut order: Vec<usize> = (0..fus.xsecs.len()).collect();
    order.sort_by(|&a, &b| {
        fus.xsecs[a].xyz_c[0]
            .partial_cmp(&fus.xsecs[b].xyz_c[0])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let xs: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].xyz_c[0]).collect();
    let ws: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].width).collect();
    let hs: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].height).collect();

    let s_ref = airplane.s_ref.max(1.0);
    let c_ref = airplane.c_ref.max(0.1);
    let b_ref = airplane.b_ref.max(1.0);
    let half_span = 0.5 * b_ref;
    let root_chord = (x_te_root - x_le_root).max(0.1);
    let x_c4_root = x_le_root + 0.25 * root_chord;
    let l_h = (x_tail_ac - x_te_root).max(0.1);

    // The forebody upwash term is singular as the station approaches the
    // wing root leading edge (`bound_vortex_upwash`'s `1/d`), floored but
    // still sharply peaked; a coarse trapezoidal sum over the builder's
    // ~20 fuselage stations under-samples that peak and understates the
    // forebody term. Sub-sample each station interval into fine steps
    // (400 per interval) so the integral converges independently of the builder's
    // station density.
    const SUBSTEPS_PER_STATION: usize = 400;
    let mut fore_accum = 0.0_f64;
    let mut aft_accum = 0.0_f64;
    for i in 0..xs.len() - 1 {
        let dx_station = xs[i + 1] - xs[i];
        if dx_station <= 0.0 {
            continue;
        }
        let dx = dx_station / SUBSTEPS_PER_STATION as f64;
        for step in 0..SUBSTEPS_PER_STATION {
            let t = (step as f64 + 0.5) / SUBSTEPS_PER_STATION as f64;
            let x_mid = xs[i] + t * dx_station;
            let w_mid = ws[i] + t * (ws[i + 1] - ws[i]);
            // Multhopp's strip method weights by the fuselage *width
            // squared*, not the cross-section area (`(pi/4) w h`): the
            // latter is the Munk/slender-body form the frozen
            // `fuselage_cm_alpha` uses, which this fix replaces for the
            // product path.
            let w2 = w_mid * w_mid;
            if x_mid < x_le_root {
                let up =
                    bound_vortex_upwash(cl_alpha_total, root_chord, half_span, x_c4_root - x_mid);
                fore_accum += w2 * (1.0 + up) * dx;
            } else if x_mid > x_te_root {
                let frac = ((x_mid - x_te_root) / l_h).min(1.0);
                aft_accum += w2 * frac * (1.0 - deps_dalpha) * dx;
            }
            // Else: the strip is covered by the wing root in planform;
            // its lift is already in the VLM wing surface, so it
            // contributes nothing here (no double count).
        }
    }
    let cm_alpha_multhopp = PI / (2.0 * s_ref * c_ref) * (fore_accum + aft_accum);
    let multhopp_shift_m = -cm_alpha_multhopp * c_ref / cl_alpha_total;

    let l_f = xs[xs.len() - 1] - xs[0];
    let b_f = ws.iter().copied().fold(0.1_f64, f64::max);
    let h_f = hs.iter().copied().fold(0.1_f64, f64::max);
    let c_g = s_ref / b_ref;
    let d2_fraction = 0.273 / (1.0 + taper.max(0.0)) * b_f * c_g * (b_ref - b_f)
        / (c_ref * c_ref * (b_ref + 2.15 * b_f))
        * sweep_c4_rad.tan();
    let torenbeek_d2_shift_m = d2_fraction * c_ref;

    let l_fn = (x_le_root - xs[0]).max(0.1);
    let d1_fraction = -1.8 / cl_alpha_total * b_f * h_f * l_fn / (s_ref * c_ref);
    let torenbeek_d1_shift_m = d1_fraction * c_ref;

    // Gilruth-style K_f(x_c4,root/L) correlation, per degree, linearly
    // anchored at the single recalled data point (K_f ~ 0.011/deg
    // at x_c4,root/L ~ 0.39); a rough order-of-magnitude cross-check, not an
    // independently retrieved fit.
    let k_f_per_deg = 0.0025 + 0.022 * (x_c4_root / l_f.max(0.1));
    let cm_alpha_gilruth = k_f_per_deg.to_radians() * b_f * b_f * l_f / (s_ref * c_ref);
    let gilruth_shift_m = -cm_alpha_gilruth * c_ref / cl_alpha_total;

    FuselageTerms {
        multhopp_shift_m,
        torenbeek_d2_shift_m,
        torenbeek_d1_shift_m,
        gilruth_shift_m,
        deps_dalpha,
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};

    fn probe_fuselage() -> Fuselage {
        let station = |x: f64, r: f64| {
            FuselageXSec::new([x, 0.0, 0.0], Some(r), None, None, DEFAULT_SHAPE)
                .expect("radius alone is valid")
        };
        Fuselage::new(
            "Fuselage",
            vec![
                station(0.0, 0.5),
                station(2.0, 1.5),
                station(10.0, 1.5),
                station(18.0, 0.8),
            ],
        )
    }

    fn probe_airplane() -> Airplane {
        Airplane {
            name: "Probe".to_owned(),
            xyz_ref: [10.0, 0.0, 0.0],
            wings: vec![],
            fuselages: vec![probe_fuselage()],
            s_ref: 24.0,
            c_ref: 3.0,
            b_ref: 8.0,
        }
    }

    #[test]
    fn fuselage_terms_are_all_zero_without_a_fuselage() {
        let mut plane = probe_airplane();
        plane.fuselages.clear();
        let terms = fuselage_terms(&plane, 5.0, 0.3, 5.0, 8.0, 30.0, 0.4, 0.3);
        assert_eq!(terms.multhopp_shift_m, 0.0);
        assert_eq!(terms.torenbeek_d2_shift_m, 0.0);
    }

    #[test]
    fn multhopp_shift_is_forward_for_a_conventional_layout() {
        let plane = probe_airplane();
        let terms = fuselage_terms(&plane, 5.0, 0.3, 5.0, 8.0, 30.0, 0.4, 0.3);
        assert!(
            terms.multhopp_shift_m < 0.0,
            "shift={}",
            terms.multhopp_shift_m
        );
    }

    #[test]
    fn torenbeek_d2_is_aft_for_a_swept_wing() {
        let plane = probe_airplane();
        let unswept = fuselage_terms(&plane, 5.0, 0.3, 5.0, 8.0, 30.0, 0.0, 0.3);
        let swept = fuselage_terms(&plane, 5.0, 0.3, 5.0, 8.0, 30.0, 0.5, 0.3);
        assert_eq!(unswept.torenbeek_d2_shift_m, 0.0);
        assert!(swept.torenbeek_d2_shift_m > 0.0);
    }

    #[test]
    fn a_larger_vlm_downwash_reduces_the_afterbody_shift_magnitude() {
        let plane = probe_airplane();
        let low_downwash = fuselage_terms(&plane, 5.0, 0.1, 5.0, 8.0, 30.0, 0.4, 0.3);
        let high_downwash = fuselage_terms(&plane, 5.0, 0.5, 5.0, 8.0, 30.0, 0.4, 0.3);
        assert!(high_downwash.multhopp_shift_m.abs() < low_downwash.multhopp_shift_m.abs());
    }
}
