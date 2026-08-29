// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/stability.py
// Reference: alas @ rust-port-baseline.

//! Longitudinal stability and balance: the static margin, the neutral point,
//! the cruise-condition trim solve, the `autobalance` CG shift, and the
//! closed-form tail-volume and Munk apparent-mass correlations they build on.
//!
//! # Two kinds of computation in one module
//!
//! [`static_margin`], [`neutral_point`] and [`stability_and_trim`] each run
//! two or three [`alas_aero::vlm`] solves on the airplane and read
//! stability derivatives off finite differences of `CL` and `Cm`;
//! [`autobalance`] shifts the CG by the static margin those produce.
//! [`munk_apparent_mass_factor`], [`fuselage_cm_alpha`] and
//! [`tail_volume_coefficients`] are closed-form `f64` arithmetic over the
//! geometry and a published table -- no factorization anywhere in them, though
//! two of the three are reached *from inside* the VLM-fed functions and so see
//! a VLM-derived slope as an input.
//!
//! # The stabilizer perturbation is a copy, not a mutation
//!
//! [`stability_and_trim`] measures the incidence derivatives by rigidly
//! deflecting the horizontal stabilizer and re-solving. Upstream overwrites
//! every stabilizer section's twist in place and restores it in a `finally`;
//! this port solves on a [`Airplane::clone`]d, perturbed copy
//! ([`with_hstab_twist`]) instead. The result is identical and the aircraft a
//! caller holds comes back unchanged either way -- and it is also why
//! upstream's restore quirk (it writes `xsecs[0]`'s twist back to *all* the
//! sections, losing any spanwise variation) has nothing to reproduce here: no
//! section of the caller's airplane is ever written to. `alas-aero::analysis`'s
//! `trimmed_performance` already made the same decision, for the same reason.
//! [`autobalance`] *is* an in-place mutation, matching upstream: its whole
//! contract is to shift the caller's `xyz_ref[0]`.
//!
//! # `_xsec_width`/`_xsec_height` are field reads here
//!
//! Every function `alas/physics/stability.py` defines is translated here;
//! `_xsec_width`/`_xsec_height` are the only two that change shape. Upstream's
//! two accessors answer an explicit `width`/`height` or fall back to
//! `2 * radius`; `alas-geom::aircraft::fuselage`'s `FuselageXSec` normalizes
//! `radius` into `width`/`height` at construction, so both reduce to a field
//! read here. (`alas-payload`'s `CabinGeometry` reimplements them locally for
//! the same reason its row records.)

use std::f64::consts::PI;

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::{self, VlmError, VlmResult};
use alas_atmo::Atmosphere;
use alas_config::analysis::AnalysisConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;
use alas_math::{interp, linalg};

/// The wing this module reads the aerodynamic centre and aspect ratio off,
/// falling back to the first wing when none is named this -- upstream's
/// `next((w for w in airplane.wings if w.name == "Main Wing"), airplane.wings[0])`.
const MAIN_WING_NAME: &str = "Main Wing";

/// The trimmable surface [`stability_and_trim`] deflects and
/// [`fuselage_cm_alpha`] reads the tail station off.
const HSTAB_NAME: &str = "Horizontal Stabilizer";

/// `aerodynamic_center()`'s default `chord_fraction` -- the quarter-MAC point,
/// which every call site here takes unspecified.
const AC_CHORD_FRACTION: f64 = 0.25;

/// The `abs(dCL) < 1e-9` (and `abs(cl_alpha) < 1e-9`, and `abs(d_alpha) < 1e-9`)
/// degeneracy floor upstream guards every slope division with.
const DEGENERACY_FLOOR: f64 = 1e-9;

