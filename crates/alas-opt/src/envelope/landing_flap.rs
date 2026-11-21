// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lift increment of the landing flaps at zero angle of attack, from the
//! configured device (family, chord extension `c'/c`, deflection, span):
//! Torenbeek, *Synthesis of Subsonic Airplane Design*, 1982, App. G.
//! Two-dimensional lift at zero angle of attack, eqs. G-3/G-4 (printed
//! p. 529), G-8 and G-9 (p. 532) with the flap efficiency of Figs. G-5 and
//! G-6; wing lift eq. G-21 (p. 537) with `K_c` of Fig. G-11 and `K_b` the
//! flapped-area share (p. 539, moderately tapered wings).
//!
//! The charts are digitised from the rendered pages, about +-0.01 on `eta`
//! and `K_c`; every input outside a chart's range is clamped to its edge.
//! Krueger panels carry no chord extension (see
//! `alas_config::presets::high_lift_chord`). The flap's pitching moment is not
//! built up here: `support::landing_pitching_moment` takes Torenbeek's lumped
//! `-0.3` times this lift increment, with the reason given there.

use alas_config::{AlasConfig, FlapSlotKind, HighLiftChordExtension};

/// Fig. G-5, Fowler (single-slotted) flap efficiency `eta_delta` against the
/// deflection, deg.
const FOWLER_DEG: [f64; 13] = [
    0.0, 2.0, 7.0, 12.0, 17.0, 22.0, 27.0, 32.0, 34.0, 36.0, 38.0, 42.0, 44.0,
];
const FOWLER_ETA: [f64; 13] = [
    0.883, 0.883, 0.879, 0.870, 0.857, 0.836, 0.806, 0.753, 0.72, 0.682, 0.634, 0.528, 0.473,
];
/// Fig. G-6, double-slotted (fixed-vane average) flap.
const DOUBLE_DEG: [f64; 13] = [
    3.0, 8.0, 13.0, 17.0, 23.0, 27.0, 33.0, 37.0, 43.0, 47.0, 53.0, 57.0, 60.0,
];
const DOUBLE_ETA: [f64; 13] = [
    0.747, 0.748, 0.747, 0.744, 0.735, 0.726, 0.704, 0.685, 0.64, 0.607, 0.565, 0.52, 0.495,
];
/// Fig. G-6, triple-slotted flap.
const TRIPLE_DEG: [f64; 13] = [
    3.0, 13.0, 17.0, 23.0, 27.0, 33.0, 37.0, 43.0, 47.0, 53.0, 57.0, 60.0, 62.0,
];
const TRIPLE_ETA: [f64; 13] = [
    0.812, 0.811, 0.810, 0.805, 0.80, 0.787, 0.776, 0.757, 0.746, 0.716, 0.692, 0.665, 0.655,
];

/// Fig. G-11, `K_c` against `(alpha_delta)_cl` at `A = 4` and `A = 10`.
const KC_ALPHA: [f64; 7] = [0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
const KC_A4: [f64; 7] = [1.30, 1.20, 1.145, 1.10, 1.07, 1.05, 1.03];
const KC_A10: [f64; 7] = [1.13, 1.09, 1.065, 1.05, 1.035, 1.025, 1.015];

/// Linear interpolation in a table, clamped to its ends.
fn interp(xs: &[f64], ys: &[f64], x: f64) -> f64 {
    let last = xs.len() - 1;
    if x <= xs[0] {
        return ys[0];
    }
    if x >= xs[last] {
        return ys[last];
    }
    let upper = xs.iter().position(|&v| v >= x).unwrap_or(last).max(1);
    let t = (x - xs[upper - 1]) / (xs[upper] - xs[upper - 1]);
    ys[upper - 1] + t * (ys[upper] - ys[upper - 1])
}

/// Thin-airfoil effectiveness `alpha_delta = 1 - (theta - sin theta)/pi` of a
/// flap of chord ratio `cf_c`, `theta = acos(2 c_f/c - 1)` (eqs. G-3/G-4).
fn alpha_delta(cf_c: f64) -> f64 {
    let theta = (2.0 * cf_c.clamp(0.0, 1.0) - 1.0).acos();
    1.0 - (theta - theta.sin()) / std::f64::consts::PI
}

/// Fig. G-5/G-6 flap efficiency of the configured family at `deflection_deg`.
fn flap_efficiency(kind: FlapSlotKind, deflection_deg: f64) -> f64 {
    match kind {
        FlapSlotKind::FowlerSingle => interp(&FOWLER_DEG, &FOWLER_ETA, deflection_deg),
        FlapSlotKind::Double => interp(&DOUBLE_DEG, &DOUBLE_ETA, deflection_deg),
        FlapSlotKind::Triple => interp(&TRIPLE_DEG, &TRIPLE_ETA, deflection_deg),
    }
}

/// Fig. G-11 `K_c` at aspect ratio `aspect_ratio` (linear in `1/A` between
/// the drawn `A = 4` and `A = 10` curves, clamped).
fn k_c(aspect_ratio: f64, alpha_delta_cl: f64) -> f64 {
    let at_4 = interp(&KC_ALPHA, &KC_A4, alpha_delta_cl);
    let at_10 = interp(&KC_ALPHA, &KC_A10, alpha_delta_cl);
    let t = ((0.25 - 1.0 / aspect_ratio.max(1.0)) / (0.25 - 0.1)).clamp(0.0, 1.0);
    at_4 + t * (at_10 - at_4)
}

/// Straight-tapered wing inputs of [`flap_zero_alpha_lift`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct WingInputs {
    /// Aspect ratio of the reference wing.
    pub(super) aspect_ratio: f64,
    /// Taper ratio.
    pub(super) taper_ratio: f64,
    /// Wing lift-curve slope, per radian.
    pub(super) lift_curve_slope: f64,
    /// Section lift at zero angle of attack of the clean wing (camber only).
    pub(super) section_zero_alpha_lift: f64,
}

