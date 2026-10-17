// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The narrow powertrain interface of the mission model and a test double.

use super::PerfError;

/// Battery state the mission integrates and hands back to the powertrain.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BatteryState {
    /// Energy drawn from the pack so far, J.
    pub energy_used_j: f64,
    /// Charge drawn from the pack so far, C (A*s).
    pub charge_used_c: f64,
}

/// Powertrain output at one operating point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PowerPoint {
    /// Net thrust along the flight path, N (>= 0; a windmilling propeller
    /// reports 0 or the drag it adds as a negative value).
    pub thrust_n: f64,
    /// Battery-side current, A (>= 0).
    pub battery_current_a: f64,
    /// Battery terminal voltage, V.
    pub battery_voltage_v: f64,
}

/// Thrust and electrical load as a function of throttle and flight condition.
///
/// `throttle` is the commanded fraction of full duty in `[0, 1]`; the model
/// owns any current or power limiting (ESC limiter, 450 W limiter), so thrust
/// and current returned are what the aircraft actually produces. `airspeed` is
/// the true airspeed along the thrust line, m/s. Implementations must be
/// deterministic and free of hidden state: the same inputs give the same
/// outputs (the mission calls the model several times per step).
pub trait Powertrain {
    /// Operating point.
    fn evaluate(
        &self,
        throttle: f64,
        airspeed_m_s: f64,
        air_density_kg_m3: f64,
        battery: &BatteryState,
    ) -> Result<PowerPoint, PerfError>;
}

/// A test double, NOT a physical motor model: thrust falls linearly with
/// airspeed, current follows `throttle^1.5`, the open-circuit voltage sags
/// with drawn charge and with the internal resistance. Constants are
/// whatever the test chooses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StubPowertrain {
    /// Static thrust at full throttle, N.
    pub static_thrust_n: f64,
    /// Airspeed at which full-throttle thrust reaches zero, m/s.
    pub zero_thrust_speed_m_s: f64,
    /// Battery current at full throttle and zero airspeed, A.
    pub max_current_a: f64,
    /// Open-circuit voltage of the full pack, V.
    pub open_circuit_v: f64,
    /// Pack internal resistance, ohm.
    pub internal_resistance_ohm: f64,
    /// Pack capacity, C.
    pub capacity_c: f64,
}

impl Powertrain for StubPowertrain {
    fn evaluate(
        &self,
        throttle: f64,
        airspeed_m_s: f64,
        _air_density_kg_m3: f64,
        battery: &BatteryState,
    ) -> Result<PowerPoint, PerfError> {
        if !(throttle.is_finite() && airspeed_m_s.is_finite()) {
            return Err(PerfError::Powertrain("non-finite operating point".into()));
        }
        let th = throttle.clamp(0.0, 1.0);
        let speed_loss = (1.0 - airspeed_m_s.max(0.0) / self.zero_thrust_speed_m_s).max(0.0);
        let current = self.max_current_a * th.powf(1.5) * (1.0 - 0.5 * (1.0 - speed_loss));
        let depth = (battery.charge_used_c / self.capacity_c).clamp(0.0, 1.0);
        let voltage =
            self.open_circuit_v * (1.0 - 0.15 * depth) - self.internal_resistance_ohm * current;
        Ok(PowerPoint {
            thrust_n: self.static_thrust_n * th * speed_loss,
            battery_current_a: current,
            battery_voltage_v: voltage,
        })
    }
}
