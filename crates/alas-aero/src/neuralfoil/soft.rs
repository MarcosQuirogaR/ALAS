// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from native aerodynamic model/numpy/surrogate_model_tools.py (softmax, softmin,
// sigmoid, swish, blend) and native aerodynamic model/modeling/splines/hermite.py
// (cosine_hermite_patch).
// Upstream: native aerodynamic model 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! The smooth replacements for `max`, `min` and `if` that the airfoil
//! surrogate is built out of.
//!
//! Every one of these exists because native aerodynamic model is written to be
//! differentiated. A `max` has a kink, an `if` has a jump, and an optimizer
//! walking through either gets a gradient that lies to it -- so the library
//! replaces them with functions that agree with the sharp version away from
//! the switch and round it off nearby. That rounding is not a detail to
//! approximate here: `alas-aero::neuralfoil`'s critical-Mach fit, its
//! wave-drag schedule and its post-stall blending are all *defined* by these
//! shapes, and a port that used a plain `max` would agree everywhere except
//! at the transitions, which is where the interesting flight conditions are.
//!
//! # Why they live here
//!
//! `alas-aero::neuralfoil` is the only thing in this port that reaches any of
//! them, and they are scoped to what it reaches. They are translations of two
//! native aerodynamic model modules rather than primitives nobody owns, so the rule
//! `alas-payload::numeric` states -- private code with no upstream module of
//! its own moves to `alas-math` when a second crate needs it -- does not
//! apply: if a second consumer appears these move to the aircraft tree beside
//! `alas-geom::aircraft::spacing`, which is where a translation carrying an
//! upstream file's provenance belongs.
//!
//! Left untranslated, each unreached: `softmax`/`softmin`'s `hardness`
//! spelling of the same parameter (nothing here passes it), the
//! `scalefree` variants, `softplus`, and `sigmoid`'s `arctan` and
//! `polynomial` shapes and its non-unit normalization ranges.
//!
//! One upstream quirk worth naming, because it looks like a bug and is:
//! `sigmoid`'s first branch reads `if sigmoid_type == ("tanh" or "logistic")`,
//! which Python evaluates as `== "tanh"` -- so asking for `"logistic"`, which
//! the docstring says is the same curve, falls through to the `else` and
//! raises. Nothing reaches it, because everything takes the default. There is
//! no behaviour to reproduce from a branch that cannot be entered, so this is
//! recorded here rather than as a `deviation-candidate`.

use std::f64::consts::PI;

/// The activation between the network's layers: `x / (1 + exp(-x))`.
///
/// Upstream's `beta` parameter is fixed at its default of 1 here, which is
/// what `neuralfoil.main`'s `np.swish(x)` passes.
pub(super) fn swish(x: f64) -> f64 {
    x / (1.0 + (-x).exp())
}

/// A soft maximum over `values`, in the units of `values` themselves.
///
/// `softness` has the same units as the inputs and means roughly "a
/// difference between them that would be physically significant"; below it
/// the result is a smooth average, above it a maximum.
///
/// The subtract-the-max-then-exponentiate form and its `-500` floor are
/// upstream's, and they are what keeps this finite when the inputs are far
/// apart relative to `softness` -- the wave-drag schedule takes a softmax of
/// two quantities scaled by 0.5, so an argument two hundred wide is ordinary
/// here.
pub(super) fn softmax(values: &[f64], softness: f64) -> f64 {
    let scaled: Vec<f64> = values.iter().map(|value| value / softness).collect();
    let mut largest = f64::NEG_INFINITY;
    for &value in &scaled {
        // `f64::max` is `fmax`, which is what upstream reduces with; the
        // `-500` floor below is `np.maximum`, which differs from `fmax` only
        // on NaN, and no input here is NaN.
        largest = largest.max(value);
    }
    let total: f64 = scaled
        .iter()
        .map(|&value| (value - largest).max(-500.0).exp())
        .sum();
    (largest + total.ln()) * softness
}

/// A soft minimum over `values` -- the negated softmax of the negated
/// inputs, exactly as upstream defines it.
pub(super) fn softmin(values: &[f64], softness: f64) -> f64 {
    let negated: Vec<f64> = values.iter().map(|value| -value).collect();
    -softmax(&negated, softness)
}

/// `sigmoid(x, sigmoid_type="tanh", normalization_range=(0, 1))`: a curve
/// running from 0 at minus infinity to 1 at plus infinity, through 0.5 at
/// zero.
pub(super) fn sigmoid(x: f64) -> f64 {
    // The general form is `tanh(x) * (max - min) / 2 + (max + min) / 2`,
    // which at (0, 1) is this. Written out rather than parameterized because
    // nothing here asks for another range.
    x.tanh() * 0.5 + 0.5
}

/// A smooth interpolation between two values on the strength of a switch.
///
/// A large positive `switch` returns `high`, a large negative one returns
/// `low`, and zero returns their mean. Note the argument order, which is
/// upstream's: the high value comes before the low one.
pub(super) fn blend(switch: f64, high: f64, low: f64) -> f64 {
    let weight = sigmoid(switch);
    high * weight + low * (1.0 - weight)
}

