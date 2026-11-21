// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The lift-off schedule: one attitude history of one airframe on one ground
//! plane, from the ground roll through the rotation to lift-off, and the `VR`
//! floor of the CS 25.107(e)(1)(iv) `VMU` margins it sets.
//!
//! With a [`LiftOffSchedule`]:
//! - The ground roll is flown at the level attitude, `theta = 0`, with that
//!   attitude's lift in ground effect (Raymer, 6th ed., Sec. 17.8.1, p. 672:
//!   the ground-roll lift coefficient is based on the wing angle of attack
//!   on the ground, with the take-off flap).
//! - From `VR` the body pitches about the main-wheel contact at the pitch
//!   rate, `theta(t) = q t`, up to the tail-strike attitude. The lift is the
//!   airframe's lift in ground effect at that attitude, `L = q S CL(theta)`,
//!   no more than `q S CLmax,TO` (the wing stalls first); the wheels carry
//!   `W - L`, the friction is `mu (W - L)`, and the induced drag is that of
//!   the same `CL` at the wing height of the same attitude.
//! - The main-gear shock absorber extends as the wheel load falls, along its
//!   air spring ([`RunwayAirframe::extension`]), and the lift and the wing
//!   height are read at that extension: lift, wheel load and extension are
//!   one state, solved by fixed point at every instant.
//! - Lift-off is the instant the lift reaches the weight: the wheel load is
//!   zero, the shock absorber fully extended, and the attitude is the one
//!   whose lift in ground effect carries the weight at `VLOF`, the attitude
//!   the tail-strike limit, `VMU` and the air leg are measured at.
//!
//! The history is a whole: `VLOF` does not fall with `VR`. While the rotation
//! lifts off below the tail-strike attitude it rises strictly (a faster
//! rotation start reaches the weight sooner, at a smaller attitude, so at a
//! higher speed); a `VR` low enough for the rotation to reach the tail-strike
//! attitude first lifts off at `VMU` itself, an attitude held at that limit
//! lifting off exactly at the geometry-limited `VMU`. Every `VMU` margin
//! target lies above `VMU`, so it still bounds `VR` from below by bisection.

use super::{Model, WingRise, GROUND_LIFT_COEFFICIENT, ROTATION_TIME_S};

/// Average pitch rate of the rotation to the lift-off attitude, deg/s: the
/// nominal "average pitch rate to 10 degrees pitch attitude" of 2.5 deg/s
/// and the "constant rate of 2 to 3 degrees per second rate of rotation" of
/// the minimum tail clearance (Boeing, *Aero* Quarterly Qtr 1 2007, "Tail
/// Strikes: Prevention", Fig. 1 note and Fig. 4).
pub const ROTATION_PITCH_RATE_DEG_S: f64 = 2.5;

/// Integration step of the scheduled rotation, s (a rotation lasts 2 to 4 s).
const ROTATION_STEP_S: f64 = 0.05;
/// Longest scheduled rotation integrated, steps (60 s): a rotation that has
/// not lifted off by then has no lift-off.
const ROTATION_STEP_LIMIT: usize = 1_200;
/// Fixed-point passes between the lift and the shock-absorber extension.
const LIFT_ITERATIONS: usize = 3;
/// Steps of the timed rotation without a schedule.
const ROTATION_STEPS: usize = 30;

/// The `VLOF` margins over `VMU` of CS 25.107(e)(1)(iv): (A) 110 % with all
/// engines and 105 % with one engine inoperative; (B) 108 % and 104 % when
/// `VMU` is limited by the airplane's geometry (tail contact).
#[must_use]
pub const fn vmu_margins(geometry_limited: bool) -> (f64, f64) {
    if geometry_limited {
        (1.08, 1.04)
    } else {
        (1.10, 1.05)
    }
}

