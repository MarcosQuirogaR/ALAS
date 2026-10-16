// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Drag increment of the airframe parts that sit in the propeller wash.
//!
//! Parts behind a tractor propeller see the faster slipstream, so their
//! profile drag `CdA` is evaluated at the slipstream dynamic pressure instead
//! of the free-stream one.  The increment per solved operating point is
//!
//! ```text
//! dD = 1/2 rho (V_s^2 - V^2) (n R-wash chord Cd_wing 2 R_s + CdA_other)
//! ```
//!
//! with `V_s = V + v(s)` the mean slipstream speed at the wing station from
//! [`crate::aero::slipstream::PropDisc`] (momentum theory at the disc, the
//! Koning/Glauert development formula flagged there as NOT read on a page),
//! `R_s` the contracted wash radius, `Cd_wing` the wing profile `CD0` (wing
//! area basis), `chord` the wing chord in the wash, and `CdA_other` the drag
//! area of itemised parts flagged as washed (nacelles, fuselage nose).  It is
//! subtracted from the thrust the powertrain adapter reports (installed
//! thrust), which is the usual bookkeeping for thrust losses.
//!
//! Not modelled: the lift the wash adds to the wing (it raises the wing's
//! lift and induced drag; here the wing polar stays that of free air), swirl,
//! the change of boundary-layer transition in the wash, and the tail in the
//! wash.  STATUS: verified against the closed forms (tests); NOT validated;
//! the Xtra2UPV MRF-CFD report quotes a larger CD0 rise (0.0195 -> 0.0337)
//! that includes nacelle and fuselage interaction, so this term is a lower
//! bound.  Magnitude on a twin 11-in aircraft: about 0.3 N at 15 m/s with
//! 10 N total thrust (3 % of thrust).

use crate::aero::air::AirState;
use crate::aero::slipstream::PropDisc;

/// Position of a tractor motor relative to the wing, geometry axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotorStation {
    /// Lateral position of the shaft, m (`y` right).
    pub y_m: f64,
    /// Distance from the propeller plane to the wing quarter-chord line
    /// downstream, m (> 0).
    pub s_to_wing_m: f64,
}

/// The wash-drag model of one aircraft.
#[derive(Debug, Clone, PartialEq)]
pub struct SlipstreamDrag {
    /// Propeller radius, m.
    pub radius_m: f64,
    /// Motor stations (one propeller each).
    pub motors: Vec<MotorStation>,
    /// Wing profile drag coefficient (wing alone, wing-area basis).
    pub wing_cd0: f64,
    /// Mean wing chord in the wash, m.
    pub chord_m: f64,
    /// Drag area of other washed parts, m2.
    pub other_cda_m2: f64,
    /// Ambient temperature, K.
    pub t_k: f64,
}

impl SlipstreamDrag {
    /// Drag increment, N, for total thrust `thrust_n` at airspeed `v` and
    /// density `rho`.  Zero for no thrust or no motors.
    pub fn drag_n(&self, thrust_n: f64, v: f64, rho: f64) -> f64 {
        let n = self.motors.len();
        if n == 0 || !(thrust_n.is_finite() && thrust_n > 0.0) {
            return 0.0;
        }
        let per = thrust_n / n as f64;
        let Ok(air) = AirState::from_rho_t(v.max(0.05), rho, self.t_k) else {
            return 0.0;
        };
        let mut total = 0.0;
        for m in &self.motors {
            let Ok(disc) = PropDisc::new([0.0; 3], self.radius_m, per) else {
                return 0.0;
            };
            let s = m.s_to_wing_m.max(0.0);
            let v_s = air.v_m_s + disc.increment_at(&air, s);
            let r_s = disc.radius_at(&air, s);
            let cda = self.wing_cd0 * self.chord_m * 2.0 * r_s + self.other_cda_m2 / n as f64;
            total += 0.5 * rho * (v_s * v_s - air.v_m_s * air.v_m_s) * cda;
        }
        total
    }
}
