// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Admissible extrapolation of the takeoff-mass fixed point.

use crate::mdo::types::CandidateFailure;

/// Consecutive passes whose dispatch stalled at the evaluable edge, the takeoff mass
/// moving by at most [`EDGE_STALL_MASS_FRACTION`] of itself, after which the sizing loop
/// stops (engineering choice: the third pass repeats the first two's status; the cycle of
/// the AVE and A380 wide-body probes was period 3 and never closed in 30 passes).
pub(super) const EDGE_STALL_PASSES: u32 = 3;

/// Largest change of the takeoff mass between passes, as a fraction of the pass mass, that
/// still counts as the same stalled edge (the mass-model feedback moved the edge 0.04 to
/// 0.16 % per pass in the probes).
pub(super) const EDGE_STALL_MASS_FRACTION: f64 = 0.01;

/// Width of a residual bracket, as a fraction of the sizing tolerance, at or
/// below which a sign change whose two ends both still miss their dispatch by
/// more than the tolerance is a jump of the map (engineering choice). On a
/// continuous map of slope `s` the two end residuals of a bracket of width
/// `w` sum to at most `|s - 1| w`, so both can exceed the tolerance at this
/// width only if `|s - 1|` exceeds `2 / JUMP_RESOLUTION_FRACTION` = 16; a
/// map less steep settles inside the bracket first, and the bisection to
/// this width costs a jump three passes more than stopping at the tolerance.
pub(super) const JUMP_RESOLUTION_FRACTION: f64 = 0.125;

/// Aitken delta-squared extrapolation of the fixed-point iterates
/// `x0 -> x1 -> x2`, or `None` when the denominator vanishes or the
/// estimate leaves `(0, ceiling]`. Iterates and ceiling are masses, kg.
pub(super) fn aitken(x0: f64, x1: f64, x2: f64, ceiling: f64) -> Option<f64> {
    let denominator = (x2 - x1) - (x1 - x0);
    if denominator.abs() < 1e-9 {
        return None;
    }
    let estimate = x2 - (x2 - x1) * (x2 - x1) / denominator;
    (estimate.is_finite() && estimate > 0.0 && estimate <= ceiling).then_some(estimate)
}

/// The closest pair of passes, on the current trim, whose takeoff-mass residual
/// `next - tow` changes sign: the fixed point, or a jump of the dispatch map,
/// lies between them. Masses, kg.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ResidualBracket {
    /// `(tow, next)` of the heaviest pass whose dispatch asked for more mass.
    rising: Option<(f64, f64)>,
    /// `(tow, next)` of the lightest pass whose dispatch asked for less mass.
    falling: Option<(f64, f64)>,
    /// Re-trims the recorded passes were flown after; a re-trim changes the
    /// map, so a bracket from an earlier trim is dropped.
    retrims: usize,
}

impl ResidualBracket {
    /// Record the pass at `tow` whose dispatch closed at `next`, flown after
    /// `retrims` re-trims.
    pub(super) fn record(&mut self, tow: f64, next: f64, retrims: usize) {
        if retrims != self.retrims {
            *self = Self {
                retrims,
                ..Self::default()
            };
        }
        let point = Some((tow, next));
        if next > tow {
            if self.falling.is_none_or(|(t, _)| tow < t) && self.rising.is_none_or(|(t, _)| tow > t)
            {
                self.rising = point;
            }
        } else if next < tow
            && self.rising.is_none_or(|(t, _)| tow > t)
            && self.falling.is_none_or(|(t, _)| tow < t)
        {
            self.falling = point;
        }
    }

    /// `candidate` while it lies strictly inside the bracket, else the
    /// bracket's midpoint: a cycle across a jump becomes a bisection of it.
    pub(super) fn safeguard(&self, candidate: f64) -> f64 {
        match (self.rising, self.falling) {
            (Some((lo, _)), Some((hi, _))) if !(lo < candidate && candidate < hi) => {
                0.5 * (lo + hi)
            }
            _ => candidate,
        }
    }

    /// The jump the bracket has narrowed onto: both ends within
    /// [`JUMP_RESOLUTION_FRACTION`] of `tolerance_kg` of each other, while
    /// each end's dispatch still disagrees with its own mass by more than
    /// `tolerance_kg`. A narrow bracket whose ends agree with their dispatch
    /// is the fixed point of a continuous map, not a jump.
    pub(super) fn jump(&self, tolerance_kg: f64) -> Option<SizingMapJump> {
        let ((lower_kg, lower_next_kg), (upper_kg, upper_next_kg)) =
            self.rising.zip(self.falling)?;
        let narrow = upper_kg - lower_kg <= JUMP_RESOLUTION_FRACTION * tolerance_kg;
        let both_miss =
            lower_next_kg - lower_kg > tolerance_kg && upper_kg - upper_next_kg > tolerance_kg;
        (narrow && both_miss).then_some(SizingMapJump {
            lower_kg,
            lower_next_kg,
            upper_kg,
            upper_next_kg,
            resolved_by_held_plan: false,
            survives_held_plan: false,
        })
    }
}

