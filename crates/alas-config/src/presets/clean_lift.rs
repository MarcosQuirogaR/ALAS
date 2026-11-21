// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Measured maximum lift of transport wings in three configurations, on the 1-g
//! stall basis of the rest of the speed model: clean (slats in, flaps up, gear
//! up), slats out with flaps up, and the landing flap.
//!
//! The class default of 1.50 (Jenkinson, *Civil Jet Aircraft Design*, "about 1.5")
//! is the Raymer Eq. 12.15 value of a wing with a blunt nose. A transport wing with
//! slats has the thin leading edge a cruise wing wants, and its measured clean value
//! is well below that. Obert, *Aerodynamic Design of Transport Aircraft* (2009),
//! Ch. 26, Figs. 26.12-26.16, printed pp. 324-325, plot the flight-test points
//! (values read from the rendered plots, about +-0.02; the 737-800 and 777-200
//! values are the mean of their two weights; landing is the largest plotted flap
//! angle, gear down). The Airbus and 737-800 slats-out points are plotted at flap 0.
//! The 737-300, 757, 767 and 777 ones are at the first flap detent, 1 deg (the slats
//! of these aircraft extend with it), and are taken to flap 0 along the secant of
//! their 1 and 5 deg points, a first-order correction of 0.03 to 0.06 (the curves are
//! concave, so it is a lower bound):
//!
//! | aircraft | flaps | sweep c/4, deg | slats in | slats out, flap 1 -> 0 | landing |
//! |---|---|---|---|---|---|
//! | 737-300 | triple | 25 | 1.45 | 1.963 -> 1.919 | 3.00 |
//! | 757-200 | double | 25 | 1.19 | 1.654 -> 1.611 | 2.60 |
//! | 767-200 | double in, single out | 31.5 | 1.08 | 1.534 -> 1.500 | 2.16 |
//! | A320-200 | single | 25 | 1.28 | 1.90 (flap 0) | 2.59 |
//! | A330-200 | single | 30 | 1.44 | 1.75 (flap 0) | 2.50 |
//! | 737-800 | double | 25 | 1.25 | 1.932 (flap 0) | 2.535 |
//! | 777-200 | double in, single out | 31.5 | 1.18 | 1.672 -> 1.614 | 2.39 |
//!
//! Divided by cos(sweep), the cosine law of Raymer Eq. 12.15, the columns have the
//! means 1.43, 1.97 and 2.87, with scatters of 9.5 %, 7.5 % and 7.4 % r.m.s. The
//! slats-out level scatters less than the slats-in level: the slat lifts a wing to a
//! leading-edge separation limit that is much the same across these wings, so a wing
//! with a high clean value gains less from it (A330: 0.31 against 0.48 mean). The
//! leading-edge increment is therefore taken as slats out less clean, both measured
//! on the same wing or both from the pooled law, never a computed increment added to
//! a measured clean value.
//!
//! The landing column is one pool, not split by flap system. Split, it gives 2.87
//! (single slotted, two aircraft), 2.75 (double, four) and 3.31 (triple, one). Tried
//! on the nine jets whose landing coefficient is recovered from a published approach
//! speed (`landing_reference`), the split leaves the seven single-slotted ones where
//! the pool puts them and moves the DC-10 (double) from -4.4 % to -8.2 % and the
//! B747-400 (triple) from +5.4 % to +21.8 %. Obert's Fig. 26.8 (printed p. 321) shows
//! the same: single-slotted A310 and A300-600 wings reach 3.00-3.10 based on Vmin,
//! double-slotted Hawker Siddeley jets 1.87-2.16; the level follows the leading edge
//! and the generation of the wing more than the number of slots.
//!
//! A preset with measured values of its own, or of the same wing, reads them. A
//! slatless wing keeps the class 1.50: Obert's Fokker 100 (Fig. 26.20, 1.72, sweep
//! 17.4 deg) and the Learjet 60 below (1.38, sweep 13 deg) bracket it among jets, and
//! the flaps-up points of the DHC-8 (1.93) and DHC-7 (1.50) of Obert's Fig. 26.9,
//! printed p. 322, about 1.73 and 1.34 on the 1-g basis, bracket it among straight-wing
//! turboprops.

