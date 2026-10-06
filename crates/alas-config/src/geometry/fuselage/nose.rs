// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The generated nose stations: the legacy ellipsoid droop and, when any of
//! the `nose_*` fields is set, a transport nose built from four laws over the
//! nose stations, an upper envelope, a lower envelope, a plan half-width and
//! a superellipse exponent.
//!
//! One law shared by the geometry builder and the sandbox handles, so a
//! drawn station and a lofted one cannot disagree.
//!
//! Frame: x aft of the nose tip, z up, metres; `xi` in `[0, 1]` is the
//! fraction of the nose length `cabin_start_x_m`. Section centre is
//! `(top + bottom) / 2`, height `top - bottom`, width twice the plan
//! half-width. With no `nose_*` field set the stations are the legacy law
//! `z = z_c + (z_n - z_c)(1 - xi)^2`, `r = R sqrt(1 - (1 - xi)^2)`.
//!
//! The shaped nose, with `Ln` the nose length, `T` and `B` the cabin crown
//! and keel heights and `z_n` the tip height (`nose_z_m`):
//!
//! - upper envelope: a convex radome arc from the tip to the windshield base
//!   at `xi = radome_length` that leaves the tip with the rounding of the keel
//!   line and meets the windshield with the windshield slope (no kink), a
//!   straight windshield at exactly the requested angle to the waterline,
//!   and a cubic Hermite blend to the cabin crown (zero slope) at
//!   `xi = crown_end`, flat afterwards;
//! - lower envelope: `z_n + (B - z_n) (1 - (1 - xi)^2)^(1 / keel_exponent)`;
//! - plan half-width: `(W / 2) (1 - (1 - xi)^2)^(1 / plan_exponent)`;
//! - section exponent: `2 + (n_ws - 2) sin(pi xi)`, 2 at both ends.
//!
//! The radome arc, with `u = x / x_base`, `q(u) = 1 - (1 - u)^2`, `D` the rise
//! from the tip to the windshield base and `m = min(1, tan(angle) x_base / D)`:
//! `z = z_n + D ((1 - m) q(u)^(1 / keel_exponent) + m u)`. Its end slope is
//! `m D / x_base = tan(angle)` (continuous with the windshield), it is concave
//! down for every `keel_exponent >= 1`, and `m = 1` is the straight line. The
//! base rise is `0.25` of the tip-to-crown rise, raised to `tan(angle) x_base`
//! (at most `0.6` of the rise) when the windshield is steeper than that chord.
//!
//! The defaults used for fields left unset are class ESTIMATES for a
//! narrow-body jet (engineering judgement, not measured from a drawing); no
//! preset sets them. Limits: no double-lobe section, no cockpit eyebrow
//! step, no radome blister, and no independent tip radius (the tip
//! curvature follows the plan and keel exponents). An upper-deck hump is a
//! separate crown rise added on top of these stations (`hump`).

use super::FuselageConfig;

/// Section exponent of a circle or ellipse, the cabin value.
const CIRCULAR_EXPONENT: f64 = 2.0;
/// Share of the rise from the windshield base to the crown the straight
/// windshield may cover before the crown blend.
const WINDSHIELD_RISE_SHARE: f64 = 0.7;
/// Share of the run from the windshield base to the crown end the straight
/// windshield may cover.
const WINDSHIELD_RUN_SHARE: f64 = 0.5;
/// Share of the tip-to-crown rise reached at the windshield base.
const WINDSHIELD_BASE_RISE_SHARE: f64 = 0.25;
/// Largest share of the tip-to-crown rise the radome arc may climb before the
/// windshield base.
const WINDSHIELD_BASE_RISE_MAX_SHARE: f64 = 0.6;

/// One generated nose station.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoseStation {
    /// Station, m aft of the nose tip.
    pub x_m: f64,
    /// Section centre height, m.
    pub z_m: f64,
    /// Full width, m.
    pub width_m: f64,
    /// Full height, m.
    pub height_m: f64,
    /// Superellipse exponent of the section.
    pub shape: f64,
}

