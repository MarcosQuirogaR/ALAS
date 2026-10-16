// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Longitudinal trim, elevator authority and tail sizing to static margin.
//!
//! Trim about the CG (nose-up positive, body axes x aft):
//!
//! ```text
//! Cm_cg = Cm0 + CL_w (x_cg - x_acw)/c - eta (S_h,eff/S) CL_h (x_ach - x_cg)/c = 0
//! CL_w  = CL - eta (S_h,eff/S) CL_h
//! => CL_h = (Cm0 + CL (x_cg - x_acw)/c) / (eta (S_h,eff/S) (x_ach - x_acw)/c)
//! ```
//!
//! which is Sadraey Eq. 6.29 (`CL_h = (Cm0 + CL (h - h0)) / (eta V_H)`,
//! p. 332) with the arm taken between the two aerodynamic centres.  The tail
//! needs `alpha_t = alpha_w (1 - de/da) + i_t` and an elevator
//! `delta = (CL_h / CLa_t - alpha_t) / (g tau)`, `g = 1` for a conventional
//! tail and `1/cos G` for a V pair (see [`super::tail::VTailMixer`]).
//! Elevator control power is `Cm_delta = -eta (S_h,eff/S) CLa_t g tau
//! (x_ach - x_cg)/c` (Sadraey Eq. 12.51, p. 673, with the arm from the CG).

use super::geometry::{AirframeGeometry, TailRole};
use super::stability::{LongitudinalOptions, NeutralPoint, NeutralPointMethod};
use super::AirframeError;

/// One trim condition.
#[derive(Debug, Clone, PartialEq)]
pub struct TrimCondition {
    /// Name (for example `cruise`, `stall-fwd-cg`).
    pub name: String,
    /// Aircraft lift coefficient `W / (q S)`.
    pub cl: f64,
    /// CG x, m.
    pub x_cg_m: f64,
}

/// Fixed inputs of a trim solve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrimSetup {
    /// Zero-lift pitching moment of wing plus fuselage about the wing AC
    /// (Sadraey Eq. 6.26 for the wing; nose-up positive).
    pub cm0: f64,
    /// Tail incidence relative to the wing zero-lift line, rad (positive nose-up).
    pub tail_incidence_rad: f64,
    /// Control effectiveness `tau` of the elevator (Sadraey Fig. 12.12).
    pub tau: f64,
    /// Maximum deflection trailing-edge down, rad.
    pub max_down_rad: f64,
    /// Maximum deflection trailing-edge up (a positive number), rad.
    pub max_up_rad: f64,
}

/// Result of a tailed trim.
#[derive(Debug, Clone, PartialEq)]
pub struct TrimResult {
    /// Condition name.
    pub name: String,
    /// Required tail lift coefficient on the effective tail area.
    pub cl_tail: f64,
    /// Wing angle of attack from zero lift, rad.
    pub alpha_wing_rad: f64,
    /// Tail angle of attack including downwash and incidence, rad.
    pub alpha_tail_rad: f64,
    /// Elevator (or pitch-ruddervator) deflection, rad, trailing-edge down positive.
    pub delta_rad: f64,
    /// Control power `Cm_delta`, 1/rad.
    pub cm_delta_per_rad: f64,
    /// Fraction of the available deflection used (1 = at the stop).
    pub authority_used: f64,
    /// Whether the deflection is inside the stops.
    pub within_stops: bool,
}

fn pitch_gain(g: &AirframeGeometry) -> f64 {
    match g.horizontal.as_ref() {
        Some(h) if h.role == TailRole::VPair => 1.0 / h.v_dihedral_rad.cos(),
        _ => 1.0,
    }
}

