// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CS 25 take-off field length by integrated ground and air motion.
//!
//! One definition for the optimiser constraint and the reports:
//!
//! `TOFL = min over VR of max(1.15 * TOD_AEO, BFL)`,
//!
//! the take-off distance of CS 25.113(a): the greater of the continued
//! take-off with the critical engine failed at `VEF` (a)(1) and 115 % of the
//! all-engine distance to 11 m (35 ft) (a)(2), with the balanced field `BFL`
//! the equality of that continued distance and the accelerate-stop distance of
//! CS 25.109(a) at the decision speed `V1`.
//!
//! Method (every constant is cited at its definition):
//! - Ground roll: `a = g [T/W - mu - (q S / W)(CD_roll - mu CL_g)]` integrated
//!   as `S = int d(V^2) / (2 a)` (Raymer, Aircraft Design, 5th ed., Eqs.
//!   17.100-17.101, pp. 672-673; Torenbeek, Synthesis of Subsonic Airplane
//!   Design, Eqs. 5-73/5-74, p. 167), with the thrust of the engine deck at
//!   every speed rather than a mean value.
//! - Rotation: with a lift-off schedule ([`LiftOffSchedule`]), one attitude
//!   history on one ground plane from the roll's level attitude through `VR`
//!   to lift-off, the lift that of the attitude and the wheel load what it
//!   leaves; without one, 3 s (Raymer p. 673) with the wheel load falling
//!   linearly to zero at lift-off.
//! - Airborne: the larger of the energy distance `((V2^2 - VLOF^2)/2g + h) /
//!   gamma` (Torenbeek Eqs. 5-85/5-86, p. 169) and the kinematic transition
//!   arc of Raymer Eqs. 17.105-17.112 (pp. 673-674, n = 1.2); each is a
//!   necessary condition on reaching 35 ft at V2.
//! - Accelerate-stop: CS 25.109(a)(1): all-engine roll to `VEF`, one-engine
//!   roll to `V1`, braking at the mean deceleration of 15 jet transports
//!   (Torenbeek Eq. 5-87 and p. 169 note 2: 0.37 g, no reverse thrust), plus
//!   2 s at `V1` and the pilot-action transition that completes the 4.5 s
//!   equivalent inertia time the 0.37 g was fitted with
//!   ([`STOP_INERTIA_TIME_S`]). The failure is recognised 1 s after `VEF`
//!   (Raymer p. 676; CS 25.107(a)(2): `V1 >= VEF` plus the speed gained
//!   meanwhile).
//! - `VR` is selected by the applicant (CS 25.107(e)); it is searched between
//!   the schedule minimum and `V2` for the shortest field, as Raymer p. 676
//!   describes ("delayed to a higher speed ... calculated to minimise BFL").
//!
//! - Ground effect: the lift-dependent drag is reduced by
//!   Torenbeek's ground-effect form (App. G-7.2, Eq. G-77, see
//!   [`GroundEffect`]) at the wing height of the rotation and of every point
//!   of the air leg, because the demonstrated take-off distance of CS 25.113
//!   is flown in ground effect up to a wing height of one span (CS
//!   25.111(d)(3)-(4)); the air leg climbs from the lift-off position.
//! - Gear: down to 35 ft. CS 25.111(b) and AMC 25.111(b)(2) forbid retraction
//!   before lift-off and demand at least 3 s of delay, and Torenbeek (1982,
//!   App. G-6, p. 551) states that retraction "will initially cause the drag
//!   to increase some 20 to 30 percent" (wheel-bay doors), so the continued
//!   take-off, which is 5 to 9 s in the air, gains no drag relief from
//!   retraction before the screen.
//!
//! Limits: dry level hard runway, no wind, no clearway or stopway, no
//! reverse thrust or lift dump, constant `mu`, and the level attitude on the
//! ground roll. Conceptual, not certified.

mod airborne;
mod breakdown;
mod ground_effect;
mod polar;
mod schedule;

use super::{VSpeeds, G};
pub use breakdown::{field_breakdown, FieldBreakdown};
pub use ground_effect::{ground_effect_beta, ground_effect_sigma, GroundEffect, WingRise};
pub use polar::FieldPolar;
pub use schedule::{vmu_margins, LiftOffSchedule, RunwayAirframe, ROTATION_PITCH_RATE_DEG_S};

