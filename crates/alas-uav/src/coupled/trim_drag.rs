// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Trim drag of a tailed aircraft: the drag of the lift the horizontal tail
//! must carry to balance the pitching moment about the CG.
//!
//! # Model
//!
//! Moments about the CG, nose-up positive, `x` aft (the convention of
//! [`crate::airframe::trim`], Sadraey Eq. 6.29), with `a = eta_h S_h,eff / S`
//! and `d_w = (x_cg - x_ac,w) / c`:
//!
//! ```text
//! CL_h = (Cm0 + CL d_w) / (a (x_ac,h - x_ac,w) / c)          tail lift on S_h,eff
//! CL_w = CL - a CL_h                                          the wing carries the rest
//! ```
//!
//! Induced drag of the pair is the wing term at its trimmed lift plus the
//! tail's own.  Sources: Raymer 6th ed. Sec. 12.6 "Trim drag" text (read: the
//! tail download is countered by extra wing lift, which adds wing induced
//! drag, on top of the tail's own drag due to lift); Roskam Part VI p. 104
//! Eq. 4.83 (read on the page, tail term `(dCL_h)^2 / (pi A_h e_h)` times the
//! area ratio; the page prints the ratio as `S/S_h`, which is dimensionally
//! `S_h/S` for a drag referred to the wing), with `e_h = 0.5` for a
//! fuselage-mounted tail and `0.75` for a T-tail (Roskam VI p. 69, from the
//! extracted page text, not re-read as an image).  Raymer's Eq. 16.34 as
//! quoted by the book notebook was NOT verified on its page (the notebook's
//! page numbers were wrong in this lookup).
//!
//! ```text
//! CDi,trim = k_w (CL_w^2 - CL^2) + a CL_h^2 / (pi A_h e_h)
//! ```
//!
//! `k_w` is the wing's own `CDi / CL^2` from the viscous VLM.  The first term
//! is the extra wing induced drag of a tail download (negative for a tail
//! upload), the second the tail's induced drag; both are what the book calls
//! trim drag due to lift.  Not modelled: the profile drag of the deflected
//! elevator (Roskam Eq. 4.84), the forward rotation of the tail lift by
//! downwash (small, reduces trim drag) and fuselage pitching moment.
//!
//! # Assumptions (inputs, not calibrated)
//!
//! * Static margin of the CG `SM` (default 0.10 MAC; Raymer Ch. 16 quotes 5-10 %
//!   for transports, about 19 % for a Cessna 172, so 10 % is a mid-range
//!   guess for a model aircraft).  The CG itself is not in the spec.
//! * The elevator is deflected to trim at every `CL` (tail incidence fixed): the
//!   result is an upper bound for an aircraft whose incidence was set at
//!   cruise.
//! * `Cm0` of the wing is the chord-squared weighted NeuralFoil section value
//!   at zero lift (Re 2e5); sweep and twist moment shifts are ignored.
//! * Tail dynamic-pressure ratio `eta_h = 0.9` (Raymer).

use crate::aero::section::SectionModel;
use crate::airframe::geometry::{AirframeGeometry, TailShape};
use crate::airframe::stability::{LongitudinalOptions, NeutralPointMethod, SimpleLiftingSurface};
use crate::spec::topology::TailKind;
use crate::spec::UavSpec;

use super::CoupledError;

/// Trim-drag settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrimSettings {
    /// Static margin of the CG, fraction of the wing MAC (positive = stable).
    pub static_margin: f64,
    /// Tail dynamic-pressure ratio `q_h / q`.
    pub eta_h: f64,
    /// Tail Oswald factor; `None` takes Roskam VI p. 69 (0.5 fuselage-mounted,
    /// 0.75 T-tail).
    pub e_tail: Option<f64>,
    /// Planform proportions used only when the empennage is `Driven`.
    pub tail_shape: TailShape,
}

impl Default for TrimSettings {
    fn default() -> Self {
        Self {
            static_margin: 0.10,
            eta_h: 0.9,
            e_tail: None,
            tail_shape: TailShape::default(),
        }
    }
}

