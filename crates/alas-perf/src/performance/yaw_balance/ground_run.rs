// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Ground minimum control speed `VMCG` (CS 25.149(e)) from the take-off run
//! after a sudden failure of the critical engine, in the three-degree-of-
//! freedom model of Koolstra, Huijbrechts and Mulder, "Analyzing Aircraft
//! Controllability After Engine Failure During Takeoff in Adverse Weather
//! Conditions", *Journal of Aircraft* 56(4), 2019, pp. 1330-1341
//! (doi:10.2514/1.C035219), Sec. II:
//!
//! - body-axis surge, sway and yaw (Eqs. 16-18) on a stiff gear, wings
//!   level; the runway is the inertial frame;
//! - nose wheel free to castor, no nose-wheel steering (CS 25.149(e)): no
//!   side force on the nose wheel; the main-wheel side force is the normal
//!   load times the dry-concrete side-force coefficient of Eq. (19),
//!   `mu_s = 0.39 exp(-0.015 V^0.5) arctan(0.33 beta_g)` with `V` in knots
//!   and the gear slip angle `beta_g` in degrees;
//! - the failed engine's thrust decays linearly to its windmilling drag in
//!   0.6 s, the pilot holds the rudder for 0.5 s and then deflects it at
//!   1 rad/s to the stop, held while the lateral deviation still grows
//!   (Sec. II.G-H; with these values the model reproduces the certified
//!   107 kt `VMCG` of the 737-300/20K within 0.7 ft, Sec. II.H);
//! - `VMCG` is the engine-failure speed whose largest lateral deviation is
//!   30 ft (CS 25.149(e)).
//!
//! The fin forces are those of the airborne balance ([`YawControlGeometry`]):
//! the local fin angle of attack includes the yaw-rate term `r l_v / V`,
//! which is the fin's yaw damping. The airframe's side force in sideslip is
//! that of the airborne balance; the wing's yaw damping is not modelled. The yaw inertia is the caller's (Raymer Eq.
//! 16.54). Thrust of the live engine is held at its value at the failure
//! speed for the few seconds of the transient, as in the reference model.

use super::{rudder_large_deflection_factor, YawControlGeometry};

/// Pilot reaction time before the rudder moves, s (Koolstra et al. 2019,
/// Sec. II.H, after Bolds-Moorehead et al., AIAA 2015-3226, p. 15: target
/// 0.4-0.6 s in certification tests).
pub const VMCG_PILOT_REACTION_S: f64 = 0.5;
/// Linear thrust decay of the failed engine after a fuel cut, s (Koolstra et
/// al. 2019, Sec. II.H).
pub const VMCG_THRUST_DECAY_S: f64 = 0.6;
/// Rudder deflection rate of the pilot input, rad/s (Koolstra et al. 2019,
/// Sec. II.H).
pub const VMCG_RUDDER_RATE_RAD_S: f64 = 1.0;
/// Largest lateral deviation from the centreline, m: 30 ft (CS 25.149(e)).
pub const VMCG_LATERAL_LIMIT_M: f64 = 30.0 * 0.3048;
/// Rolling friction coefficient of the free-rolling wheels (Koolstra et al.
/// 2019, Sec. II.D).
pub const VMCG_ROLLING_FRICTION: f64 = 0.015;

const STEP_S: f64 = 0.005;
const MAX_TIME_S: f64 = 30.0;
const KNOT: f64 = 0.514_444;

/// The ground-run data that the airborne yaw geometry does not carry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundRunGeometry {
    /// Aircraft mass, kg.
    pub mass_kg: f64,
    /// Yaw moment of inertia, kg m^2.
    pub yaw_inertia_kg_m2: f64,
    /// Main-gear station aft of the centre of gravity, m (positive aft).
    pub main_gear_arm_m: f64,
    /// Centre of gravity aft of the nose gear, m (positive aft).
    pub nose_gear_arm_m: f64,
    /// Lift coefficient of the ground run on the reference area.
    pub ground_lift_coefficient: f64,
    /// Zero-lift drag coefficient of the ground run on the reference area.
    pub ground_drag_coefficient: f64,
}

/// One engine-failure case: the thrust of each engine at the failure speed
/// and the failed engine's windmilling drag.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundRunThrust {
    /// Take-off thrust per engine, N.
    pub thrust_per_engine_n: f64,
    /// Windmilling drag of the failed engine, N (positive).
    pub windmilling_drag_n: f64,
    /// Total thrust of all engines before the failure, N.
    pub total_thrust_n: f64,
}

/// NASA dry-concrete side-force coefficient of the main wheels (Koolstra et
/// al. 2019 Eq. 19); `speed_m_s` ground speed, `slip_rad` gear slip angle.
pub fn dry_runway_side_force_coefficient(speed_m_s: f64, slip_rad: f64) -> f64 {
    let knots = (speed_m_s / KNOT).max(0.0);
    0.39 * (-0.015 * knots.sqrt()).exp() * (0.33 * slip_rad.abs().to_degrees()).atan()
}

