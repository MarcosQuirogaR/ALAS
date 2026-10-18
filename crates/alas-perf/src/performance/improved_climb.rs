// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Improved-climb selection of the take-off safety speed.
//!
//! CS 25.107(b) and (c) set `V2` no lower than `V2MIN` (1.13 VSR, 1.10 VMC) and
//! no lower than `VR`; the operator may take off faster. At `V2` the aircraft
//! flies below its minimum-drag speed, so a higher speed lowers the induced
//! drag faster than the lapse lowers the thrust and improves the one-engine-
//! inoperative gradient of CS 25.121(b); the price is a later rotation and a
//! longer take-off field:
//!
//! - Jenkinson, *Civil Jet Aircraft Design*, Ch. 10, second-segment
//!   discussion, item 4: "Taking off at speeds higher than V2 (overspeeding).
//!   ... the climb gradient can be improved ... by virtue of the reduced drag
//!   ... a reduction in thrust due to the increased momentum drag ... will, in
//!   general, not be sufficient to nullify the drag improvement."
//! - Torenbeek, *Advanced Aircraft Design* (2013), Sec. 9.3.2: "the best
//!   speed to achieve a high climb gradient V_X is higher than 1.20 V_S [so]
//!   the aircraft improves its climb performance for V2 > 1.20 V_S. This
//!   overspeed situation is achieved by delaying the rotation. In the design
//!   stage, overspeed is not an option since the TOFL requirement is usually a
//!   dominant requirement." The field-length charge here is that dominance
//!   made explicit rather than ignored.
//!
//! The selection returns the lowest `V2 / VSR` at or above the schedule value
//! at which the required sea-level-static `T/W` does not exceed the installed
//! one, searched up to [`MAX_IMPROVED_V2_OVER_VSR`]. When even the best-climb
//! speed misses, it returns that best speed with `meets = false`, so the
//! shortfall finding stays. The take-off field length is charged through the
//! lift-off speed: the distance scales with the square of `V2 / V2_schedule`
//! (the Raymer correlation's lift-off speed enters through `CLmax_TO`).

/// Highest `V2 / VSR` the search considers. Beyond roughly 1.5 the induced-
/// drag gain is exhausted for every transport polar here (best-climb speeds
/// 1.3 to 1.54 VSR) and the field-length charge grows with the square.
pub const MAX_IMPROVED_V2_OVER_VSR: f64 = 1.50;

/// The outcome of one selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImprovedClimbSelection {
    /// `V2 / VSR` of the schedule (the CS 25.107 rule at the declared factor).
    pub schedule_v2_over_vsr: f64,
    /// Selected `V2 / VSR` (equal to the schedule when nothing was raised).
    pub v2_over_vsr: f64,
    /// Required installed SLS `T/W` at the schedule speed.
    pub required_at_schedule: f64,
    /// Required installed SLS `T/W` at the selected speed.
    pub required: f64,
    /// Whether the selected speed is above the schedule.
    pub improved: bool,
    /// Whether the installed `T/W` meets the requirement at the selected speed.
    pub meets: bool,
}

impl ImprovedClimbSelection {
    /// Factor on the take-off field length: `(V2 / V2_schedule)^2`.
    pub fn field_length_factor(&self) -> f64 {
        let ratio = self.v2_over_vsr / self.schedule_v2_over_vsr;
        ratio * ratio
    }
}