/// Trimmed tail lift and the resulting drag increment as functions of `CL`.
#[derive(Debug, Clone, PartialEq)]
pub struct TrimDrag {
    /// Wing zero-lift moment coefficient about the quarter chord (nose-up +).
    pub cm0: f64,
    /// `a = eta_h S_h,eff / S`.
    pub a: f64,
    /// `(x_cg - x_ac,w) / c`.
    pub d_w: f64,
    /// `(x_ac,h - x_ac,w) / c`.
    pub arm_ac: f64,
    /// Tail induced-drag factor `1 / (pi A_h e_h)`.
    pub k_tail: f64,
}

impl TrimDrag {
    /// Build for `spec` with the wing `cm0` (see [`wing_cm0`]); `None` for a
    /// tailless layout.
    ///
    /// # Errors
    ///
    /// [`CoupledError::Spec`] for a non-physical geometry or settings.
    pub fn new(
        spec: &UavSpec,
        settings: &TrimSettings,
        cm0: f64,
    ) -> Result<Option<Self>, CoupledError> {
        if matches!(spec.topology.tail, TailKind::None) {
            return Ok(None);
        }
        let bad = |e: &dyn std::fmt::Display| CoupledError::Spec(format!("trim drag: {e}"));
        let g = AirframeGeometry::from_master(&spec.master, &spec.topology, &settings.tail_shape)
            .map_err(|e| bad(&e))?;
        let Some(h) = g.horizontal.as_ref() else {
            return Ok(None);
        };
        let np = SimpleLiftingSurface
            .neutral_point(&g, &LongitudinalOptions::default())
            .map_err(|e| bad(&e))?;
        let c = np.mac_m;
        let a = settings.eta_h * h.effective_horizontal_area_m2() / np.s_ref_m2;
        let arm_ac = (np.x_ac_tail_m - np.x_ac_wing_m) / c;
        let d_w = (np.x_cg_for_margin_m(settings.static_margin) - np.x_ac_wing_m) / c;
        let t_tail = matches!(spec.topology.tail, TailKind::T);
        let e_h = settings.e_tail.unwrap_or(if t_tail { 0.75 } else { 0.5 });
        let ok = a > 0.0 && arm_ac > 0.0 && e_h > 0.0 && cm0.is_finite() && d_w.is_finite();
        if !ok {
            return Err(CoupledError::Spec(
                "trim drag: tail must be aft of the wing AC with positive area".into(),
            ));
        }
        Ok(Some(Self {
            cm0,
            a,
            d_w,
            arm_ac,
            k_tail: 1.0 / (std::f64::consts::PI * h.planform.aspect_ratio() * e_h),
        }))
    }

    /// Tail lift coefficient (on the effective tail area) that trims lift
    /// coefficient `cl`.
    pub fn cl_tail(&self, cl: f64) -> f64 {
        (self.cm0 + cl * self.d_w) / (self.a * self.arm_ac)
    }

    /// Induced-drag increment, on the wing area, for the wing induced-drag
    /// factor `k_wing` at lift coefficient `cl`.  Never negative: a tail
    /// that carries upload (high `CL`, forward CG) is not credited, since the
    /// wing-alone polar already is the optimum the model can claim.
    pub fn delta_cdi(&self, cl: f64, k_wing: f64) -> f64 {
        let cl_h = self.cl_tail(cl);
        let cl_w = cl - self.a * cl_h;
        (k_wing * (cl_w * cl_w - cl * cl) + self.a * self.k_tail * cl_h * cl_h).max(0.0)
    }
}

/// Chord-squared weighted zero-lift moment coefficient of the wing sections
/// (about the quarter chord, nose-up positive) at Reynolds number `re`.
/// `weights` are the `c^2 dy` weights of the stations.
///
/// # Errors
///
/// [`CoupledError::Aero`] if a section cannot be evaluated.
pub fn wing_cm0(sections: &[SectionModel], weights: &[f64], re: f64) -> Result<f64, CoupledError> {
    let (mut num, mut den) = (0.0, 0.0);
    for (s, w) in sections.iter().zip(weights) {
        let (mut lo, mut hi) = (-14.0_f64.to_radians(), 8.0_f64.to_radians());
        let mut cm = 0.0;
        for _ in 0..30 {
            let mid = 0.5 * (lo + hi);
            let p = s.eval(mid, re, 0.0)?;
            cm = p.cm;
            if p.cl > 0.0 {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        num += cm * w;
        den += w;
    }
    Ok(if den > 0.0 { num / den } else { 0.0 })
}
