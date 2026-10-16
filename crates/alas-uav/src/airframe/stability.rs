// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Neutral point and static margin.
//!
//! The method sits behind [`NeutralPointMethod`] so the VLM-based
//! implementation of the aerodynamics lane can replace the closed-form
//! default without touching callers.  [`SimpleLiftingSurface`] implements
//! Raymer Eq. 16.9 (stick-fixed, power-off):
//!
//! ```text
//!        CLa_w x_acw + eta_h (S_h/S) CLa_h (1 - de/da) x_ach - c Cma_fus
//! x_np = ----------------------------------------------------------------
//!              CLa_w + eta_h (S_h/S) CLa_h (1 - de/da)
//! ```
//!
//! with `x` along body x (aft positive), `c` the wing MAC and `Cma_fus` the
//! destabilising fuselage/nacelle pitching-moment slope (positive reduces
//! stability, Raymer Eq. 16.25; the printed DR-1 example enters it as
//! `-0.12`).  Static margin `SM = (x_np - x_cg) / c` and
//! `Cm_alpha = -CLa_total SM` (Raymer Eq. 16.10-16.11, p. 593).
//!
//! Validity: subsonic, linear lift, power-off; propeller normal force and
//! slipstream are NOT included (Raymer: allow 1-3 % SM for jets and about
//! 2 % per MAC of propeller-ahead-of-CG distance for propeller aircraft).

use super::geometry::{AirframeGeometry, TailRole};
use super::liftcurve::{downwash_gradient, fuselage_lift_factor, lift_curve_slope};
use super::AirframeError;

/// Fuselage and nacelle pitching-moment slope, per radian, positive
/// destabilising, referenced to the wing MAC and area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuselageContribution {
    /// `Cm_alpha` of the fuselage/nacelles, 1/rad (positive = destabilising).
    pub cm_alpha_per_rad: f64,
}

impl FuselageContribution {
    /// No fuselage contribution (ESTIMATE: unconservative for a bulky pod).
    #[must_use]
    pub fn none() -> Self {
        Self {
            cm_alpha_per_rad: 0.0,
        }
    }

    /// Raymer Eq. 16.25 (NACA TR 711): `Cm_a = K_f W_f^2 L_f / (c S)` per
    /// degree, converted to per radian.  `k_f` is read from Raymer Fig. 16.14
    /// as a function of the wing-AC position along the fuselage.
    #[must_use]
    pub fn naca_tr711(k_f: f64, width_m: f64, length_m: f64, mac_m: f64, s_ref_m2: f64) -> Self {
        Self {
            cm_alpha_per_rad: k_f * width_m * width_m * length_m / (mac_m * s_ref_m2)
                * (180.0 / std::f64::consts::PI),
        }
    }
}

/// Options of the longitudinal model.  Defaults are ESTIMATES (flagged).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LongitudinalOptions {
    /// Mach number (0 for low-speed UAVs).
    pub mach: f64,
    /// Airfoil lift-slope efficiency of the wing (Raymer: about 0.95).
    pub wing_airfoil_efficiency: f64,
    /// Same for the tail surfaces.
    pub tail_airfoil_efficiency: f64,
    /// Wing `(S_exposed / S_ref) F` product; `None` computes `F` from the
    /// fuselage diameter over span and takes `S_exposed/S = 1`.
    pub wing_exposed_factor: Option<f64>,
    /// Maximum fuselage diameter over wing span for `F` when
    /// `wing_exposed_factor` is `None` (0 disables the fuselage lift).
    pub fuselage_diameter_over_span: f64,
    /// Chord fraction of the line of maximum thickness for the sweep used in
    /// the lift slope (ESTIMATE 0.35).
    pub max_thickness_fraction: f64,
    /// Tail dynamic-pressure ratio `eta_h = q_h / q` (Raymer DR-1 assumes 0.9).
    pub tail_dynamic_pressure_ratio: f64,
    /// Fuselage and nacelle pitching moment.
    pub fuselage: FuselageContribution,
    /// Override of `d eps / d alpha` (for example from Raymer Fig. 16.12).
    pub downwash_gradient: Option<f64>,
    /// Override of the wing lift-curve slope, 1/rad.
    pub wing_cl_alpha: Option<f64>,
    /// Override of the tail lift-curve slope (on the true tail area), 1/rad.
    pub tail_cl_alpha: Option<f64>,
}

impl Default for LongitudinalOptions {
    fn default() -> Self {
        Self {
            mach: 0.0,
            wing_airfoil_efficiency: 0.95,
            tail_airfoil_efficiency: 0.95,
            wing_exposed_factor: None,
            fuselage_diameter_over_span: 0.0,
            max_thickness_fraction: 0.35,
            tail_dynamic_pressure_ratio: 0.9,
            fuselage: FuselageContribution::none(),
            downwash_gradient: None,
            wing_cl_alpha: None,
            tail_cl_alpha: None,
        }
    }
}

/// Result of a neutral-point evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct NeutralPoint {
    /// Neutral point x, body axes, m.
    pub x_np_m: f64,
    /// Reference (wing MAC) chord, m.
    pub mac_m: f64,
    /// Reference area, m^2.
    pub s_ref_m2: f64,
    /// Wing lift-curve slope on the reference area, 1/rad.
    pub cl_alpha_wing: f64,
    /// Tail lift-curve slope on the true tail area, 1/rad (0 without a tail).
    pub cl_alpha_tail: f64,
    /// Tail term `eta_h (S_h,eff/S) CLa_h (1 - de/da)`, 1/rad.
    pub tail_term: f64,
    /// Downwash gradient used.
    pub downwash_gradient: f64,
    /// Wing aerodynamic centre x, m.
    pub x_ac_wing_m: f64,
    /// Tail aerodynamic centre x, m (wing value without a tail).
    pub x_ac_tail_m: f64,
    /// Name of the method that produced this.
    pub method: &'static str,
}