/// Cruise-condition static margin plus the closed-form longitudinal trim
/// solve -- `StabilityTrimResult`. `trim_ih_deg` is NaN and `cl_ih`/`cm_ih`
/// are `0.0` when the airplane has no horizontal stabilizer (the pure-alpha
/// trim fallback).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StabilityTrimResult {
    /// Whether the requested lift and pitching-moment equations were solved
    /// with a finite, nonsingular trim Jacobian.
    pub converged: bool,
    /// The corrected neutral-point station, in geometry axes.
    pub x_np: f64,
    /// Static margin `(x_np - x_cg) / c_bar`, measured against the real CG.
    pub static_margin: f64,
    /// Lift-curve slope `dCL/dalpha`, *per degree* (the probe delta is in
    /// degrees), as the two alpha probes measured it.
    pub cl_alpha: f64,
    /// The angle of attack that trims `CL = cl_target`, degrees.
    pub trim_alpha_deg: f64,
    /// The horizontal-stabilizer incidence that trims `Cm = 0`, degrees; NaN
    /// with no stabilizer.
    pub trim_ih_deg: f64,
    /// `dCL/di_h`, per degree of stabilizer incidence; `0.0` with no
    /// stabilizer.
    pub cl_ih: f64,
    /// `dCm/di_h`, per degree of stabilizer incidence; `0.0` with no
    /// stabilizer.
    pub cm_ih: f64,
}

/// The Munk `(k2 - k1)` apparent-mass factor against fuselage fineness ratio
/// `L/d` -- `munk_apparent_mass_factor`. Clamped to the tabulated range
/// `[4, 20]`: approaches 1 for a very slender body, lower for a stubby one.
/// (Munk; tabulated in Roskam / USAF DATCOM.)
pub fn munk_apparent_mass_factor(fineness: f64) -> f64 {
    const FINENESS: [f64; 7] = [4.0, 6.0, 8.0, 10.0, 12.0, 16.0, 20.0];
    const FACTOR: [f64; 7] = [0.77, 0.86, 0.91, 0.94, 0.955, 0.97, 0.98];
    // Upstream's `max(4.0, min(20.0, fineness))`; identical to a clamp for
    // every reachable (finite) fineness, which is all this is ever called with.
    interp(fineness.clamp(4.0, 20.0), &FINENESS, &FACTOR)
}

/// The fuselage pitching-moment slope `dCm/dalpha` [per rad] by the
/// slender-body (Munk/Multhopp) method -- `fuselage_cm_alpha`. Positive is
/// destabilising (moves the neutral point forward). Integrates the *actual*
/// fuselage cross-section area distribution `A(x) = (pi/4) w(x) h(x)`, with a
/// local-flow factor that reduces the afterbody's contribution by the wing
/// downwash. `cl_alpha` is the wing lift-curve slope, per radian.
pub fn fuselage_cm_alpha(airplane: &Airplane, cl_alpha: f64) -> f64 {
    fuselage_cm_alpha_with_reference_mode(airplane, cl_alpha, false)
}

/// Frozen translation/parity form of [`fuselage_cm_alpha`].
///
/// The historical Python expression used the wing's unfolded YZ span and
/// area for aspect ratio. Product callers use the aircraft's selected
/// projected references through [`fuselage_cm_alpha`].
pub fn fuselage_cm_alpha_reference_compatibility(airplane: &Airplane, cl_alpha: f64) -> f64 {
    fuselage_cm_alpha_with_reference_mode(airplane, cl_alpha, true)
}

