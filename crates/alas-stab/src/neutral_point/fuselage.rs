// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fuselage contribution to the neutral point by Torenbeek's pair
//! (Synthesis of Subsonic Airplane Design, 1982, App. E, eq. E-36): the
//! empirical body term E-37 (`D1`) and the swept-wing lift-loss term E-38
//! (`D2`), the same pair the forward-limit wing-body centre takes. Two
//! cross-check-only correlations are reported beside them: the
//! DATCOM/Multhopp strip method (USAF DATCOM 1978, Sec. 4.2.2.1, Method 1,
//! printed pp. 4.2.2.1-1 and -2, Eq. 4.2.2.1-a and Sketch (a)) and a
//! NACA TR 711 `K_fus` correlation of Raymer Eq. 16.25. Torenbeek pairs
//! E-38 with E-37, not with a strip integral, so the strip method is not
//! summed with `D2`.
//!
//! The `reference_compatibility` `fuselage_cm_alpha` in `trim` integrates the
//! *whole* body ahead of the wing root trailing edge at free-stream angle
//! (double-counting the wing-root strip, whose lift the VLM already carries)
//! and applies a Munk `(k2-k1)` apparent-mass factor on top of a strip
//! integral that does not call for one. The strip cross-check follows DATCOM:
//!
//! - `Cm_alpha = 1/(36.5 S c) * Int w_f^2 (d eps_u/d alpha + 1) dx` per degree
//!   (`pi/(2 S c)` per radian), summed over the body segments of Sketch (a);
//! - the wing root is the *exposed* root chord at the wing-fuselage junction
//!   (`c_re`), the body alongside it (between its leading and trailing edge)
//!   carries no segment, so it is excluded and not counted twice with the
//!   VLM wing;
//! - segments ahead of the wing use the upwash gradient of Fig. 4.2.2.1-22:
//!   curve (a) for the increment immediately forward of the wing (segment 6),
//!   curve (b) for segments 1-5, both against `x_1 / c_re` with `x_1` measured
//!   forward of the exposed-root leading edge, scaled by `CLa_wb / 0.0785`
//!   per degree;
//! - segments aft of the wing carry `1 + d eps_u/d alpha = (x_1 / l_h)
//!   (1 - d eps / d alpha)` with `x_1` and `l_h` both measured from the
//!   trailing edge of the exposed root chord, `d eps / d alpha` the
//!   downwash gradient at the tail (here the VLM-implied one).
//!
//! The two chart curves are digitised from the printed figure (page 906 of
//! the DTIC scan, read to about +-0.02 on curve (b) and +-0.05 on curve (a));
//! segment 6 is taken as the increment `0 <= x_1 <= 0.5 c_re`, curve (a) read at
//! the abscissa of its length (0.5), because curve (b) starts at 0.5, and curve
//! (b) is held at its last value out to `2.5 c_re` then faded to zero by
//! `5 c_re`. Those two choices are the assumptions the chart leaves open.
//!
//! # What is primary vs cross-check
//!
//! [`fuselage_terms`] returns Torenbeek `D1` (Eq. E-37 on printed p. 480,
//! with `l_fn` from the nose to where the wing leading edge meets the
//! fuselage, Fig. E-13) and `D2` (Eq. E-38) as the *primary* fuselage
//! correction (both summed into the product neutral point); the Multhopp
//! strip shift and the TR 711 `K_fus` correlation are reported as
//! cross-check bands only and are never added into the neutral point.

use std::f64::consts::PI;

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;

