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
//! - Rotation: 3 s (Raymer p. 673), integrated with the wheel load falling
//!   linearly to zero at lift-off.
//! - Airborne: the larger of the energy distance `((V2^2 - VLOF^2)/2g + h) /
//!   gamma` (Torenbeek Eqs. 5-85/5-86, p. 169) and the kinematic transition
//!   arc of Raymer Eqs. 17.105-17.112 (pp. 673-674, n = 1.2); each is a
//!   necessary condition on reaching 35 ft at V2.
//! - Accelerate-stop: CS 25.109(a)(1): all-engine roll to `VEF`, one-engine
//!   roll to `V1`, braking at the mean deceleration of 15 jet transports
//!   (Torenbeek Eq. 5-87 and p. 169 note 2: 0.37 g, no reverse thrust), plus
//!   2 s at `V1`. The failure is recognised 1 s after `VEF` (Raymer p. 676;
//!   CS 25.107(a)(2): `V1 >= VEF` plus the speed gained meanwhile).
//! - `VR` is selected by the applicant (CS 25.107(e)); it is searched between
//!   the schedule minimum and `V2` for the shortest field, as Raymer p. 676
//!   describes ("delayed to a higher speed ... calculated to minimise BFL").
//!
//! Limits: dry level hard runway, no wind, no clearway or stopway, no
//! reverse thrust or lift dump, no ground effect (CS 25.111(d)(3)), gear down
//! to 35 ft, constant `mu` and `CL` on the ground. Conceptual, not certified.

use super::{VSpeeds, G};

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
/// Factor on the all-engine distance: CS 25.113(a)(2).
pub const ALL_ENGINE_FACTOR: f64 = 1.15;
/// Screen height, m: CS 25.113 (35 ft).
pub const SCREEN_HEIGHT_M: f64 = 35.0 * alas_units::FOOT;
/// Load factor of the transition arc: Raymer Eqs. 17.105-17.107, p. 674.
pub const TRANSITION_LOAD_FACTOR: f64 = 1.2;
/// Ground lift coefficient: "typically less than 0.1" (Raymer p. 672).
pub const GROUND_LIFT_COEFFICIENT: f64 = 0.1;

const THRUST_POINTS: usize = 33;
const ROLL_CELLS: usize = 256;
const VR_POINTS: usize = 17;
const ROTATION_STEPS: usize = 30;
const MIN_GRADIENT: f64 = 0.002;

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
    /// Quadratic induced-drag factor.
    pub induced_factor: f64,
    /// Failed-engine (windmilling plus trim) drag coefficient, one-engine paths.
    pub failed_engine_cd: f64,
    /// All-engine thrust at speed.
    pub thrust_aeo_n: ThrustFn<'a>,
    /// Thrust with the critical engine failed at speed.
    pub thrust_oei_n: ThrustFn<'a>,
}

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

    /// Ground acceleration, m/s^2, at lift coefficient `cl`, with `unload` the
    /// fraction of the weight carried by the wings.
    fn accel(&self, v: f64, one_engine_out: bool, cl: f64, unload: f64) -> f64 {
        let i = self.inputs;
        let cd =
            i.cd0 + i.gear_delta_cd + self.failed_cd(one_engine_out) + i.induced_factor * cl * cl;
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
            let q_s = self.q_s(mid);
            let unload = q_s * GROUND_LIFT_COEFFICIENT / self.inputs.weight_n;
            let a = self.accel(mid, one_engine_out, GROUND_LIFT_COEFFICIENT, unload);
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

    /// Rotation to lift-off: wheel load falls linearly to zero.
    fn rotation(&self, vr: f64, one_engine_out: bool) -> (f64, f64) {
        let dt = ROTATION_TIME_S / ROTATION_STEPS as f64;
        let (mut v, mut s) = (vr, 0.0);
        for k in 0..ROTATION_STEPS {
            let lambda = (k as f64 + 0.5) / ROTATION_STEPS as f64;
            let q_s = self.q_s(v).max(1.0);
            let cl = lambda * self.inputs.weight_n / q_s;
            let a = self.accel(v, one_engine_out, cl, lambda).max(0.0);
            s += (v + 0.5 * a * dt) * dt;
            v += a * dt;
        }
        (v, s)
    }

    /// Climb gradient `(T - D)/W` at speed `v`, lift equal to weight, gear down.
    fn gradient(&self, v: f64, one_engine_out: bool) -> f64 {
        let i = self.inputs;
        let cl = i.weight_n / self.q_s(v);
        let cd =
            i.cd0 + i.gear_delta_cd + self.failed_cd(one_engine_out) + i.induced_factor * cl * cl;
        (self.thrust_n(v, one_engine_out) - self.q_s(v) * cd) / i.weight_n
    }

    /// Distance from lift-off at `vlof` to 35 ft at no less than `V2`, m.
    fn air(&self, vlof: f64, one_engine_out: bool) -> f64 {
        let v2 = self.inputs.speeds.v2_ms.max(vlof);
        let gamma_c = self.gradient(v2, one_engine_out);
        let mean = 0.5 * (self.gradient(vlof, one_engine_out) + gamma_c);
        if !(gamma_c > MIN_GRADIENT && mean > MIN_GRADIENT) {
            return f64::INFINITY;
        }
        let energy = ((v2 * v2 - vlof * vlof) / (2.0 * G) + SCREEN_HEIGHT_M) / mean;
        let angle = gamma_c.min(0.99).asin();
        let radius = (0.5 * (vlof + v2)).powi(2) / ((TRANSITION_LOAD_FACTOR - 1.0) * G);
        let arc_height = radius * (1.0 - angle.cos());
        let kinematic = if arc_height >= SCREEN_HEIGHT_M {
            (2.0 * radius * SCREEN_HEIGHT_M - SCREEN_HEIGHT_M.powi(2)).sqrt()
        } else {
            radius * angle.sin() + (SCREEN_HEIGHT_M - arc_height) / angle.tan()
        };
        energy.max(kinematic)
    }
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
        ("induced factor", inputs.induced_factor),
        ("VR", inputs.speeds.v_r_ms),
        ("V2", inputs.speeds.v2_ms),
    ] {
        if !(value.is_finite() && value > 0.0) {
            return Err(format!(
                "take-off field {name} must be positive, got {value}"
            ));
        }
    }
    if ![inputs.cd0, inputs.gear_delta_cd, inputs.failed_engine_cd]
        .iter()
        .all(|x| x.is_finite() && *x >= 0.0)
    {
        return Err("take-off drag coefficients must be finite and nonnegative".to_owned());
    }
    let (vr_min, v2) = (
        inputs.speeds.v_r_ms,
        inputs.speeds.v2_ms.max(inputs.speeds.v_r_ms),
    );
    let top = 1.3 * v2;
    let model = Model {
        inputs,
        thrust: Table::sample(top, inputs.thrust_aeo_n)?,
        thrust_oei: Table::sample(top, inputs.thrust_oei_n)?,
    };
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
                + STOP_ALLOWANCE_S * v1
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
            induced_factor: 0.045,
            failed_engine_cd: 0.006,
            thrust_aeo_n: &aeo,
            thrust_oei_n: &oei,
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
            induced_factor: 0.045,
            failed_engine_cd: 0.006,
            thrust_aeo_n: &aeo,
            thrust_oei_n: &oei,
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
            induced_factor: 0.05,
            failed_engine_cd: 0.0,
            thrust_aeo_n: &flat,
            thrust_oei_n: &flat,
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