/// The airframe on the runway on one ground plane: its lift, the height of
/// its wing and the extension of its main-gear shock absorber, all from one
/// geometry, so the roll, the rotation, the lift-off attitude and the air
/// leg are one attitude history.
pub trait RunwayAirframe {
    /// Lift coefficient in ground effect at runway pitch `theta_deg` about
    /// the main-wheel contact, the shock absorber `extension` of the way from
    /// its static position (0) to full extension (1).
    fn lift_coefficient(&self, theta_deg: f64, extension: f64) -> f64;
    /// Rise of the wing over its static, level position in the same state.
    fn wing_rise(&self, theta_deg: f64, extension: f64) -> WingRise;
    /// Shock-absorber extension, 0 static to 1 full, at a main-gear load of
    /// `wheel_load` times the static load.
    fn extension(&self, wheel_load: f64) -> f64;
    /// Tail-strike attitude, shock absorber fully extended, degrees.
    fn tail_strike_deg(&self) -> f64;
    /// The attitude whose lift in ground effect, shock absorber fully
    /// extended, is `cl`, degrees, in `[0, tail_strike_deg]`.
    fn liftoff_attitude_deg(&self, cl: f64) -> f64;
}

/// The lift-off inputs of [`super::TakeoffFieldInputs::liftoff`].
#[derive(Clone, Copy)]
pub struct LiftOffSchedule<'a> {
    /// Minimum unstick speed `VMU`, m/s (CS 25.107(d)).
    pub vmu_m_s: f64,
    /// `VMU` is reached with the tail on the runway (CS 25.107(e)(1)(iv)(B)).
    pub geometry_limited: bool,
    /// Average rotation pitch rate, deg/s ([`ROTATION_PITCH_RATE_DEG_S`]).
    pub pitch_rate_deg_s: f64,
    /// Take-off `CLmax`: the lift on the runway never exceeds it.
    pub cl_max_to: f64,
    /// The airframe the roll and the rotation fly, on the plane `VMU` and
    /// the tail-strike attitude were measured on.
    pub airframe: &'a dyn RunwayAirframe,
}

/// One instant on the runway: lift coefficient, lift over weight, wing rise.
struct Runway {
    cl: f64,
    lift_fraction: f64,
    rise: WingRise,
}

impl LiftOffSchedule<'_> {
    /// The airframe at runway attitude `theta_deg` with `q_s` the dynamic
    /// pressure times the wing area, N: the lift coefficient (no more than
    /// `cl_max_to`) and the wing rise at the extension the remaining wheel
    /// load gives.
    fn runway(&self, theta_deg: f64, q_s: f64, weight_n: f64) -> Runway {
        let lift = |extension: f64| {
            self.airframe
                .lift_coefficient(theta_deg, extension)
                .min(self.cl_max_to)
        };
        let mut extension = 0.0;
        let mut cl = lift(extension);
        for _ in 0..LIFT_ITERATIONS {
            extension = self.airframe.extension(1.0 - q_s * cl / weight_n);
            cl = lift(extension);
        }
        Runway {
            cl,
            lift_fraction: q_s * cl / weight_n,
            rise: self.airframe.wing_rise(theta_deg, extension),
        }
    }
}