/// Every term [`fuselage_terms`] computes, all in metres of neutral-point
/// shift (positive aft, negative forward), plus the downwash gradient it
/// used.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct FuselageTerms {
    /// Multhopp strip shift: a cross-check of [`Self::torenbeek_d1_shift_m`],
    /// *not* summed into the neutral point.
    pub multhopp_shift_m: f64,
    /// Torenbeek `D2` (eq. E-38, swept-wing body lift-loss): the second
    /// primary correction, summed with [`Self::torenbeek_d1_shift_m`] after
    /// weighting with the wing-body share of the lift-curve slope.
    pub torenbeek_d2_shift_m: f64,
    /// Torenbeek `D1` (eq. E-37): the primary body correction, forward
    /// (negative), normalised by the total lift-curve slope.
    pub torenbeek_d1_shift_m: f64,
    /// The NACA TR 711 (Gilruth and White) `K_fus` cross-check, Raymer 6th ed.
    /// Eq. 16.25 with Fig. 16.14 (read on the page), *not* summed into the
    /// neutral point.
    pub gilruth_shift_m: f64,
    /// FLAGGED empirical term: the public tail-off tunnel-data correction of
    /// the wing-body aerodynamic centre ([`super::tunnel_ac_correction`]
    /// times the wing's own mean aerodynamic chord), summed with `D2`'s
    /// weighting. Not from a handbook method; reported apart so it can be
    /// removed.
    #[serde(default)]
    pub tunnel_correction_shift_m: f64,
    /// The downwash gradient at the tail this call was given (passed
    /// through for the diagnostics struct one layer up).
    pub deps_dalpha: f64,
}

/// Digitised DATCOM Fig. 4.2.2.1-22(a): `d eps_u / d alpha` for the body
/// increment immediately forward of the wing, against `x_1 / c_re` (distance
/// forward of the exposed-root leading edge over the exposed root chord), for
/// a wing-body lift slope of 0.0785 per degree.
const UPWASH_NEAR_WING: [(f64, f64); 10] = [
    (0.1, 3.6),
    (0.2, 2.5),
    (0.3, 2.0),
    (0.4, 1.6),
    (0.5, 1.32),
    (0.6, 1.15),
    (0.7, 1.0),
    (0.8, 0.9),
    (0.9, 0.83),
    (1.06, 0.73),
];

/// Digitised DATCOM Fig. 4.2.2.1-22(b): the same gradient for body segments
/// 1-5, which start at `x_1 / c_re = 0.5`.
const UPWASH_FAR_FOREBODY: [(f64, f64); 6] = [
    (0.5, 0.36),
    (0.8, 0.26),
    (1.2, 0.14),
    (1.6, 0.085),
    (2.0, 0.05),
    (2.5, 0.035),
];

/// The chart wing-body lift slope both upwash curves are drawn for: 0.0785
/// per degree.
const CHART_CL_ALPHA_PER_RAD: f64 = 0.0785 * 180.0 / PI;

/// Length of body segment 6 (immediately forward of the exposed root) over the
/// exposed root chord: curve (b) starts at 0.5, so segment 6 is taken to end
/// there; curve (a) is read at this abscissa (Sketch (a): for segment 6 the
/// abscissa `x_1` is the segment length, not a centroid distance).
const SEGMENT_6_LENGTH_OVER_CRE: f64 = 0.5;

/// Raymer (6th ed.) Fig. 16.14, "Fuselage moment term" (after NACA TR 711),
/// printed p. 604: `K_fus` per degree against the wing-root quarter-chord
/// station as a fraction of the fuselage length. Digitised from the page
/// (rendered at 250 dpi, curve centre-line, about +-0.0005 read error); the
/// chart spans 0.10-0.60 and is held at its end values outside.
const RAYMER_K_FUS_PER_DEG: [(f64, f64); 12] = [
    (0.10, 0.0047),
    (0.15, 0.0051),
    (0.20, 0.0061),
    (0.25, 0.0077),
    (0.30, 0.0100),
    (0.35, 0.0131),
    (0.40, 0.0174),
    (0.45, 0.0234),
    (0.50, 0.0303),
    (0.55, 0.0378),
    (0.575, 0.0417),
    (0.60, 0.0460),
];

/// [`RAYMER_K_FUS_PER_DEG`] at the root quarter-chord fraction `x_over_l`.
pub(super) fn raymer_k_fus_per_deg(x_over_l: f64) -> f64 {
    interpolate(&RAYMER_K_FUS_PER_DEG, x_over_l)
}

/// Piecewise-linear interpolation in `table` (ascending in `x`), held at the
/// end values outside it.
fn interpolate(table: &[(f64, f64)], x: f64) -> f64 {
    let (first, last) = (table[0], table[table.len() - 1]);
    if x <= first.0 {
        return first.1;
    }
    if x >= last.0 {
        return last.1;
    }
    table
        .windows(2)
        .find(|pair| x <= pair[1].0)
        .map_or(last.1, |pair| {
            let t = (x - pair[0].0) / (pair[1].0 - pair[0].0);
            pair[0].1 + t * (pair[1].1 - pair[0].1)
        })
}