/// What [`super::converge`] flies after a pass that did not settle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum NextPass {
    /// The next pass, at this takeoff mass, kg.
    Fly(f64),
    /// None: a jump survived the held plan.
    Stop,
}

/// The outer takeoff-mass iteration of [`super::converge`] between passes:
/// Picard with an Aitken step every third iterate, safeguarded by the
/// residual bracket, and the plan hold that a jump of the map triggers.
/// [`super::converge`] flies each pass, decides whether it settled, and asks
/// this for the next one, so the iteration can be driven by any dispatch map.
#[derive(Debug, Clone)]
pub(super) struct TakeoffMassIteration {
    tolerance_kg: f64,
    aitken_ceiling_kg: f64,
    iterates: Vec<f64>,
    bracket: ResidualBracket,
    /// The mass the plan is held at after a jump: the heavier side's, whose
    /// plan the lighter aircraft can also fly, while the lighter side's plan
    /// is not chosen at the mass it closes on.
    held_plan_kg: Option<f64>,
    jump: Option<SizingMapJump>,
}

impl TakeoffMassIteration {
    /// An iteration from the first pass at `seed_kg`, to `tolerance_kg`, whose
    /// extrapolations stay at or below `aitken_ceiling_kg`.
    pub(super) fn new(seed_kg: f64, tolerance_kg: f64, aitken_ceiling_kg: f64) -> Self {
        Self {
            tolerance_kg,
            aitken_ceiling_kg,
            iterates: vec![seed_kg],
            bracket: ResidualBracket::default(),
            held_plan_kg: None,
            jump: None,
        }
    }

    /// The mass a pass at `tow_kg` freezes its trip plan at: the held mass
    /// after a jump, else its own.
    pub(super) fn plan_mass_kg(&self, tow_kg: f64) -> f64 {
        self.held_plan_kg.unwrap_or(tow_kg)
    }

    /// The next pass after the unsettled pass at `tow_kg` whose dispatch
    /// closed at `next_kg`, flown after `retrims` re-trims.
    ///
    /// A jump of the map holds the plan at the heavier side's mass and
    /// restarts from that side's dispatch; a jump met again on the held plan
    /// is not the plan's, and stops the closure.
    pub(super) fn advance(&mut self, tow_kg: f64, next_kg: f64, retrims: usize) -> NextPass {
        self.bracket.record(tow_kg, next_kg, retrims);
        if let Some(jump) = self.bracket.jump(self.tolerance_kg) {
            if self.held_plan_kg.is_some() {
                self.jump = Some(SizingMapJump {
                    survives_held_plan: true,
                    ..jump
                });
                return NextPass::Stop;
            }
            tracing::info!(
                lower_kg = jump.lower_kg,
                upper_kg = jump.upper_kg,
                "sizing map jumps across its fixed point; holding the heavier side's mission plan"
            );
            self.jump = Some(jump);
            self.held_plan_kg = Some(jump.upper_kg);
            self.bracket = ResidualBracket::default();
            self.iterates = vec![jump.upper_next_kg];
            return NextPass::Fly(jump.upper_next_kg);
        }
        self.iterates.push(next_kg);
        // Every third iterate is extrapolated from the two plain passes
        // before it; the plain iterate stands when the extrapolation is not
        // admissible.
        let candidate = match self.iterates.as_slice() {
            [.., x0, x1, x2] if self.iterates.len() % 3 == 0 => {
                aitken(*x0, *x1, *x2, self.aitken_ceiling_kg).unwrap_or(next_kg)
            }
            _ => next_kg,
        };
        NextPass::Fly(self.bracket.safeguard(candidate))
    }

    /// The jump the closure met, if any, once the closure ended `settled` or
    /// not: resolved by the held plan only when the closure settled on it.
    pub(super) fn finish(self, settled: bool) -> Option<SizingMapJump> {
        let held = self.held_plan_kg.is_some();
        self.jump.map(|jump| SizingMapJump {
            resolved_by_held_plan: held && settled && !jump.survives_held_plan,
            ..jump
        })
    }
}

