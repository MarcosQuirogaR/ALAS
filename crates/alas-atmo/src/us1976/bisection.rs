// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The altitude bisection of the standard pressure law, with the steps far
//! from the answer decided in closed form.

use super::{pressure_pa, Segment, BREAKS, GAS_CONSTANT_AIR, SEA_LEVEL_GRAVITY, SEGMENTS};

/// Half-width, m, of the band around a layer's closed-form crossing inside
/// which a bisection step evaluates the pressure. Outside it the comparison
/// is certain: 1e-8 m changes the pressure by more than 1e-12 of itself
/// (scale heights in the table are under 9 km), while rounding moves the
/// layer law by under 1e-14 of the pressure (its power amplifies the
/// rounding of its base at most 35 times) and the solved crossing by under
/// 1e-10 m, a hundredth of the band.
const EVALUATED_BAND_M: f64 = 1.0e-8;

/// The bisection of [`pressure_pa`] for the altitude of `target_pa`:
/// `iterations` halvings of `[low_m, high_m]`, each keeping the upper half
/// when the pressure at the midpoint exceeds `target_pa` and the lower half
/// otherwise, then the midpoint of the last bracket.
///
/// The result is bit for bit that of evaluating the pressure at every
/// midpoint. A midpoint is evaluated only within [`EVALUATED_BAND_M`] of the
/// altitude at which its own layer's law, continued past the layer, gives
/// `target_pa`; the others compare against that altitude, solved once per
/// layer. A target that is not a positive finite pressure (where the power
/// laws' infinities and signed zeros would mislead), midpoints outside the
/// table and comparisons a NaN reaches are evaluated.
#[must_use]
pub fn bisect_pressure_altitude_m(
    target_pa: f64,
    low_m: f64,
    high_m: f64,
    iterations: usize,
) -> f64 {
    let mut crossings = [None; BREAKS.len() - 1];
    let (mut low_m, mut high_m) = (low_m, high_m);
    for _ in 0..iterations {
        let mid_m = 0.5 * (low_m + high_m);
        if pressure_above(target_pa, mid_m, &mut crossings) {
            low_m = mid_m;
        } else {
            high_m = mid_m;
        }
    }
    0.5 * (low_m + high_m)
}

/// Exactly `pressure_pa(altitude_m) > target_pa`; `crossings` holds each
/// layer's solved crossing once a midpoint in that layer has needed it.
fn pressure_above(
    target_pa: f64,
    altitude_m: f64,
    crossings: &mut [Option<f64>; BREAKS.len() - 1],
) -> bool {
    let table_m = BREAKS[0].altitude_m..=BREAKS[BREAKS.len() - 1].altitude_m;
    let solvable = target_pa > 0.0 && target_pa.is_finite() && table_m.contains(&altitude_m);
    // In the table the layer is the one `pressure_pa` itself selects.
    let layer = SEGMENTS
        .iter()
        .rposition(|segment| altitude_m >= segment.base_altitude_m)
        .filter(|_| solvable);
    if let Some(layer) = layer {
        let crossing_m =
            *crossings[layer].get_or_insert_with(|| crossing_m(&SEGMENTS[layer], target_pa));
        if altitude_m < crossing_m - EVALUATED_BAND_M {
            return true;
        }
        if altitude_m > crossing_m + EVALUATED_BAND_M {
            return false;
        }
    }
    pressure_pa(altitude_m) > target_pa
}

/// The altitude, m, at which `segment`'s pressure law, continued past the
/// layer, gives `target_pa`: the power law or the exponential solved for the
/// height above the base (NaN when no real altitude does).
fn crossing_m(segment: &Segment, target_pa: f64) -> f64 {
    let ratio = target_pa / segment.base_pressure_pa;
    let rise_m = if segment.alpha_k_per_m == 0.0 {
        -GAS_CONSTANT_AIR * segment.base_temperature_k / SEA_LEVEL_GRAVITY * ratio.ln()
    } else {
        let exponent = SEA_LEVEL_GRAVITY / (segment.alpha_k_per_m * GAS_CONSTANT_AIR);
        segment.base_temperature_k / segment.alpha_k_per_m * (1.0 - ratio.powf(exponent.recip()))
    };
    segment.base_altitude_m + rise_m
}

#[cfg(test)]
mod tests {
    use super::{bisect_pressure_altitude_m, pressure_pa, BREAKS};

    /// The bisection evaluating the pressure at every midpoint.
    fn evaluated(target_pa: f64, low_m: f64, high_m: f64, iterations: usize) -> f64 {
        let (mut low_m, mut high_m) = (low_m, high_m);
        for _ in 0..iterations {
            let mid_m = 0.5 * (low_m + high_m);
            if pressure_pa(mid_m) > target_pa {
                low_m = mid_m;
            } else {
                high_m = mid_m;
            }
        }
        0.5 * (low_m + high_m)
    }

    /// `target_pa` and its two floating-point neighbours.
    fn with_neighbours(target_pa: f64) -> [f64; 3] {
        let bits = target_pa.to_bits();
        [
            f64::from_bits(bits - 1),
            target_pa,
            f64::from_bits(bits + 1),
        ]
    }

    fn assert_same(target_pa: f64, (low_m, high_m): (f64, f64), iterations: usize) {
        let solved = bisect_pressure_altitude_m(target_pa, low_m, high_m, iterations);
        let reference = evaluated(target_pa, low_m, high_m, iterations);
        assert_eq!(
            solved.to_bits(),
            reference.to_bits(),
            "{target_pa:e} Pa in [{low_m}, {high_m}] m after {iterations} halvings: \
             {solved} m against {reference} m"
        );
    }

    #[test]
    fn every_bracket_matches_the_evaluated_bisection_bit_for_bit() {
        // The speed caps' bracket and halvings, the whole table, and a
        // bracket reaching past it on both sides.
        let brackets = [
            ((0.0, 30_000.0), 45),
            ((BREAKS[0].altitude_m, BREAKS[8].altitude_m), 60),
            ((-3_000.0, 90_000.0), 52),
        ];
        // Pressures of altitudes on a 1.1 m grid through the table and on a
        // 0.1 mm grid across every layer break (where the tabulated base
        // pressures step), each with its floating-point neighbours.
        let grid = (0..79_000).map(|step| BREAKS[0].altitude_m + 1.1 * f64::from(step));
        let breaks = BREAKS.iter().flat_map(|level| {
            (-500..=500).map(move |step| level.altitude_m + 1.0e-4 * f64::from(step))
        });
        let mut targets: Vec<f64> = grid
            .chain(breaks)
            .flat_map(|altitude_m| with_neighbours(pressure_pa(altitude_m)))
            .collect();
        // Pressures spread evenly in logarithm over the whole table, and the
        // degenerate ones.
        let (highest_pa, lowest_pa) = (
            pressure_pa(BREAKS[0].altitude_m),
            pressure_pa(BREAKS[8].altitude_m),
        );
        targets.extend(
            (0..20_000)
                .map(|step| lowest_pa * (highest_pa / lowest_pa).powf(f64::from(step) / 2.0e4)),
        );
        targets.extend([
            0.0,
            -0.0,
            -1.0,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
            1.0e-300,
        ]);
        for &(bracket, iterations) in &brackets {
            for &target_pa in &targets {
                assert_same(target_pa, bracket, iterations);
            }
        }
        for iterations in [0, 1, 2, 80] {
            assert_same(pressure_pa(9_000.0), (0.0, 30_000.0), iterations);
        }
    }
}