/// The resolved nose shape parameters, every one inside its valid range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoseShape {
    /// Side-view angle of the straight windshield to the waterline, degrees.
    pub windshield_angle_deg: f64,
    /// Fraction of the nose length where the crown reaches the cabin crown.
    pub crown_end_fraction: f64,
    /// Fraction of the nose length occupied by the radome, tip to windshield.
    pub radome_length_fraction: f64,
    /// Exponent of the lower line; larger gives a deeper chin.
    pub keel_exponent: f64,
    /// Exponent of the plan-view half-width; smaller gives a pointier plan.
    pub plan_exponent: f64,
    /// Peak superellipse exponent of the sections (2 = elliptical).
    pub section_exponent: f64,
}

impl NoseShape {
    /// Narrow-body class ESTIMATES used for fields left unset (not measured
    /// from a drawing).
    pub const NARROWBODY_ESTIMATE: Self = Self {
        windshield_angle_deg: 32.0,
        crown_end_fraction: 0.80,
        radome_length_fraction: 0.30,
        keel_exponent: 2.2,
        plan_exponent: 2.0,
        section_exponent: 2.4,
    };

    /// Valid range of the windshield angle, degrees.
    pub const WINDSHIELD_ANGLE_DEG: (f64, f64) = (15.0, 60.0);
    /// Valid range of the crown end fraction.
    pub const CROWN_END_FRACTION: (f64, f64) = (0.55, 1.0);
    /// Valid range of the radome length fraction.
    pub const RADOME_LENGTH_FRACTION: (f64, f64) = (0.10, 0.45);
    /// Valid range of the keel exponent.
    pub const KEEL_EXPONENT: (f64, f64) = (1.3, 4.0);
    /// Valid range of the plan exponent.
    pub const PLAN_EXPONENT: (f64, f64) = (1.0, 2.6);
    /// Valid range of the section exponent.
    pub const SECTION_EXPONENT: (f64, f64) = (2.0, 3.5);
}

/// `value` clamped into `range`, or `fallback` when unset or not finite.
fn resolve(value: Option<f64>, range: (f64, f64), fallback: f64) -> f64 {
    value
        .filter(|v| v.is_finite())
        .map_or(fallback, |v| v.clamp(range.0, range.1))
}

impl FuselageConfig {
    /// The shaped nose in effect: `None` (legacy ellipsoid law) when no
    /// `nose_*` field is set, else every field resolved, unset or non-finite
    /// ones from [`NoseShape::NARROWBODY_ESTIMATE`] and the rest clamped to
    /// their valid range.
    #[must_use]
    pub fn nose_shape(&self) -> Option<NoseShape> {
        let fields = [
            self.nose_windshield_angle_deg,
            self.nose_crown_end_fraction,
            self.nose_radome_length_fraction,
            self.nose_keel_exponent,
            self.nose_plan_exponent,
            self.nose_section_exponent,
        ];
        if fields.iter().all(Option::is_none) {
            return None;
        }
        let d = NoseShape::NARROWBODY_ESTIMATE;
        Some(NoseShape {
            windshield_angle_deg: resolve(
                self.nose_windshield_angle_deg,
                NoseShape::WINDSHIELD_ANGLE_DEG,
                d.windshield_angle_deg,
            ),
            crown_end_fraction: resolve(
                self.nose_crown_end_fraction,
                NoseShape::CROWN_END_FRACTION,
                d.crown_end_fraction,
            ),
            radome_length_fraction: resolve(
                self.nose_radome_length_fraction,
                NoseShape::RADOME_LENGTH_FRACTION,
                d.radome_length_fraction,
            ),
            keel_exponent: resolve(
                self.nose_keel_exponent,
                NoseShape::KEEL_EXPONENT,
                d.keel_exponent,
            ),
            plan_exponent: resolve(
                self.nose_plan_exponent,
                NoseShape::PLAN_EXPONENT,
                d.plan_exponent,
            ),
            section_exponent: resolve(
                self.nose_section_exponent,
                NoseShape::SECTION_EXPONENT,
                d.section_exponent,
            ),
        })
    }