/// A discontinuity of the takeoff-mass map with no fixed point on either
/// side: at `lower_kg` the dispatch closes heavier (`lower_next_kg`), at
/// `upper_kg`, within the sizing tolerance above, lighter (`upper_next_kg`).
/// Masses, kg.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct SizingMapJump {
    /// Heaviest pass mass whose dispatch closed heavier than it.
    pub lower_kg: f64,
    /// Where that dispatch closed.
    pub lower_next_kg: f64,
    /// Lightest pass mass whose dispatch closed lighter than it.
    pub upper_kg: f64,
    /// Where that dispatch closed.
    pub upper_next_kg: f64,
    /// Whether the closure then settled on the mission plan of the heavier
    /// side, held for every later pass.
    pub resolved_by_held_plan: bool,
    /// Whether the jump was met again on the held plan, which stopped the
    /// closure unsettled: no takeoff mass is its own dispatch.
    pub survives_held_plan: bool,
}

impl SizingMapJump {
    /// The jump and what the closure made of it, in one sentence.
    pub fn describe(&self) -> String {
        let outcome = if self.resolved_by_held_plan {
            "the closure settled on the mission plan of the heavier side"
        } else if self.survives_held_plan {
            "no takeoff mass is its own dispatch, even on the mission plan of the heavier side, so the closure stopped"
        } else {
            "the closure did not settle on the mission plan of the heavier side"
        };
        format!(
            "the dispatch closes at {:.0} kg from {:.0} kg but at {:.0} kg from {:.0} kg; {outcome}",
            self.lower_next_kg, self.lower_kg, self.upper_next_kg, self.upper_kg
        )
    }
}

/// A failure with the `mass_coordinates` reason, for the one arm of
/// [`converge`] that keeps it panic-free.
///
/// This is deliberately **not** the seam for a station-placement failure.
/// Every mass and coordinate result this loop consumes comes from
/// `build::mass_analysis_with_structural_feedback`, which already classifies
/// a missing main-gear station as `main_gear_station_not_measured` and
/// propagates it with `?`; the candidate is rejected there, before the loop
/// body runs. The arm below is reached only if `max_passes` were zero, which
/// the caller's own `max(1)` prevents, so it names no physical cause at all.
pub(super) fn mass_coordinates_failure() -> CandidateFailure {
    CandidateFailure::new("mass_coordinates")
}

#[cfg(test)]
mod tests {
    use super::{aitken, NextPass, SizingMapJump, TakeoffMassIteration, JUMP_RESOLUTION_FRACTION};

    /// Sizing tolerance of the closure tests, kg (the configured default).
    const TOLERANCE_KG: f64 = 1.0;

    /// Pass budget of the closure tests (the configured default).
    const PASSES: usize = 30;

    /// Drive [`TakeoffMassIteration`] as `converge` does, on the dispatch map
    /// `map(tow, plan_mass)`, from `seed`: the takeoff mass it settled at, or
    /// `None`, and the jump it met.
    fn close(map: impl Fn(f64, f64) -> f64, seed: f64) -> (Option<f64>, Option<SizingMapJump>) {
        let mut iteration = TakeoffMassIteration::new(seed, TOLERANCE_KG, f64::INFINITY);
        let mut tow = seed;
        for _ in 0..PASSES {
            let next = map(tow, iteration.plan_mass_kg(tow));
            if (next - tow).abs() < TOLERANCE_KG {
                return (Some(next), iteration.finish(true));
            }
            match iteration.advance(tow, next, 0) {
                NextPass::Fly(mass) => tow = mass,
                NextPass::Stop => break,
            }
        }
        (None, iteration.finish(false))
    }

    /// Mass at which the synthetic dispatch maps jump, kg (between the
    /// A350-900 cycle's iterates).
    const JUMP_KG: f64 = 225_600.0;

    /// A contraction of slope 0.3 onto `fixed_kg`.
    fn towards(fixed_kg: f64, tow: f64) -> f64 {
        fixed_kg + 0.3 * (tow - fixed_kg)
    }