/// A patch joining two lines with a cosine, matching their values and slopes
/// at each end -- `cosine_hermite_patch(..., extrapolation="continue")`.
///
/// `alas-aero::neuralfoil`'s wave-drag schedule uses it to carry the drag
/// rise from drag divergence up to Mach 1.1, where the two ends are a
/// physical value and slope rather than free parameters.
///
/// The `extrapolation="linear"` branch, which clamps the blend parameter to
/// `[0, 1]`, is not translated: the one call site takes the default, and
/// outside `[x_a, x_b]` this is selected against rather than evaluated.
pub(super) fn cosine_hermite_patch(
    x: f64,
    x_a: f64,
    x_b: f64,
    f_a: f64,
    f_b: f64,
    dfdx_a: f64,
    dfdx_b: f64,
) -> f64 {
    let t = (x - x_a) / (x_b - x_a);
    let line_a = (x - x_a) * dfdx_a + f_a;
    let line_b = (x - x_b) * dfdx_b + f_b;
    let weight = 0.5 + 0.5 * (PI * t).cos();
    weight * line_a + (1.0 - weight) * line_b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_softmax_with_a_tiny_softness_approaches_the_sharp_maximum() {
        assert!((softmax(&[1.0, 3.0, 2.0], 1e-6) - 3.0).abs() < 1e-9);
        assert!((softmin(&[1.0, 3.0, 2.0], 1e-6) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_softmax_of_equal_values_exceeds_them_by_softness_times_log_of_the_count() {
        // Two equal arguments give `v + softness * ln(2)`, which is the
        // rounding-off this function exists for: the sharp maximum would
        // return `v` and have no gradient in either direction.
        let value = softmax(&[2.0, 2.0], 0.5);
        assert!((value - (2.0 + 0.5 * 2.0_f64.ln())).abs() < 1e-14);
    }

    #[test]
    fn a_softmax_is_never_below_the_sharp_maximum() {
        for softness in [1e-3, 0.01, 0.5, 1.0, 10.0] {
            assert!(softmax(&[-4.0, 7.5], softness) >= 7.5);
            assert!(softmin(&[-4.0, 7.5], softness) <= -4.0);
        }
    }

    #[test]
    fn a_softmax_stays_finite_when_its_arguments_are_far_apart() {
        // Without the subtract-the-max form this overflows: exp(1e4 / 0.5)
        // is not representable.
        assert!(softmax(&[1e4, -1e4], 0.5).is_finite());
        assert!((softmax(&[1e4, -1e4], 0.5) - 1e4).abs() < 1e-9);
    }

    #[test]
    fn blend_returns_the_mean_at_zero_and_each_end_far_from_it() {
        assert!((blend(0.0, 10.0, 20.0) - 15.0).abs() < 1e-15);
        assert!((blend(40.0, 10.0, 20.0) - 10.0).abs() < 1e-15);
        assert!((blend(-40.0, 10.0, 20.0) - 20.0).abs() < 1e-15);
    }

    #[test]
    fn the_sigmoid_spans_zero_to_one_through_a_half() {
        assert_eq!(sigmoid(0.0), 0.5);
        assert!(sigmoid(-40.0).abs() < 1e-15);
        assert!((sigmoid(40.0) - 1.0).abs() < 1e-15);
    }

    #[test]
    fn swish_is_flat_below_zero_and_linear_above_it() {
        assert_eq!(swish(0.0), 0.0);
        assert!(swish(-30.0).abs() < 1e-11);
        assert!((swish(30.0) - 30.0).abs() < 1e-11);
        // It dips below zero on the way, which is the whole difference from
        // a rectified linear unit and the reason it has a gradient there.
        assert!(swish(-1.5) < 0.0);
    }

    #[test]
    fn the_hermite_patch_matches_both_endpoints_in_value_and_slope() {
        let (x_a, x_b, f_a, f_b, slope_a, slope_b) = (0.7, 1.1, 0.02, 0.1, 0.1, -0.8);
        let at_a = cosine_hermite_patch(x_a, x_a, x_b, f_a, f_b, slope_a, slope_b);
        let at_b = cosine_hermite_patch(x_b, x_a, x_b, f_a, f_b, slope_a, slope_b);
        assert!((at_a - f_a).abs() < 1e-14);
        assert!((at_b - f_b).abs() < 1e-14);

        let step = 1e-6;
        let forward = cosine_hermite_patch(x_a + step, x_a, x_b, f_a, f_b, slope_a, slope_b);
        assert!(((forward - at_a) / step - slope_a).abs() < 1e-4);
        let backward = cosine_hermite_patch(x_b - step, x_a, x_b, f_a, f_b, slope_a, slope_b);
        assert!(((at_b - backward) / step - slope_b).abs() < 1e-4);
    }
}
