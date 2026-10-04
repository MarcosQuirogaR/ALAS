// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The static nose-gear share an airport-planning document publishes at the
//! most-aft centre of gravity, as a function of aircraft weight, and the
//! minimum nose-gear load it implies.
//!
//! Airport-planning documents print the ground aft limit in gear-load terms:
//! Airbus section 7-3-0 tabulates, per weight variant, the main-gear load at
//! the most-aft CG at that variant's maximum ramp weight (the nose share
//! follows by subtraction), and Boeing and Airbus A220 section 7.4 charts
//! plot the whole ground CG envelope as percent of weight on the main gear
//! against aircraft weight. Each point is a published (weight, nose share)
//! pair. Between points the share is interpolated linearly in weight.
//!
//! - **Below the lightest point** the share of the lightest point is held.
//!   The share is a function of the CG station alone (static moment balance
//!   about the main gear), so holding it holds the aft CG, and no source
//!   prints a further-aft ground limit at lower weight; relaxing it would be
//!   less conservative than the published data.
//! - **Above the heaviest point** the main gear carries `(1 - f) W` there.
//!   Read as a main-gear load limit, it raises the nose share as
//!   `1 - (1 - f) W / m`. The same floor applies at every weight, so the
//!   linear chord between two points never undercuts the main-gear load
//!   (the Boeing 787-9 Figure 7.4.2 top edge is that constant load).
//! - **Steering.** The manufacturer accepts the smallest published share at
//!   its certified aft CG, so a class steering minimum above it is not this
//!   aircraft's requirement: the steering minimum is the class value lowered
//!   to that share. It never exceeds the tabulated share.
//!
//! A single published point is the constant MRW share at every lighter
//! weight.

/// One published static gear split at the most-aft centre of gravity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AftCgNoseLoadPoint {
    /// Aircraft mass the split is published at, kg.
    pub mass_kg: f64,
    /// Static nose-gear load over aircraft weight at that mass and the
    /// most-aft centre of gravity, dimensionless.
    pub nose_gear_fraction: f64,
    /// The most-aft centre of gravity at that mass, % MAC of the source's
    /// own frame, when the source prints it.
    pub aft_cg_pct_mac: Option<f64>,
}

/// The published nose-gear share at the most-aft centre of gravity against
/// aircraft weight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PublishedAftCgNoseLoad {
    /// Published points in ascending mass order (at least one).
    pub points: &'static [AftCgNoseLoadPoint],
    /// Revision-locked document, figures and rows.
    pub source: &'static str,
}

impl PublishedAftCgNoseLoad {
    /// The smallest published nose share, the aircraft's own steering floor.
    #[must_use]
    pub fn lowest_published_fraction(&self) -> Option<f64> {
        self.points
            .iter()
            .map(|point| point.nose_gear_fraction)
            .reduce(f64::min)
    }

    /// The published nose share at `mass_kg` (see the module doc): linear
    /// between points, the lightest point's share below them and the
    /// heaviest point's main-gear load limit above them. `None` without
    /// points or for a non-finite or non-positive mass.
    #[must_use]
    pub fn published_fraction_at(&self, mass_kg: f64) -> Option<f64> {
        let (first, last) = (self.points.first()?, self.points.last()?);
        if !(mass_kg.is_finite() && mass_kg > 0.0) {
            return None;
        }
        let main_gear_load_kg = (1.0 - last.nose_gear_fraction) * last.mass_kg;
        let main_gear_floor = 1.0 - main_gear_load_kg / mass_kg;
        let tabulated = if mass_kg <= first.mass_kg {
            first.nose_gear_fraction
        } else if mass_kg >= last.mass_kg {
            main_gear_floor
        } else {
            self.points
                .windows(2)
                .find(|pair| mass_kg <= pair[1].mass_kg)
                .map_or(last.nose_gear_fraction, |pair| {
                    let (lo, hi) = (pair[0], pair[1]);
                    let t = (mass_kg - lo.mass_kg) / (hi.mass_kg - lo.mass_kg);
                    lo.nose_gear_fraction + t * (hi.nose_gear_fraction - lo.nose_gear_fraction)
                })
        };
        Some(tabulated.max(main_gear_floor))
    }

    /// The minimum static nose-gear load fraction at `mass_kg`: the larger of
    /// the steering minimum (`class_minimum`, lowered to the smallest
    /// published share) and the published share at this mass.
    ///
    /// A non-finite or non-positive `mass_kg` returns the steering minimum
    /// alone, because no published point can be read at it.
    #[must_use]
    pub fn minimum_nose_gear_fraction(&self, class_minimum: f64, mass_kg: f64) -> f64 {
        let steering = self
            .lowest_published_fraction()
            .map_or(class_minimum, |lowest| class_minimum.min(lowest));
        self.published_fraction_at(mass_kg)
            .map_or(steering, |published| steering.max(published))
    }
}