/// Rotation time, s: "a typical assumption for large aircraft is that rotation
/// takes 3 s" (Raymer, 5th ed., p. 673).
pub const ROTATION_TIME_S: f64 = 3.0;
/// Time from `VEF` to the pilot's first stopping action, s: "usually assumed
/// that the pilot waits 1 s before recognizing the engine failure" (Raymer
/// p. 676); CS 25.107(a)(2) defines `V1` through it.
pub const RECOGNITION_TIME_S: f64 = 1.0;
/// Distance at `V1` added to the accelerate-stop distance, expressed in
/// seconds: CS 25.109(a)(1)(iv).
pub const STOP_ALLOWANCE_S: f64 = 2.0;
/// Equivalent inertia time of the stop, from the engine failure, s: Torenbeek,
/// *Synthesis of Subsonic Airplane Design* (1982), Eqs. 5-87 to 5-89, p. 169.
/// The mean stopping deceleration `a_stop = .37 g` "has been found from
/// application of the method to 15 jet transports" with the stop written as
/// `V^2 / (2 a_stop) + V dt` and the inertia allowance "655 ft (200 m) for
/// dt = 4 1/2 seconds": the deceleration and the time are one fitted pair.
pub const STOP_INERTIA_TIME_S: f64 = 4.5;
/// Time at `V1` between the first stopping action and the full braking
/// configuration that the pair above holds beyond the recognition second and
/// the CS 25.109 two-second increment, s: `4.5 - 1 - 2`. FAA AC 25-7D
/// (4 May 2018) para 4.3.6 and Fig. 4-1 place the demonstrated intervals
/// between the first, second and third pilot actions (brakes, thrust levers,
/// spoilers) here, ahead of the two-second distance increment.
pub const STOP_TRANSITION_TIME_S: f64 = STOP_INERTIA_TIME_S - RECOGNITION_TIME_S - STOP_ALLOWANCE_S;
/// Factor on the all-engine distance: CS 25.113(a)(2).
pub const ALL_ENGINE_FACTOR: f64 = 1.15;
/// Screen height, m: CS 25.113 (35 ft).
pub const SCREEN_HEIGHT_M: f64 = 35.0 * alas_units::FOOT;
/// Load factor of the transition arc: Raymer Eqs. 17.105-17.107, p. 674.
pub const TRANSITION_LOAD_FACTOR: f64 = 1.2;
/// Ground lift coefficient without a lift-off schedule: "typically less than
/// 0.1" (Raymer p. 672); a schedule flies the level attitude's own lift.
pub const GROUND_LIFT_COEFFICIENT: f64 = 0.1;

const THRUST_POINTS: usize = 33;
const ROLL_CELLS: usize = 256;
const VR_POINTS: usize = 17;

/// Callback for total thrust, N, at a true airspeed in m/s.
pub type ThrustFn<'a> = &'a dyn Fn(f64) -> Result<f64, String>;

