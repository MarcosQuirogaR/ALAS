// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The generated aft-body stations: the tailcone taper and, when the belly
//! upsweep is set, the straight lower line that rises ahead of it.
//!
//! One law shared by the geometry builder and the sandbox handles, so a
//! drawn station and a lofted one cannot disagree.
//!
//! Frame: x aft of the nose tip, z up, metres. Without a belly upsweep the
//! nine tail stations span the tailcone, `[L - l_tc, L]`, and the centreline
//! rises and the radius shrinks as `xi^1.5`. With one (`l_up > l_tc`) they
//! span `[L - l_up, L]`: the top line and the width keep the tailcone law
//! (constant ahead of it), and the lower line is straight from the cabin
//! floor of the body at `L - l_up` to the tail-tip bottom. That is the
//! upswept afterbody of a transport, whose belly starts to rise near the aft
//! cargo hold, well ahead of the crown taper; it sets the tail-down angle.

//!
//! The shaped aft body (any of `tail_end_*`, `aft_plan_*`, `aft_crown_*` set)
//! decouples the three lines that the tailcone law ties together. With `c_p`,
//! `c_c`, `c_b` the fractions (0 at the start, 1 at the tail end) of the plan
//! taper, the crown drop and the belly line, `R` and `hs` the cabin radius and
//! height scale, and `(w_e, h_e)` the closing section centred on `tail_end_z_m` (else `tail_z_m`):
//!
//! - half-width: `w_e/2 + (R - w_e/2) (1 - c_p^p_p)`;
//! - upper line: `top_cabin + (tail_end_z + h_e/2 - top_cabin) c_c^p_c`;
//! - lower line: the belly line `keel + (end bottom - keel) t^p_b` from the
//!   cabin keel at `L - belly_upsweep_length_m` (straight for `p_b = 1`) to
//!   `tail_end_z - h_e/2`, never below the
//!   crown-drop keel `bottom_cabin + (tail_end_z - h_e/2 - bottom_cabin) c_c^p_c`.
//!
//! With the closing section zero and both exponents 1.5 this is exactly the
//! tailcone law above; it is selected only when one of the fields is set, so
//! every other body keeps its stations bit for bit.

use super::FuselageConfig;

/// Default exponent of the plan taper and the crown drop, the tailcone law's.
const TAILCONE_EXPONENT: f64 = 1.5;
/// Valid range of the aft plan and crown exponents.
const AFT_EXPONENT_RANGE: (f64, f64) = (1.0, 3.0);

/// One generated aft-body station.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AftBodyStation {
    /// Station, m aft of the nose tip.
    pub x_m: f64,
    /// Section centre height, m.
    pub z_m: f64,
    /// Full width, m.
    pub width_m: f64,
    /// Full height, m.
    pub height_m: f64,
}

/// Smallest section height, as a fraction of the cabin height, the upswept
/// lower line may leave: it keeps every section a closed, positive-height
/// loop where the belly line meets the crown.
const MIN_HEIGHT_FRACTION: f64 = 0.02;

impl FuselageConfig {
    /// The belly upsweep length in effect at fuselage length `length_m`:
    /// `belly_upsweep_length_m` when it is finite, longer than the tailcone
    /// and ends aft of the cabin start, else `None` (tailcone law only).
    #[must_use]
    pub fn active_belly_upsweep_m(&self, length_m: f64) -> Option<f64> {
        let shaped = self.shaped_aft_body();
        self.belly_upsweep_length_m.filter(|upsweep| {
            upsweep.is_finite()
                && (shaped || *upsweep > self.tailcone_length_m)
                && *upsweep > 0.0
                && length_m - upsweep > self.nose_loft_length_m()
        })
    }

    /// Whether any aft-body shaping field is set (closing section, plan taper
    /// or crown drop), which selects the shaped aft-body law.
    #[must_use]
    pub fn shaped_aft_body(&self) -> bool {
        [
            self.tail_end_width_m,
            self.tail_end_height_m,
            self.tail_end_z_m,
            self.aft_belly_exponent,
            self.aft_plan_length_m,
            self.aft_plan_exponent,
            self.aft_crown_length_m,
            self.aft_crown_exponent,
        ]
        .iter()
        .any(Option::is_some)
    }

    /// The fraction of the aft-body span at which the builder places its
    /// `xi`-th station: the shaped aft body clusters them towards the tail end
    /// where its lines curve, every other body spaces them uniformly.
    #[must_use]
    pub fn aft_station_fraction(&self, xi: f64) -> f64 {
        if self.shaped_aft_body() {
            1.0 - (1.0 - xi).powf(1.5)
        } else {
            xi
        }
    }