fn fuselage_cm_alpha_with_reference_mode(
    airplane: &Airplane,
    cl_alpha: f64,
    reference_compatibility: bool,
) -> f64 {
    let fus = &airplane.fuselages[0];

    // `np.argsort` on the station X coordinates, then applied to the widths
    // and heights (which are `_xsec_width`/`_xsec_height`; see the module doc).
    let mut order: Vec<usize> = (0..fus.xsecs.len()).collect();
    order.sort_by(|&a, &b| {
        fus.xsecs[a].xyz_c[0]
            .partial_cmp(&fus.xsecs[b].xyz_c[0])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let xs: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].xyz_c[0]).collect();
    let ws: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].width).collect();
    let hs: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].height).collect();
    if xs.len() < 2 {
        return 0.0;
    }

    let wing = main_wing(airplane);
    let x_le = wing.xsecs[0].xyz_le[0];
    let x_te = x_le + wing.xsecs[0].chord;
    let x_h = match hstab(airplane) {
        Some(h) => h.aerodynamic_center(AC_CHORD_FRACTION)[0],
        None => x_te + 3.0 * airplane.c_ref,
    };

    let s_ref = airplane.s_ref.max(1.0);
    let c_ref = airplane.c_ref.max(0.1);
    // Downwash uses the aircraft reference-plane aspect ratio in the product
    // path. The explicit parity mode retains the historical unfolded wing
    // aspect ratio used by the translated Python correlation.
    let ar = if reference_compatibility {
        (wing.unfolded_span().powi(2) / wing.unfolded_area().max(1e-6)).max(1.0)
    } else {
        (airplane.b_ref.powi(2) / airplane.s_ref.max(1e-6)).max(1.0)
    };
    // Downwash gradient at the tail.
    let d_eps_d_alpha = 2.0 * cl_alpha.max(0.1) / (PI * ar);

    let l_f = xs[xs.len() - 1] - xs[0];
    // `max(0.1, ws.max())`: folding from 0.1 is that maximum, in one pass.
    let d_f = ws.iter().copied().fold(0.1_f64, f64::max);
    let k_fac = munk_apparent_mass_factor(l_f / d_f);

    let mut accum = 0.0;
    for i in 0..xs.len() - 1 {
        let dx = xs[i + 1] - xs[i];
        if dx <= 0.0 {
            continue;
        }
        let x_mid = 0.5 * (xs[i] + xs[i + 1]);
        let w_mid = 0.5 * (ws[i] + ws[i + 1]);
        let h_mid = 0.5 * (hs[i] + hs[i + 1]);
        let area = PI / 4.0 * w_mid * h_mid;
        let eta_local = if x_mid <= x_te {
            // Fore-body / over-wing: ~free-stream alpha.
            1.0
        } else {
            // After-body: reduced by the wing downwash.
            let frac = ((x_mid - x_te) / (x_h - x_te).max(0.1)).min(1.0);
            1.0 - d_eps_d_alpha * frac
        };
        accum += area * eta_local * dx;
    }

    k_fac * 2.0 * accum / (s_ref * c_ref)
}

/// Static margin `SM = -dCm/dCL` from two low-speed VLM operating points --
/// `static_margin`. NaN when the two probes carry the same lift (a degenerate
/// `dCL`).
///
/// # Errors
///
/// See [`VlmError`]: the VLM solve failing to mesh or factorize.
pub fn static_margin(airplane: &Airplane, analysis: &AnalysisConfig) -> Result<f64, VlmError> {
    let atmosphere = Atmosphere::new(0.0);
    let velocity = analysis.autobalance_velocity_m_s;
    let r_lo = probe(
        airplane,
        analysis,
        atmosphere,
        velocity,
        analysis.autobalance_alpha_low_deg,
    )?;
    let r_hi = probe(
        airplane,
        analysis,
        atmosphere,
        velocity,
        analysis.autobalance_alpha_high_deg,
    )?;
    let d_cm = r_hi.cm_pitch - r_lo.cm_pitch;
    let d_cl = r_hi.cl_lift - r_lo.cl_lift;
    if d_cl.abs() < DEGENERACY_FLOOR {
        return Ok(f64::NAN);
    }
    Ok(-d_cm / d_cl)
}

/// Shift the CG so the aircraft's static margin equals `target_static_margin`
/// -- `autobalance`. Mutates `airplane.xyz_ref[0]` in place and returns the
/// static margin measured *before* the correction (a free byproduct of the
/// VLM calls [`static_margin`] already made, for penalty terms). A positive
/// shift moves the CG aft, reducing the margin. Returns NaN and leaves the
/// airplane unshifted when the static margin is itself NaN.
///
/// # Errors
///
/// See [`VlmError`].
pub fn autobalance(
    airplane: &mut Airplane,
    target_static_margin: f64,
    analysis: &AnalysisConfig,
) -> Result<f64, VlmError> {
    let sm_current = static_margin(airplane, analysis)?;
    if sm_current.is_nan() {
        return Ok(f64::NAN);
    }
    let shift = (sm_current - target_static_margin) * airplane.c_ref;
    airplane.xyz_ref[0] += shift;
    Ok(sm_current)
}

/// Physically-anchored neutral point, static margin and lift-curve slope --
/// `neutral_point`, returning `(x_np, static_margin, cl_alpha)`. Probes at the
/// fixed low-speed reference condition (`autobalance_velocity_m_s`), applies a
/// tail dynamic-pressure efficiency to the tail's stabilising contribution,
/// then shifts the neutral point forward by the fuselage (Munk/Multhopp) term.
/// `static_margin` is measured against the real CG (`xyz_ref[0]`). NaN margin
/// and slope on a degenerate probe.
///
/// # Errors
///
/// See [`VlmError`].
pub fn neutral_point(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
) -> Result<(f64, f64, f64), VlmError> {
    neutral_point_with_reference_mode(airplane, analysis, false)
}