    /// A plan chosen below the jump closes heavier, one chosen above it
    /// lighter, and neither side has its fixed point on its own side: the
    /// jump holds the heavier side's plan, on which the closure settles.
    #[test]
    fn a_held_plan_settles_a_jump_of_the_plan_choice() {
        let map = |tow: f64, plan: f64| {
            if plan < JUMP_KG {
                towards(225_650.0, tow)
            } else {
                towards(225_236.0, tow)
            }
        };
        let (settled, jump) = close(map, 225_236.0);
        let settled = settled.unwrap_or_else(|| panic!("the held plan settles"));
        assert!((settled - 225_236.0).abs() < TOLERANCE_KG, "{settled}");
        let jump = jump.unwrap_or_else(|| panic!("the jump is on the record"));
        assert!(jump.resolved_by_held_plan && !jump.survives_held_plan);
        assert!(jump.lower_kg < JUMP_KG && JUMP_KG <= jump.upper_kg);
        assert!(jump.upper_kg - jump.lower_kg <= JUMP_RESOLUTION_FRACTION * TOLERANCE_KG);
        assert!(jump
            .describe()
            .contains("settled on the mission plan of the heavier side"));
    }

    /// A jump of the airframe itself, whatever the plan, survives the held
    /// plan and stops the closure, unsettled and saying why.
    #[test]
    fn a_jump_that_survives_the_held_plan_stops_the_closure() {
        let map = |tow: f64, _plan: f64| {
            if tow < JUMP_KG {
                towards(225_650.0, tow)
            } else {
                towards(225_236.0, tow)
            }
        };
        let (settled, jump) = close(map, 225_236.0);
        assert!(settled.is_none());
        let jump = jump.unwrap_or_else(|| panic!("the jump is on the record"));
        assert!(jump.survives_held_plan && !jump.resolved_by_held_plan);
        assert!(jump.lower_kg < JUMP_KG && JUMP_KG <= jump.upper_kg);
        assert!(
            jump.describe()
                .contains("no takeoff mass is its own dispatch"),
            "{}",
            jump.describe()
        );
    }

    /// The same 414 kg fall of the dispatch, spread smoothly over about
    /// 80 kg (slope -11 at the fixed point), is a steep continuous map: the
    /// bisection settles it and names no jump.
    #[test]
    fn a_steep_continuous_map_settles_without_a_jump() {
        let fixed = 225_443.0;
        let map = |tow: f64, _plan: f64| fixed - 207.0 * ((tow - fixed) / 40.0).tanh() * 2.0;
        let (settled, jump) = close(map, 225_236.0);
        let settled = settled.unwrap_or_else(|| panic!("the continuous map settles"));
        assert!((settled - fixed).abs() < TOLERANCE_KG, "{settled}");
        assert!(jump.is_none(), "{jump:?}");
    }

    #[test]
    fn aitken_reaches_the_fixed_point_of_a_linear_contraction_in_one_step() {
        // x -> 0.25 x + 75 has the fixed point 100; three iterates from 0
        // extrapolate to it exactly.
        let g = |x: f64| 0.25 * x + 75.0;
        let x0 = 0.0;
        let x1 = g(x0);
        let x2 = g(x1);
        let estimate = aitken(x0, x1, x2, 1_000.0).unwrap_or_else(|| panic!("admissible"));
        assert!((estimate - 100.0).abs() < 1e-9);
    }

    /// A map that jumps across the diagonal (the A350-900 closure cycled
    /// 225,236 -> 225,643 -> 225,441 -> 225,652 kg) is bisected onto the jump
    /// instead of cycling, and the jump is named once it is narrower than the
    /// tolerance; a re-trim drops the bracket.
    #[test]
    fn a_cycle_across_a_jump_is_bisected_onto_it() {
        use super::ResidualBracket;
        let g = |t: f64| if t < 225_600.0 { 225_650.0 } else { 225_236.0 };
        let mut bracket = ResidualBracket::default();
        let mut tow = 225_236.0;
        let mut jump = None;
        for _ in 0..30 {
            let next = g(tow);
            bracket.record(tow, next, 1);
            jump = bracket.jump(TOLERANCE_KG);
            if jump.is_some() {
                break;
            }
            tow = bracket.safeguard(next);
        }
        let jump = jump.unwrap_or_else(|| panic!("the jump is bracketed"));
        assert!(jump.lower_kg < 225_600.0 && 225_600.0 <= jump.upper_kg);
        assert!(jump.upper_kg - jump.lower_kg <= JUMP_RESOLUTION_FRACTION * TOLERANCE_KG);
        assert_eq!(
            (jump.lower_next_kg, jump.upper_next_kg),
            (225_650.0, 225_236.0)
        );

        bracket.record(225_000.0, 225_100.0, 2);
        assert!(bracket.jump(1.0e9).is_none());
        assert_eq!(bracket.safeguard(1.0), 1.0);
    }

    #[test]
    fn a_stalled_or_out_of_range_extrapolation_is_declined() {
        assert!(aitken(1.0, 1.0, 1.0, 10.0).is_none());
        assert!(aitken(0.0, 10.0, 15.0, 12.0).is_none());
    }
}
