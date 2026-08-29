// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from native aerodynamic model/atmosphere/_diff_atmo_functions.py
// Upstream: native aerodynamic model 4.2.8, MIT.
// Reference: alas @ 7d1555c1f4db5110cf6cd187c156718e1a033b50.

//! native aerodynamic model's differentiable atmosphere: a cubic B-spline fitted through
//! the ISA at thirty-eight altitudes.
//!
//! This is the model `Atmosphere(altitude=...)` uses when no `method` is
//! named, which is how every module in the reference implementation but one
//! constructs it -- the turbofan cycle, the performance envelope, the
//! aerodynamic analysis, stability, the full analysis. It exists so that a
//! gradient-based optimizer sees a smooth function rather than the ISA's
//! piecewise-linear temperature, and it is not a small correction to the
//! closed form: it disagrees with the ISA by up to 1.1% in temperature and
//! 0.4% in density over the altitudes this program flies at. A port that
//! reached for [`crate::pressure_isa`] wherever upstream wrote
//! `Atmosphere(...)` would be wrong by four thousand times the `closed`
//! tier before evaluating any physics, so the fit is reproduced rather than
//! approximated.
//!
//! The fit is built in two pieces, both interpolating rather than smoothing:
//! temperature directly, and pressure through its logarithm, since pressure
//! falls by five orders of magnitude across the fitted band and a spline
//! through the raw values would ring badly between knots. Upstream's comment
//! records the resulting mean absolute pressure error against the ISA as
//! 0.02% over 0-100 km.
//!
//! # The altitude grid
//!
//! Sixteen hand-picked altitudes bracketing the ISA's layer boundaries, plus
//! two geometric fans reaching far outside any atmosphere -- up to 2,087 km
//! and down to -5,000 km. The fans are not physical; they exist so an
//! optimizer that steps an altitude variable somewhere absurd still gets a
//! finite number and a finite gradient back instead of leaving the model's
//! domain. Outside even that range the answer is NaN, because
//! `InterpolatedModel` is constructed with `fill_value=np.nan` and `interpn`
//! overwrites every out-of-range result with it.
//!
//! The grid is computed here from the same construction upstream writes
//! rather than transcribed as a table of thirty-eight literals, so that what
//! it *is* -- two geometric fans and a hand-picked list -- stays legible. The
//! parity fixture records the resulting altitudes and compares them, which is
//! what makes computing them safe: a mistake in the construction is a
//! different grid and a different atmosphere, and the comparison sees it.

use std::sync::LazyLock;

use alas_math::CubicBSpline;

use crate::isa::{pressure_isa, temperature_isa};

/// The hand-picked altitudes, in metres, bracketing the ISA layer boundaries.
const EXPLICIT_KNOTS_M: [f64; 16] = [
    0.0, 5e3, 10e3, 13e3, 18e3, 22e3, 30e3, 34e3, 45e3, 49e3, 53e3, 69e3, 73e3, 77e3, 83e3, 87e3,
];

/// Where the upward geometric fan is anchored, in metres: the top of the
/// hand-picked list, which each fan value is measured above.
const UPWARD_FAN_BASE_M: f64 = 87e3;

/// How many points each geometric fan has.
const FAN_POINTS: usize = 11;

/// `numpy.geomspace(start, stop, count)`: `count` points spaced evenly on a
/// log scale, with both endpoints reproduced exactly.
///
/// The endpoint assignments are not cosmetic. NumPy computes the interior
/// points as `10 ** linspace(log10(start), log10(stop), count)` and then
/// overwrites the first and last with `start` and `stop`, because a round
/// trip through the logarithm does not return either exactly. Skipping that
/// would put the grid's outermost knots an ulp or two away from where
/// upstream puts them.
fn geomspace(start: f64, stop: f64, count: usize) -> Vec<f64> {
    let log_start = start.log10();
    let step = (stop.log10() - log_start) / (count - 1) as f64;

    let mut values: Vec<f64> = (0..count)
        .map(|index| 10f64.powf(index as f64 * step + log_start))
        .collect();
    values[0] = start;
    values[count - 1] = stop;
    values
}

/// The thirty-eight altitudes the fit interpolates, in metres, ascending.
///
/// `np.sort(np.unique(...))` upstream. The three sources produce no
/// duplicates -- one fan is entirely negative, the other entirely above 87 km
/// -- so the deduplication has nothing to remove here; it is reproduced
/// because leaving it out would make the grid silently depend on that
/// remaining true.
static ALTITUDE_KNOTS_M: LazyLock<Vec<f64>> = LazyLock::new(|| {
    let mut knots = EXPLICIT_KNOTS_M.to_vec();
    knots.extend(
        geomspace(5e3, 2000e3, FAN_POINTS)
            .into_iter()
            .map(|offset| UPWARD_FAN_BASE_M + offset),
    );
    knots.extend(
        geomspace(5e3, 5000e3, FAN_POINTS)
            .into_iter()
            .map(|offset| 0.0 - offset),
    );

    knots.sort_by(f64::total_cmp);
    knots.dedup();
    knots
});

// A `LazyLock` cannot report an error to a caller, and the two fits below are
// built from the constant grid above: strictly increasing and thirty-eight
// long, which is the whole of `CubicBSpline::interpolate`'s contract. An
// `expect` here therefore cannot fire for any input a caller supplies, only
// for a malformed constant, which `the_two_fits_are_constructible` asserts
// directly rather than leaving to the first caller to discover.
#[allow(clippy::expect_used)]
mod fits {
    use super::{pressure_isa, temperature_isa, CubicBSpline, LazyLock, ALTITUDE_KNOTS_M};