/// Frozen translation/parity form of [`neutral_point`].
pub fn neutral_point_reference_compatibility(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
) -> Result<(f64, f64, f64), VlmError> {
    neutral_point_with_reference_mode(airplane, analysis, true)
}

fn neutral_point_with_reference_mode(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    reference_compatibility: bool,
) -> Result<(f64, f64, f64), VlmError> {
    let atmosphere = Atmosphere::new(0.0);
    let velocity = analysis.autobalance_velocity_m_s;
    let r_lo = probe(
        airplane,
        analysis,
        atmosphere,
        velocity,
        analysis.autobalance_alpha_low_deg,
    )?;
    let r_hi = probe(
        airplane,
        analysis,
        atmosphere,
        velocity,
        analysis.autobalance_alpha_high_deg,
    )?;

    let d_cl = r_hi.cl_lift - r_lo.cl_lift;
    let d_cm = r_hi.cm_pitch - r_lo.cm_pitch;
    let d_alpha =
        (analysis.autobalance_alpha_high_deg - analysis.autobalance_alpha_low_deg).to_radians();
    let c_ref = airplane.c_ref.max(0.1);
    let x_cg = airplane.xyz_ref[0];

    if d_cl.abs() < DEGENERACY_FLOOR || d_alpha.abs() < DEGENERACY_FLOOR {
        return Ok((x_cg, f64::NAN, f64::NAN));
    }

    let cl_alpha = d_cl / d_alpha;
    let sm_vlm = -d_cm / d_cl;
    let x_np_vlm = x_cg + sm_vlm * c_ref;
    let x_wing_ac = main_wing(airplane).aerodynamic_center(AC_CHORD_FRACTION)[0];
    let eta_t = analysis.tail_efficiency;
    let mut x_np = x_wing_ac + eta_t * (x_np_vlm - x_wing_ac);

    if analysis.include_fuselage_stability && cl_alpha > 0.1 {
        let cm_a_fus =
            fuselage_cm_alpha_with_reference_mode(airplane, cl_alpha, reference_compatibility);
        x_np -= cm_a_fus * c_ref / cl_alpha;
    }

    let sm = (x_np - x_cg) / c_ref;
    Ok((x_np, sm, cl_alpha))
}

/// Cruise-condition three-point VLM probe: static margin plus a closed-form
/// longitudinal trim solve -- `stability_and_trim`. Probes at the actual
/// cruise Mach/altitude, so the same solves serve both the static-margin
/// measurement and a genuine trimmed solve: alpha and stabilizer incidence
/// jointly satisfying `CL = cl_target` and `Cm = 0` (a closed-form 2x2). With
/// no horizontal stabilizer, degrades to a pure-alpha trim (`trim_ih_deg`
/// NaN), never raising for that case.
///
/// # Errors
///
/// See [`VlmError`].
pub fn stability_and_trim(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    cl_target: f64,
    mach: f64,
    altitude_m: f64,
) -> Result<StabilityTrimResult, VlmError> {
    stability_and_trim_with_reference_mode(airplane, analysis, cl_target, mach, altitude_m, false)
}

/// Frozen translation/parity form of [`stability_and_trim`].
pub fn stability_and_trim_reference_compatibility(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    cl_target: f64,
    mach: f64,
    altitude_m: f64,
) -> Result<StabilityTrimResult, VlmError> {
    stability_and_trim_with_reference_mode(airplane, analysis, cl_target, mach, altitude_m, true)
}