use super::high_lift_system::{declared_system, quarter_chord_sweep_deg};
use super::AircraftPreset;

/// Mean `CLmax,1g / cos(sweep)`, slats in, flaps up, of the module table.
pub const SLATS_IN_CLEAN_OVER_COS_SWEEP: f64 = 1.43;
/// Mean `CLmax,1g / cos(sweep)`, slats out, flaps up, of the module table.
pub const SLATS_OUT_FLAPS_UP_OVER_COS_SWEEP: f64 = 1.97;
/// Mean `CLmax,1g / cos(sweep)` at the landing flap, gear down, of the module table.
pub const SLATTED_LANDING_OVER_COS_SWEEP: f64 = 2.87;

/// Coefficients measured on the aircraft or on the same wing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PublishedCleanLift {
    /// Registered aircraft.
    pub preset_name: &'static str,
    /// `CLmax,1g`, slats in, flaps up, gear up.
    pub cl_max_clean: f64,
    /// `CLmax,1g`, slats out, flaps up, gear up, when the same document gives it.
    pub cl_max_slats_out: Option<f64>,
    /// Document, page and how the value was read.
    pub source: &'static str,
}

const PUBLISHED: [PublishedCleanLift; 3] = [
    PublishedCleanLift {
        preset_name: "A320-200",
        cl_max_clean: 1.28,
        cl_max_slats_out: Some(1.90),
        source: "Obert, Aerodynamic Design of Transport Aircraft, Fig. 26.13, printed p. 324: A320-200 slats in, flaps up, gear up 1.28; slats out, flaps 0 1.90.",
    },
    PublishedCleanLift {
        preset_name: "A340-300",
        cl_max_clean: 1.44,
        cl_max_slats_out: Some(1.75),
        source: "Obert, Aerodynamic Design of Transport Aircraft, Fig. 26.14, printed p. 324: A330-200 slats in, flaps up, gear up 1.44; slats out, flaps 0 1.75; sweep 30 deg; the A330 and A340 share the wing (Jane's All the World's Aircraft 2008-09, Airbus A330/A340 entry, PDF p. 323).",
    },
    PublishedCleanLift {
        preset_name: "LJ60",
        cl_max_clean: 1.38,
        cl_max_slats_out: None,
        source: "Learjet 60 Airplane Flight Manual FM-123, Fig. 5-22 p. 5-34: stall speed flaps 0, idle, 129.5 KCAS at 23,500 lb; divided by 0.94 for the 1-g basis (AMC 25.103), 24.57 m^2.",
    },
];

/// The measured coefficients of one preset, when it has them.
pub fn published_clean_lift(name: &str) -> Option<&'static PublishedCleanLift> {
    PUBLISHED.iter().find(|entry| entry.preset_name == name)
}

/// Slats-out, flaps-up `CLmax,1g` of a slatted registered wing: the measured value
/// of the same wing, else the pooled law at the wing's quarter-chord sweep.
pub(super) fn slats_out_flaps_up(preset: &AircraftPreset) -> Option<f64> {
    if let Some(measured) = published_clean_lift(preset.name).and_then(|e| e.cl_max_slats_out) {
        return Some(measured);
    }
    quarter_chord_sweep_deg(preset)
        .map(|sweep| SLATS_OUT_FLAPS_UP_OVER_COS_SWEEP * sweep.to_radians().cos())
}

/// Landing `CLmax,1g` of a slatted registered wing without a published approach
/// speed: the pooled law at its quarter-chord sweep.
pub(super) fn pooled_slatted_landing(preset: &AircraftPreset) -> Option<f64> {
    quarter_chord_sweep_deg(preset)
        .map(|sweep| SLATTED_LANDING_OVER_COS_SWEEP * sweep.to_radians().cos())
}