/// The DATCOM forebody upwash gradient `d eps_u / d alpha` at a body station
/// `x_1_over_cre` exposed-root chords ahead of the exposed-root leading
/// edge, for a wing-body lift slope `cl_alpha_wb` (per rad): segment 6 (the
/// increment `0 <= x_1 <= 0.5 c_re`) takes curve (a) at the abscissa of its
/// length (Sketch (a) draws `x_1` for segment 6 as its length to the leading edge),
/// further forward curve (b); both scale with `cl_alpha_wb / 0.0785` per
/// degree, and curve (b) fades to zero between 2.5 and 5 root chords.
pub(super) fn forebody_upwash_gradient(x_1_over_cre: f64, cl_alpha_wb: f64) -> f64 {
    let scale = cl_alpha_wb / CHART_CL_ALPHA_PER_RAD;
    let chart = if x_1_over_cre < 0.5 {
        interpolate(&UPWASH_NEAR_WING, SEGMENT_6_LENGTH_OVER_CRE)
    } else if x_1_over_cre <= 2.5 {
        interpolate(&UPWASH_FAR_FOREBODY, x_1_over_cre)
    } else {
        UPWASH_FAR_FOREBODY[UPWASH_FAR_FOREBODY.len() - 1].1 * (1.0 - (x_1_over_cre - 2.5) / 2.5)
    };
    scale * chart.max(0.0)
}

/// Half the primary fuselage width at the station nearest `x`, m (`0.0` with
/// no fuselage): where the wing meets the body.
pub(super) fn half_width_near(airplane: &Airplane, x: f64) -> f64 {
    airplane
        .fuselages
        .first()
        .and_then(|fus| {
            fus.xsecs
                .iter()
                .min_by(|a, b| (a.xyz_c[0] - x).abs().total_cmp(&(b.xyz_c[0] - x).abs()))
        })
        .map_or(0.0, |section| 0.5 * section.width)
}

/// Leading-edge station and chord of `wing` at spanwise station `y_abs`
/// (linear interpolation between the bounding cross-sections); `None` when
/// the station lies outside every section pair.
pub(super) fn wing_section_at(wing: &Wing, y_abs: f64) -> Option<(f64, f64)> {
    wing.xsecs.windows(2).find_map(|pair| {
        let (y0, y1) = (pair[0].xyz_le[1].abs(), pair[1].xyz_le[1].abs());
        let (lo, hi) = (y0.min(y1), y0.max(y1));
        if y_abs >= lo && y_abs <= hi && (hi - lo) > 1e-9 {
            let t = (y_abs - y0) / (y1 - y0);
            Some((
                pair[0].xyz_le[0] + t * (pair[1].xyz_le[0] - pair[0].xyz_le[0]),
                pair[0].chord + t * (pair[1].chord - pair[0].chord),
            ))
        } else {
            None
        }
    })
}

