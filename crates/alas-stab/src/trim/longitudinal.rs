// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// The wing this module reads the aerodynamic centre and aspect ratio off,
/// falling back to the first wing when none is named this: upstream's
/// `next((w for w in airplane.wings if w.name == "Main Wing"), airplane.wings[0])`.
pub(crate) const MAIN_WING_NAME: &str = "Main Wing";

/// The trimmable surface [`stability_and_trim`] deflects and
/// [`fuselage_cm_alpha`] reads the tail station off.
pub(crate) const HSTAB_NAME: &str = "Horizontal Stabilizer";

/// `aerodynamic_center()`'s default `chord_fraction`: the quarter-MAC point,
/// which every call site here takes unspecified.
pub(crate) const AC_CHORD_FRACTION: f64 = 0.25;

/// The `abs(dCL) < 1e-9` (and `abs(cl_alpha) < 1e-9`, and `abs(d_alpha) < 1e-9`)
/// degeneracy floor upstream guards every slope division with.
pub(super) const DEGENERACY_FLOOR: f64 = 1e-9;

/// Cruise-condition static margin plus the closed-form longitudinal trim
/// solve: `StabilityTrimResult`. `trim_ih_deg` is NaN and `cl_ih`/`cm_ih`
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
    /// Pitching-moment slope `dCm/dalpha`, per degree, as the same two alpha
    /// probes measured it. Exposing this keeps a failed trim Jacobian
    /// diagnosable at the pipeline boundary instead of collapsing it to a
    /// Boolean `converged` flag.
    pub cm_alpha: f64,
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
/// `L/d`: `munk_apparent_mass_factor`. Clamped to the tabulated range
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
/// slender-body (Munk/Multhopp) method: `fuselage_cm_alpha`. Positive is
/// destabilising (moves the neutral point forward). Integrates the *actual*
/// fuselage cross-section area distribution `A(x) = (pi/4) w(x) h(x)`, with a
/// local-flow factor that reduces the afterbody's contribution by the wing
/// downwash. `cl_alpha` is the wing lift-curve slope, per radian. Zero with no
/// fuselage; NaN with a fuselage but no wing.
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

pub(super) fn fuselage_cm_alpha_with_reference_mode(
    airplane: &Airplane,
    cl_alpha: f64,
    reference_compatibility: bool,
) -> f64 {
    let Some(fus) = airplane.fuselages.first() else {
        return 0.0;
    };

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

    let Some(wing) = main_wing(airplane).filter(|wing| !wing.xsecs.is_empty()) else {
        return f64::NAN;
    };
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

/// Static margin `SM = -dCm/dCL` from two VLM operating points: `static_margin`.
/// The probe velocity (`autobalance_velocity_m_s`, 250 m/s / M~0.73 at sea
/// level by default, not "low speed") only sets a valid dynamic pressure:
/// this VLM route is incompressible and linear, so `CL_alpha` and `Cm_alpha`
/// are functions of geometry and angle of attack alone, and the ratio
/// `-dCm/dCL` this function returns does not depend on which velocity was
/// probed. NaN when the two probes carry the same lift (a degenerate `dCL`).
///
/// # Errors
///
/// See [`VlmError`]: the VLM solve failing to mesh or factorize.
pub fn static_margin(airplane: &Airplane, analysis: &AnalysisConfig) -> Result<f64, VlmError> {
    let atmosphere = Atmosphere::new(0.0);
    let velocity = analysis.autobalance_velocity_m_s;
    let (a_lo, a_hi) = (
        analysis.autobalance_alpha_low_deg,
        analysis.autobalance_alpha_high_deg,
    );
    let system = assemble(airplane, analysis)?;
    let r_lo = probe(&system, atmosphere, velocity, a_lo)?;
    let r_hi = probe(&system, atmosphere, velocity, a_hi)?;
    let d_cm = r_hi.cm_pitch - r_lo.cm_pitch;
    let d_cl = r_hi.cl_lift - r_lo.cl_lift;
    if d_cl.abs() < DEGENERACY_FLOOR {
        return Ok(f64::NAN);
    }
    Ok(-d_cm / d_cl)
}

/// Shift the CG so the aircraft's static margin equals `target_static_margin`:
/// `autobalance`. Mutates `airplane.xyz_ref[0]` in place and returns the
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

/// Physically-anchored neutral point, static margin and lift-curve slope:
/// `neutral_point`, returning `(x_np, static_margin, cl_alpha)`. Probes at a
/// fixed reference condition (`autobalance_velocity_m_s`, 250 m/s at sea level
/// by default). This route is incompressible and linear (no Goethert or
/// Prandtl-Glauert stretch), so the result does not depend on the probe
/// velocity: it is effectively the M = 0 neutral point (physics review v1.2,
/// finding A4). `static_margin` is measured against the real CG
/// (`xyz_ref[0]`). NaN margin and slope on a degenerate probe.
///
/// The *product* path (this function, not [`neutral_point_reference_compatibility`])
/// delegates the wing/tail/fuselage/nacelle physics to
/// [`crate::neutral_point`] (see its module doc for the F1-F3 fixes;
/// [`crate::neutral_point::neutral_point_conditions`] for a Mach-explicit,
/// multi-condition evaluation).
///
/// # Errors
///
/// See [`VlmError`].
pub fn neutral_point(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
) -> Result<(f64, f64, f64), VlmError> {
    let system = assemble(airplane, analysis)?;
    with_system::neutral_point_with_system(&system, airplane, analysis)
}

/// Frozen translation/parity form of [`neutral_point`].
pub fn neutral_point_reference_compatibility(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
) -> Result<(f64, f64, f64), VlmError> {
    let system = assemble(airplane, analysis)?;
    with_system::neutral_point_reference_compatibility_with_system(&system, airplane, analysis)
}

/// Cruise-condition three-point VLM probe: static margin plus a closed-form
/// longitudinal trim solve: `stability_and_trim`. Probes at the actual
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
    let system = assemble(airplane, analysis)?;
    with_system::stability_and_trim_with_system(
        &system, airplane, analysis, cl_target, mach, altitude_m,
    )
}

/// Frozen translation/parity form of [`stability_and_trim`].
pub fn stability_and_trim_reference_compatibility(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    cl_target: f64,
    mach: f64,
    altitude_m: f64,
) -> Result<StabilityTrimResult, VlmError> {
    let system = assemble(airplane, analysis)?;
    with_system::stability_and_trim_reference_compatibility_with_system(
        &system, airplane, analysis, cl_target, mach, altitude_m,
    )
}