impl NeutralPoint {
    /// Total lift-curve slope of the aircraft on the reference area, 1/rad.
    #[must_use]
    pub fn cl_alpha_total(&self) -> f64 {
        self.cl_alpha_wing + self.tail_term
    }

    /// Static margin `(x_np - x_cg) / c`, positive when stable.
    #[must_use]
    pub fn static_margin(&self, x_cg_m: f64) -> f64 {
        (self.x_np_m - x_cg_m) / self.mac_m
    }

    /// Pitching-moment slope `Cm_alpha = -CLa_total SM`, 1/rad (negative is stable).
    #[must_use]
    pub fn cm_alpha_per_rad(&self, x_cg_m: f64) -> f64 {
        -self.cl_alpha_total() * self.static_margin(x_cg_m)
    }

    /// CG x for a requested static margin, m.
    #[must_use]
    pub fn x_cg_for_margin_m(&self, margin: f64) -> f64 {
        self.x_np_m - margin * self.mac_m
    }
}

/// A neutral-point estimator.  The VLM-based implementation of the
/// aerodynamics lane implements this trait and replaces the closed form.
pub trait NeutralPointMethod {
    /// Short identifier recorded in results.
    fn name(&self) -> &'static str;

    /// Neutral point of the geometry under the options.
    ///
    /// # Errors
    ///
    /// Non-physical geometry or options.
    fn neutral_point(
        &self,
        geometry: &AirframeGeometry,
        options: &LongitudinalOptions,
    ) -> Result<NeutralPoint, AirframeError>;
}

/// Closed-form lifting-surface estimate (Raymer 12.6, 16.9; Sadraey 6.56).
#[derive(Debug, Clone, Copy, Default)]
pub struct SimpleLiftingSurface;

impl NeutralPointMethod for SimpleLiftingSurface {
    fn name(&self) -> &'static str {
        "simple-lifting-surface"
    }

    fn neutral_point(
        &self,
        g: &AirframeGeometry,
        o: &LongitudinalOptions,
    ) -> Result<NeutralPoint, AirframeError> {
        let wing = &g.wing;
        let s = wing.projected_area_m2();
        let c = wing.mac_m();
        let ar_w = wing.aspect_ratio();
        let x_acw = wing.x_le_mac_m() + 0.25 * c;
        let cl_w = match o.wing_cl_alpha {
            Some(v) => v,
            None => {
                let sweep = wing.sweep_at_chord_fraction_rad(o.max_thickness_fraction);
                let base = lift_curve_slope(ar_w, o.mach, sweep, o.wing_airfoil_efficiency)?;
                let factor = o.wing_exposed_factor.unwrap_or_else(|| {
                    fuselage_lift_factor(o.fuselage_diameter_over_span) / fuselage_lift_factor(0.0)
                });
                base * factor
            }
        };
        let deps = o
            .downwash_gradient
            .unwrap_or_else(|| downwash_gradient(cl_w, ar_w));
        let (cl_t, term, x_act) = match &g.horizontal {
            None => (0.0, 0.0, x_acw),
            Some(h) => {
                let s_eff = h.effective_horizontal_area_m2();
                let cl_t = match o.tail_cl_alpha {
                    Some(v) => v,
                    None => {
                        let sweep = h
                            .planform
                            .sweep_at_chord_fraction_rad(o.max_thickness_fraction);
                        let ar = if h.role == TailRole::VPair {
                            h.planform.true_aspect_ratio()
                        } else {
                            h.planform.aspect_ratio()
                        };
                        lift_curve_slope(ar, o.mach, sweep, o.tail_airfoil_efficiency)?
                    }
                };
                let term = o.tail_dynamic_pressure_ratio * s_eff / s * cl_t * (1.0 - deps);
                (cl_t, term, h.planform.aerodynamic_centre_m()[0])
            }
        };
        let numerator = cl_w * x_acw + term * x_act - c * o.fuselage.cm_alpha_per_rad;
        let x_np = numerator / (cl_w + term);
        if !x_np.is_finite() {
            return Err(AirframeError::Invalid("neutral point is not finite".into()));
        }
        Ok(NeutralPoint {
            x_np_m: x_np,
            mac_m: c,
            s_ref_m2: s,
            cl_alpha_wing: cl_w,
            cl_alpha_tail: cl_t,
            tail_term: term,
            downwash_gradient: deps,
            x_ac_wing_m: x_acw,
            x_ac_tail_m: x_act,
            method: self.name(),
        })
    }
}

/// Static margin at one load case.
#[derive(Debug, Clone, PartialEq)]
pub struct StabilityPoint {
    /// Load case name.
    pub case: String,
    /// Total mass, kg.
    pub mass_kg: f64,
    /// CG x, m.
    pub x_cg_m: f64,
    /// Static margin (fraction of the wing MAC).
    pub static_margin: f64,
    /// `Cm_alpha`, 1/rad.
    pub cm_alpha_per_rad: f64,
}

impl StabilityPoint {
    /// Evaluate `(name, mass, x_cg)` triples against a neutral point.
    #[must_use]
    pub fn evaluate(np: &NeutralPoint, cases: &[(String, f64, f64)]) -> Vec<Self> {
        cases
            .iter()
            .map(|(name, m, x)| Self {
                case: name.clone(),
                mass_kg: *m,
                x_cg_m: *x,
                static_margin: np.static_margin(*x),
                cm_alpha_per_rad: np.cm_alpha_per_rad(*x),
            })
            .collect()
    }
}