pub(super) fn apply(preset: &mut AircraftPreset) {
    let clean = match published_clean_lift(preset.name) {
        Some(entry) => Some(entry.cl_max_clean),
        // A declared high-lift system is a slatted wing. Slatless wings keep the class value.
        None if declared_system(preset.name).is_some() => quarter_chord_sweep_deg(preset)
            .map(|sweep| SLATS_IN_CLEAN_OVER_COS_SWEEP * sweep.to_radians().cos()),
        None => None,
    };
    if let Some(clean) = clean {
        preset
            .performance
            .get_or_insert_with(Default::default)
            .cl_max_clean = clean;
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    /// (sweep, slats in, slats out at flap 0, landing) of the module table.
    const POINTS: [(f64, f64, f64, f64); 7] = [
        (25.0, 1.45, 1.919, 3.00),
        (25.0, 1.19, 1.611, 2.60),
        (31.5, 1.08, 1.500, 2.16),
        (25.0, 1.28, 1.90, 2.59),
        (30.0, 1.44, 1.75, 2.50),
        (25.0, 1.25, 1.932, 2.535),
        (31.5, 1.18, 1.614, 2.39),
    ];

    /// Flap 1 and flap 5 deg slats-out readings of the Boeing aircraft plotted at
    /// flap 1 (Figs. 26.12, 26.16), the 777-200 as the mean of two weights.
    const BOEING_FLAP_1_AND_5: [(f64, f64, f64); 4] = [
        (1.919, 1.963, 2.138),
        (1.611, 1.654, 1.826),
        (1.500, 1.534, 1.672),
        (1.614, 1.672, 1.900),
    ];

    #[test]
    fn the_boeing_flap_1_points_are_taken_to_flap_0_along_their_secant() {
        for (flap_0, flap_1, flap_5) in BOEING_FLAP_1_AND_5 {
            let secant = (flap_5 - flap_1) / 4.0;
            assert!((flap_1 - secant - flap_0).abs() < 0.0015, "{flap_0}");
        }
    }

    fn mean(column: impl Fn(&(f64, f64, f64, f64)) -> f64) -> f64 {
        POINTS
            .iter()
            .map(|point| column(point) / point.0.to_radians().cos())
            .sum::<f64>()
            / POINTS.len() as f64
    }

    #[test]
    fn the_constants_are_the_means_of_the_flight_test_points() {
        let slats_in = mean(|p| p.1);
        let slats_out = mean(|p| p.2);
        let landing = mean(|p| p.3);
        assert!(
            (slats_in - SLATS_IN_CLEAN_OVER_COS_SWEEP).abs() < 0.005,
            "{slats_in}"
        );
        assert!(
            (slats_out - SLATS_OUT_FLAPS_UP_OVER_COS_SWEEP).abs() < 0.005,
            "{slats_out}"
        );
        assert!(
            (landing - SLATTED_LANDING_OVER_COS_SWEEP).abs() < 0.005,
            "{landing}"
        );
    }

    #[test]
    fn every_real_aircraft_stays_in_the_measured_range_and_below_its_take_off_value() {
        for name in super::super::available() {
            let preset = super::super::get(name).unwrap();
            let performance = preset.performance.as_ref().unwrap();
            let clean = performance.cl_max_clean;
            assert!((1.05..=1.72).contains(&clean), "{name} {clean}");
            assert!(performance.cl_max_to > clean, "{name}");
            assert!(performance.cl_max_land > performance.cl_max_to, "{name}");
        }
    }

    #[test]
    fn measured_values_win_and_slatless_wings_keep_the_class_value() {
        let clean = |name| {
            super::super::get(name)
                .unwrap()
                .performance
                .as_ref()
                .unwrap()
                .cl_max_clean
        };
        assert_eq!(clean("A320-200"), 1.28);
        assert_eq!(clean("A340-300"), 1.44);
        assert_eq!(clean("LJ60"), 1.38);
        for name in ["ATR72-600", "C295", "A400M"] {
            assert_eq!(clean(name), 1.50, "{name}");
        }
        // The swept wide body sits below the narrow body on the cosine law.
        assert!(clean("B747-400") < clean("A220-300"));
    }

    #[test]
    fn the_slat_increment_is_slats_out_less_clean_on_the_same_wing() {
        let a340 = super::super::get("A340-300").unwrap();
        assert_eq!(slats_out_flaps_up(a340), Some(1.75));
        let b787 = super::super::get("B787-9").unwrap();
        let ratio =
            slats_out_flaps_up(b787).unwrap() / b787.performance.as_ref().unwrap().cl_max_clean;
        assert!(
            (ratio - SLATS_OUT_FLAPS_UP_OVER_COS_SWEEP / SLATS_IN_CLEAN_OVER_COS_SWEEP).abs()
                < 1e-12
        );
    }
}