/// Inputs of the field-length integration.
#[derive(Clone, Copy)]
pub struct TakeoffFieldInputs<'a> {
    /// Take-off weight, N.
    pub weight_n: f64,
    /// Reference wing area, m^2.
    pub wing_area_m2: f64,
    /// Air density at the field, kg/m^3.
    pub density_kg_m3: f64,
    /// Speed schedule: `v_mc_ms`, `v_r_ms` (minimum `VR`) and `v2_ms` are read.
    pub speeds: VSpeeds,
    /// Rolling friction coefficient, brakes off.
    pub rolling_friction: f64,
    /// Mean stopping deceleration over g.
    pub stop_deceleration_g: f64,
    /// Zero-lift drag with the take-off flap and slat, gear up.
    pub cd0: f64,
    /// Gear drag increment, down to 35 ft.
    pub gear_delta_cd: f64,
    /// Lift-dependent drag `CD(CL) - CD(0)` in free air at a lift coefficient:
    /// the aircraft drag table itself (or a parabola `k CL^2`), the same
    /// function the second-segment assessor reads.
    pub lift_drag: &'a dyn Fn(f64) -> f64,
    /// Wing over the ground, for the ground-effect reduction of `lift_drag`;
    /// `None` for free air throughout.
    pub ground_effect: Option<GroundEffect>,
    /// Failed-engine (windmilling plus trim) drag coefficient, one-engine paths.
    pub failed_engine_cd: f64,
    /// All-engine thrust at speed.
    pub thrust_aeo_n: ThrustFn<'a>,
    /// Thrust with the critical engine failed at speed.
    pub thrust_oei_n: ThrustFn<'a>,
    /// Drag of the take-off configuration in the air, gear up, at a lift
    /// coefficient, with the critical engine failed or not, as `(part not
    /// reduced in ground effect, lift-dependent part)`: the climb-out polar
    /// the CS 25.121 gradients are judged on, so the air legs and the
    /// gradient check fly one polar. The failed engine's own drag is in the
    /// first part. `None` flies the air legs on `cd0`, `lift_drag` and
    /// `failed_engine_cd` like the ground roll.
    pub airborne_drag: Option<AirborneDragFn<'a>>,
    /// The lift-off schedule of CS 25.107(e)(1)(iv): the minimum unstick
    /// speed that bounds `VLOF` and the airframe on the runway whose attitude
    /// history the roll and the rotation fly. `None` keeps the fixed
    /// [`ROTATION_TIME_S`], [`GROUND_LIFT_COEFFICIENT`] and no `VMU`.
    pub liftoff: Option<LiftOffSchedule<'a>>,
}

/// Callback for the airborne drag of the take-off configuration, see
/// [`TakeoffFieldInputs::airborne_drag`].
pub type AirborneDragFn<'a> = &'a dyn Fn(f64, bool) -> (f64, f64);

/// Distances and speeds of the field-length solution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TakeoffField {
    /// Take-off field length `max(1.15 AEO, BFL)`, m.
    pub tofl_m: f64,
    /// All-engine distance to 35 ft, unfactored, m.
    pub all_engine_m: f64,
    /// Balanced field length, m.
    pub balanced_m: f64,
    /// Continued take-off distance at the balanced `V1`, m.
    pub continued_m: f64,
    /// Accelerate-stop distance at the balanced `V1`, m.
    pub accelerate_stop_m: f64,
    /// Balanced decision speed, m/s.
    pub v1_m_s: f64,
    /// Failure speed `VEF`, m/s.
    pub vef_m_s: f64,
    /// Selected rotation speed, m/s.
    pub vr_m_s: f64,
}

struct Table {
    grid: Vec<f64>,
    values: Vec<f64>,
}

impl Table {
    fn sample(top: f64, f: ThrustFn<'_>) -> Result<Self, String> {
        let grid: Vec<f64> = (0..THRUST_POINTS)
            .map(|i| top * i as f64 / (THRUST_POINTS - 1) as f64)
            .collect();
        let mut values = Vec::with_capacity(THRUST_POINTS);
        for &v in &grid {
            let thrust = f(v)?;
            if !(thrust.is_finite() && thrust > 0.0) {
                return Err(format!(
                    "take-off thrust {thrust} N at {v:.1} m/s is not usable"
                ));
            }
            values.push(thrust);
        }
        Ok(Self { grid, values })
    }

    fn at(&self, v: f64) -> f64 {
        let step = self.grid[1] - self.grid[0];
        let x = (v / step).clamp(0.0, (THRUST_POINTS - 1) as f64);
        let i = (x.floor() as usize).min(THRUST_POINTS - 2);
        let w = x - i as f64;
        self.values[i] * (1.0 - w) + self.values[i + 1] * w
    }
}

/// Ground roll of one configuration: cumulative distance on a grid of V^2.
struct Roll {
    step: f64,
    cumulative: Vec<f64>,
    accel: Vec<f64>,
}

impl Roll {
    fn distance(&self, v: f64) -> f64 {
        let x = (v / self.step).clamp(0.0, ROLL_CELLS as f64);
        let i = (x.floor() as usize).min(ROLL_CELLS - 1);
        let v0 = self.step * i as f64;
        self.cumulative[i] + (v * v - v0 * v0) / (2.0 * self.accel[i])
    }

    fn acceleration(&self, v: f64) -> f64 {
        let x = (v / self.step).clamp(0.0, ROLL_CELLS as f64);
        self.accel[(x.floor() as usize).min(ROLL_CELLS - 1)]
    }
}

