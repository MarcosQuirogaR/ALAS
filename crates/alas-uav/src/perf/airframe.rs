// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Point-mass airframe, drag polar and environment.

use crate::aero::ground_effect::{induced_drag_factor, GroundModel};

use super::PerfError;

/// Standard gravity, m/s^2 (CGPM 1901).
pub const G0: f64 = 9.806_65;

/// Drag polar entering the mission model. Implemented by the aerodynamic
/// lane; [`ParabolicPolar`] is the built-in closed form.
pub trait DragPolar {
    /// Profile and induced drag coefficients at lift coefficient `cl`
    /// (free air, no ground effect).
    fn cd_parts(&self, cl: f64) -> (f64, f64);
    /// Maximum usable lift coefficient (clean or with the flap setting flown).
    fn cl_max(&self) -> f64;
}

/// `CD = CD0 + k CL^2` with `k = 1 / (pi e AR)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParabolicPolar {
    /// Zero-lift drag coefficient (whole aircraft, reference area S).
    pub cd0: f64,
    /// Induced-drag factor `k`.
    pub k: f64,
    /// Maximum lift coefficient.
    pub cl_max: f64,
}

impl ParabolicPolar {
    /// Build from aspect ratio and Oswald factor.
    pub fn from_aspect_ratio(cd0: f64, aspect_ratio: f64, oswald: f64, cl_max: f64) -> Self {
        Self {
            cd0,
            k: 1.0 / (std::f64::consts::PI * oswald * aspect_ratio),
            cl_max,
        }
    }
}

impl DragPolar for ParabolicPolar {
    fn cd_parts(&self, cl: f64) -> (f64, f64) {
        (self.cd0, self.k * cl * cl)
    }
    fn cl_max(&self) -> f64 {
        self.cl_max
    }
}

/// Air and wind. A positive `headwind_m_s` opposes the direction of the
/// straight being flown (alternating legs see +w and -w).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Environment {
    /// Air density, kg/m^3.
    pub rho: f64,
    /// Headwind component on the upwind leg and the runway, m/s.
    pub headwind_m_s: f64,
}

impl Environment {
    /// ISA density at a density altitude, with a headwind.
    pub fn at_density_altitude(altitude_m: f64, headwind_m_s: f64) -> Result<Self, PerfError> {
        let atm = alas_atmo::Atmosphere::try_isa(altitude_m)
            .map_err(|e| PerfError::InvalidInput(format!("density altitude: {e}")))?;
        Self::new(atm.density(), headwind_m_s)
    }

    /// Explicit density.
    pub fn new(rho: f64, headwind_m_s: f64) -> Result<Self, PerfError> {
        if !(rho.is_finite() && rho > 0.2 && rho < 1.6 && headwind_m_s.is_finite()) {
            return Err(PerfError::InvalidInput(
                "density must be in (0.2, 1.6) kg/m^3 and the wind finite".into(),
            ));
        }
        Ok(Self { rho, headwind_m_s })
    }
}

/// The aircraft as the mission model sees it.
pub struct Airframe {
    /// Take-off mass, kg (constant over the mission).
    pub mass_kg: f64,
    /// Payload mass included in `mass_kg`, kg.
    pub payload_kg: f64,
    /// Reference wing area, m^2.
    pub wing_area_m2: f64,
    /// Wing span, m.
    pub span_m: f64,
    /// Drag polar.
    pub polar: Box<dyn DragPolar + Send + Sync>,
    /// Lift coefficient on the ground roll (set by the gear attitude).
    pub cl_ground: f64,
    /// Extra drag area of the landing gear on the ground, m^2 (counts as
    /// CD = this / S added on the ground roll only).
    pub gear_drag_area_m2: f64,
    /// Wing height above ground on the roll, m (ground-effect factor).
    pub wing_height_m: f64,
    /// Rolling friction coefficient (0.03 to 0.07 reported on grass; 0.05 and
    /// 0.07 move a 52 m roll to 57 m in the WUT ACC 2026 report).
    pub mu_roll: f64,
    /// Braking friction coefficient on the landing roll.
    pub mu_brake: f64,
    /// Load-factor limit of the structure (turns), -.
    pub n_max: f64,
}

