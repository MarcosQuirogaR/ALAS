// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Propeller compressibility (tip-Mach) loss and the indicative blade angle.
//!
//! The propeller thrust is the actuator-disk bound on the share of shaft power
//! that survives blade losses, so `eta = T V / P <= eta_ideal(J) <= 1` by
//! construction. This module adds the one loss the blade-efficiency blend does
//! not carry: the compressibility loss at a high helical tip Mach number.
//!
//! **Flag.** The onset (0.89) and the quadratic coefficient are class values
//! that were not re-read from a primary page in this repository; they are
//! stated on the model so a consumer can see them, and they are inert below
//! the onset (the ATR 72 at its cruise rpm sits at about 0.89).

use super::*;

/// Ratio of specific heats of air.
const GAMMA_AIR: f64 = 1.4;
/// Specific gas constant of dry air, J/(kg K).
const GAS_CONSTANT_AIR_J_KG_K: f64 = 287.052_87;
/// Floor of the tip-Mach efficiency factor, so a pathological rpm never
/// zeroes the thrust: a guard, not a working part of the model.
const MINIMUM_TIP_MACH_FACTOR: f64 = 0.3;

impl Pw127m568fModel {
    /// Helical tip Mach number `sqrt((omega R)^2 + V^2) / a`, with the speed
    /// of sound `a = sqrt(gamma R T)` at the ambient static temperature.
    #[must_use]
    pub fn helical_tip_mach(
        self,
        true_airspeed_m_s: f64,
        propeller_speed_rpm: f64,
        ambient_temperature_k: f64,
    ) -> f64 {
        let tip_speed_m_s = propeller_speed_rpm * PI / 60.0 * self.propeller_diameter_m;
        let speed_of_sound_m_s =
            (GAMMA_AIR * GAS_CONSTANT_AIR_J_KG_K * ambient_temperature_k).sqrt();
        tip_speed_m_s.hypot(true_airspeed_m_s) / speed_of_sound_m_s
    }

    /// Multiplier on the blade efficiency for compressibility at a helical tip
    /// Mach number: 1 up to [`Self::tip_mach_critical`], then
    /// `1 - c (M_h - M_crit)^2`, never below 0.3. Always in (0, 1].
    #[must_use]
    pub fn tip_mach_efficiency_factor(self, helical_tip_mach: f64) -> f64 {
        if !helical_tip_mach.is_finite() || helical_tip_mach <= self.tip_mach_critical {
            return 1.0;
        }
        let excess = helical_tip_mach - self.tip_mach_critical;
        (1.0 - self.tip_mach_loss_coefficient * excess * excess).max(MINIMUM_TIP_MACH_FACTOR)
    }

    /// Blade angle of the generic governor surface that absorbs the propeller
    /// power, deg; the nearest pitch bound when none does.
    ///
    /// **Indicative only**: it reads the generic CP polynomial (not a 568F
    /// map) and no longer feeds thrust, which is the energy-consistent bound
    /// in `evaluate_at_temperature`.
    pub(super) fn indicative_blade_angle_deg(
        self,
        density_kg_m3: f64,
        revolutions_s: f64,
        advance_ratio: f64,
        propeller_power_w: f64,
        allow_negative_thrust: bool,
    ) -> f64 {
        if let Ok((angle_deg, _)) = self.solve_governor(
            density_kg_m3,
            revolutions_s,
            advance_ratio,
            propeller_power_w,
            allow_negative_thrust,
        ) {
            return angle_deg;
        }
        let power_scale = density_kg_m3 * revolutions_s.powi(3) * self.propeller_diameter_m.powi(5);
        let lower_deg = self.surrogate.minimum_blade_angle_deg;
        let upper_deg = self.surrogate.maximum_blade_angle_deg;
        let lower_power_w = self.surrogate.coefficients(advance_ratio, lower_deg).1 * power_scale;
        let upper_power_w = self.surrogate.coefficients(advance_ratio, upper_deg).1 * power_scale;
        if (lower_power_w - propeller_power_w).abs() <= (upper_power_w - propeller_power_w).abs() {
            lower_deg
        } else {
            upper_deg
        }
    }
}