struct Model<'a> {
    inputs: &'a TakeoffFieldInputs<'a>,
    thrust: Table,
    thrust_oei: Table,
}

impl Model<'_> {
    fn thrust_n(&self, v: f64, one_engine_out: bool) -> f64 {
        if one_engine_out {
            self.thrust_oei.at(v)
        } else {
            self.thrust.at(v)
        }
    }

    fn failed_cd(&self, one_engine_out: bool) -> f64 {
        if one_engine_out {
            self.inputs.failed_engine_cd
        } else {
            0.0
        }
    }

    fn q_s(&self, v: f64) -> f64 {
        0.5 * self.inputs.density_kg_m3 * v * v * self.inputs.wing_area_m2
    }

    /// Drag coefficient with the gear down at lift coefficient `cl`, the
    /// wing `rise` above its static, level position.
    fn drag_cd(&self, cl: f64, one_engine_out: bool, rise: WingRise) -> f64 {
        let i = self.inputs;
        let lift = (i.lift_drag)(cl);
        let lift = i
            .ground_effect
            .map_or(lift, |g| lift * g.induced_drag_ratio(cl, rise));
        i.cd0 + i.gear_delta_cd + self.failed_cd(one_engine_out) + lift
    }

    /// Drag coefficient in the air, gear down to the screen, at lift
    /// coefficient `cl` and wing `rise`: the climb-out polar when the inputs
    /// carry one, else the ground-roll terms of [`Self::drag_cd`].
    fn air_drag_cd(&self, cl: f64, one_engine_out: bool, rise: Option<WingRise>) -> f64 {
        let i = self.inputs;
        match i.airborne_drag {
            Some(drag) => {
                let (base, lift) = drag(cl, one_engine_out);
                let lift = match (i.ground_effect, rise) {
                    (Some(g), Some(rise)) => lift * g.induced_drag_ratio(cl, rise),
                    _ => lift,
                };
                base + i.gear_delta_cd + lift
            }
            None => match rise {
                Some(rise) => self.drag_cd(cl, one_engine_out, rise),
                None => {
                    i.cd0 + i.gear_delta_cd + self.failed_cd(one_engine_out) + (i.lift_drag)(cl)
                }
            },
        }
    }

    /// Ground acceleration, m/s^2, at lift coefficient `cl`, with `unload` the
    /// fraction of the weight carried by the wings and the wing `rise` above
    /// its static, level position.
    fn accel(&self, v: f64, one_engine_out: bool, cl: f64, unload: f64, rise: WingRise) -> f64 {
        let i = self.inputs;
        let cd = self.drag_cd(cl, one_engine_out, rise);
        G * ((self.thrust_n(v, one_engine_out) - self.q_s(v) * cd) / i.weight_n
            - i.rolling_friction * (1.0 - unload))
    }

    fn roll(&self, top: f64, one_engine_out: bool) -> Result<Roll, String> {
        let step = top / ROLL_CELLS as f64;
        let mut cumulative = vec![0.0];
        let mut accel = Vec::with_capacity(ROLL_CELLS);
        for i in 0..ROLL_CELLS {
            let (v0, v1) = (step * i as f64, step * (i + 1) as f64);
            let mid = 0.5 * (v0 + v1);
            let (cl, unload, rise) = self.level_roll(mid);
            let a = self.accel(mid, one_engine_out, cl, unload, rise);
            if !(a.is_finite() && a > 0.02) {
                return Err(format!(
                    "take-off acceleration {a:.4} m/s^2 at {mid:.1} m/s ({}) leaves no ground roll",
                    if one_engine_out {
                        "one engine out"
                    } else {
                        "all engines"
                    }
                ));
            }
            accel.push(a);
            let last = cumulative[i];
            cumulative.push(last + (v1 * v1 - v0 * v0) / (2.0 * a));
        }
        Ok(Roll {
            step,
            cumulative,
            accel,
        })
    }

    /// Climb gradient `(T - D)/W` at speed `v` with the wing `rise` above its
    /// static, level position, lift equal to weight, gear down.
    fn gradient(&self, v: f64, one_engine_out: bool, rise: WingRise) -> f64 {
        let i = self.inputs;
        let cl = i.weight_n / self.q_s(v);
        let cd = self.air_drag_cd(cl, one_engine_out, Some(rise));
        (self.thrust_n(v, one_engine_out) - self.q_s(v) * cd) / i.weight_n
    }

    /// The same gradient in free air (no ground effect): the quantity the
    /// CS 25.121 gradients are defined for (Torenbeek 2013, p. 266).
    fn gradient_free_air(&self, v: f64, one_engine_out: bool) -> f64 {
        let i = self.inputs;
        let cl = i.weight_n / self.q_s(v);
        let cd = self.air_drag_cd(cl, one_engine_out, None);
        (self.thrust_n(v, one_engine_out) - self.q_s(v) * cd) / i.weight_n
    }
}