    /// The generated nose station at `xi` in `[0, 1]` of the nose length.
    #[must_use]
    pub fn nose_station(&self, xi: f64) -> NoseStation {
        let radius = self.diameter_m / 2.0;
        let height_scale = self.effective_height_m() / self.diameter_m;
        let x_m = xi * self.cabin_start_x_m;
        let Some(nose) = self.nose_shape() else {
            let r_val = radius * (1.0 - (1.0 - xi).powi(2)).sqrt();
            return NoseStation {
                x_m,
                z_m: self.cabin_z_m + (self.nose_z_m - self.cabin_z_m) * (1.0 - xi).powi(2),
                width_m: r_val * 2.0,
                height_m: r_val * 2.0 * height_scale,
                shape: CIRCULAR_EXPONENT,
            };
        };
        let top = self.nose_top_z_m(&nose, xi);
        let bottom = self.nose_bottom_z_m(&nose, xi);
        let rounded = 1.0 - (1.0 - xi).powi(2);
        let half_width = radius * rounded.max(0.0).powf(1.0 / nose.plan_exponent);
        let bump = if xi >= 1.0 {
            0.0
        } else {
            (std::f64::consts::PI * xi).sin().max(0.0)
        };
        NoseStation {
            x_m,
            z_m: 0.5 * (top + bottom),
            width_m: half_width * 2.0,
            height_m: top - bottom,
            shape: CIRCULAR_EXPONENT + (nose.section_exponent - CIRCULAR_EXPONENT) * bump,
        }
    }

    /// The cabin half height, m.
    fn nose_cabin_half_height_m(&self) -> f64 {
        self.diameter_m / 2.0 * (self.effective_height_m() / self.diameter_m)
    }

    /// Lower envelope at `xi`, m.
    fn nose_bottom_z_m(&self, nose: &NoseShape, xi: f64) -> f64 {
        let keel = self.cabin_z_m - self.nose_cabin_half_height_m();
        let rounded = (1.0 - (1.0 - xi).powi(2)).max(0.0);
        self.nose_z_m + (keel - self.nose_z_m) * rounded.powf(1.0 / nose.keel_exponent)
    }

    /// Upper envelope at `xi`: radome arc, straight windshield, crown blend.
    fn nose_top_z_m(&self, nose: &NoseShape, xi: f64) -> f64 {
        let length = self.cabin_start_x_m;
        let crown = self.cabin_z_m + self.nose_cabin_half_height_m();
        let tip = self.nose_z_m;
        let x = xi * length;
        let x_base = nose.radome_length_fraction * length;
        let x_crown = nose.crown_end_fraction * length;
        let slope = nose.windshield_angle_deg.to_radians().tan();
        let rise = crown - tip;
        let base_rise = if rise > 0.0 {
            (WINDSHIELD_BASE_RISE_SHARE * rise)
                .max(slope * x_base)
                .min(WINDSHIELD_BASE_RISE_MAX_SHARE * rise)
        } else {
            WINDSHIELD_BASE_RISE_SHARE * rise
        };
        let z_base = tip + base_rise;
        if x >= x_crown {
            return crown;
        }
        if x <= x_base {
            // Convex arc: a straight share `m` (the windshield slope at the
            // base) plus a rounded share with the keel line's tip rounding.
            let straight = if base_rise > 0.0 {
                (slope * x_base / base_rise).min(1.0)
            } else {
                1.0
            };
            let u = x / x_base;
            let rounded = (1.0 - (1.0 - u).powi(2)).max(0.0);
            return tip
                + base_rise
                    * ((1.0 - straight) * rounded.powf(1.0 / nose.keel_exponent) + straight * u);
        }
        let run = (WINDSHIELD_RISE_SHARE * (crown - z_base) / slope)
            .min(WINDSHIELD_RUN_SHARE * (x_crown - x_base))
            .max(0.0);
        if x <= x_base + run {
            return z_base + slope * (x - x_base);
        }
        // Cubic Hermite from the windshield end (the windshield slope) to the
        // crown (zero slope), held inside [windshield end, crown] so the
        // envelope never overshoots or falls.
        let x1 = x_base + run;
        let z1 = z_base + slope * run;
        let span = x_crown - x1;
        let t = (x - x1) / span;
        let h00 = 2.0 * t.powi(3) - 3.0 * t.powi(2) + 1.0;
        let h10 = t.powi(3) - 2.0 * t.powi(2) + t;
        let h01 = -2.0 * t.powi(3) + 3.0 * t.powi(2);
        let z = h00 * z1 + h10 * span * slope + h01 * crown;
        z.clamp(z1.min(crown), z1.max(crown))
    }
}

