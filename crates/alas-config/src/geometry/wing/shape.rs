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
//! # Who reads which shape
//!
//! - Flight shape: every `AircraftBuilder::build` caller, so the overlays
//!   against the manufacturer drawings, the aerodynamic lattice, the dihedral
//!   effect, the layout dihedral check, the spar box and the mass stations. A
//!   preset fits its front-view drawing here; the drawing is an unloaded shape,
//!   so the drawn loft stands for both (a true 1 g deflection is not
//!   lofted).
//! - Ground shape: the tip height of the airport-planning clearance tables, the
//!   line nacelles hang from under `nacelles_hold_ground_height`, and the
//!   clearance residuals and FLOPS main-gear oleo length (equation 66, whose
//!   dihedral term is a ground-roll clearance), which take the linear rise back
//!   off the built wing.
//!
//! The rise between the two is therefore a physical statement about one wing,
//! not the difference of two fits: it is zero on the centreline, where the wing
//! box sits in the body, and grows outboard. A ground line that ends up flat
//! inboard or below the body-mounted root says the fit is wrong, not that the
//! wing bends that way.
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
use super::smooth::{ProfileSpline, WingProfileInterpolation};

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
#[derive(Debug, Clone, PartialEq)]
pub struct WingHeights {
    /// Centreline root leading-edge height.
    pub root_z_m: f64,
    /// Kink (break) leading-edge height.
    pub break_z_m: f64,
    /// Tip leading-edge height.
    pub tip_z_m: f64,
    /// Interior dihedral stations as `(span fraction, height)`, ascending;
    /// empty for the plain root/kink/tip polyline.
    pub interior: Vec<(f64, f64)>,
    /// How the heights are interpolated between the nodes.
    pub interpolation: WingProfileInterpolation,
}

impl WingHeights {
    /// Natural cubic spline through the root, interior, kink and tip heights
    /// (every node exact, C2 between them), as a function of span fraction.
    pub fn profile_spline(&self, planform: &TransportPlanform) -> ProfileSpline {
        let mut nodes = vec![
            (0.0, self.root_z_m),
            (planform.kink.span_fraction, self.break_z_m),
            (1.0, self.tip_z_m),
        ];
        nodes.extend(self.interior.iter().copied());
        ProfileSpline::natural(&nodes).unwrap_or_else(|| ProfileSpline::constant(0.0))
    }

    /// Height on the root, interior stations, kink and tip polyline at
    /// span fraction `eta`.
    fn at_polyline(&self, planform: &TransportPlanform, eta: f64) -> f64 {
        let mut nodes = vec![
            (0.0, self.root_z_m),
            (planform.kink.span_fraction, self.break_z_m),
            (1.0, self.tip_z_m),
        ];
        nodes.extend(self.interior.iter().copied());
        nodes.sort_by(|a, b| a.0.total_cmp(&b.0));
        for pair in nodes.windows(2) {
            if eta <= pair[1].0 {
                let t = (eta - pair[0].0) / (pair[1].0 - pair[0].0);
                return pair[0].1 + t * (pair[1].1 - pair[0].1);
            }
        }
        self.tip_z_m
    }

    /// Leading-edge height at `y_m` from the centreline on the root, kink
    /// and tip polyline of `planform`, either semispan. The `1e-9`
    /// denominators guard a zero-length root- or tip-side interval.
    pub fn at(&self, planform: &TransportPlanform, y_m: f64) -> f64 {
        let y_abs = y_m.abs();
        if self.interpolation == WingProfileInterpolation::C2Spline {
            return self.profile_spline(planform).eval(y_abs / planform.tip.y_m);
        }
        let y_break = planform.kink.y_m;
        let semi_span = planform.tip.y_m;
        if !self.interior.is_empty() {
            return self.at_polyline(planform, y_abs / semi_span);
        }
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
    /// Dihedral stations strictly inside the span that are not on the kink.
    fn interior_stations(
        &self,
        kink_span_fraction: f64,
    ) -> impl Iterator<Item = &super::profile::WingDihedralStation> {
        self.dihedral_stations.iter().filter(move |station| {
            (station.span_fraction - kink_span_fraction).abs()
                > super::profile::STATION_SPAN_TOLERANCE
        })
    }

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
            interior: self
                .interior_stations(planform.kink.span_fraction)
                .map(|station| (station.span_fraction, station.z_m))
                .collect(),
            interpolation: self.wing_profile_interpolation,
        };
        if shape == WingShape::Ground {
            return Ok(ground);
        }
        let tip_rise_m = self.flight_tip_rise_m(planform.tip.y_m)?;
        let interior = ground
            .interior
            .iter()
            .zip(self.interior_stations(planform.kink.span_fraction))
            .map(|(&(fraction, z_m), station)| {
                (
                    fraction,
                    z_m + station.flight_rise_m.unwrap_or(tip_rise_m * fraction),
                )
            })
            .collect();
        let kink_rise_m = self
            .dihedral_stations
            .iter()
            .find(|station| {
                (station.span_fraction - planform.kink.span_fraction).abs()
                    <= super::profile::STATION_SPAN_TOLERANCE
            })
            .and_then(|station| station.flight_rise_m)
            .unwrap_or_else(|| self.flight_rise_between_stations(planform, tip_rise_m));
        Ok(WingHeights {
            root_z_m: ground.root_z_m + self.flight_root_rise_m.unwrap_or(0.0),
            break_z_m: ground.break_z_m + kink_rise_m,
            tip_z_m: ground.tip_z_m + tip_rise_m,
            interior,
            interpolation: self.wing_profile_interpolation,
        })
    }

    /// Flight rise of a kink no station names: linear between the rises either
    /// side of it, so that the kink stays on the declared flight shape. Rises left
    /// undeclared follow the tip rise in proportion to the span, which is then also
    /// the answer here. The proportional rule alone, at a kink between declared
    /// rises, would leave the kink below its neighbours and fold the wing there.
    fn flight_rise_between_stations(&self, planform: &TransportPlanform, tip_rise_m: f64) -> f64 {
        let kink = planform.kink.span_fraction;
        let rises = std::iter::once((0.0, self.flight_root_rise_m.unwrap_or(0.0)))
            .chain(self.interior_stations(kink).map(|station| {
                (
                    station.span_fraction,
                    station
                        .flight_rise_m
                        .unwrap_or(tip_rise_m * station.span_fraction),
                )
            }))
            .chain(std::iter::once((1.0, tip_rise_m)));
        let (mut inboard, mut outboard) = ((0.0, 0.0), (1.0, tip_rise_m));
        for (fraction, rise_m) in rises {
            if fraction < kink {
                inboard = (fraction, rise_m);
            } else {
                outboard = (fraction, rise_m);
                break;
            }
        }
        inboard.1 + (kink - inboard.0) / (outboard.0 - inboard.0) * (outboard.1 - inboard.1)
    }
}
