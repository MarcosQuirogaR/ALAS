// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Propeller slipstream on the wing: actuator-disk model (UAV-PLAN 3.1 item 6).
//!
//! Frame: geometry axes of `alas-geom`, x aft, y right, z up, metres.  A
//! tractor propeller sits ahead of the wing (smaller x) with its thrust along
//! -x; the slipstream flows along +x.
//!
//! # Model
//!
//! Momentum theory for an ideal disk of radius `R` (area `A = pi R^2`)
//! producing thrust `T` in axial inflow `V` and density `rho`:
//!
//! ```text
//! v_i^2 + V v_i = T / (2 rho A)        axial induced velocity at the disk
//! V_far = V + 2 v_i = V sqrt(1 + 2 T/(rho A V^2))
//! ```
//!
//! At axial distance `s` behind the disk the mean slipstream increment is
//! `v(s) = v_i (1 + s / sqrt(R^2 + s^2))` (Koning/Glauert development; formula
//! from memory of the classical result, NOT read on a page here), going from
//! `v_i` at the disk to `2 v_i` far downstream, and continuity gives the
//! contracted radius `R_s = R sqrt((V + v_i) / (V + v(s)))`.
//!
//! A wing strip of width `w` centred at `(y, z)` and quarter-chord station
//! `x` is covered over the length where the contracted circle of radius `R_s`
//! centred at `(y_d, z_d)` overlaps it; the covered fraction `f` carries the
//! slipstream speed `V_s = V + v(s)` and the rest the freestream.  Per strip:
//!
//! * dynamic-pressure ratio `q/q_inf = (1 - f) + f (V_s/V)^2`,
//! * flow-angle reduction (axial speed up, vertical component unchanged):
//!   `alpha_loc = atan(tan(alpha) V/V_s)` over the covered part.
//!
//! Not modelled: swirl and the tangential velocity (second order for the
//! lift of a 3-D wing, but it shifts the lift between the up-going and
//! down-going blade sides), p-factor, disk inclination to the freestream,
//! radial thrust distribution (uniform disk), slipstream turbulence and the
//! consequent transition changes, pusher configurations (wing upstream of the
//! disk: no effect applied).  STATUS: implemented and verified against the
//! closed forms above; NOT validated against a slipstream experiment (the plan
//! cites an Xtra2UPV MRF-CFD report and AeroUD numbers, which are
//! single-team data and not independent).

use crate::power::{OperatingPoint as PowerPoint, Powertrain};

use super::air::AirState;
use super::AeroError;

/// One propeller disk.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropDisc {
    /// Disk centre, geometry axes, m.
    pub center_m: [f64; 3],
    /// Disk radius, m.
    pub radius_m: f64,
    /// Thrust of this propeller, N (>= 0).
    pub thrust_n: f64,
}

/// Slipstream state at one wing strip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StripFlow {
    /// Fraction of the strip width inside the slipstream, `[0, 1]`.
    pub coverage: f64,
    /// `q_strip / q_inf`, `>= 1`.
    pub q_ratio: f64,
    /// Flow-angle reduction, rad (>= 0): `alpha - alpha_loc` averaged over the
    /// strip, for the freestream angle of attack passed in.
    pub d_alpha_rad: f64,
}

impl StripFlow {
    /// No slipstream.
    pub const NONE: StripFlow = StripFlow {
        coverage: 0.0,
        q_ratio: 1.0,
        d_alpha_rad: 0.0,
    };

    /// Local speed ratio `sqrt(q_ratio)` (used for the strip Reynolds number).
    pub fn speed_ratio(&self) -> f64 {
        self.q_ratio.sqrt()
    }
}

impl PropDisc {
    /// A disk with checked fields.
    ///
    /// # Errors
    ///
    /// [`AeroError::InvalidInput`] for a non-positive radius, negative thrust
    /// or non-finite fields.
    pub fn new(center_m: [f64; 3], radius_m: f64, thrust_n: f64) -> Result<Self, AeroError> {
        let ok = center_m.iter().all(|c| c.is_finite())
            && radius_m.is_finite()
            && radius_m > 0.0
            && thrust_n.is_finite()
            && thrust_n >= 0.0;
        if !ok {
            return Err(AeroError::InvalidInput(
                "propeller disk needs finite centre, radius > 0 and thrust >= 0".into(),
            ));
        }
        Ok(Self {
            center_m,
            radius_m,
            thrust_n,
        })
    }

