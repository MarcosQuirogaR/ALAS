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

use super::FuselageConfig;

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
        self.belly_upsweep_length_m.filter(|upsweep| {
            upsweep.is_finite()
                && *upsweep > self.tailcone_length_m
                && length_m - upsweep > self.cabin_start_x_m
        })
    }

    /// Where the generated aft body starts (the last cabin station), m.
    #[must_use]
    pub fn aft_body_start_m(&self, length_m: f64) -> f64 {
        length_m
            - self
                .active_belly_upsweep_m(length_m)
                .unwrap_or(self.tailcone_length_m)
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

    /// The generated aft-body station at `xi` in `(0, 1]` of the aft body
    /// for fuselage length `length_m`.
    #[must_use]
    pub fn aft_body_station(&self, xi: f64, length_m: f64) -> AftBodyStation {
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
}