impl YawControlGeometry {
    /// Largest lateral deviation, m, after the critical engine fails at
    /// `failure_speed_m_s` on the ground (see the module notes). `thrust`
    /// gives the engine forces at that speed. `None` for unusable inputs.
    pub fn ground_run_lateral_deviation_m(
        &self,
        ground: &GroundRunGeometry,
        thrust: &GroundRunThrust,
        failure_speed_m_s: f64,
        rho_kg_m3: f64,
        speed_of_sound_m_s: f64,
    ) -> Option<f64> {
        let inputs_ok = self.is_usable()
            && [
                ground.mass_kg,
                ground.yaw_inertia_kg_m2,
                ground.main_gear_arm_m,
                ground.nose_gear_arm_m,
                failure_speed_m_s,
                rho_kg_m3,
                speed_of_sound_m_s,
            ]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
            && thrust.thrust_per_engine_n.is_finite()
            && thrust.windmilling_drag_n.is_finite()
            && thrust.total_thrust_n.is_finite();
        if !inputs_ok {
            return None;
        }
        let authority = self.fin_authority(failure_speed_m_s / speed_of_sound_m_s)?;
        let g = &self.balance;
        let gravity = 9.80665;
        let weight = ground.mass_kg * gravity;
        let wheelbase = ground.main_gear_arm_m + ground.nose_gear_arm_m;
        let main_share = ground.nose_gear_arm_m / wheelbase;
        let delta_max = self.rudder_max_deflection_rad;
        // Body axes, y toward the failed engine; the heading `psi` and the
        // lateral position `y` are positive toward the failed engine too.
        let (mut u, mut v, mut r, mut psi, mut y): (f64, f64, f64, f64, f64) =
            (failure_speed_m_s, 0.0, 0.0, 0.0, 0.0);
        let mut largest = 0.0_f64;
        let mut t = 0.0;
        while t < MAX_TIME_S {
            let q = 0.5 * rho_kg_m3 * u * u;
            // The failed engine's thrust falls linearly to minus its
            // windmilling drag; the live engine keeps its thrust.
            let failed = if t < VMCG_THRUST_DECAY_S {
                thrust.thrust_per_engine_n * (1.0 - t / VMCG_THRUST_DECAY_S)
                    - thrust.windmilling_drag_n * t / VMCG_THRUST_DECAY_S
            } else {
                -thrust.windmilling_drag_n
            };
            let asymmetric_moment = (thrust.thrust_per_engine_n - failed) * g.engine_arm_m;
            let delta =
                ((t - VMCG_PILOT_REACTION_S).max(0.0) * VMCG_RUDDER_RATE_RAD_S).min(delta_max);
            let delta_linear =
                delta * rudder_large_deflection_factor(delta.to_degrees(), self.rudder_chord_ratio);
            // Nose-from-velocity angle (positive with the nose toward the
            // failed engine) and the fin's local flow angle with yaw rate.
            let beta = (-v).atan2(u);
            let fin_flow = (-(v - r * g.fin_arm_m)).atan2(u);
            let fin_force = q
                * authority.area_slope_m2
                * (self.fin_sidewash_factor * fin_flow + authority.tau * delta_linear);
            let body_moment =
                -self.cn_beta_without_fin_per_rad * q * self.wing_area_m2 * self.wing_span_m * beta;
            // The airframe's side force in sideslip, in the sense of the
            // fin's sideslip force (Roskam Part VI Eq. 10.25).
            let body_force = -self.side_force_without_fin_per_rad * q * self.wing_area_m2 * beta;
            // Main wheels: side force against the lateral slip at the gear.
            let lift = q * self.wing_area_m2 * ground.ground_lift_coefficient;
            let main_load = ((weight - lift) * main_share).max(0.0);
            let gear_v = v - r * ground.main_gear_arm_m;
            let slip = gear_v.abs().atan2(u);
            let gear_force =
                -gear_v.signum() * main_load * dry_runway_side_force_coefficient(u, slip);
            let side_force = fin_force + body_force + gear_force;
            let yaw_moment = asymmetric_moment - g.fin_arm_m * fin_force + body_moment
                - ground.main_gear_arm_m * gear_force;
            let drag = q * self.wing_area_m2 * ground.ground_drag_coefficient;
            let thrust_now = thrust.total_thrust_n - thrust.thrust_per_engine_n + failed;
            let axial = thrust_now - drag - VMCG_ROLLING_FRICTION * (weight - lift).max(0.0);
            let du = axial / ground.mass_kg + r * v;
            let dv = side_force / ground.mass_kg - r * u;
            let dr = yaw_moment / ground.yaw_inertia_kg_m2;
            let dy = u * psi.sin() + v * psi.cos();
            u += du * STEP_S;
            v += dv * STEP_S;
            r += dr * STEP_S;
            psi += r * STEP_S;
            y += dy * STEP_S;
            t += STEP_S;
            if y > largest {
                largest = y;
            } else if delta >= delta_max && y < largest {
                // Full rudder holds and the deviation has turned: its peak.
                return Some(largest);
            }
            if largest > 10.0 * VMCG_LATERAL_LIMIT_M {
                return Some(largest);
            }
        }
        Some(largest)
    }

    /// `VMCG`, true airspeed in m/s: the engine-failure speed whose largest
    /// lateral deviation is [`VMCG_LATERAL_LIMIT_M`]. `thrust_at` gives the
    /// engine forces at a speed. `None` when the deviation does not fall
    /// through the limit between 10 and 150 m/s or the inputs are unusable.
    pub fn ground_minimum_control_speed(
        &self,
        ground: &GroundRunGeometry,
        thrust_at: &dyn Fn(f64) -> Option<GroundRunThrust>,
        rho_kg_m3: f64,
        speed_of_sound_m_s: f64,
    ) -> Option<f64> {
        let excess = |speed: f64| -> Option<f64> {
            let thrust = thrust_at(speed)?;
            Some(
                self.ground_run_lateral_deviation_m(
                    ground,
                    &thrust,
                    speed,
                    rho_kg_m3,
                    speed_of_sound_m_s,
                )? - VMCG_LATERAL_LIMIT_M,
            )
        };
        let (mut low, mut high) = (10.0_f64, 150.0_f64);
        if excess(high)? > 0.0 || excess(low)? <= 0.0 {
            return None;
        }
        for _ in 0..40 {
            let mid = 0.5 * (low + high);
            if excess(mid)? > 0.0 {
                low = mid;
            } else {
                high = mid;
            }
        }
        Some(high)
    }
}