    fn positive(value: Option<f64>) -> Option<f64> {
        value.filter(|v| v.is_finite() && *v > 0.0)
    }

    /// Where the generated aft body starts (the last cabin station), m.
    #[must_use]
    pub fn aft_body_start_m(&self, length_m: f64) -> f64 {
        let tailcone = self.tailcone_length_m;
        let span = if self.shaped_aft_body() {
            [
                Self::positive(self.aft_plan_length_m).unwrap_or(tailcone),
                Self::positive(self.aft_crown_length_m).unwrap_or(tailcone),
                self.active_belly_upsweep_m(length_m).unwrap_or(tailcone),
            ]
            .into_iter()
            .fold(0.0, f64::max)
        } else {
            self.active_belly_upsweep_m(length_m).unwrap_or(tailcone)
        };
        length_m - span
    }

    /// The sandbox field id whose value moves the aft-body start: the belly
    /// upsweep length when active, else the tailcone length.
    #[must_use]
    pub fn aft_body_handle_field(&self, length_m: f64) -> &'static str {
        if self.active_belly_upsweep_m(length_m).is_some() {
            "geometry.fuselage.belly_upsweep_length_m"
        } else {
            "geometry.fuselage.tailcone_length_m"
        }
    }

    /// The interior section at `x_m` for a loft section with centre `z_m` and
    /// height `height_m`: with `hold_interior_end_x_m` set, a section aft of
    /// the cabin start and not past that station keeps the cabin keel (only the
    /// outer belly rises), else the section unchanged. Returns `(z, height)`.
    #[must_use]
    pub fn hold_interior_section(&self, x_m: f64, z_m: f64, height_m: f64) -> (f64, f64) {
        let Some(end) = self.hold_interior_end_x_m.filter(|v| v.is_finite()) else {
            return (z_m, height_m);
        };
        let keel = self.cabin_z_m - 0.5 * self.effective_height_m();
        let top = z_m + 0.5 * height_m;
        if x_m <= end && x_m > self.cabin_start_x_m && top > keel && z_m - 0.5 * height_m > keel {
            (0.5 * (top + keel), top - keel)
        } else {
            (z_m, height_m)
        }
    }

    /// The generated aft-body station at `xi` in `(0, 1]` of the aft body
    /// for fuselage length `length_m`.
    #[must_use]
    pub fn aft_body_station(&self, xi: f64, length_m: f64) -> AftBodyStation {
        if self.shaped_aft_body() {
            return self.shaped_aft_body_station(xi, length_m);
        }
        let radius = self.diameter_m / 2.0;
        let height_scale = self.effective_height_m() / self.diameter_m;
        let Some(upsweep) = self.active_belly_upsweep_m(length_m) else {
            let cabin_end = length_m - self.tailcone_length_m;
            let r_val = radius * (1.0 - xi.powf(1.5));
            return AftBodyStation {
                x_m: cabin_end + xi * self.tailcone_length_m,
                z_m: self.cabin_z_m + (self.tail_z_m - self.cabin_z_m) * xi.powf(1.5),
                width_m: r_val * 2.0,
                height_m: r_val * 2.0 * height_scale,
            };
        };
        let start = length_m - upsweep;
        let x_m = start + xi * upsweep;
        let taper =
            ((x_m - (length_m - self.tailcone_length_m)) / self.tailcone_length_m).clamp(0.0, 1.0);
        let r_val = radius * (1.0 - taper.powf(1.5));
        let crown_z = self.cabin_z_m + (self.tail_z_m - self.cabin_z_m) * taper.powf(1.5);
        let top = crown_z + r_val * height_scale;
        let cabin_bottom = self.cabin_z_m - radius * height_scale;
        let lower = cabin_bottom + xi * (self.tail_z_m - cabin_bottom);
        let tailcone_bottom = crown_z - r_val * height_scale;
        let min_height = MIN_HEIGHT_FRACTION * 2.0 * radius * height_scale;
        let bottom = lower.max(tailcone_bottom).min(top - min_height);
        AftBodyStation {
            x_m,
            z_m: 0.5 * (top + bottom),
            width_m: r_val * 2.0,
            height_m: top - bottom,
        }
    }
}