    /// The temperature fit, in Kelvin against metres.
    pub(super) static TEMPERATURE: LazyLock<CubicBSpline> = LazyLock::new(|| {
        let values: Vec<f64> = ALTITUDE_KNOTS_M
            .iter()
            .copied()
            .map(temperature_isa)
            .collect();
        CubicBSpline::interpolate(&ALTITUDE_KNOTS_M, &values)
            .expect("the altitude grid is strictly increasing and 38 points long")
    });

    /// The pressure fit, in log-pascals against metres.
    pub(super) static LOG_PRESSURE: LazyLock<CubicBSpline> = LazyLock::new(|| {
        let values: Vec<f64> = ALTITUDE_KNOTS_M
            .iter()
            .copied()
            .map(|altitude_m| pressure_isa(altitude_m).ln())
            .collect();
        CubicBSpline::interpolate(&ALTITUDE_KNOTS_M, &values)
            .expect("the altitude grid is strictly increasing and 38 points long")
    });
}

/// The altitudes the fit interpolates, in metres, ascending.
pub fn altitude_knots_m() -> &'static [f64] {
    &ALTITUDE_KNOTS_M
}

/// Pressure at `altitude_m` (geopotential, in metres) under the differentiable
/// fit, in pascals.
///
/// NaN outside the fitted band; see the module documentation.
pub fn pressure_differentiable(altitude_m: f64) -> f64 {
    fits::LOG_PRESSURE.evaluate(altitude_m).exp()
}

/// Temperature at `altitude_m` (geopotential, in metres) under the
/// differentiable fit, in Kelvin.
///
/// NaN outside the fitted band; see the module documentation.
pub fn temperature_differentiable(altitude_m: f64) -> f64 {
    fits::TEMPERATURE.evaluate(altitude_m)
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_has_thirty_eight_strictly_increasing_altitudes() {
        let knots = altitude_knots_m();
        assert_eq!(knots.len(), 38);
        for pair in knots.windows(2) {
            assert!(pair[1] > pair[0], "{} is not above {}", pair[1], pair[0]);
        }
    }

    #[test]
    fn the_grid_reaches_five_thousand_kilometres_down_and_two_thousand_up() {
        let knots = altitude_knots_m();
        assert_eq!(knots[0], -5_000_000.0);
        assert_eq!(knots[knots.len() - 1], 87e3 + 2_000_000.0);
    }

    #[test]
    fn the_grid_contains_every_hand_picked_altitude() {
        let knots = altitude_knots_m();
        for altitude_m in EXPLICIT_KNOTS_M {
            assert!(
                knots.contains(&altitude_m),
                "{altitude_m} m is missing from the grid"
            );
        }
    }

    #[test]
    fn geomspace_reproduces_both_endpoints_exactly() {
        let values = geomspace(5e3, 2000e3, 11);
        assert_eq!(values.len(), 11);
        assert_eq!(values[0], 5e3);
        assert_eq!(values[10], 2000e3);
        // Evenly spaced on a log scale: consecutive ratios are equal.
        let ratio = values[1] / values[0];
        for pair in values.windows(2) {
            assert!((pair[1] / pair[0] - ratio).abs() < 1e-12 * ratio);
        }
    }

    #[test]
    fn the_two_fits_are_constructible() {
        // The `expect`s in `fits` rest on this; see the comment there.
        let values: Vec<f64> = altitude_knots_m()
            .iter()
            .copied()
            .map(temperature_isa)
            .collect();
        assert!(CubicBSpline::interpolate(altitude_knots_m(), &values).is_ok());
    }

    #[test]
    fn the_fit_reproduces_the_isa_at_every_altitude_it_was_fitted_at() {
        // Interpolation, not smoothing: the fit passes through its own data.
        for &altitude_m in altitude_knots_m() {
            let expected_t = temperature_isa(altitude_m);
            let actual_t = temperature_differentiable(altitude_m);
            assert!(
                (actual_t - expected_t).abs() < 1e-9 * expected_t.abs(),
                "temperature at {altitude_m} m: {actual_t} against {expected_t}"
            );

            let expected_p = pressure_isa(altitude_m);
            let actual_p = pressure_differentiable(altitude_m);
            assert!(
                (actual_p - expected_p).abs() < 1e-9 * expected_p.abs(),
                "pressure at {altitude_m} m: {actual_p} against {expected_p}"
            );
        }
    }

    #[test]
    fn between_the_fitted_altitudes_it_departs_from_the_isa_by_about_a_per_cent() {
        // The whole reason this module exists rather than deferring to the
        // closed form. If this ever came out negligible, either the fit or
        // the ISA would have stopped being what it is.
        let worst = (0..=250)
            .map(|step| f64::from(step) * 100.0)
            .map(|altitude_m| {
                let isa = temperature_isa(altitude_m);
                (temperature_differentiable(altitude_m) - isa).abs() / isa
            })
            .fold(0.0_f64, f64::max);
        assert!(
            (1e-3..1e-1).contains(&worst),
            "worst temperature disagreement with the ISA was {worst}, which is \
             not the per-cent-scale departure this fit is known to have"
        );
    }

    #[test]
    fn outside_the_fitted_band_the_answer_is_nan_rather_than_an_extrapolation() {
        let knots = altitude_knots_m();
        let below = knots[0] - 1.0;
        let above = knots[knots.len() - 1] + 1.0;
        assert!(temperature_differentiable(below).is_nan());
        assert!(temperature_differentiable(above).is_nan());
        assert!(pressure_differentiable(below).is_nan());
        assert!(pressure_differentiable(above).is_nan());
    }
}