mod published;

pub(crate) use published::{A220_300, A320_200, A340_300, A380_800, B787_9, DC_10_30};

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() < 1.0e-12
    }

    #[test]
    fn every_published_table_is_ascending_and_physical() {
        for table in [A220_300, A320_200, A340_300, A380_800, B787_9, DC_10_30] {
            assert!(!table.points.is_empty(), "{}", table.source);
            for pair in table.points.windows(2) {
                assert!(pair[0].mass_kg < pair[1].mass_kg, "{}", table.source);
            }
            for point in table.points {
                assert!(
                    point.nose_gear_fraction > 0.0 && point.nose_gear_fraction < 0.15,
                    "{}: {point:?}",
                    table.source
                );
            }
        }
    }

    #[test]
    fn the_a320_table_returns_each_published_weight_variant_split() {
        // Figure 7-3-0-991-010-A01: WV000 at 73,900 kg, 40 % MAC, carries
        // 34,720 kg per main strut; WV017 at 78,400 kg, 36.8 %, 36,410 kg.
        let wv000 = A320_200.minimum_nose_gear_fraction(0.06, 73_900.0);
        assert!(close(wv000, 1.0 - 2.0 * 34_720.0 / 73_900.0), "{wv000}");
        let wv017 = A320_200.minimum_nose_gear_fraction(0.06, 78_400.0);
        assert!(close(wv017, 1.0 - 2.0 * 36_410.0 / 78_400.0), "{wv017}");
        // Halfway between two variants: the linear mean.
        let between = A320_200.minimum_nose_gear_fraction(0.06, 74_900.0);
        let expected =
            0.5 * ((1.0 - 2.0 * 34_720.0 / 73_900.0) + (1.0 - 2.0 * 35_490.0 / 75_900.0));
        assert!(close(between, expected), "{between} against {expected}");
        // Below WV006 (66,400 kg, 43 %) the 43 % share is held.
        let light = A320_200.minimum_nose_gear_fraction(0.06, 42_000.0);
        assert!(close(light, 1.0 - 2.0 * 31_540.0 / 66_400.0), "{light}");
    }

    #[test]
    fn a_single_aft_cg_holds_one_share_at_every_lighter_weight() {
        // A380 WV001 (512 t) and WV000 (562 t) share the 43 % MAC aft CG,
        // so their nose shares agree to 4e-6 and the limit stays there.
        for mass_kg in [270_000.0, 326_000.0, 512_000.0, 562_000.0] {
            let fraction = A380_800.minimum_nose_gear_fraction(0.06, mass_kg);
            assert!((fraction - 0.04875).abs() < 1.0e-4, "{mass_kg}: {fraction}");
        }
        // Heavier than the heaviest point the main-gear load limit tightens.
        let heavy = A380_800.minimum_nose_gear_fraction(0.06, 600_000.0);
        assert!(heavy > 0.05, "{heavy}");
        // No mass to read at: the steering minimum alone.
        let lowest = A380_800.lowest_published_fraction().unwrap_or(f64::NAN);
        assert!(close(
            A380_800.minimum_nose_gear_fraction(0.06, f64::NAN),
            lowest
        ));
    }

    #[test]
    fn the_main_gear_load_floor_holds_between_chart_points() {
        // Boeing 787-9 Figure 7.4.2: between 549,800 lb and the MTW the aft
        // boundary is the constant 519,148 lb main-gear load, which the
        // linear chord alone would undercut.
        let main_gear_load_kg = 2.0 * 259_574.0 / 563_000.0 * 255_372.0;
        let mass_kg = 252_400.0;
        let fraction = B787_9.minimum_nose_gear_fraction(0.06, mass_kg);
        assert!(
            fraction >= 1.0 - main_gear_load_kg / mass_kg - 1.0e-12,
            "{fraction}"
        );
    }

    #[test]
    fn the_steering_minimum_never_exceeds_the_published_share() {
        // A220-300: the 6 % class minimum is above the 5.13 % plateau the
        // manufacturer certifies, so the plateau governs there.
        let plateau = A220_300.minimum_nose_gear_fraction(0.06, 55_000.0);
        assert!(plateau < 0.0520 && plateau > 0.0510, "{plateau}");
    }
}
