// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The two shapes of the main wing: static on the ground, and bent in 1 g
//! flight.
//!
//! The root, break and tip heights of [`WingConfig`] (and the heights of its
//! custom sections) describe the **ground shape**: the wing standing on its
//! gear at maximum ramp weight, the state an airport-planning ground-clearance
//! table measures (nacelle low points, wing-tip height). Lift bends the wing
//! up in flight, so the **flight shape** stands higher towards the tip by the
//! static-to-1 g tip rise `flight_tip_rise_semispan_fraction` times the
//! semispan. A wing that declares no rise has one shape for both roles.
//!
//! The ground shape is the one ground clearance, nacelle strike and the FLOPS
//! main-gear oleo length (equation 66, whose dihedral term is a ground-roll
//! clearance) read. The flight shape is the one the aerodynamic lattice, the
//! dihedral effect and the layout dihedral check read: the geometry builder
//! lofts it by default.
//!
//! # Spanwise distribution
//!
//! Only the tip rise is published, so the rise is spread as one uniform
//! dihedral increment from the centreline, `w(y) = rise y / s` (`s` the
//! semispan), which keeps the ground shape's own crank. A cantilever of
//! uniform curvature from the side of body (`w'' = M/EI` constant) would put
//! almost none of the rise inboard; but on the ground the wing gear props the
//! inner wing against the body weight and reverses its curvature there, and
//! lifting off removes that reaction too, so the inboard panel rises by more
//! than a free cantilever bends. Neither reaction is modelled: the uniform
//! increment is an engineering estimate of the shape, not a solved
//! aeroelastic deflection. The increment is linear in `y`, so the root,
//! kink and tip heights carry it exactly and the lattice, the nacelles and
//! any custom section, all interpolated between them, sit on the flight
//! shape.
//!
//! Frame: `y` metres outboard from the centreline, `z` metres up in the
//! geometry axes; the rise is positive up.

use super::config::WingConfig;
use super::planform::{TransportPlanform, TransportPlanformError};

/// Largest static-to-1 g tip rise, as a fraction of the semispan, the
/// height-only shift above describes: raising the tip without moving it
/// inboard shortens nothing only while the rise is small, and at a fifth of
/// the semispan the deflected panel is already 2 % longer than its span
/// (`sqrt(1 + 0.2^2)`). An engineering bound, twice the A380's published
/// rise of about a tenth of its semispan.
pub const MAX_FLIGHT_TIP_RISE_SEMISPAN_FRACTION: f64 = 0.2;

/// Which of the main wing's two shapes a geometry describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WingShape {
    /// Static on the gear at maximum ramp weight: the configured heights.
    Ground,
    /// Bent by the lift of 1 g flight: the configured heights plus the
    /// static-to-1 g rise.
    Flight,
}

/// Leading-edge heights of the root, kink and tip stations of one wing
/// shape, metres in the geometry axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingHeights {
    /// Centreline root leading-edge height.
    pub root_z_m: f64,
    /// Kink (break) leading-edge height.
    pub break_z_m: f64,
    /// Tip leading-edge height.
    pub tip_z_m: f64,
}

impl WingHeights {
    /// Leading-edge height at `y_m` from the centreline on the root, kink
    /// and tip polyline of `planform`, either semispan. The `1e-9`
    /// denominators guard a zero-length root- or tip-side interval.
    pub fn at(&self, planform: &TransportPlanform, y_m: f64) -> f64 {
        let y_abs = y_m.abs();
        let y_break = planform.kink.y_m;
        let semi_span = planform.tip.y_m;
        if y_abs <= y_break {
            self.root_z_m + (self.break_z_m - self.root_z_m) * (y_abs / (y_break + 1e-9))
        } else {
            self.break_z_m
                + (self.tip_z_m - self.break_z_m)
                    * ((y_abs - y_break) / (semi_span - y_break + 1e-9))
        }
    }
}

impl WingConfig {
    /// Static-to-1 g rise of the tip of a wing of semispan `semi_span_m`,
    /// metres: zero when the wing declares none.
    ///
    /// # Errors
    ///
    /// [`TransportPlanformError::NonFinite`] for a non-finite fraction and
    /// [`TransportPlanformError::FlightTipRiseOutOfRange`] outside
    /// `[0, MAX_FLIGHT_TIP_RISE_SEMISPAN_FRACTION]`: a wing does not sag
    /// under its own lift, and beyond the bound the beam does not apply.
    pub fn flight_tip_rise_m(&self, semi_span_m: f64) -> Result<f64, TransportPlanformError> {
        let Some(fraction) = self.flight_tip_rise_semispan_fraction else {
            return Ok(0.0);
        };
        if !fraction.is_finite() {
            return Err(TransportPlanformError::NonFinite {
                field: "flight tip rise",
                value: fraction,
            });
        }
        if !(0.0..=MAX_FLIGHT_TIP_RISE_SEMISPAN_FRACTION).contains(&fraction) {
            return Err(TransportPlanformError::FlightTipRiseOutOfRange { value: fraction });
        }
        Ok(fraction * semi_span_m)
    }

    /// Root, kink and tip heights of `shape` on `planform`.
    ///
    /// # Errors
    ///
    /// See [`Self::flight_tip_rise_m`].
    pub fn heights(
        &self,
        shape: WingShape,
        planform: &TransportPlanform,
    ) -> Result<WingHeights, TransportPlanformError> {
        let ground = WingHeights {
            root_z_m: self.root_z_m,
            break_z_m: self.break_z_m,
            tip_z_m: self.tip_z_m,
        };
        if shape == WingShape::Ground {
            return Ok(ground);
        }
        let tip_rise_m = self.flight_tip_rise_m(planform.tip.y_m)?;
        Ok(WingHeights {
            break_z_m: ground.break_z_m + tip_rise_m * planform.kink.span_fraction,
            tip_z_m: ground.tip_z_m + tip_rise_m,
            ..ground
        })
    }
}