// A test asserts on values it built here, so an unwrap failing is the
// assertion failing.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn legacy(fuselage: &FuselageConfig, xi: f64) -> (f64, f64, f64, f64) {
        let radius = fuselage.diameter_m / 2.0;
        let scale = fuselage.height_m.map_or(1.0, |h| h / fuselage.diameter_m);
        let z = fuselage.cabin_z_m + (fuselage.nose_z_m - fuselage.cabin_z_m) * (1.0 - xi).powi(2);
        let r = radius * (1.0 - (1.0 - xi).powi(2)).sqrt();
        (xi * fuselage.cabin_start_x_m, z, r * 2.0, r * 2.0 * scale)
    }

    fn shaped() -> FuselageConfig {
        FuselageConfig {
            nose_windshield_angle_deg: Some(32.0),
            ..FuselageConfig::default()
        }
    }

    #[test]
    fn unset_fields_reproduce_the_legacy_stations_bit_for_bit() {
        for height_m in [None, Some(7.1)] {
            let fuselage = FuselageConfig {
                height_m,
                ..FuselageConfig::default()
            };
            assert!(fuselage.nose_shape().is_none());
            for index in 0..9 {
                let angle = std::f64::consts::FRAC_PI_2 * f64::from(index) / 9.0;
                let xi = 1.0 - angle.cos();
                let station = fuselage.nose_station(xi);
                let (x, z, w, h) = legacy(&fuselage, xi);
                assert_eq!(station.x_m.to_bits(), x.to_bits());
                assert_eq!(station.z_m.to_bits(), z.to_bits());
                assert_eq!(station.width_m.to_bits(), w.to_bits());
                assert_eq!(station.height_m.to_bits(), h.to_bits());
                assert_eq!(station.shape, 2.0);
            }
        }
    }

    #[test]
    fn the_tip_is_closed_and_every_other_station_has_positive_size() {
        let fuselage = shaped();
        let tip = fuselage.nose_station(0.0);
        assert!(tip.width_m.abs() < 1.0e-12 && tip.height_m.abs() < 1.0e-12);
        assert!((tip.z_m - fuselage.nose_z_m).abs() < 1.0e-12);
        for index in 1..=200 {
            let station = fuselage.nose_station(f64::from(index) / 200.0);
            assert!(station.width_m > 0.0 && station.height_m > 0.0);
            assert!((2.0..=3.5).contains(&station.shape));
        }
    }

    #[test]
    fn envelopes_and_the_plan_are_monotone_and_reach_the_cabin() {
        let fuselage = shaped();
        let (mut top, mut bottom, mut width) = (f64::NEG_INFINITY, f64::INFINITY, 0.0);
        for index in 0..=400 {
            let s = fuselage.nose_station(f64::from(index) / 400.0);
            let (t, b) = (s.z_m + s.height_m / 2.0, s.z_m - s.height_m / 2.0);
            assert!(t >= top - 1.0e-12, "the crown line never falls");
            // The default tip (-0.5 m) is above the keel: the lower line only descends.
            assert!(b <= bottom + 1.0e-12, "the keel line never rises");
            assert!(s.width_m >= width - 1.0e-12, "the plan never narrows");
            (top, bottom, width) = (t, b, s.width_m);
        }
        assert!((top - 3.3).abs() < 1.0e-12);
        assert!((bottom + 2.9).abs() < 1.0e-12);
        assert!((width - 6.2).abs() < 1.0e-12);
        assert!((fuselage.nose_station(1.0).shape - 2.0).abs() < 1.0e-12);
    }

    #[test]
    fn the_windshield_has_the_requested_slope() {
        for angle in [25.0, 32.0, 45.0] {
            let fuselage = FuselageConfig {
                nose_windshield_angle_deg: Some(angle),
                ..FuselageConfig::default()
            };
            let nose = fuselage.nose_shape().unwrap();
            let length = fuselage.cabin_start_x_m;
            // Two points inside the straight part, just past the windshield base.
            let xi0 = nose.radome_length_fraction + 0.005;
            let xi1 = xi0 + 0.01;
            let (t0, t1) = (
                fuselage.nose_top_z_m(&nose, xi0),
                fuselage.nose_top_z_m(&nose, xi1),
            );
            let measured = ((t1 - t0) / ((xi1 - xi0) * length)).atan().to_degrees();
            assert!((measured - angle).abs() < 1.0e-9, "{measured} vs {angle}");
        }
    }

    #[test]
    fn the_radome_is_convex_and_meets_the_windshield_without_a_kink() {
        for (angle, radome, keel) in [(32.0, 0.1, 1.3), (45.0, 0.2, 2.2), (30.0, 0.3, 1.5)] {
            let fuselage = FuselageConfig {
                nose_windshield_angle_deg: Some(angle),
                nose_radome_length_fraction: Some(radome),
                nose_keel_exponent: Some(keel),
                cabin_start_x_m: 8.0,
                ..FuselageConfig::default()
            };
            let nose = fuselage.nose_shape().unwrap();
            let length = fuselage.cabin_start_x_m;
            let base = nose.radome_length_fraction;
            let top = |xi: f64| fuselage.nose_top_z_m(&nose, xi);
            let step = 1.0e-4;
            let slope = |xi: f64| (top(xi + step) - top(xi - step)) / (2.0 * step * length);
            // Concave down: the slope never rises along the arc.
            let mut previous = f64::INFINITY;
            for index in 1..=40 {
                let xi = base * f64::from(index) / 41.0;
                let s = slope(xi);
                assert!(
                    s <= previous + 1.0e-9,
                    "slope rises at {xi}: {s} > {previous}"
                );
                assert!(s > 0.0);
                previous = s;
            }
            // Continuous slope across the windshield base.
            let tan = angle.to_radians().tan();
            let before = slope(base - 2.0 * step);
            let after = slope(base + 2.0 * step);
            assert!((before - tan).abs() < 2.0e-3, "{before} vs {tan}");
            assert!((after - tan).abs() < 1.0e-6, "{after} vs {tan}");
        }
    }

    #[test]
    fn the_widened_ranges_are_accepted_and_clamped_at_the_new_limits() {
        let fuselage = FuselageConfig {
            nose_plan_exponent: Some(1.0),
            nose_keel_exponent: Some(1.3),
            nose_crown_end_fraction: Some(1.0),
            ..FuselageConfig::default()
        };
        let nose = fuselage.nose_shape().unwrap();
        assert_eq!(
            (
                nose.plan_exponent,
                nose.keel_exponent,
                nose.crown_end_fraction
            ),
            (1.0, 1.3, 1.0)
        );
        let below = FuselageConfig {
            nose_plan_exponent: Some(0.5),
            nose_keel_exponent: Some(1.0),
            nose_crown_end_fraction: Some(1.5),
            ..FuselageConfig::default()
        };
        let nose = below.nose_shape().unwrap();
        assert_eq!(
            (
                nose.plan_exponent,
                nose.keel_exponent,
                nose.crown_end_fraction
            ),
            (1.0, 1.3, 1.0)
        );
        let mut width = 0.0;
        let mut top = f64::NEG_INFINITY;
        for index in 1..=200 {
            let s = fuselage.nose_station(f64::from(index) / 200.0);
            assert!(s.height_m > 0.0 && s.width_m >= width - 1.0e-12);
            assert!(s.z_m + s.height_m / 2.0 >= top - 1.0e-12);
            width = s.width_m;
            top = s.z_m + s.height_m / 2.0;
        }
        assert!((top - 3.3).abs() < 1.0e-12 && (width - 6.2).abs() < 1.0e-12);
    }

    #[test]
    fn out_of_range_and_non_finite_fields_are_resolved_safely() {
        let fuselage = FuselageConfig {
            nose_windshield_angle_deg: Some(f64::NAN),
            nose_keel_exponent: Some(99.0),
            ..FuselageConfig::default()
        };
        let nose = fuselage.nose_shape().unwrap();
        assert_eq!(nose.windshield_angle_deg, 32.0);
        assert_eq!(nose.keel_exponent, 4.0);
        for index in 1..=20 {
            let s = fuselage.nose_station(f64::from(index) / 20.0);
            assert!(s.z_m.is_finite() && s.height_m > 0.0);
        }
    }
}