/// Trim an aircraft with a horizontal tail or V pair.
///
/// # Errors
///
/// No tail, or a degenerate geometry (tail AC not aft of the wing AC).
pub fn trim_tailed(
    np: &NeutralPoint,
    geometry: &AirframeGeometry,
    options: &LongitudinalOptions,
    setup: &TrimSetup,
    condition: &TrimCondition,
) -> Result<TrimResult, AirframeError> {
    let tail = geometry
        .horizontal
        .as_ref()
        .ok_or_else(|| AirframeError::Invalid("trim_tailed needs a horizontal tail".into()))?;
    let c = np.mac_m;
    let a = options.tail_dynamic_pressure_ratio * tail.effective_horizontal_area_m2() / np.s_ref_m2;
    let arm_ac = (np.x_ac_tail_m - np.x_ac_wing_m) / c;
    if !(a > 0.0 && arm_ac > 0.0) {
        return Err(AirframeError::Invalid(
            "tail must be aft of the wing aerodynamic centre".into(),
        ));
    }
    let d_w = (condition.x_cg_m - np.x_ac_wing_m) / c;
    let cl_h = (setup.cm0 + condition.cl * d_w) / (a * arm_ac);
    let cl_w = condition.cl - a * cl_h;
    let alpha_w = cl_w / np.cl_alpha_wing;
    let alpha_t = alpha_w * (1.0 - np.downwash_gradient) + setup.tail_incidence_rad;
    let gain = pitch_gain(geometry);
    let delta = (cl_h / np.cl_alpha_tail - alpha_t) / (gain * setup.tau);
    let cm_delta =
        -a * np.cl_alpha_tail * gain * setup.tau * (np.x_ac_tail_m - condition.x_cg_m) / c;
    let limit = if delta >= 0.0 {
        setup.max_down_rad
    } else {
        setup.max_up_rad
    };
    Ok(TrimResult {
        name: condition.name.clone(),
        cl_tail: cl_h,
        alpha_wing_rad: alpha_w,
        alpha_tail_rad: alpha_t,
        delta_rad: delta,
        cm_delta_per_rad: cm_delta,
        authority_used: delta.abs() / limit,
        within_stops: delta.abs() <= limit,
    })
}

/// Elevon trim of a flying wing: the elevons (area `s_cs`, centroid `x_cs`)
/// provide `Cm_delta = -CLa_w tau (S_cs/S) (x_cs - x_cg)/c`; trim solves
/// `Cm0 + CL (x_cg - x_acw)/c + Cm_delta delta = 0`.
///
/// # Errors
///
/// Elevon not aft of the CG (no pitch authority of the right sign).
pub fn trim_flying_wing(
    np: &NeutralPoint,
    setup: &TrimSetup,
    s_cs_m2: f64,
    x_cs_m: f64,
    condition: &TrimCondition,
) -> Result<TrimResult, AirframeError> {
    let c = np.mac_m;
    let cm_delta =
        -np.cl_alpha_wing * setup.tau * (s_cs_m2 / np.s_ref_m2) * (x_cs_m - condition.x_cg_m) / c;
    if !(cm_delta.is_finite() && cm_delta < 0.0) {
        return Err(AirframeError::NoSolution(
            "elevons are not aft of the CG: no nose-down authority".into(),
        ));
    }
    let moment = setup.cm0 + condition.cl * (condition.x_cg_m - np.x_ac_wing_m) / c;
    let delta = -moment / cm_delta;
    let limit = if delta >= 0.0 {
        setup.max_down_rad
    } else {
        setup.max_up_rad
    };
    Ok(TrimResult {
        name: condition.name.clone(),
        cl_tail: 0.0,
        alpha_wing_rad: condition.cl / np.cl_alpha_wing,
        alpha_tail_rad: 0.0,
        delta_rad: delta,
        cm_delta_per_rad: cm_delta,
        authority_used: delta.abs() / limit,
        within_stops: delta.abs() <= limit,
    })
}

/// Requirements for [`size_horizontal_tail`].
#[derive(Debug, Clone)]
pub struct SizingRequirements {
    /// Minimum static margin at the most aft CG (fraction of MAC).
    pub sm_min_aft: f64,
    /// Maximum static margin at the most forward CG (checked, not enforced).
    pub sm_max_fwd: f64,
    /// Most aft CG x, m.
    pub x_cg_aft_m: f64,
    /// Most forward CG x, m.
    pub x_cg_fwd_m: f64,
    /// Trim conditions that must stay inside `authority_fraction` of the stops.
    pub trim_conditions: Vec<TrimCondition>,
    /// Allowed share of the deflection range (0.8 leaves a 20 % reserve).
    pub authority_fraction: f64,
    /// Trim set-up (Cm0, incidence, tau, stops).
    pub setup: TrimSetup,
}

