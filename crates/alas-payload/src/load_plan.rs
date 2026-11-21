// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Load planning: where the payload centre of gravity should sit so every
//! loaded state falls inside its CG window.
//!
//! A real flight is not loaded at whatever centroid the cabin and holds happen
//! to give. The dispatcher builds a load plan (passenger zones, cargo and bag
//! compartments, fuel by tank sequence) that puts the zero-fuel and take-off
//! CG inside the certified envelope, at the operator's preferred trim point
//! when one is declared. This is the standard weight-and-balance practice of
//! IATA, Airport Handling Manual (AHM) 500 series (weight and balance
//! control, load planning and the balance/trim chart), and of the
//! manufacturer's Weight and Balance Manual: the masses are fixed by the
//! flight, and only their placement is chosen. Payload mass and fuel mass
//! are never changed here.
//!
//! # Method
//!
//! Every loaded state `i` carries the fixed mass `M_i`, the moment of
//! everything except the payload `c_i` (operating empty mass and, for a fuelled
//! state, the fuel on its declared tank sequence) and the payload mass `P`.
//! Its CG is `x_i = (c_i + P x_p) / M_i`, linear in the one placement variable
//! `x_p`, the payload centroid that cabin zones and hold compartments
//! determine. Each state has a CG window `[lo_i, hi_i]` (the model's forward
//! and aft physical limits at that state, which include the minimum nose-gear
//! load) and a target `t_i`, the window midpoint unless the operator declares
//! another. [`steer_payload_cg`] returns the `x_p` that minimises the largest
//! normalised deviation `w_i |x_i - t_i| / h_i` (`h_i` the window half width):
//! the placement that puts the CG of every state nearest its target in the
//! minimax sense, so one state is never sacrificed while another is centred.
//! The deviation is convex and piecewise linear in `x_p`, so the minimum is
//! exact. [`evaluate`] then reports, for the placement the hold and cabin
//! solvers actually achieved, how far each state lies outside its window;
//! a plan that cannot bring every state inside is reported, never hidden.
//!
//! All stations in metres aft of the nose, masses in kilograms.

/// One loaded state the plan must keep inside its CG window.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanState {
    /// Report label of the state.
    pub label: &'static str,
    /// Total mass, kg, payload included.
    pub mass_kg: f64,
    /// Moment about the datum of everything except the payload, kg m.
    pub other_moment_kg_m: f64,
    /// Forward and aft CG limits, m; `None` when the state has no usable
    /// window (not evaluated, or an inverted envelope).
    pub window_x_m: Option<(f64, f64)>,
    /// The CG the plan aims for, m.
    pub target_x_m: f64,
    /// Relative importance of the state; the take-off and zero-fuel states
    /// carry 1, intermediate fuel states less.
    pub weight: f64,
}

/// A state's CG after a placement, against its window.
#[derive(Debug, Clone, PartialEq)]
pub struct StateFit {
    /// Report label of the state.
    pub label: &'static str,
    /// The state's CG, m.
    pub cg_x_m: f64,
    /// Distance outside the window, m: negative forward of the forward limit,
    /// positive aft of the aft limit, zero inside or when no window exists.
    pub gap_m: f64,
}

impl PlanState {
    fn cg_x_m(&self, payload_kg: f64, payload_cg_x_m: f64) -> f64 {
        (self.other_moment_kg_m + payload_kg * payload_cg_x_m) / self.mass_kg
    }

    /// Weighted, window-normalised distance of the CG from the target.
    fn deviation(&self, payload_kg: f64, payload_cg_x_m: f64) -> f64 {
        let half_width_m = self
            .window_x_m
            .map_or(1.0, |(lo, hi)| (0.5 * (hi - lo)).max(1.0e-6));
        self.weight * (self.cg_x_m(payload_kg, payload_cg_x_m) - self.target_x_m).abs()
            / half_width_m
    }
}

/// The window midpoint, m; the target when the operator declares none.
#[must_use]
pub fn window_midpoint_m(window_x_m: (f64, f64)) -> f64 {
    0.5 * (window_x_m.0 + window_x_m.1)
}

/// A usable window: finite, forward limit strictly forward of the aft one.
#[must_use]
pub fn usable_window(forward_x_m: f64, aft_x_m: f64) -> Option<(f64, f64)> {
    (forward_x_m.is_finite() && aft_x_m.is_finite() && forward_x_m < aft_x_m)
        .then_some((forward_x_m, aft_x_m))
}

/// The payload centroid, m, that minimises the largest weighted normalised
/// deviation of the states from their targets. `None` for a non-positive or
/// non-finite payload, no states with a window, or non-finite inputs.
#[must_use]
pub fn steer_payload_cg(payload_kg: f64, states: &[PlanState]) -> Option<f64> {
    let active: Vec<&PlanState> = states
        .iter()
        .filter(|state| {
            state.window_x_m.is_some()
                && state.mass_kg > 0.0
                && state.weight > 0.0
                && state.mass_kg.is_finite()
                && state.other_moment_kg_m.is_finite()
                && state.target_x_m.is_finite()
        })
        .collect();
    if !payload_kg.is_finite() || payload_kg <= 0.0 || active.is_empty() {
        return None;
    }
    // Payload centroid that puts a state exactly on `x`.
    let centroid_for =
        |state: &PlanState, x: f64| (x * state.mass_kg - state.other_moment_kg_m) / payload_kg;
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for state in &active {
        let half = state
            .window_x_m
            .map_or(1.0, |(a, b)| (0.5 * (b - a)).max(1.0e-6));
        for side in [-50.0, 50.0] {
            let x_p = centroid_for(state, state.target_x_m + side * half);
            lo = lo.min(x_p);
            hi = hi.max(x_p);
        }
    }
    let worst = |x_p: f64| {
        active
            .iter()
            .map(|state| state.deviation(payload_kg, x_p))
            .fold(0.0_f64, f64::max)
    };
    // The objective is a maximum of absolute values of affine functions:
    // convex, so a ternary search converges to its global minimum.
    for _ in 0..200 {
        let a = lo + (hi - lo) / 3.0;
        let b = hi - (hi - lo) / 3.0;
        if worst(a) <= worst(b) {
            hi = b;
        } else {
            lo = a;
        }
    }
    let x_p = 0.5 * (lo + hi);
    x_p.is_finite().then_some(x_p)
}