/// Select the improved-climb `V2 / VSR`.
///
/// `required(x)` is the required installed SLS `T/W` at `V2 / VSR = x` (thrust
/// ratio, drag and lift coefficient all re-evaluated at that speed); `None`
/// when it cannot be evaluated. `None` is returned when the schedule point
/// itself cannot be evaluated or the inputs are non-physical.
pub fn select_improved_v2(
    schedule_v2_over_vsr: f64,
    installed_tw: f64,
    required: &dyn Fn(f64) -> Option<f64>,
) -> Option<ImprovedClimbSelection> {
    if !(schedule_v2_over_vsr.is_finite() && schedule_v2_over_vsr > 1.0)
        || !(installed_tw.is_finite() && installed_tw > 0.0)
    {
        return None;
    }
    let at_schedule = required(schedule_v2_over_vsr).filter(|v| v.is_finite())?;
    let selection = |x: f64, required: f64, improved: bool| ImprovedClimbSelection {
        schedule_v2_over_vsr,
        v2_over_vsr: x,
        required_at_schedule: at_schedule,
        required,
        improved,
        meets: required <= installed_tw,
    };
    if at_schedule <= installed_tw || schedule_v2_over_vsr >= MAX_IMPROVED_V2_OVER_VSR {
        return Some(selection(schedule_v2_over_vsr, at_schedule, false));
    }
    let eval = |x: f64| {
        required(x)
            .filter(|v| v.is_finite())
            .unwrap_or(f64::INFINITY)
    };
    // Coarse scan for the best-climb speed, then golden-section refinement.
    const STEPS: usize = 28;
    let width = MAX_IMPROVED_V2_OVER_VSR - schedule_v2_over_vsr;
    let grid = |i: usize| schedule_v2_over_vsr + width * i as f64 / STEPS as f64;
    let (mut best_i, mut best) = (0, at_schedule);
    for i in 1..=STEPS {
        let value = eval(grid(i));
        if value < best {
            (best_i, best) = (i, value);
        }
    }
    let (mut lo, mut hi) = (
        grid(best_i.saturating_sub(1)),
        grid((best_i + 1).min(STEPS)),
    );
    let phi = 0.618_033_988_749_894_9;
    for _ in 0..16 {
        let (a, b) = (hi - phi * (hi - lo), lo + phi * (hi - lo));
        if eval(a) < eval(b) {
            hi = b;
        } else {
            lo = a;
        }
    }
    let refined = 0.5 * (lo + hi);
    let (x_best, best) = if eval(refined) <= best {
        (refined, eval(refined))
    } else {
        (grid(best_i), best)
    };
    if best >= at_schedule {
        return Some(selection(schedule_v2_over_vsr, at_schedule, false));
    }
    if best > installed_tw {
        return Some(selection(x_best, best, true));
    }
    // The lowest speed that meets: bisection between the failing schedule
    // speed and the passing best-climb speed (the requirement falls with speed
    // on that interval).
    let (mut fail, mut pass) = (schedule_v2_over_vsr, x_best);
    for _ in 0..40 {
        let mid = 0.5 * (fail + pass);
        if eval(mid) <= installed_tw {
            pass = mid;
        } else {
            fail = mid;
        }
    }
    Some(selection(pass, eval(pass), true))
}

#[cfg(test)]
mod tests {
    // A test asserts on values it built, so a failed unwrap is the assertion failing.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Twin: required = 2 (gamma + cd_fixed / CL + k CL) / r with CL = CLmax / x^2
    /// and a constant thrust ratio. B787-9 class numbers.
    fn required_twin(x: f64) -> Option<f64> {
        let (cl_max, gamma, cd_fixed, k, r) = (2.10, 0.024, 0.0428, 0.0359, 0.742);
        let cl = cl_max / (x * x);
        Some(2.0 * (gamma + cd_fixed / cl + k * cl) / r)
    }

    #[test]
    fn a_schedule_that_meets_is_kept() {
        let s = select_improved_v2(1.20, 0.35, &required_twin).unwrap();
        assert!(!s.improved && s.meets && s.v2_over_vsr == 1.20);
        assert_eq!(s.field_length_factor(), 1.0);
    }

    /// The minimum of required(x) is at CL_opt = sqrt(cd_fixed / k) = 1.0919, i.e.
    /// x = sqrt(2.10 / 1.0919) = 1.3868, where required = 2 (0.024 + 0.0784) / 0.742 =
    /// 0.2760. With T/W 0.2800 the schedule (required 0.2849) fails; the
    /// lowest passing speed solves required(x) = 0.2800.
    #[test]
    fn the_lowest_meeting_speed_solves_the_requirement_and_charges_the_field() {
        let installed = 0.2800;
        assert!((required_twin(1.20).unwrap() - 0.2849).abs() < 1e-3);
        let s = select_improved_v2(1.20, installed, &required_twin).unwrap();
        assert!(s.improved && s.meets);
        assert!(s.v2_over_vsr > 1.20 && s.v2_over_vsr < 1.3868, "{s:?}");
        assert!((required_twin(s.v2_over_vsr).unwrap() - installed).abs() < 1e-9);
        // Slightly slower fails: it is the lowest.
        assert!(required_twin(s.v2_over_vsr - 1e-3).unwrap() > installed);
        let expected = (s.v2_over_vsr / 1.20).powi(2);
        assert!((s.field_length_factor() - expected).abs() < 1e-15);
        assert!(s.field_length_factor() > 1.0);
    }

    #[test]
    fn when_even_the_best_climb_speed_misses_it_is_reported_as_not_meeting() {
        let s = select_improved_v2(1.20, 0.2700, &required_twin).unwrap();
        assert!(s.improved && !s.meets);
        let cl_opt = (0.0428_f64 / 0.0359).sqrt();
        let x_opt = (2.10 / cl_opt).sqrt();
        assert!((s.v2_over_vsr - x_opt).abs() < 2e-3, "{s:?} vs {x_opt}");
        assert!((s.required - 0.2760).abs() < 1e-3, "{s:?}");
    }

    #[test]
    fn unevaluable_or_non_physical_inputs_give_no_selection() {
        assert!(select_improved_v2(1.20, 0.3, &|_| None).is_none());
        assert!(select_improved_v2(f64::NAN, 0.3, &required_twin).is_none());
        assert!(select_improved_v2(1.20, 0.0, &required_twin).is_none());
    }
}