fn stability_and_trim_with_reference_mode(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    cl_target: f64,
    mach: f64,
    altitude_m: f64,
    reference_compatibility: bool,
) -> Result<StabilityTrimResult, VlmError> {
    let atmosphere = Atmosphere::new(altitude_m);
    let velocity = mach * atmosphere.speed_of_sound();
    let a_lo = analysis.probe_alpha_low_deg;
    let a_hi = analysis.probe_alpha_high_deg;
    let delta_ih = analysis.trim_incidence_probe_delta_deg;

    let r1 = probe(airplane, analysis, atmosphere, velocity, a_lo)?;
    let r2 = probe(airplane, analysis, atmosphere, velocity, a_hi)?;

    let d_alpha = a_hi - a_lo;
    let (cl_alpha, cm_alpha) = if d_alpha.abs() > DEGENERACY_FLOOR {
        (
            (r2.cl_lift - r1.cl_lift) / d_alpha,
            (r2.cm_pitch - r1.cm_pitch) / d_alpha,
        )
    } else {
        (f64::NAN, f64::NAN)
    };

    let x_cg = airplane.xyz_ref[0];
    let c_ref = airplane.c_ref.max(0.1);

    let (x_np, sm) = if cl_alpha.is_nan() || cl_alpha.abs() < DEGENERACY_FLOOR {
        (x_cg, f64::NAN)
    } else {
        let sm_vlm = -cm_alpha / cl_alpha;
        let x_np_vlm = x_cg + sm_vlm * c_ref;
        let x_wing_ac = main_wing(airplane).aerodynamic_center(AC_CHORD_FRACTION)[0];
        let eta_t = analysis.tail_efficiency;
        let mut x_np = x_wing_ac + eta_t * (x_np_vlm - x_wing_ac);

        // `cl_alpha` is per degree here; the fuselage term wants it per radian.
        let cl_alpha_per_rad = cl_alpha / 1.0_f64.to_radians();
        if analysis.include_fuselage_stability && cl_alpha_per_rad > 0.1 {
            let cm_a_fus = fuselage_cm_alpha_with_reference_mode(
                airplane,
                cl_alpha_per_rad,
                reference_compatibility,
            );
            x_np -= cm_a_fus * c_ref / cl_alpha_per_rad;
        }
        (x_np, (x_np - x_cg) / c_ref)
    };

    // `has_ih = hstab is not None and len(hstab.xsecs) > 0`, and the root twist
    // in one step: `Some(twist)` exactly when both hold.
    let i_h0 = match hstab(airplane).and_then(|h| h.xsecs.first()) {
        Some(root) => root.twist,
        None => {
            let trim_alpha = if cl_alpha.abs() > DEGENERACY_FLOOR {
                a_lo + (cl_target - r1.cl_lift) / cl_alpha
            } else {
                a_lo
            };
            return Ok(StabilityTrimResult {
                converged: false,
                x_np,
                static_margin: sm,
                cl_alpha,
                trim_alpha_deg: trim_alpha,
                trim_ih_deg: f64::NAN,
                cl_ih: 0.0,
                cm_ih: 0.0,
            });
        }
    };

    let perturbed = with_hstab_twist(airplane, i_h0 + delta_ih);
    let r3 = probe(&perturbed, analysis, atmosphere, velocity, a_lo)?;

    let cl_ih = (r3.cl_lift - r1.cl_lift) / delta_ih;
    let cm_ih = (r3.cm_pitch - r1.cm_pitch) / delta_ih;

    // Closed-form 2x2 solve: [[cl_alpha, cl_ih], [cm_alpha, cm_ih]] @ [d_a, d_ih]
    //                       = [cl_target - CL_1, -Cm_1]  (slopes per degree).
    let jacobian = vec![vec![cl_alpha, cl_ih], vec![cm_alpha, cm_ih]];
    let rhs = vec![vec![cl_target - r1.cl_lift], vec![-r1.cm_pitch]];
    let (d_a, d_ih, converged) = match linalg::solve(&jacobian, &rhs) {
        Ok(solution)
            if solution.len() >= 2
                && solution[0].first().is_some_and(|value| value.is_finite())
                && solution[1].first().is_some_and(|value| value.is_finite())
                && jacobian
                    .first()
                    .and_then(|row| row.first())
                    .zip(jacobian.get(1).and_then(|row| row.get(1)))
                    .zip(jacobian.first().and_then(|row| row.get(1)))
                    .zip(jacobian.get(1).and_then(|row| row.first()))
                    .is_some_and(|(((a, d), b), c)| (a * d - b * c).abs() > DEGENERACY_FLOOR) =>
        {
            (solution[0][0], solution[1][0], true)
        }
        // Upstream's `except np.linalg.LinAlgError`: a singular Jacobian falls
        // back to the pure-alpha correction with the incidence left as flown.
        Err(_) => {
            let d_a = if cl_alpha.abs() > DEGENERACY_FLOOR {
                (cl_target - r1.cl_lift) / cl_alpha
            } else {
                0.0
            };
            (d_a, 0.0, false)
        }
        Ok(_) => (0.0, 0.0, false),
    };

    Ok(StabilityTrimResult {
        converged,
        x_np,
        static_margin: sm,
        cl_alpha,
        trim_alpha_deg: a_lo + d_a,
        trim_ih_deg: i_h0 + d_ih,
        cl_ih,
        cm_ih,
    })
}