impl Airframe {
    /// Weight, N.
    pub fn weight(&self) -> f64 {
        self.mass_kg * G0
    }

    /// Reject non-physical parameter sets.
    pub fn validate(&self) -> Result<(), PerfError> {
        let ok = [
            self.mass_kg,
            self.wing_area_m2,
            self.span_m,
            self.polar.cl_max(),
            self.n_max,
        ]
        .iter()
        .all(|v| v.is_finite() && *v > 0.0)
            && self.payload_kg >= 0.0
            && self.payload_kg < self.mass_kg
            && self.cl_ground.is_finite()
            && self.cl_ground >= 0.0
            && self.cl_ground < self.polar.cl_max()
            && self.gear_drag_area_m2 >= 0.0
            && self.wing_height_m >= 0.0
            && (0.0..0.5).contains(&self.mu_roll)
            && (0.0..1.0).contains(&self.mu_brake)
            && self.n_max >= 1.0;
        if ok {
            Ok(())
        } else {
            Err(PerfError::InvalidInput(
                "airframe parameters out of range".into(),
            ))
        }
    }

    /// 1 g stall speed, m/s.
    pub fn stall_speed(&self, rho: f64) -> f64 {
        (2.0 * self.weight() / (rho * self.wing_area_m2 * self.polar.cl_max())).sqrt()
    }

    /// Dynamic pressure times area, N per unit coefficient.
    pub fn qs(&self, rho: f64, v: f64) -> f64 {
        0.5 * rho * v * v * self.wing_area_m2
    }

    /// Total drag, N, at lift coefficient `cl`; `induced_factor` multiplies the
    /// induced part (ground effect), `extra_cd` is added to the profile drag.
    pub fn drag(&self, rho: f64, v: f64, cl: f64, induced_factor: f64, extra_cd: f64) -> f64 {
        let (cdp, cdi) = self.polar.cd_parts(cl);
        self.qs(rho, v) * (cdp + extra_cd + induced_factor * cdi)
    }

    /// Ground-effect factor on induced drag at the same `CL`, Raymer Eq. 12.60
    /// `phi = 33 (h/b)^1.5 / (1 + 33 (h/b)^1.5)`: the default model. It
    /// matches an exact image-system VLM within 5 % for `h/b >= 0.15`
    /// (`aero::ground_effect`); the McCormick `(16 h/b)^2` form is 21 % high
    /// at `h/b = 0.1` and is available through
    /// [`Self::ground_effect_factor_with`] only.
    pub fn ground_effect_factor(&self) -> f64 {
        self.ground_effect_factor_with(GroundModel::Raymer)
    }

    /// Ground-effect factor with an explicit model (`h = wing_height_m`).
    pub fn ground_effect_factor_with(&self, model: GroundModel) -> f64 {
        induced_drag_factor(model, self.wing_height_m / self.span_m).unwrap_or(1.0)
    }

    /// Lift coefficient required for lift `lift_n` at speed `v`, with a stall
    /// check against `CLmax`.
    pub fn cl_required(
        &self,
        rho: f64,
        v: f64,
        lift_n: f64,
        context: &str,
    ) -> Result<f64, PerfError> {
        let cl = lift_n / self.qs(rho, v).max(1e-9);
        if cl > self.polar.cl_max() * 1.000_001 {
            Err(PerfError::Stall {
                cl,
                cl_max: self.polar.cl_max(),
                context: context.to_string(),
            })
        } else {
            Ok(cl)
        }
    }

    /// Speed of best glide ratio at 1 g, m/s (maximum `CL/CD` over `0 < CL <= CLmax`).
    pub fn best_glide_speed(&self, rho: f64) -> f64 {
        let cl_max = self.polar.cl_max();
        let mut best = (0.0_f64, cl_max);
        for k in 1..=400 {
            let cl = cl_max * f64::from(k) / 400.0;
            let (p, i) = self.polar.cd_parts(cl);
            let ratio = cl / (p + i);
            if ratio > best.0 {
                best = (ratio, cl);
            }
        }
        (2.0 * self.weight() / (rho * self.wing_area_m2 * best.1)).sqrt()
    }
}