/// The landing flap's lift increment at zero angle of attack, `Delta_f C_L0`
/// (eq. G-21), on the wing's reference area; `None` when not finite.
pub(super) fn flap_zero_alpha_lift(
    wing: &WingInputs,
    devices: &HighLiftChordExtension,
    config: &AlasConfig,
) -> Option<f64> {
    let controls = &config.control_surfaces;
    let two_pi = 2.0 * std::f64::consts::PI;
    let taper = wing.taper_ratio.clamp(0.0, 1.0);
    // Trailing-edge flap, two-dimensional (eqs. G-3, G-4, G-8, G-9).
    let chord_ratio = 1.0 + devices.flap.max(0.0);
    let deflection = devices.flap_landing_deg.max(0.0).to_radians();
    let cf_over_extended = controls.flap_chord_fraction / chord_ratio;
    let efficiency = flap_efficiency(devices.flap_kind, devices.flap_landing_deg);
    let extended_lift = two_pi * efficiency * alpha_delta(cf_over_extended) * deflection;
    let section_lift =
        extended_lift * chord_ratio + wing.section_zero_alpha_lift * (chord_ratio - 1.0);
    // Wing lift (eqs. G-21, G-23, Fig. G-11, K_b = S_wf/S).
    let flapped_share = super::tail_stall::strip_area_fraction(
        taper,
        controls.flap_span_start_frac,
        controls.flap_span_end_frac,
    );
    let alpha_delta_cl = if deflection > 0.0 {
        section_lift / (two_pi * deflection)
    } else {
        0.0
    };
    let lift = section_lift * wing.lift_curve_slope / two_pi
        * k_c(wing.aspect_ratio, alpha_delta_cl)
        * flapped_share;
    lift.is_finite().then_some(lift)
}

// A test asserts on values it built here, so a failed expect is the assertion failing.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn wing() -> WingInputs {
        WingInputs {
            aspect_ratio: 9.0,
            taper_ratio: 0.25,
            lift_curve_slope: 5.0,
            section_zero_alpha_lift: 0.2,
        }
    }

    fn lift_with(edit: impl FnOnce(&mut AlasConfig)) -> f64 {
        let mut config =
            AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).expect("preset");
        edit(&mut config);
        flap_zero_alpha_lift(
            &wing(),
            &config.performance.high_lift_chord_extension,
            &config,
        )
        .expect("finite")
    }

    #[test]
    fn the_flap_lift_grows_with_the_deflection_and_the_flapped_span() {
        let at = |deg: f64| {
            lift_with(|c| c.performance.high_lift_chord_extension.flap_landing_deg = deg)
        };
        assert!(at(0.0) >= 0.0 && at(20.0) > at(10.0) && at(40.0) > at(20.0));
        let full = lift_with(|_| {});
        let short = lift_with(|c| {
            let controls = &mut c.control_surfaces;
            controls.flap_span_end_frac = controls.flap_span_start_frac
                + 0.5 * (controls.flap_span_end_frac - controls.flap_span_start_frac);
        });
        assert!(short > 0.0 && short < full, "{short} {full}");
    }

    #[test]
    fn an_unextended_undeflected_flap_adds_no_lift() {
        let lift = lift_with(|c| {
            c.performance.high_lift_chord_extension.flap = 0.0;
            c.performance.high_lift_chord_extension.flap_landing_deg = 0.0;
        });
        assert!(lift.abs() < 1e-12, "{lift}");
    }
}
