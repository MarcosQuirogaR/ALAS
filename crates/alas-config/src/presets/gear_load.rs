// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The static gear-load split an airport-planning document tabulates at the
//! most-aft centre of gravity, and the minimum nose-gear load it implies.
//!
//! Manufacturer airport-planning documents print, for each weight variant,
//! the share of the maximum ramp weight the main gear carries with the
//! centre of gravity at its most-aft limit (Airbus section 7-2-0 "percentage
//! of weight on main gear group", or the section 7-3-0 main-gear loads at the
//! most-aft CG, from which the nose share follows by subtraction). That point
//! is published aircraft data. It fixes two independent ground mechanisms:
//!
//! - **Steering.** The manufacturer accepts that nose-gear share at the
//!   certified aft CG, so a class minimum above it is not this aircraft's
//!   steering requirement.
//! - **Main-gear load.** The main gear carries `(1 - f) MRW` at that point.
//!   Read as a main-gear load limit, it curtails the aft CG only near the
//!   tabulated weight and relaxes as the aircraft gets lighter.
//!
//! The Airbus A320 shows both on one aircraft (Airbus A320 Aircraft
//! Characteristics, Jun 01/24, Figure 7-2-0-991-010-A01): WV000 carries
//! 94.0 % on the main gear at 73,900 kg (aft CG 40 %) and WV017 92.9 % at
//! 78,400 kg (aft CG 36.8 %). The WV017 point read as a main-gear load limit
//! returns the WV000 6.0 % nose share at 73,900 kg, where the 6 % class
//! steering minimum governs; a constant 7.1 % would move the 73,900 kg aft
//! limit 3.3 % MAC forward of the published 40 % (1.1 % of the 12.64 m
//! wheelbase over the 4.1935 m MAC).

/// One published static gear-load split at the most-aft centre of gravity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PublishedAftCgNoseLoad {
    /// Aircraft mass the split is tabulated at (normally MRW), kg.
    pub mass_kg: f64,
    /// Static nose-gear load over aircraft weight at that mass and the
    /// most-aft centre of gravity, dimensionless.
    pub nose_gear_fraction: f64,
    /// The most-aft centre of gravity the split is tabulated at, % MAC of the
    /// source's own frame, when the source prints it.
    pub aft_cg_pct_mac: Option<f64>,
    /// Revision-locked document, figure and row.
    pub source: &'static str,
}

impl PublishedAftCgNoseLoad {
    /// The minimum static nose-gear load fraction at `mass_kg`: the larger of
    /// the steering minimum (`class_minimum`, lowered to the published share
    /// when that is smaller) and the share the published main-gear load
    /// leaves to the nose gear at this mass (see the module doc).
    ///
    /// A non-finite or non-positive `mass_kg` returns the steering minimum
    /// alone, because no main-gear load can be divided by it.
    #[must_use]
    pub fn minimum_nose_gear_fraction(&self, class_minimum: f64, mass_kg: f64) -> f64 {
        let steering = class_minimum.min(self.nose_gear_fraction);
        if !(mass_kg.is_finite() && mass_kg > 0.0) {
            return steering;
        }
        let main_gear_load_kg = (1.0 - self.nose_gear_fraction) * self.mass_kg;
        steering.max(1.0 - main_gear_load_kg / mass_kg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Airbus A320 ACAP Jun 01/24, Figure 7-2-0-991-010-A01 sheet 6, WV017.
    const A320_WV017: PublishedAftCgNoseLoad = PublishedAftCgNoseLoad {
        mass_kg: 78_400.0,
        nose_gear_fraction: 0.071,
        aft_cg_pct_mac: Some(36.8),
        source: "test",
    };

    #[test]
    fn the_a320_wv017_point_returns_the_published_wv000_split_at_73_9_t() {
        // WV017 at its own MRW: the published 7.1 %.
        let at_mrw = A320_WV017.minimum_nose_gear_fraction(0.06, 78_400.0);
        assert!((at_mrw - 0.071).abs() < 1.0e-12);
        // WV000 at 73,900 kg prints 94.0 % on the main gear, 6.0 % nose. The
        // WV017 main-gear load (0.929 x 78,400 = 72,834 kg) leaves 1.4 % to
        // the nose there, so the 6 % steering minimum governs.
        let at_wv000 = A320_WV017.minimum_nose_gear_fraction(0.06, 73_900.0);
        assert!((at_wv000 - 0.060).abs() < 1.0e-12);
    }

    #[test]
    fn a_published_share_below_the_class_minimum_is_the_steering_minimum() {
        // Airbus A380 ACAP Dec 01/25, 7-3-0: 4.88 % at 562,000 kg, 43 % MAC.
        let a380 = PublishedAftCgNoseLoad {
            mass_kg: 562_000.0,
            nose_gear_fraction: 0.0488,
            aft_cg_pct_mac: Some(43.0),
            source: "test",
        };
        for mass_kg in [300_000.0, 562_000.0] {
            let fraction = a380.minimum_nose_gear_fraction(0.06, mass_kg);
            assert!((fraction - 0.0488).abs() < 1.0e-12, "{mass_kg}: {fraction}");
        }
        // Heavier than the tabulated point the main-gear load limit tightens.
        assert!(a380.minimum_nose_gear_fraction(0.06, 600_000.0) > 0.0488);
        // No mass to divide by: steering alone.
        assert_eq!(a380.minimum_nose_gear_fraction(0.06, f64::NAN), 0.0488);
    }
}