impl FuselageConfig {
    /// The shaped aft-body station at fraction `xi` in `(0, 1]` of the span
    /// from `aft_body_start_m` to the tail end (see the module doc).
    fn shaped_aft_body_station(&self, xi: f64, length_m: f64) -> AftBodyStation {
        let radius = self.diameter_m / 2.0;
        let height_scale = self.effective_height_m() / self.diameter_m;
        let tailcone = self.tailcone_length_m;
        let start = self.aft_body_start_m(length_m);
        let x_m = start + xi * (length_m - start);
        let fraction =
            |taper_length: f64| ((x_m - (length_m - taper_length)) / taper_length).clamp(0.0, 1.0);
        let plan_length = Self::positive(self.aft_plan_length_m).unwrap_or(tailcone);
        let crown_length = Self::positive(self.aft_crown_length_m).unwrap_or(tailcone);
        let plan_exponent = self
            .aft_plan_exponent
            .filter(|v| v.is_finite())
            .map_or(TAILCONE_EXPONENT, |v| {
                v.clamp(AFT_EXPONENT_RANGE.0, AFT_EXPONENT_RANGE.1)
            });
        let crown_exponent = self
            .aft_crown_exponent
            .filter(|v| v.is_finite())
            .map_or(TAILCONE_EXPONENT, |v| {
                v.clamp(AFT_EXPONENT_RANGE.0, AFT_EXPONENT_RANGE.1)
            });
        let end_half_width = (0.5
            * self
                .tail_end_width_m
                .filter(|v| v.is_finite())
                .unwrap_or(0.0))
        .clamp(0.0, radius);
        let end_half_height = (0.5
            * self
                .tail_end_height_m
                .filter(|v| v.is_finite())
                .unwrap_or(0.0))
        .clamp(0.0, radius * height_scale);
        let c_plan = fraction(plan_length);
        let c_crown = fraction(crown_length).powf(crown_exponent);
        let half_width =
            end_half_width + (radius - end_half_width) * (1.0 - c_plan.powf(plan_exponent));
        let cabin_top = self.cabin_z_m + radius * height_scale;
        let cabin_bottom = self.cabin_z_m - radius * height_scale;
        let end_z = self
            .tail_end_z_m
            .filter(|v| v.is_finite())
            .unwrap_or(self.tail_z_m);
        let end_top = end_z + end_half_height;
        let end_bottom = end_z - end_half_height;
        let belly_exponent = self
            .aft_belly_exponent
            .filter(|v| v.is_finite())
            .map_or(1.0, |v| v.clamp(AFT_EXPONENT_RANGE.0, AFT_EXPONENT_RANGE.1));
        let top = cabin_top + (end_top - cabin_top) * c_crown;
        let taper_bottom = cabin_bottom + (end_bottom - cabin_bottom) * c_crown;
        let min_height = MIN_HEIGHT_FRACTION * 2.0 * radius * height_scale;
        let bottom = match self.active_belly_upsweep_m(length_m) {
            Some(upsweep) => {
                let belly = ((x_m - (length_m - upsweep)) / upsweep)
                    .clamp(0.0, 1.0)
                    .powf(belly_exponent);
                let lower = cabin_bottom + belly * (end_bottom - cabin_bottom);
                lower.max(taper_bottom).min(top - min_height)
            }
            None => taper_bottom.min(top),
        };
        AftBodyStation {
            x_m,
            z_m: 0.5 * (top + bottom),
            width_m: 2.0 * half_width,
            height_m: top - bottom,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_an_upsweep_the_stations_are_the_tailcone_law() {
        let fuselage = FuselageConfig::default();
        let length = 70.0;
        assert_eq!(fuselage.aft_body_start_m(length), length - 14.0);
        let station = fuselage.aft_body_station(0.5, length);
        assert_eq!(station.x_m, length - 14.0 + 7.0);
        let expected_r = 3.1 * (1.0 - 0.5_f64.powf(1.5));
        assert_eq!(station.width_m, expected_r * 2.0);
        // An upsweep no longer than the tailcone is ignored.
        let short = FuselageConfig {
            belly_upsweep_length_m: Some(10.0),
            ..FuselageConfig::default()
        };
        assert_eq!(short.aft_body_station(0.5, length), station);
    }

    #[test]
    fn an_upsweep_raises_a_straight_belly_ahead_of_the_crown_taper() {
        let fuselage = FuselageConfig {
            belly_upsweep_length_m: Some(24.0),
            ..FuselageConfig::default()
        };
        let length = 70.0;
        assert_eq!(fuselage.aft_body_start_m(length), 46.0);
        let cabin_top = 0.2 + 3.1;
        let cabin_bottom = 0.2 - 3.1;
        let mut previous_bottom = cabin_bottom;
        for index in 1..=9 {
            let xi = f64::from(index) / 9.0;
            let station = fuselage.aft_body_station(xi, length);
            let top = station.z_m + station.height_m / 2.0;
            let bottom = station.z_m - station.height_m / 2.0;
            assert!(station.height_m > 0.0);
            assert!(bottom > previous_bottom - 1.0e-12, "the belly only rises");
            previous_bottom = bottom;
            if station.x_m <= 56.0 {
                // Ahead of the crown taper the top and the width hold.
                assert!((top - cabin_top).abs() < 1.0e-12);
                assert!((station.width_m - 6.2).abs() < 1.0e-12);
                // The belly is the straight line from the cabin bottom.
                let line = cabin_bottom + xi * (1.8 - cabin_bottom);
                assert!((bottom - line).abs() < 1.0e-12);
            }
        }
        let tip = fuselage.aft_body_station(1.0, length);
        assert_eq!(tip.x_m, length);
        assert!(tip.width_m.abs() < 1.0e-12);
    }

    #[test]
    fn the_shaped_law_with_default_exponents_and_a_point_end_is_the_tailcone_law() {
        let legacy = FuselageConfig::default();
        let shaped = FuselageConfig {
            aft_plan_exponent: Some(1.5),
            ..FuselageConfig::default()
        };
        let length = 70.0;
        assert!(shaped.shaped_aft_body() && !legacy.shaped_aft_body());
        assert_eq!(
            shaped.aft_body_start_m(length),
            legacy.aft_body_start_m(length)
        );
        for index in 1..=9 {
            let xi = f64::from(index) / 9.0;
            let a = legacy.aft_body_station(xi, length);
            let b = shaped.aft_body_station(xi, length);
            assert!((a.x_m - b.x_m).abs() < 1.0e-12);
            assert!((a.z_m - b.z_m).abs() < 1.0e-12, "z at {xi}");
            assert!((a.width_m - b.width_m).abs() < 1.0e-12);
            assert!((a.height_m - b.height_m).abs() < 1.0e-12);
        }
    }

    #[test]
    fn a_closing_section_ends_the_body_finite_and_a_late_crown_drop_keeps_the_crown() {
        let fuselage = FuselageConfig {
            tail_end_width_m: Some(0.6),
            tail_end_height_m: Some(0.8),
            tail_end_z_m: Some(1.2),
            aft_crown_length_m: Some(4.0),
            aft_plan_length_m: Some(10.0),
            belly_upsweep_length_m: Some(13.0),
            aft_belly_exponent: Some(1.4),
            ..FuselageConfig::default()
        };
        let length = 70.0;
        assert_eq!(fuselage.aft_body_start_m(length), 57.0);
        let tip = fuselage.aft_body_station(1.0, length);
        assert!((tip.width_m - 0.6).abs() < 1.0e-12);
        assert!((tip.height_m - 0.8).abs() < 1.0e-12);
        assert!((tip.z_m - 1.2).abs() < 1.0e-12);
        let early = fuselage.aft_body_station(0.5, length);
        assert!(
            (early.z_m + early.height_m / 2.0 - 3.3).abs() < 1.0e-12,
            "crown level"
        );
        assert!(early.width_m < 6.2, "the plan taper has started");
        let mut previous = f64::MAX;
        for index in 1..=9 {
            let station = fuselage.aft_body_station(f64::from(index) / 9.0, length);
            assert!(station.height_m > 0.0 && station.width_m > 0.0);
            assert!(
                station.width_m < previous + 1.0e-12,
                "the plan only narrows"
            );
            previous = station.width_m;
        }
    }

    #[test]
    fn the_hold_interior_keeps_the_cabin_keel_up_to_its_end_only() {
        let fuselage = FuselageConfig {
            hold_interior_end_x_m: Some(60.0),
            belly_upsweep_length_m: Some(24.0),
            ..FuselageConfig::default()
        };
        let length = 70.0;
        let keel = 0.2 - 3.1;
        let station = fuselage.aft_body_station(0.5, length);
        let (z, h) = fuselage.hold_interior_section(station.x_m, station.z_m, station.height_m);
        assert!(
            (z - 0.5 * h - keel).abs() < 1.0e-12,
            "keel held at {}",
            station.x_m
        );
        assert!((z + 0.5 * h - (station.z_m + 0.5 * station.height_m)).abs() < 1.0e-12);
        let late = fuselage.aft_body_station(1.0, length);
        assert_eq!(
            fuselage.hold_interior_section(late.x_m, late.z_m, late.height_m),
            (late.z_m, late.height_m)
        );
        let unset = FuselageConfig::default();
        assert_eq!(unset.hold_interior_section(50.0, 1.0, 2.0), (1.0, 2.0));
    }
}