impl Model<'_> {
    /// The ground roll at speed `v`: lift coefficient, the share of the
    /// weight the wings carry, and the wing rise. With a schedule, the level
    /// attitude of its airframe; without, [`GROUND_LIFT_COEFFICIENT`] on the
    /// static wing.
    pub(super) fn level_roll(&self, v: f64) -> (f64, f64, WingRise) {
        let weight_n = self.inputs.weight_n;
        match self.inputs.liftoff {
            Some(schedule) => {
                let level = schedule.runway(0.0, self.q_s(v), weight_n);
                (level.cl, level.lift_fraction.min(1.0), level.rise)
            }
            None => {
                let unload = self.q_s(v) * GROUND_LIFT_COEFFICIENT / weight_n;
                (GROUND_LIFT_COEFFICIENT, unload, WingRise::level(0.0))
            }
        }
    }

    /// Rotation from `vr` to lift-off, `(VLOF, distance)`: the schedule's
    /// attitude history (module note), or without one [`ROTATION_TIME_S`]
    /// with the wheel load falling linearly to zero.
    pub(super) fn rotation(&self, vr: f64, one_engine_out: bool) -> (f64, f64) {
        let Some(schedule) = self.inputs.liftoff else {
            return self.timed_rotation(vr, one_engine_out);
        };
        let weight_n = self.inputs.weight_n;
        let ceiling = schedule.airframe.tail_strike_deg();
        let rate = schedule.pitch_rate_deg_s;
        let state =
            |t: f64, v: f64| schedule.runway((rate * t).min(ceiling), self.q_s(v), weight_n);
        let accel = |t: f64, v: f64| {
            let runway = state(t, v);
            let unload = runway.lift_fraction.min(1.0);
            self.accel(v, one_engine_out, runway.cl, unload, runway.rise)
                .max(0.0)
        };
        let dt = ROTATION_STEP_S;
        let (mut v, mut s) = (vr, 0.0);
        let mut excess = state(0.0, vr).lift_fraction - 1.0;
        if excess >= 0.0 {
            return (vr, 0.0);
        }
        for step in 0..ROTATION_STEP_LIMIT {
            let t = step as f64 * dt;
            let a = accel(t + 0.5 * dt, v + 0.5 * dt * accel(t, v));
            let (v_next, s_next) = (v + a * dt, s + (v + 0.5 * a * dt) * dt);
            let next = state(t + dt, v_next).lift_fraction - 1.0;
            if next >= 0.0 {
                // Lift-off inside the step: the instant the lift reaches the
                // weight, linear in the excess lift over the step.
                let x = excess / (excess - next);
                return (v + x * (v_next - v), s + x * (s_next - s));
            }
            (v, s, excess) = (v_next, s_next, next);
        }
        (v, f64::INFINITY)
    }

    /// The rotation without a schedule: [`ROTATION_TIME_S`] with the wheel
    /// load falling linearly to zero at lift-off (Raymer p. 673).
    fn timed_rotation(&self, vr: f64, one_engine_out: bool) -> (f64, f64) {
        let dt = ROTATION_TIME_S / ROTATION_STEPS as f64;
        let (mut v, mut s) = (vr, 0.0);
        for k in 0..ROTATION_STEPS {
            let lambda = (k as f64 + 0.5) / ROTATION_STEPS as f64;
            let cl = lambda * self.inputs.weight_n / self.q_s(v).max(1.0);
            let a = self
                .accel(v, one_engine_out, cl, lambda, WingRise::level(0.0))
                .max(0.0);
            s += (v + 0.5 * a * dt) * dt;
            v += a * dt;
        }
        (v, s)
    }

    /// Rise of the wing over its static, level position at lift-off at
    /// `vlof`: the attitude whose lift in ground effect carries the weight at
    /// `vlof` (the end of the rotation's attitude history), shock absorber
    /// fully extended. Zero without a schedule.
    pub(super) fn liftoff_rise(&self, vlof: f64) -> WingRise {
        self.liftoff_attitude(vlof)
            .map_or(WingRise::level(0.0), |(schedule, theta)| {
                schedule.airframe.wing_rise(theta, 1.0)
            })
    }

    /// The tail-strike attitude less the lift-off attitude at `vlof`,
    /// degrees: the pitch margin to tail contact the rotation leaves at
    /// lift-off. `None` without a schedule.
    pub(super) fn tail_margin_deg(&self, vlof: f64) -> Option<f64> {
        self.liftoff_attitude(vlof)
            .map(|(schedule, theta)| schedule.airframe.tail_strike_deg() - theta)
    }

    /// The schedule and the attitude, degrees, whose lift in ground effect
    /// carries the weight at `vlof`, shock absorber fully extended (no more
    /// than `CLmax,TO`).
    fn liftoff_attitude(&self, vlof: f64) -> Option<(LiftOffSchedule<'_>, f64)> {
        self.inputs.liftoff.map(|schedule| {
            let cl = (self.inputs.weight_n / self.q_s(vlof).max(1.0)).min(schedule.cl_max_to);
            (schedule, schedule.airframe.liftoff_attitude_deg(cl))
        })
    }

    /// The lowest `VR`, no less than `vr_min`, whose rotation lifts off at
    /// the CS 25.107(e)(1)(iv) margin over `VMU`, all engines and one engine
    /// inoperative (`VLOF` does not fall with `VR`, so by bisection); `vr_min`
    /// without a schedule.
    pub(super) fn vr_floor(&self, vr_min: f64) -> f64 {
        let Some(schedule) = self.inputs.liftoff else {
            return vr_min;
        };
        let (aeo_margin, oei_margin) = vmu_margins(schedule.geometry_limited);
        let floor = |margin: f64, one_engine_out: bool| {
            let target = margin * schedule.vmu_m_s;
            let (mut lo, mut hi) = (0.0, target);
            if self.rotation(lo, one_engine_out).0 >= target {
                return lo;
            }
            for _ in 0..50 {
                let mid = 0.5 * (lo + hi);
                if self.rotation(mid, one_engine_out).0 >= target {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            hi
        };
        vr_min
            .max(floor(aeo_margin, false))
            .max(floor(oei_margin, true))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        field_breakdown, search_model, takeoff_field, GroundEffect, TakeoffFieldInputs, ThrustFn,
    };
    use super::*;
    use crate::performance::VSpeeds;

    /// A wing whose lift in ground effect is `0.8 + 0.12 theta` up to a 12
    /// deg tail strike, its aerodynamic centre `ahead_m` ahead of the main
    /// wheels and its trailing edge `chord_m` aft of that, on a shock
    /// absorber of `extension_m` that extends linearly as the wheel load
    /// falls.
    struct Wing {
        ahead_m: f64,
        chord_m: f64,
        extension_m: f64,
    }

    impl RunwayAirframe for Wing {
        fn lift_coefficient(&self, theta_deg: f64, _extension: f64) -> f64 {
            0.8 + 0.12 * theta_deg
        }
        fn wing_rise(&self, theta_deg: f64, extension: f64) -> WingRise {
            let (sin, lift) = (theta_deg.to_radians().sin(), self.extension_m * extension);
            WingRise {
                aerodynamic_centre_m: lift + self.ahead_m * sin,
                trailing_edge_m: lift + (self.ahead_m - self.chord_m) * sin,
            }
        }
        fn extension(&self, wheel_load: f64) -> f64 {
            (1.0 - wheel_load).clamp(0.0, 1.0)
        }
        fn tail_strike_deg(&self) -> f64 {
            12.0
        }
        fn liftoff_attitude_deg(&self, cl: f64) -> f64 {
            ((cl - 0.8) / 0.12).clamp(0.0, 12.0)
        }
    }

    const WEIGHT_N: f64 = 78_000.0 * 9.81;

    fn inputs<'a>(
        airframe: &'a Wing,
        thrust: (ThrustFn<'a>, ThrustFn<'a>),
        v_r_ms: f64,
    ) -> TakeoffFieldInputs<'a> {
        let cl_unstick = airframe.lift_coefficient(12.0, 1.0);
        TakeoffFieldInputs {
            weight_n: WEIGHT_N,
            wing_area_m2: 122.0,
            density_kg_m3: 1.225,
            speeds: VSpeeds {
                v_stall_to_ms: 65.0,
                v_stall_land_ms: 60.0,
                v_mc_ms: 60.0,
                v1_ms: 70.0,
                v_r_ms,
                v2_ms: 78.0,
                v_app_ms: 74.0,
                v_td_ms: 69.0,
            },
            rolling_friction: 0.02,
            stop_deceleration_g: 0.37,
            cd0: 0.04,
            gear_delta_cd: 0.02,
            lift_drag: &|cl| 0.045 * cl * cl,
            ground_effect: GroundEffect::new(34.1, 3.6, 3.0, 2.6),
            failed_engine_cd: 0.006,
            thrust_aeo_n: thrust.0,
            thrust_oei_n: thrust.1,
            airborne_drag: None,
            liftoff: Some(LiftOffSchedule {
                vmu_m_s: (2.0 * WEIGHT_N / (1.225 * 122.0 * cl_unstick)).sqrt(),
                geometry_limited: true,
                pitch_rate_deg_s: ROTATION_PITCH_RATE_DEG_S,
                cl_max_to: 2.6,
                airframe,
            }),
        }
    }

    /// The search contract of the `VMU` floor: an attitude held at the
    /// tail-strike limit (a rotation from rest, or any `VR` whose rotation
    /// reaches that limit first: 40 and 55 m/s here) lifts off at the
    /// geometry-limited `VMU` itself, and above that `VLOF` rises strictly
    /// with `VR` (60 m/s and up lift off below the tail strike).
    #[test]
    fn the_lift_off_speed_rises_with_vr_and_is_vmu_at_the_tail_strike() -> Result<(), String> {
        let aeo = |_: f64| Ok::<_, String>(0.30 * WEIGHT_N);
        let oei = |_: f64| Ok::<_, String>(0.15 * WEIGHT_N);
        let wing = Wing {
            ahead_m: 1.5,
            chord_m: 3.6,
            extension_m: 0.33,
        };
        let field = inputs(&wing, (&aeo, &oei), 71.5);
        let (model, vr_min, _) = search_model(&field)?;
        let vmu = field.liftoff.map_or(f64::NAN, |schedule| schedule.vmu_m_s);
        let (from_rest, _) = model.rotation(1.0, false);
        assert!((from_rest / vmu - 1.0).abs() < 1e-3, "{from_rest} {vmu}");
        // Lifting off at the geometry-limited VMU leaves no tail margin.
        let margin = model.tail_margin_deg(from_rest);
        assert!(margin.is_some_and(|m| m.abs() < 0.1), "{margin:?}");
        let held = [40.0, 55.0].map(|vr| model.rotation(vr, false).0);
        assert!(
            held.iter().all(|v| (v / vmu - 1.0).abs() < 1e-3),
            "{held:?} {vmu}"
        );
        let speeds = [60.0, 65.0, 72.0, 78.0].map(|vr| model.rotation(vr, false).0);
        assert!(
            speeds[0] > held[1] && speeds.windows(2).all(|pair| pair[1] > pair[0]),
            "{speeds:?}"
        );
        // The floor meets the all-engine margin (1.08 VMU, tail contact).
        assert!(model.rotation(vr_min, false).0 >= 1.08 * vmu - 1e-6 || vr_min == 71.5);
        Ok(())
    }

    /// One attitude history: at one `VR` (the search interval closed at
    /// `V2`), a wing that the rotation and the extending gear raise gains
    /// less from the ground than the same wing held at its static height, so
    /// the roll, the rotation, the continued air leg, the all-engine distance
    /// and the field are strictly longer and the engine-out gradient lower.
    /// The all-engine air leg alone is not asserted: here it is the
    /// transition arc of Raymer (the energy distance is shorter), which reads
    /// no drag, flown from the slightly lower `VLOF` of the draggier rotation.
    #[test]
    fn a_wing_raised_by_the_rotation_lengthens_the_field_at_one_vr() -> Result<(), String> {
        let aeo = |_: f64| Ok::<_, String>(0.30 * WEIGHT_N);
        let oei = |_: f64| Ok::<_, String>(0.15 * WEIGHT_N);
        let solve = |wing: &Wing| {
            let field = inputs(wing, (&aeo, &oei), 78.0);
            let solved = takeoff_field(&field)?;
            Ok::<_, String>((solved, field_breakdown(&field, &solved)?))
        };
        let (held, held_legs) = solve(&Wing {
            ahead_m: 0.0,
            chord_m: 0.0,
            extension_m: 0.0,
        })?;
        let (raised, raised_legs) = solve(&Wing {
            ahead_m: 1.5,
            chord_m: 3.6,
            extension_m: 0.33,
        })?;
        assert!((raised.vr_m_s - 78.0).abs() < 1e-9 && (held.vr_m_s - 78.0).abs() < 1e-9);
        assert!(raised_legs.all_engine_roll_m > held_legs.all_engine_roll_m);
        assert!(raised_legs.all_engine_rotation_m > held_legs.all_engine_rotation_m);
        assert!(raised_legs.continued_rotation_m > held_legs.continued_rotation_m);
        assert!(raised_legs.continued_air_m > held_legs.continued_air_m);
        assert!(raised.all_engine_m > held.all_engine_m);
        assert!(
            raised.tofl_m > held.tofl_m,
            "{} {}",
            raised.tofl_m,
            held.tofl_m
        );
        assert!(raised_legs.engine_out_gradient_v2 < held_legs.engine_out_gradient_v2);
        // Above the VMU margin the lift-off attitude stays below the tail strike.
        let margin = raised_legs.tail_margin_deg;
        assert!(margin.is_some_and(|m| m > 0.0), "{margin:?}");
        Ok(())
    }
}