/// Horizontal and vertical tail volume coefficients `(Vh, Vv)` --
/// `tail_volume_coefficients`. `Vh = Sh Lh / (S c_bar)`,
/// `Vv = Sv Lv / (S b)`, with the moment arms taken from the wings' quarter-
/// MAC aerodynamic centres. `None` for either when the airplane has fewer than
/// two/three wings. The tails are identified by position (`wings[1]`,
/// `wings[2]`), as upstream indexes them, not by name.
pub fn tail_volume_coefficients(airplane: &Airplane) -> (Option<f64>, Option<f64>) {
    tail_volume_coefficients_with_reference_mode(airplane, false)
}

/// Frozen translation/parity form of [`tail_volume_coefficients`].
pub fn tail_volume_coefficients_reference_compatibility(
    airplane: &Airplane,
) -> (Option<f64>, Option<f64>) {
    tail_volume_coefficients_with_reference_mode(airplane, true)
}

fn tail_volume_coefficients_with_reference_mode(
    airplane: &Airplane,
    reference_compatibility: bool,
) -> (Option<f64>, Option<f64>) {
    let s_ref = airplane.s_ref.max(1.0);
    let c_bar = airplane.c_ref.max(0.1);
    let b_ref = airplane.b_ref.max(1.0);
    let x_wing_ac = airplane.wings[0].aerodynamic_center(AC_CHORD_FRACTION)[0];

    let vh = if airplane.wings.len() > 1 {
        let hstab = &airplane.wings[1];
        let l_h = (hstab.aerodynamic_center(AC_CHORD_FRACTION)[0] - x_wing_ac).max(0.0);
        let area = if reference_compatibility {
            hstab.unfolded_area()
        } else {
            hstab.reference_area()
        };
        Some(area * l_h / (s_ref * c_bar))
    } else {
        None
    };

    let vv = if airplane.wings.len() > 2 {
        let vstab = &airplane.wings[2];
        let l_v = (vstab.aerodynamic_center(AC_CHORD_FRACTION)[0] - x_wing_ac).max(0.0);
        // A vertical fin's planform is the XZ surface, so projecting it onto
        // the aircraft XY reference plane would collapse its area to zero.
        // Keep its physical fin planform explicit; only the main-wing
        // denominator and lateral arm are aircraft XY reference quantities.
        Some(vstab.unfolded_area() * l_v / (s_ref * b_ref))
    } else {
        None
    };

    (vh, vv)
}

/// One low-side/high-side VLM probe: a level operating point (no sideslip, no
/// rotation) at `alpha_deg`, solved at the analysis mesh resolution.
fn probe(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    atmosphere: Atmosphere,
    velocity: f64,
    alpha_deg: f64,
) -> Result<VlmResult, VlmError> {
    let op_point = OperatingPoint::new(atmosphere, velocity, alpha_deg, 0.0, 0.0, 0.0, 0.0);
    vlm::run(
        airplane,
        &op_point,
        resolution(analysis.spanwise_resolution),
        resolution(analysis.chordwise_resolution),
    )
}

/// The configuration's `i64` mesh resolution as the `usize` [`vlm::run`]
/// takes -- floored at one, as `alas-aero::analysis` does: a zero or negative
/// resolution is not a mesh, and both fields are documented multipliers of at
/// least one.
fn resolution(value: i64) -> usize {
    value.max(1) as usize
}

/// The main wing (or the first wing when none is named [`MAIN_WING_NAME`]) --
/// upstream's `next((w ... if w.name == "Main Wing"), airplane.wings[0])`.
fn main_wing(airplane: &Airplane) -> &Wing {
    airplane
        .wings
        .iter()
        .find(|w| w.name == MAIN_WING_NAME)
        .unwrap_or(&airplane.wings[0])
}

/// The horizontal stabilizer, if the airplane has one named [`HSTAB_NAME`].
fn hstab(airplane: &Airplane) -> Option<&Wing> {
    airplane.wings.iter().find(|w| w.name == HSTAB_NAME)
}

