// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The CS 25.107 take-off speed rule as ratios to the reference stall speed.
//!
//! One definition for every consumer that needs the effective speed ratios
//! rather than speeds: the recovery of an effective `CLmax_TO` from a
//! published V2, and the unit tests that tie `alas_perf`'s speed schedule to
//! the same rule. All ratios are relative to the take-off reference stall
//! speed `VSR` read on a 1-g basis (see the module documentation of
//! [`crate::performance`]).
//!
//! - `VMC = vmc_vstall_factor * VSR`. The speed schedules of a built aircraft
//!   carry the `VMC` computed from its one-engine-inoperative yaw balance in
//!   this field (`alas_opt::mdo::performance_with_vmc`); the configured value
//!   is the declared fallback and the cap for a registered aircraft (CS
//!   25.149(c) puts the ceiling at 1.13 VSR).
//! - `VR = max(vr_vmc_factor * VMC, vr_vstall_factor * VSR)`, so
//!   `VR >= 1.05 VMC` (CS 25.107(e)(1)(ii)).
//! - `V1 = max(v1_vr_factor * VR, VMC)`.
//! - `V2MIN = max(v2_min_vsr_factor * VSR, 1.10 * VMC)` (CS 25.107(b)) and
//!   `V2 = max(v2_vstall_factor * VSR, V2MIN, VR)` (CS 25.107(c)); the
//!   "VR plus the speed gained before 35 ft" term needs the rotation
//!   time-march and is taken as zero. The translated legacy replay
//!   (`legacy_field_correlations`) keeps `V2 = max(v2_vstall_factor * VSR, VR)`.

use crate::PerformanceConfig;

/// CS 25.107(b): `V2MIN` is not less than `1.10 VMC`.
pub const V2_MIN_OVER_VMC: f64 = 1.10;

/// The take-off speeds as multiples of the take-off reference stall speed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TakeoffSpeedRatios {
    /// Minimum control speed estimate over `VSR`.
    pub vmc: f64,
    /// Decision speed proxy over `VSR`.
    pub v1: f64,
    /// Rotation speed over `VSR`.
    pub vr: f64,
    /// Take-off safety speed over `VSR`.
    pub v2: f64,
}

impl PerformanceConfig {
    /// The effective take-off speed ratios of this configuration, including a
    /// resolved improved-climb V2 (improved_climb_v2_over_vsr).
    pub fn takeoff_speed_ratios(&self) -> TakeoffSpeedRatios {
        let mut ratios = self.schedule_takeoff_speed_ratios();
        if let Some(improved) = self.improved_climb_v2_over_vsr.filter(|x| x.is_finite()) {
            ratios.v2 = ratios.v2.max(improved);
        }
        ratios
    }

    /// Factor on the jet take-off field length for flying V2 at the improved-climb
    /// speed: the correlation's lift-off speed scales with V2, so the distance
    /// scales with the square of the speed ratio to the schedule (1 when no improved
    /// climb is resolved).
    pub fn improved_climb_field_factor(&self) -> f64 {
        let ratio = self.takeoff_speed_ratios().v2 / self.schedule_takeoff_speed_ratios().v2;
        if ratio.is_finite() && ratio > 1.0 {
            ratio * ratio
        } else {
            1.0
        }
    }

    /// CLmax_TO as the jet take-off field correlation must read it: the declared
    /// value divided by [Self::improved_climb_field_factor].
    pub fn takeoff_field_cl_max(&self, cl_max_to: f64) -> f64 {
        cl_max_to / self.improved_climb_field_factor()
    }

    /// The schedule ratios without any improved-climb override.
    pub fn schedule_takeoff_speed_ratios(&self) -> TakeoffSpeedRatios {
        let vmc = self.vmc_vstall_factor;
        let vr = (self.vr_vmc_factor * vmc).max(self.vr_vstall_factor);
        let v1 = (self.v1_vr_factor * vr).max(vmc);
        let v2_min = if self.legacy_field_correlations {
            0.0
        } else {
            self.v2_min_vsr_factor.max(V2_MIN_OVER_VMC * vmc)
        };
        let v2 = self.v2_vstall_factor.max(v2_min).max(vr);
        TakeoffSpeedRatios { vmc, v1, vr, v2 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_estimate_gives_the_cs25_ratios() {
        let ratios = PerformanceConfig::default().takeoff_speed_ratios();
        assert!((ratios.vmc - 1.13).abs() < 1e-15);
        assert!((ratios.vr - 1.05 * 1.13).abs() < 1e-15);
        assert!((ratios.v2 - 1.10 * 1.13).abs() < 1e-15);
        assert!(ratios.v1 <= ratios.vr && ratios.vr <= ratios.v2);
    }

    #[test]
    fn the_legacy_replay_keeps_the_translated_rule() {
        let config = PerformanceConfig {
            legacy_field_correlations: true,
            ..PerformanceConfig::default()
        };
        assert!((config.takeoff_speed_ratios().v2 - 1.20).abs() < 1e-15);
    }

    #[test]
    fn a_lower_vmc_estimate_lets_the_selected_factor_govern_but_never_below_the_floors() {
        let config = PerformanceConfig {
            vmc_vstall_factor: 1.05,
            ..PerformanceConfig::default()
        };
        // 1.10 * 1.05 = 1.155 and V2MIN 1.13 are both below the selected 1.20.
        assert!((config.takeoff_speed_ratios().v2 - 1.20).abs() < 1e-15);
        // VMC estimate 1.0 VSR: 1.10 VMC = 1.10 < V2MIN 1.13, VR = 1.10.
        let low = PerformanceConfig {
            vmc_vstall_factor: 1.0,
            v2_vstall_factor: 1.0,
            ..PerformanceConfig::default()
        };
        assert!((low.takeoff_speed_ratios().v2 - 1.13).abs() < 1e-15);
    }
}