/// The integration model and the `VR` search interval `[vr_min, v2]` of
/// `inputs`: the schedule minimum `VR` and `V2`, raised by the lift-off
/// schedule ([`Model::vr_floor`]). CS 25.107(c)(2): `V2` no less than `VR`,
/// so a `VMU`-bound `VR` raises the top of the search.
fn search_model<'a>(inputs: &'a TakeoffFieldInputs<'a>) -> Result<(Model<'a>, f64, f64), String> {
    let v2 = inputs.speeds.v2_ms.max(inputs.speeds.v_r_ms);
    // A VMU-bound VR may exceed the scheduled V2: sample the thrust higher.
    let top = if inputs.liftoff.is_some() { 1.6 } else { 1.3 } * v2;
    let model = Model {
        inputs,
        thrust: Table::sample(top, inputs.thrust_aeo_n)?,
        thrust_oei: Table::sample(top, inputs.thrust_oei_n)?,
    };
    let vr_min = model.vr_floor(inputs.speeds.v_r_ms);
    Ok((model, vr_min, v2.max(vr_min)))
}

/// Solve the CS 25.113(a) take-off field length.
///
/// # Errors
///
/// A thrust callback failure, or a take-off that cannot accelerate, or a
/// continued take-off without a positive gradient at any admissible `VR`.
pub fn takeoff_field(inputs: &TakeoffFieldInputs<'_>) -> Result<TakeoffField, String> {
    for (name, value) in [
        ("weight", inputs.weight_n),
        ("wing area", inputs.wing_area_m2),
        ("density", inputs.density_kg_m3),
        ("rolling friction", inputs.rolling_friction),
        ("stop deceleration", inputs.stop_deceleration_g),
        ("VR", inputs.speeds.v_r_ms),
        ("V2", inputs.speeds.v2_ms),
    ] {
        if !(value.is_finite() && value > 0.0) {
            return Err(format!(
                "take-off field {name} must be positive, got {value}"
            ));
        }
    }
    if ![
        inputs.cd0,
        inputs.gear_delta_cd,
        inputs.failed_engine_cd,
        (inputs.lift_drag)(1.0),
    ]
    .iter()
    .all(|x| x.is_finite() && *x >= 0.0)
    {
        return Err("take-off drag coefficients must be finite and nonnegative".to_owned());
    }
    let (model, vr_min, v2) = search_model(inputs)?;
    let aeo = model.roll(v2, false)?;
    let oei = model.roll(v2, true)?;
    let stop_a = inputs.stop_deceleration_g * G;
    let mut best: Option<TakeoffField> = None;
    for k in 0..VR_POINTS {
        let vr = vr_min + (v2 - vr_min) * k as f64 / (VR_POINTS - 1) as f64;
        let (vlof_a, rot_a) = model.rotation(vr, false);
        let all_engine = aeo.distance(vr) + rot_a + model.air(vlof_a, false);
        let (vlof_o, rot_o) = model.rotation(vr, true);
        let tail = oei.distance(vr) + rot_o + model.air(vlof_o, true);
        let vef_of = |v1: f64| (v1 - oei.acceleration(v1) * RECOGNITION_TIME_S).max(0.0);
        let stop = |v1: f64| {
            let vef = vef_of(v1);
            aeo.distance(vef) + oei.distance(v1) - oei.distance(vef)
                + v1 * v1 / (2.0 * stop_a)
                + (STOP_ALLOWANCE_S + STOP_TRANSITION_TIME_S) * v1
        };
        let go = |v1: f64| aeo.distance(vef_of(v1)) - oei.distance(vef_of(v1)) + tail;
        let (mut lo, mut hi) = (inputs.speeds.v_mc_ms.min(vr), vr);
        let gap = |v1: f64| stop(v1) - go(v1);
        let v1 = if gap(lo) >= 0.0 || !go(lo).is_finite() {
            lo
        } else if gap(hi) <= 0.0 {
            hi
        } else {
            for _ in 0..60 {
                let mid = 0.5 * (lo + hi);
                if gap(mid) > 0.0 {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            0.5 * (lo + hi)
        };
        let (accelerate_stop_m, continued_m) = (stop(v1), go(v1));
        let balanced_m = accelerate_stop_m.max(continued_m);
        let tofl_m = (ALL_ENGINE_FACTOR * all_engine).max(balanced_m);
        let candidate = TakeoffField {
            tofl_m,
            all_engine_m: all_engine,
            balanced_m,
            continued_m,
            accelerate_stop_m,
            v1_m_s: v1,
            vef_m_s: vef_of(v1),
            vr_m_s: vr,
        };
        if tofl_m.is_finite() && best.is_none_or(|b| tofl_m < b.tofl_m) {
            best = Some(candidate);
        }
    }
    best.ok_or_else(|| {
        format!(
            "continued take-off has no positive gradient at V2 {v2:.1} m/s (one engine out, \
             gear down): the field length is undefined"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speeds() -> VSpeeds {
        VSpeeds {
            v_stall_to_ms: 65.0,
            v_stall_land_ms: 60.0,
            v_mc_ms: 60.0,
            v1_ms: 70.0,
            v_r_ms: 71.5,
            v2_ms: 78.0,
            v_app_ms: 74.0,
            v_td_ms: 69.0,
        }
    }

    fn solve(tw: f64, mass_kg: f64, stop_g: f64) -> Result<TakeoffField, String> {
        let weight_n = mass_kg * 9.81;
        let aeo = move |_: f64| Ok(tw * 9.81 * 78_000.0);
        let oei = move |_: f64| Ok(tw * 9.81 * 78_000.0 * 0.5);
        takeoff_field(&TakeoffFieldInputs {
            weight_n,
            wing_area_m2: 122.0,
            density_kg_m3: 1.225,
            speeds: speeds(),
            rolling_friction: 0.02,
            stop_deceleration_g: stop_g,
            cd0: 0.04,
            gear_delta_cd: 0.02,
            lift_drag: &|cl| 0.045 * cl * cl,
            ground_effect: None,
            failed_engine_cd: 0.006,
            thrust_aeo_n: &aeo,
            thrust_oei_n: &oei,
            airborne_drag: None,
            liftoff: None,
        })
    }

    #[test]
    fn the_solution_balances_and_obeys_the_two_cs_25_113_branches() -> Result<(), String> {
        let f = solve(0.30, 78_000.0, 0.37)?;
        assert!(f.vr_m_s >= speeds().v_r_ms - 1e-9 && f.vr_m_s <= speeds().v2_ms + 1e-9);
        assert!(f.v1_m_s <= f.vr_m_s && f.vef_m_s <= f.v1_m_s);
        assert!(f.tofl_m >= 1.15 * f.all_engine_m - 1e-9 && f.tofl_m >= f.balanced_m - 1e-9);
        assert!(f.balanced_m >= f.continued_m.max(f.accelerate_stop_m) - 1e-9);
        if f.v1_m_s > speeds().v_mc_ms + 1e-6 && f.v1_m_s < f.vr_m_s - 1e-6 {
            assert!((f.continued_m / f.accelerate_stop_m - 1.0).abs() < 1e-6);
        }
        Ok(())
    }

    #[test]
    fn more_thrust_shortens_and_more_weight_lengthens_the_field() -> Result<(), String> {
        let base = solve(0.30, 78_000.0, 0.37)?.tofl_m;
        assert!(solve(0.34, 78_000.0, 0.37)?.tofl_m < base);
        assert!(solve(0.30, 84_000.0, 0.37)?.tofl_m > base);
        Ok(())
    }

    #[test]
    fn ground_effect_shortens_the_engine_out_air_leg_and_leaves_the_stop_alone(
    ) -> Result<(), String> {
        let weight_n = 78_000.0 * 9.81;
        let aeo = move |_: f64| Ok(0.30 * weight_n);
        let oei = move |_: f64| Ok(0.15 * weight_n);
        let inputs = |ground_effect| TakeoffFieldInputs {
            weight_n,
            wing_area_m2: 122.0,
            density_kg_m3: 1.225,
            speeds: speeds(),
            rolling_friction: 0.02,
            stop_deceleration_g: 0.37,
            cd0: 0.04,
            gear_delta_cd: 0.02,
            lift_drag: &|cl| 0.045 * cl * cl,
            ground_effect,
            failed_engine_cd: 0.006,
            thrust_aeo_n: &aeo,
            thrust_oei_n: &oei,
            airborne_drag: None,
            liftoff: None,
        };
        let wing = GroundEffect::new(34.1, 3.6, 3.0, 3.0);
        let free = inputs(None);
        let near = inputs(wing);
        let (a, b) = (takeoff_field(&free)?, takeoff_field(&near)?);
        let (pa, pb) = (field_breakdown(&free, &a)?, field_breakdown(&near, &b)?);
        assert!(pb.continued_air_m < pa.continued_air_m);
        assert!(pb.all_engine_air_m < pa.all_engine_air_m);
        assert!(b.tofl_m < a.tofl_m);
        // The gradient out of ground effect is the free-air value.
        assert!((pb.engine_out_gradient_v2_free_air - pa.engine_out_gradient_v2).abs() < 1e-12);
        assert!(pb.engine_out_gradient_v2 > pb.engine_out_gradient_v2_free_air);
        Ok(())
    }

    #[test]
    fn harder_braking_shortens_the_accelerate_stop_leg() -> Result<(), String> {
        let soft = solve(0.30, 78_000.0, 0.30)?;
        let hard = solve(0.30, 78_000.0, 0.50)?;
        assert!(hard.tofl_m <= soft.tofl_m + 1e-9);
        Ok(())
    }

    #[test]
    fn an_engine_out_without_climb_gradient_has_no_field_length() {
        let weight_n = 78_000.0 * 9.81;
        let aeo = move |_: f64| Ok(0.30 * weight_n);
        let oei = move |_: f64| Ok(0.03 * weight_n);
        let result = takeoff_field(&TakeoffFieldInputs {
            weight_n,
            wing_area_m2: 122.0,
            density_kg_m3: 1.225,
            speeds: speeds(),
            rolling_friction: 0.02,
            stop_deceleration_g: 0.37,
            cd0: 0.04,
            gear_delta_cd: 0.02,
            lift_drag: &|cl| 0.045 * cl * cl,
            ground_effect: None,
            failed_engine_cd: 0.006,
            thrust_aeo_n: &aeo,
            thrust_oei_n: &oei,
            airborne_drag: None,
            liftoff: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn the_constant_acceleration_roll_matches_the_energy_balance() -> Result<(), String> {
        let weight_n = 50_000.0 * 9.81;
        let flat = move |_: f64| Ok(0.25 * weight_n);
        let inputs = TakeoffFieldInputs {
            weight_n,
            wing_area_m2: 1.0e-6,
            density_kg_m3: 1.225,
            speeds: speeds(),
            rolling_friction: 0.0,
            stop_deceleration_g: 0.37,
            cd0: 0.0,
            gear_delta_cd: 0.0,
            lift_drag: &|cl| 0.05 * cl * cl,
            ground_effect: None,
            failed_engine_cd: 0.0,
            thrust_aeo_n: &flat,
            thrust_oei_n: &flat,
            airborne_drag: None,
            liftoff: None,
        };
        let model = Model {
            inputs: &inputs,
            thrust: Table::sample(100.0, &flat)?,
            thrust_oei: Table::sample(100.0, &flat)?,
        };
        let roll = model.roll(80.0, false)?;
        let exact = 80.0_f64.powi(2) / (2.0 * crate::performance::G * 0.25);
        assert!((roll.distance(80.0) / exact - 1.0).abs() < 1e-9);
        Ok(())
    }
}