/// Why the final tail area is what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizingDriver {
    /// The static margin at the aft CG.
    Margin,
    /// Elevator authority in a trim condition.
    Authority,
}

/// Result of tail sizing.
#[derive(Debug, Clone, PartialEq)]
pub struct SizingResult {
    /// Sized geometry (tail area scaled, tail AC x unchanged).
    pub geometry: AirframeGeometry,
    /// Area scale for the margin requirement relative to the input tail.
    pub scale_margin: f64,
    /// Area scale for the authority requirement.
    pub scale_authority: f64,
    /// Final area scale (the larger of the two).
    pub scale: f64,
    /// Driver of the final scale.
    pub driver: SizingDriver,
    /// Static margin at the aft CG after sizing.
    pub sm_aft: f64,
    /// Static margin at the forward CG after sizing.
    pub sm_fwd: f64,
    /// Forward margin is within the allowed maximum.
    pub fwd_margin_ok: bool,
    /// Neutral point of the sized geometry.
    pub np: NeutralPoint,
}

fn scaled_geometry(g: &AirframeGeometry, k: f64) -> Result<AirframeGeometry, AirframeError> {
    let mut out = g.clone();
    let h = out
        .horizontal
        .as_mut()
        .ok_or_else(|| AirframeError::Invalid("no horizontal tail to size".into()))?;
    let ac_old = h.planform.aerodynamic_centre_m()[0];
    let scaled = h.planform.scaled_area(k);
    let dx = ac_old - scaled.aerodynamic_centre_m()[0];
    h.planform = scaled.shifted_x(dx);
    Ok(out)
}

fn bisect(
    mut lo: f64,
    mut hi: f64,
    f: impl Fn(f64) -> Result<f64, AirframeError>,
) -> Result<f64, AirframeError> {
    // f(lo) < 0 <= f(hi) expected; monotonic increasing in the scale.
    if f(hi)? < 0.0 {
        return Err(AirframeError::NoSolution(
            "requirement cannot be met with a tail up to 50 times the starting area".into(),
        ));
    }
    if f(lo)? >= 0.0 {
        return Ok(lo);
    }
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if f(mid)? >= 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Ok(hi)
}

/// Size the horizontal tail (or V pair) area: the smallest scale of the
/// starting tail (from volume coefficients) that gives `sm_min_aft` at the
/// aft CG and keeps every trim condition inside the authority fraction.
///
/// # Errors
///
/// No tail, or a requirement that no tail up to 50 times the starting area meets.
pub fn size_horizontal_tail(
    method: &dyn NeutralPointMethod,
    start: &AirframeGeometry,
    options: &LongitudinalOptions,
    req: &SizingRequirements,
) -> Result<SizingResult, AirframeError> {
    let np_of = |k: f64| -> Result<(AirframeGeometry, NeutralPoint), AirframeError> {
        let g = scaled_geometry(start, k)?;
        let np = method.neutral_point(&g, options)?;
        Ok((g, np))
    };
    let scale_margin = bisect(0.02, 50.0, |k| {
        let (_, np) = np_of(k)?;
        Ok(np.static_margin(req.x_cg_aft_m) - req.sm_min_aft)
    })?;
    let authority_slack = |k: f64| -> Result<f64, AirframeError> {
        let (g, np) = np_of(k)?;
        let mut worst = f64::INFINITY;
        for cond in &req.trim_conditions {
            let r = trim_tailed(&np, &g, options, &req.setup, cond)?;
            worst = worst.min(req.authority_fraction - r.authority_used);
        }
        Ok(worst)
    };
    let scale_authority = if req.trim_conditions.is_empty() {
        0.02
    } else {
        bisect(0.02, 50.0, authority_slack)?
    };
    let (scale, driver) = if scale_margin >= scale_authority {
        (scale_margin, SizingDriver::Margin)
    } else {
        (scale_authority, SizingDriver::Authority)
    };
    let (geometry, np) = np_of(scale)?;
    let sm_aft = np.static_margin(req.x_cg_aft_m);
    let sm_fwd = np.static_margin(req.x_cg_fwd_m);
    Ok(SizingResult {
        geometry,
        scale_margin,
        scale_authority,
        scale,
        driver,
        sm_aft,
        sm_fwd,
        fwd_margin_ok: sm_fwd <= req.sm_max_fwd,
        np,
    })
}