/// Torenbeek `D1` and `D2`, and the two cross-check terms (Multhopp, TR 711),
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
    cl_alpha_wing_body: f64,
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
        tunnel_correction_shift_m: 0.0,
        deps_dalpha,
    };
    let Some(fus) = airplane.fuselages.first() else {
        return zero;
    };
    if fus.xsecs.len() < 2 || cl_alpha_total.abs() < 1e-9 {
        return zero;
    }

    let mut order: Vec<usize> = (0..fus.xsecs.len()).collect();
    order.sort_by(|&a, &b| fus.xsecs[a].xyz_c[0].total_cmp(&fus.xsecs[b].xyz_c[0]));
    let xs: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].xyz_c[0]).collect();
    let ws: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].width).collect();
    let hs: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].height).collect();

    let s_ref = airplane
        .s_ref
        .max(crate::reference_dimensions::REFERENCE_AREA_FLOOR_M2);
    let c_ref = airplane
        .c_ref
        .max(crate::reference_dimensions::REFERENCE_CHORD_FLOOR_M);
    let b_ref = airplane
        .b_ref
        .max(crate::reference_dimensions::REFERENCE_SPAN_FLOOR_M);
    let root_chord = (x_te_root - x_le_root).max(0.1);
    let x_c4_root = x_le_root + 0.25 * root_chord;
    // DATCOM Sec. 4.2.2.1, Sketch (a): the afterbody factor
    // `(x_1 / l_h)(1 - d eps / d alpha)` has `x_1` and `l_h` measured from the
    // trailing edge of the exposed root chord.
    let l_h = (x_tail_ac - x_te_root).max(0.1);

    // The upwash charts are steep near the wing; a coarse trapezoidal sum
    // over the builder's ~20 fuselage stations would under-sample them.
    // Sub-sample each station interval into fine steps (400 per interval) so
    // the integral converges independently of the builder's station density.
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
                    forebody_upwash_gradient((x_le_root - x_mid) / root_chord, cl_alpha_wing_body);
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
    let b_f = ws.iter().copied().fold(
        crate::reference_dimensions::FUSELAGE_SECTION_FLOOR_M,
        f64::max,
    );
    let h_f = hs.iter().copied().fold(
        crate::reference_dimensions::FUSELAGE_SECTION_FLOOR_M,
        f64::max,
    );
    let c_g = s_ref / b_ref;
    // Eq. E-38's `c` is the wing's own mean aerodynamic chord, the lofted
    // planform's, not the reference frame's: the shift in metres goes as
    // `1/c`, so a published reference chord must not move it.
    let c_wing = crate::trim::main_wing(airplane)
        .map(alas_geom::aircraft::wing::Wing::mean_aerodynamic_chord)
        .filter(|chord| chord.is_finite() && *chord > 0.1)
        .unwrap_or(c_ref);
    let d2_fraction = 0.273 / (1.0 + taper.max(0.0)) * b_f * c_g * (b_ref - b_f)
        / (c_wing * c_wing * (b_ref + 2.15 * b_f))
        * sweep_c4_rad.tan();
    let torenbeek_d2_shift_m = d2_fraction * c_wing;
    // The flagged tunnel-data correction is a fraction of the reference MAC:
    // its fit data are referred to each model's (and the 747's published)
    // reference chord, not to a lofted planform with an inboard extension.
    let tunnel_correction_shift_m = super::tunnel_ac_correction(sweep_c4_rad) * c_ref;

    // Torenbeek (1982) Eq. E-37 (printed p. 480): `l_fn` runs from the nose to
    // where the wing leading edge meets the fuselage side (Fig. E-13).
    let l_fn = (x_le_root - xs[0]).max(0.1);
    let d1_fraction = -1.8 / cl_alpha_total * b_f * h_f * l_fn / (s_ref * c_ref);
    let torenbeek_d1_shift_m = d1_fraction * c_ref;

    // Raymer (6th ed., Sec. 16.3.8, Eq. 16.25, printed p. 603) from NACA TR 711
    // (Gilruth and White): Cm_alpha,fus = K_fus W_f^2 L_f / (c S) per degree,
    // W_f the maximum width, L_f the length, K_fus from Fig. 16.14 (printed
    // p. 604) against the root quarter-chord station as a fraction of the
    // fuselage length, measured from the nose.
    let x_c4_fraction = ((x_c4_root - xs[0]) / l_f.max(0.1)).max(0.0);
    let k_f_per_deg = raymer_k_fus_per_deg(x_c4_fraction);
    let cm_alpha_gilruth = k_f_per_deg.to_degrees() * b_f * b_f * l_f / (s_ref * c_ref);
    let gilruth_shift_m = -cm_alpha_gilruth * c_ref / cl_alpha_total;

    FuselageTerms {
        multhopp_shift_m,
        torenbeek_d2_shift_m,
        torenbeek_d1_shift_m,
        gilruth_shift_m,
        tunnel_correction_shift_m,
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
        let terms = fuselage_terms(&plane, 5.0, 4.5, 0.3, 5.0, 8.0, 30.0, 0.4, 0.3);
        assert_eq!(terms.multhopp_shift_m, 0.0);
        assert_eq!(terms.torenbeek_d2_shift_m, 0.0);
    }

    #[test]
    fn multhopp_shift_is_forward_for_a_conventional_layout() {
        let plane = probe_airplane();
        let terms = fuselage_terms(&plane, 5.0, 4.5, 0.3, 5.0, 8.0, 30.0, 0.4, 0.3);
        assert!(
            terms.multhopp_shift_m < 0.0,
            "shift={}",
            terms.multhopp_shift_m
        );
    }

    #[test]
    fn torenbeek_d2_is_aft_for_a_swept_wing() {
        let plane = probe_airplane();
        let unswept = fuselage_terms(&plane, 5.0, 4.5, 0.3, 5.0, 8.0, 30.0, 0.0, 0.3);
        let swept = fuselage_terms(&plane, 5.0, 4.5, 0.3, 5.0, 8.0, 30.0, 0.5, 0.3);
        assert_eq!(unswept.torenbeek_d2_shift_m, 0.0);
        assert!(swept.torenbeek_d2_shift_m > 0.0);
    }

    #[test]
    fn a_larger_vlm_downwash_reduces_the_afterbody_shift_magnitude() {
        let plane = probe_airplane();
        let low_downwash = fuselage_terms(&plane, 5.0, 4.5, 0.1, 5.0, 8.0, 30.0, 0.4, 0.3);
        let high_downwash = fuselage_terms(&plane, 5.0, 4.5, 0.5, 5.0, 8.0, 30.0, 0.4, 0.3);
        assert!(high_downwash.multhopp_shift_m.abs() < low_downwash.multhopp_shift_m.abs());
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod source_convention_tests {
    use super::*;
    use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};

    /// A radius-1 cylinder 30 m long: `w_f = 2`, `b_f = h_f = 2`.
    fn cylinder_airplane() -> Airplane {
        let station = |x: f64| {
            FuselageXSec::new([x, 0.0, 0.0], Some(1.0), None, None, DEFAULT_SHAPE)
                .expect("radius alone is valid")
        };
        Airplane {
            name: "Cylinder".to_owned(),
            xyz_ref: [10.0, 0.0, 0.0],
            wings: vec![],
            fuselages: vec![Fuselage::new("Fuselage", vec![station(0.0), station(30.0)])],
            s_ref: 24.0,
            c_ref: 3.0,
            b_ref: 8.0,
        }
    }

    /// DATCOM Sec. 4.2.2.1, Eq. 4.2.2.1-a with Sketch (a), worked by hand:
    /// exposed root 10 m to 14 m, tail 28 m, downwash 0.3, `CLa_wb = 0` (no
    /// upwash, so the forebody factor is exactly 1). Forebody: `w^2 = 4` over
    /// 0-10 m = 40. Afterbody: `(x_1 / l_h)(1 - 0.3)` with `x_1` and `l_h`
    /// from the trailing edge (14 m, `l_h = 14 m`): `Int_14^28 (x-14)/14 dx =
    /// 7`, plus 2 m at factor 1 = 9, times `4 * 0.7` = 25.2. Strip sum 65.2,
    /// `Cm_a = pi / (2 S c) * 65.2`, shift `= -Cm_a c / CLa`. A ramp measured
    /// from the root quarter-chord (11 m) gives a different sum (about 68.7, 5 % off)
    /// and fails this test.
    #[test]
    fn the_strip_sum_follows_the_datcom_trailing_edge_convention() {
        let plane = cylinder_airplane();
        let terms = fuselage_terms(&plane, 5.0, 0.0, 0.3, 10.0, 14.0, 28.0, 0.0, 0.3);
        let expected_m = -PI / (2.0 * 24.0 * 3.0) * 65.2 * 3.0 / 5.0;
        assert!(
            (terms.multhopp_shift_m - expected_m).abs() < 0.01 * expected_m.abs(),
            "shift {} m, hand value {expected_m} m",
            terms.multhopp_shift_m
        );
    }

    /// The over-wing strip carries no segment: widening the exposed root chord
    /// at fixed upwash removes that much body from the integral.
    #[test]
    fn the_body_alongside_the_exposed_root_is_excluded() {
        let plane = cylinder_airplane();
        let narrow = fuselage_terms(&plane, 5.0, 0.0, 0.3, 10.0, 12.0, 28.0, 0.0, 0.3);
        let wide = fuselage_terms(&plane, 5.0, 0.0, 0.3, 10.0, 16.0, 28.0, 0.0, 0.3);
        assert!(wide.multhopp_shift_m.abs() < narrow.multhopp_shift_m.abs());
    }

    /// Torenbeek (1982) Eq. E-37 (printed p. 480, Fig. E-13): `l_fn` runs
    /// from the nose to where the wing leading edge meets the fuselage.
    #[test]
    fn d1_uses_the_nose_to_junction_leading_edge_length() {
        let plane = cylinder_airplane();
        let terms = fuselage_terms(&plane, 5.0, 4.5, 0.3, 8.0, 12.0, 28.0, 0.0, 0.3);
        let l_fn = 8.0;
        let expected_m = -1.8 / 5.0 * 2.0 * 2.0 * l_fn / (24.0 * 3.0) * 3.0;
        assert!((terms.torenbeek_d1_shift_m - expected_m).abs() < 1e-12);
    }

    /// Raymer Fig. 16.14 as digitised (0.0100 per deg at 30 % of the body
    /// length, 0.0303 at 50 %), rising monotonically, held outside 10-60 %;
    /// and Eq. 16.25 by hand on the cylinder (root 8-12 m, quarter chord at
    /// 9 m = 30 % of 30 m): `Cm_a = 0.0100 * 57.296 * 2^2 * 30 / (3 * 24)`.
    #[test]
    fn the_raymer_fuselage_term_follows_fig_16_14_and_eq_16_25() {
        assert!((raymer_k_fus_per_deg(0.30) - 0.0100).abs() < 1e-12);
        assert!((raymer_k_fus_per_deg(0.50) - 0.0303).abs() < 1e-12);
        assert_eq!(raymer_k_fus_per_deg(0.05), raymer_k_fus_per_deg(0.10));
        assert_eq!(raymer_k_fus_per_deg(0.70), raymer_k_fus_per_deg(0.60));
        let samples: Vec<f64> = (10..=60)
            .map(|p| raymer_k_fus_per_deg(f64::from(p) / 100.0))
            .collect();
        assert!(samples.windows(2).all(|w| w[1] >= w[0]));
        let plane = cylinder_airplane();
        let terms = fuselage_terms(&plane, 5.0, 4.5, 0.3, 8.0, 12.0, 28.0, 0.0, 0.3);
        let cm_alpha = 0.0100_f64.to_degrees() * 4.0 * 30.0 / (3.0 * 24.0);
        let expected_m = -cm_alpha * 3.0 / 5.0;
        assert!(
            (terms.gilruth_shift_m - expected_m).abs() < 1e-9,
            "{}",
            terms.gilruth_shift_m
        );
    }

    #[test]
    fn the_upwash_charts_are_read_and_scaled_as_digitised() {
        // Curve (a) at the segment-6 length (0.5 c_re), chart wing-body slope.
        let chart_slope = 0.0785 * 180.0 / PI;
        assert!((forebody_upwash_gradient(0.25, chart_slope) - 1.32).abs() < 1e-9);
        // Curve (b), scaled by CLa_wb / 0.0785 per degree.
        assert!((forebody_upwash_gradient(0.8, chart_slope) - 0.26).abs() < 1e-9);
        assert!(
            (forebody_upwash_gradient(0.8, 2.0 * chart_slope) - 0.52).abs() < 1e-9,
            "scales with the wing-body lift slope"
        );
        assert_eq!(forebody_upwash_gradient(5.0, chart_slope), 0.0);
    }

    #[test]
    fn the_junction_section_sits_aft_of_the_centreline_section_on_a_swept_wing() {
        use alas_geom::aircraft::airfoil::Airfoil;
        use alas_geom::aircraft::wing::WingXSec;
        let naca = Airfoil::from_name("naca0012").expect("airfoil");
        let wing = Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([8.0, 0.0, 0.0], 4.0, 0.0, naca.clone()),
                WingXSec::new([14.0, 10.0, 0.0], 1.5, 0.0, naca),
            ],
            true,
        );
        let (x_le, chord) = wing_section_at(&wing, 2.5).expect("inside the section pair");
        assert!((x_le - 9.5).abs() < 1e-12 && (chord - 3.375).abs() < 1e-12);
        assert!(wing_section_at(&wing, 11.0).is_none());
    }
}