/// A clone of `airplane` with every [`HSTAB_NAME`] section's twist set to
/// `twist_deg` -- the rigid stabilizer perturbation, on a copy. See the module
/// doc for why this is a clone rather than a mutate-and-restore.
fn with_hstab_twist(airplane: &Airplane, twist_deg: f64) -> Airplane {
    let mut plane = airplane.clone();
    for wing in plane.wings.iter_mut().filter(|w| w.name == HSTAB_NAME) {
        for xsec in &mut wing.xsecs {
            xsec.twist = twist_deg;
        }
    }
    plane
}

// A test asserts on values it constructed here directly, so a failed unwrap or
// expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
    use alas_geom::aircraft::wing::WingXSec;

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    /// A short circular fuselage spanning past the wing trailing edge, so
    /// [`fuselage_cm_alpha`] reaches both its fore-body and after-body
    /// branches -- what the VLM-fed functions need present (upstream's
    /// `fuselages[0]` would `IndexError` on an airplane with none).
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

    /// A small probe: a symmetric main wing, a circular fuselage, and
    /// optionally a horizontal stabilizer aft of the wing.
    fn probe_airplane(with_hstab: bool) -> Airplane {
        let main = Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 3.0, 0.0, naca("naca0012")),
                WingXSec::new([0.0, 8.0, 0.0], 3.0, 0.0, naca("naca0012")),
            ],
            true,
        );
        let mut wings = vec![main];
        if with_hstab {
            wings.push(Wing::new(
                HSTAB_NAME,
                vec![
                    WingXSec::new([15.0, 0.0, 0.0], 1.5, -2.0, naca("naca0012")),
                    WingXSec::new([15.0, 3.0, 0.0], 1.5, -2.0, naca("naca0012")),
                ],
                true,
            ));
        }
        let s_ref = wings[0].reference_area();
        let b_ref = wings[0].reference_span();
        let c_ref = wings[0].mean_aerodynamic_chord();
        Airplane {
            name: "Probe".to_owned(),
            xyz_ref: [1.0, 0.0, 0.0],
            wings,
            fuselages: vec![probe_fuselage()],
            s_ref,
            c_ref,
            b_ref,
        }
    }

    #[test]
    fn munk_factor_clamps_to_the_tabulated_range() {
        assert_eq!(munk_apparent_mass_factor(2.0), 0.77);
        assert_eq!(munk_apparent_mass_factor(4.0), 0.77);
        assert_eq!(munk_apparent_mass_factor(20.0), 0.98);
        assert_eq!(munk_apparent_mass_factor(100.0), 0.98);
    }

    #[test]
    fn munk_factor_interpolates_between_table_points() {
        // Fineness 9 sits halfway between the 8 (0.91) and 10 (0.94) rows.
        assert!((munk_apparent_mass_factor(9.0) - 0.925).abs() < 1e-15);
    }

    #[test]
    fn fuselage_cm_alpha_is_zero_for_a_single_station_body() {
        // The `len(xs) < 2` guard: a one-station fuselage has no area to
        // integrate over.
        let mut plane = probe_airplane(true);
        plane.fuselages = vec![Fuselage::new(
            "Fuselage",
            vec![
                FuselageXSec::new([0.0, 0.0, 0.0], Some(2.0), None, None, DEFAULT_SHAPE)
                    .expect("radius alone is valid"),
            ],
        )];
        assert_eq!(fuselage_cm_alpha(&plane, 5.5), 0.0);
    }

    #[test]
    fn static_margin_is_nan_when_the_two_probes_carry_the_same_lift() {
        // Coincident probe alphas make dCL exactly zero, reaching the
        // degeneracy guard -- the branch no real geometry reaches.
        let plane = probe_airplane(true);
        let analysis = AnalysisConfig {
            autobalance_alpha_low_deg: 2.0,
            autobalance_alpha_high_deg: 2.0,
            ..Default::default()
        };
        let sm = static_margin(&plane, &analysis).expect("the probe meshes and solves");
        assert!(sm.is_nan(), "sm={sm}");
    }

    #[test]
    fn autobalance_leaves_the_cg_unshifted_when_the_static_margin_is_nan() {
        let mut plane = probe_airplane(true);
        let before = plane.xyz_ref[0];
        let analysis = AnalysisConfig {
            autobalance_alpha_low_deg: 2.0,
            autobalance_alpha_high_deg: 2.0,
            ..Default::default()
        };
        let sm = autobalance(&mut plane, 0.1, &analysis).expect("the probe meshes and solves");
        assert!(sm.is_nan(), "sm={sm}");
        assert_eq!(plane.xyz_ref[0], before);
    }

    #[test]
    fn autobalance_shifts_the_cg_aft_toward_a_lower_target_margin() {
        // A stable probe has a positive static margin; asking for a smaller
        // one moves the CG aft (a positive shift), by SM being measured in
        // fractions of the chord.
        let mut plane = probe_airplane(true);
        let before = plane.xyz_ref[0];
        let analysis = AnalysisConfig::default();
        let sm_before =
            autobalance(&mut plane, 0.05, &analysis).expect("the probe meshes and solves");
        assert!(sm_before > 0.05, "sm_before={sm_before}");
        let expected = before + (sm_before - 0.05) * plane.c_ref;
        assert!(
            (plane.xyz_ref[0] - expected).abs() < 1e-12,
            "xyz_ref_x={} expected={expected}",
            plane.xyz_ref[0]
        );
    }

    #[test]
    fn stability_and_trim_degrades_to_pure_alpha_without_a_stabilizer() {
        let plane = probe_airplane(false);
        let analysis = AnalysisConfig::default();
        let result = stability_and_trim(&plane, &analysis, 0.4, 0.3, 0.0)
            .expect("the probe meshes and solves");
        assert!(result.trim_ih_deg.is_nan(), "i_h={}", result.trim_ih_deg);
        assert_eq!(result.cl_ih, 0.0);
        assert_eq!(result.cm_ih, 0.0);
        assert!(result.trim_alpha_deg.is_finite());
    }

    #[test]
    fn stability_and_trim_leaves_the_stabilizer_twist_unchanged() {
        // The perturbation is a clone, so the caller's aircraft comes back as
        // it went in -- the property that keeps repeated evaluations from
        // drifting onto altered geometry.
        let plane = probe_airplane(true);
        let before = plane.clone();
        let analysis = AnalysisConfig::default();
        stability_and_trim(&plane, &analysis, 0.4, 0.3, 0.0).expect("the probe meshes and solves");
        assert_eq!(plane.wings, before.wings);
    }

    #[test]
    fn with_hstab_twist_perturbs_the_copy_and_not_the_source() {
        let plane = probe_airplane(true);
        let perturbed = with_hstab_twist(&plane, 5.0);
        for xsec in &perturbed.wings[1].xsecs {
            assert_eq!(xsec.twist, 5.0);
        }
        for xsec in &plane.wings[1].xsecs {
            assert_eq!(xsec.twist, -2.0);
        }
    }

    #[test]
    fn tail_volume_coefficients_report_none_for_missing_surfaces() {
        let main_only = probe_airplane(false);
        let (vh, vv) = tail_volume_coefficients(&main_only);
        assert!(vh.is_none());
        assert!(vv.is_none());

        let with_hstab = probe_airplane(true);
        let (vh, vv) = tail_volume_coefficients(&with_hstab);
        assert!(vh.is_some_and(|v| v > 0.0), "Vh={vh:?}");
        assert!(vv.is_none());
    }

    #[test]
    fn tail_volume_uses_projected_stabilizer_area() {
        let mut plane = probe_airplane(true);
        // Introduce a nonzero stabilizer dihedral so unfolded and reference
        // planform areas are observably different.
        plane.wings[1].xsecs[1].xyz_le[2] = 1.0;
        let (vh, _) = tail_volume_coefficients(&plane);
        let hstab = &plane.wings[1];
        let x_wing_ac = plane.wings[0].aerodynamic_center(AC_CHORD_FRACTION)[0];
        let l_h = (hstab.aerodynamic_center(AC_CHORD_FRACTION)[0] - x_wing_ac).max(0.0);
        let expected = hstab.reference_area() * l_h / (plane.s_ref * plane.c_ref);
        assert!((vh.expect("horizontal tail volume") - expected).abs() < 1e-12);
        assert!((hstab.reference_area() - hstab.unfolded_area()).abs() > 1e-6);
    }
}