    /// One propeller of a solved powertrain: thrust per propeller is the
    /// operating point's total thrust over `n_motors`, the radius is half the
    /// propeller diameter (lane B public API: `Powertrain::prop.diameter_m()`,
    /// `OperatingPoint::thrust_n`).
    ///
    /// # Errors
    ///
    /// As [`Self::new`].
    pub fn from_powertrain(
        powertrain: &Powertrain,
        point: &PowerPoint,
        center_m: [f64; 3],
    ) -> Result<Self, AeroError> {
        let n = f64::from(powertrain.n_motors.max(1));
        Self::new(
            center_m,
            0.5 * powertrain.prop.diameter_m(),
            point.thrust_n.max(0.0) / n,
        )
    }

    /// Disk area, m2.
    pub fn area_m2(&self) -> f64 {
        std::f64::consts::PI * self.radius_m * self.radius_m
    }

    /// Disk loading `T/A`, N/m2.
    pub fn disk_loading_n_m2(&self) -> f64 {
        self.thrust_n / self.area_m2()
    }

    /// Axial induced velocity at the disk for inflow `v_m_s >= 0`, m/s.
    pub fn induced_velocity(&self, air: &AirState) -> f64 {
        let v = air.v_m_s;
        let k = self.thrust_n / (2.0 * air.rho_kg_m3 * self.area_m2());
        -0.5 * v + (0.25 * v * v + k).sqrt()
    }

    /// Far-wake speed `V + 2 v_i`, m/s.
    pub fn far_wake_speed(&self, air: &AirState) -> f64 {
        air.v_m_s + 2.0 * self.induced_velocity(air)
    }

    /// Mean slipstream speed increment `v(s)` at distance `s >= 0` behind the
    /// disk, m/s.
    pub fn increment_at(&self, air: &AirState, s_m: f64) -> f64 {
        let s = s_m.max(0.0);
        self.induced_velocity(air) * (1.0 + s / (self.radius_m * self.radius_m + s * s).sqrt())
    }

    /// Contracted slipstream radius at distance `s >= 0`, m.
    pub fn radius_at(&self, air: &AirState, s_m: f64) -> f64 {
        let vi = self.induced_velocity(air);
        let v = air.v_m_s;
        let vs = self.increment_at(air, s_m);
        self.radius_m * ((v + vi) / (v + vs)).sqrt()
    }
}

/// All propeller disks of an aircraft acting on one lifting surface.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Slipstream {
    /// Disks (each independent; overlapping coverage adds speed increments).
    pub discs: Vec<PropDisc>,
}

impl Slipstream {
    /// Flow at the strip centred at `(y, z)` with width `w` and quarter-chord
    /// station `x` (all geometry axes, m), freestream angle of attack
    /// `alpha_rad` relative to the thrust axis.
    pub fn strip_flow(
        &self,
        air: &AirState,
        x_m: f64,
        y_m: f64,
        z_m: f64,
        width_m: f64,
        alpha_rad: f64,
    ) -> StripFlow {
        if width_m.is_nan() || width_m <= 0.0 || self.discs.is_empty() {
            return StripFlow::NONE;
        }
        let v = air.v_m_s;
        let mut coverage_total = 0.0_f64;
        let mut speed_gain = 0.0_f64;
        for disc in &self.discs {
            let s = x_m - disc.center_m[0];
            if s < 0.0 || disc.thrust_n <= 0.0 {
                continue;
            }
            let radius = disc.radius_at(air, s);
            let dz = z_m - disc.center_m[2];
            if dz.abs() >= radius {
                continue;
            }
            let half = (radius * radius - dz * dz).sqrt();
            let lo = (y_m - 0.5 * width_m).max(disc.center_m[1] - half);
            let hi = (y_m + 0.5 * width_m).min(disc.center_m[1] + half);
            let f = ((hi - lo) / width_m).max(0.0);
            if f > 0.0 {
                coverage_total += f;
                speed_gain += f * disc.increment_at(air, s);
            }
        }
        if coverage_total <= 0.0 {
            return StripFlow::NONE;
        }
        // Mean covered-part speed ratio; coverage of several disks is capped
        // at the full strip.
        let coverage = coverage_total.min(1.0);
        let v_slip = v + speed_gain / coverage_total;
        let q_ratio = (1.0 - coverage) + coverage * (v_slip / v).powi(2);
        let alpha_loc = (alpha_rad.tan() * v / v_slip).atan();
        StripFlow {
            coverage,
            q_ratio,
            d_alpha_rad: coverage * (alpha_rad - alpha_loc),
        }
    }
}