/// Each state's CG and distance outside its window for the payload placed at
/// `payload_cg_x_m`.
#[must_use]
pub fn evaluate(payload_kg: f64, payload_cg_x_m: f64, states: &[PlanState]) -> Vec<StateFit> {
    states
        .iter()
        .map(|state| {
            let cg_x_m = state.cg_x_m(payload_kg, payload_cg_x_m);
            let gap_m = state.window_x_m.map_or(0.0, |(lo, hi)| {
                if cg_x_m < lo {
                    cg_x_m - lo
                } else if cg_x_m > hi {
                    cg_x_m - hi
                } else {
                    0.0
                }
            });
            StateFit {
                label: state.label,
                cg_x_m,
                gap_m,
            }
        })
        .collect()
}

/// Whether every state with a window lies inside it.
#[must_use]
pub fn all_inside(fits: &[StateFit]) -> bool {
    fits.iter().all(|fit| fit.gap_m == 0.0)
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn state(
        label: &'static str,
        oew: f64,
        x_oew: f64,
        p: f64,
        fuel: f64,
        x_fuel: f64,
        window: (f64, f64),
    ) -> PlanState {
        PlanState {
            label,
            mass_kg: oew + p + fuel,
            other_moment_kg_m: oew * x_oew + fuel * x_fuel,
            window_x_m: Some(window),
            target_x_m: window_midpoint_m(window),
            weight: 1.0,
        }
    }

    #[test]
    fn a_single_state_is_placed_exactly_on_its_target_and_masses_are_untouched() {
        let s = state(
            "tow",
            180_000.0,
            30.0,
            60_000.0,
            120_000.0,
            36.0,
            (28.0, 30.0),
        );
        let x_p = steer_payload_cg(60_000.0, std::slice::from_ref(&s)).unwrap();
        let fit = evaluate(60_000.0, x_p, std::slice::from_ref(&s));
        assert!((fit[0].cg_x_m - 29.0).abs() < 1e-6);
        assert!(all_inside(&fit));
    }

    #[test]
    fn two_states_with_nested_windows_both_end_inside() {
        let zfw = state("zfw", 180_000.0, 30.0, 60_000.0, 0.0, 0.0, (27.0, 32.0));
        let tow = state(
            "tow",
            180_000.0,
            30.0,
            60_000.0,
            120_000.0,
            36.0,
            (28.0, 30.0),
        );
        let states = [zfw, tow];
        let x_p = steer_payload_cg(60_000.0, &states).unwrap();
        assert!(all_inside(&evaluate(60_000.0, x_p, &states)));
    }

    #[test]
    fn unreachable_windows_are_reported_not_hidden() {
        // The two windows cannot both hold: ZFW wants the payload far aft,
        // TOW (with aft fuel) far forward.
        let zfw = state("zfw", 100_000.0, 30.0, 20_000.0, 0.0, 0.0, (34.0, 35.0));
        let tow = state(
            "tow",
            100_000.0,
            30.0,
            20_000.0,
            80_000.0,
            40.0,
            (24.0, 25.0),
        );
        let states = [zfw, tow];
        let x_p = steer_payload_cg(20_000.0, &states).unwrap();
        assert!(!all_inside(&evaluate(20_000.0, x_p, &states)));
    }

    #[test]
    fn degenerate_inputs_have_no_plan() {
        let s = state("tow", 1.0, 1.0, 1.0, 1.0, 1.0, (0.0, 1.0));
        assert!(steer_payload_cg(0.0, std::slice::from_ref(&s)).is_none());
        assert!(steer_payload_cg(f64::NAN, std::slice::from_ref(&s)).is_none());
        assert!(steer_payload_cg(1.0, &[]).is_none());
        assert!(usable_window(2.0, 1.0).is_none());
        assert!(usable_window(f64::NAN, 1.0).is_none());
    }

    #[test]
    fn the_planned_centroid_is_independent_of_the_datum() {
        let a = state(
            "tow",
            180_000.0,
            30.0,
            60_000.0,
            120_000.0,
            36.0,
            (28.0, 30.0),
        );
        let shift = 100.0;
        let b = state(
            "tow",
            180_000.0,
            30.0 + shift,
            60_000.0,
            120_000.0,
            36.0 + shift,
            (28.0 + shift, 30.0 + shift),
        );
        let xa = steer_payload_cg(60_000.0, std::slice::from_ref(&a)).unwrap();
        let xb = steer_payload_cg(60_000.0, std::slice::from_ref(&b)).unwrap();
        assert!((xb - xa - shift).abs() < 1e-6);
    }
}
