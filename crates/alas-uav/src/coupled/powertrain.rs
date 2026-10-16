// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mission `Powertrain` trait implemented by powertrain v2.
//!
//! [`ElectricPowertrain`] wraps `power::Powertrain::solve` (motor - ESC -
//! battery - propeller equilibrium with the battery-current limiter).  The
//! mission hands in the energy and charge drawn so far; the state of charge
//! follows from the pack capacity.  The trait is stateless, so the winding
//! temperature is a fixed configured value (cold, ambient by default): the
//! thermal transient of a 3-4 minute mission is outside this adapter.
//!
//! BEMT propellers are tabulated once at the mission density
//! ([`crate::power::tabulate_bemt`]) because a mission integration asks for
//! thousands of equilibria; a propeller that already is a map is used as is.
//!
//! A throttle command at which the motor windmills (airspeed above the
//! free-running speed) has no equilibrium with positive current; it is
//! reported as zero thrust and zero current, never as an error, because the
//! mission legitimately passes through that corner when it coasts.

use super::slipstream_drag::SlipstreamDrag;
use crate::perf::{BatteryState, PerfError, PowerPoint, Powertrain};
use crate::power::{
    self, tabulate_bemt, BatteryState as PackState, Demand, Dynamics, Flight, Limiter, PowerError,
    PropModel, PropSource,
};

/// Adapter settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElectricSettings {
    /// Battery-side current cap of the electronic limiter, A (`None` = none).
    pub battery_current_cap_a: Option<f64>,
    /// Ambient (and pack) temperature, K.
    pub t_amb_k: f64,
    /// Winding temperature held during the mission, K (`None` = ambient).
    pub t_winding_k: Option<f64>,
    /// State of charge at the start, 0..1.
    pub soc_start: f64,
}

impl Default for ElectricSettings {
    fn default() -> Self {
        Self {
            battery_current_cap_a: None,
            t_amb_k: 288.15,
            t_winding_k: None,
            soc_start: 1.0,
        }
    }
}

/// `power::Powertrain` behind the mission interface.
#[derive(Debug, Clone)]
pub struct ElectricPowertrain {
    chain: power::Powertrain,
    settings: ElectricSettings,
    rho: f64,
    /// Where the propeller loads come from after tabulation.
    pub prop_source: PropSource,
    /// Drag of the airframe parts in the propeller wash, subtracted from the
    /// reported thrust (`None`: installed thrust = propeller thrust).
    slipstream: Option<SlipstreamDrag>,
    /// Warnings raised when the adapter was built (low-Re blade sections,
    /// map interpolation error).
    pub warnings: Vec<String>,
}

impl ElectricPowertrain {
    /// Build the adapter at air density `rho` (kg/m3; viscosity from
    /// Sutherland at the ambient temperature).
    ///
    /// # Errors
    ///
    /// [`PowerError`] for a bad setting or a BEMT table that cannot be built.
    pub fn new(
        mut chain: power::Powertrain,
        settings: ElectricSettings,
        rho: f64,
    ) -> Result<Self, PowerError> {
        let ok = (0.0..=1.0).contains(&settings.soc_start)
            && settings.t_amb_k > 200.0
            && settings.battery_current_cap_a.is_none_or(|c| c > 0.0)
            && rho > 0.0;
        if !ok {
            return Err(PowerError::InvalidInput(format!(
                "electric settings out of range: {settings:?}, rho {rho}"
            )));
        }
        let mut warnings = Vec::new();
        let flight = Flight::new(0.0, rho, settings.t_amb_k);
        if chain.prop.bemt_omega_ceiling().is_some() {
            let tab = tabulate_bemt(&chain.prop, rho, flight.mu_pa_s)?;
            if tab.low_re_fraction > 0.0 {
                warnings.push(format!(
                    "{:.0} % of the propeller table has blade Re(0.75R) below {:.0e}: NeuralFoil sections in their warning band",
                    100.0 * tab.low_re_fraction,
                    power::polar::LOW_RE_WARN
                ));
            }
            if tab.max_ct_error > 0.03 {
                warnings.push(format!(
                    "propeller table deviates from the BEMT by {:.1} % in CT",
                    100.0 * tab.max_ct_error
                ));
            }
            chain.prop = tab.model;
        }
        let prop_source = chain.prop.source();
        Ok(Self {
            chain,
            settings,
            rho,
            prop_source,
            slipstream: None,
            warnings,
        })
    }

    /// Report installed thrust: propeller thrust minus the wash drag.
    pub fn with_slipstream_drag(mut self, slipstream: Option<SlipstreamDrag>) -> Self {
        self.slipstream = slipstream;
        self
    }

    /// The wrapped chain (for reports and stand comparisons).
    pub fn chain(&self) -> &power::Powertrain {
        &self.chain
    }

    /// Air density the table was built at, kg/m3.
    pub fn rho(&self) -> f64 {
        self.rho
    }

    /// The propeller model in use (after tabulation).
    pub fn prop(&self) -> &PropModel {
        &self.chain.prop
    }

    fn limiter(&self) -> Limiter {
        Limiter {
            battery_current_cap_a: self.settings.battery_current_cap_a,
            ..Limiter::none()
        }
    }

    /// Full operating point at duty `duty` and airspeed `v` (for stand tests).
    ///
    /// # Errors
    ///
    /// [`PowerError`] from the solver.
    pub fn operating_point(
        &self,
        duty: f64,
        v: f64,
        soc: f64,
    ) -> Result<power::OperatingPoint, PowerError> {
        let fl = Flight::new(v, self.rho, self.settings.t_amb_k);
        let dy = Dynamics {
            battery: PackState::new(soc, self.settings.t_amb_k),
            t_winding_k: self.settings.t_winding_k.unwrap_or(self.settings.t_amb_k),
        };
        self.chain
            .solve(&fl, &dy, Demand::Throttle(duty), &self.limiter())
    }

    /// Thrust after the drag of the parts in the wash (never below zero).
    fn installed_thrust(&self, thrust_n: f64, v: f64) -> f64 {
        let loss = self
            .slipstream
            .as_ref()
            .map_or(0.0, |s| s.drag_n(thrust_n, v, self.rho));
        (thrust_n - loss).max(0.0)
    }

    fn soc(&self, used: &BatteryState) -> f64 {
        (self.settings.soc_start - used.charge_used_c / self.chain.battery.capacity_c())
            .clamp(0.0, 1.0)
    }
}

impl Powertrain for ElectricPowertrain {
    fn evaluate(
        &self,
        throttle: f64,
        airspeed_m_s: f64,
        air_density_kg_m3: f64,
        battery: &BatteryState,
    ) -> Result<PowerPoint, PerfError> {
        if !(throttle.is_finite() && airspeed_m_s.is_finite() && air_density_kg_m3.is_finite()) {
            return Err(PerfError::Powertrain("non-finite operating point".into()));
        }
        if (air_density_kg_m3 - self.rho).abs() > 0.02 * self.rho {
            return Err(PerfError::Powertrain(format!(
                "density {air_density_kg_m3:.3} differs from the {:.3} the propeller table was built at",
                self.rho
            )));
        }
        let soc = self.soc(battery);
        let v_oc = self.chain.battery.voc(soc);
        let duty = throttle.clamp(0.0, 1.0);
        let idle = PowerPoint {
            thrust_n: 0.0,
            battery_current_a: 0.0,
            battery_voltage_v: v_oc,
        };
        if duty <= 1.0e-6 {
            return Ok(idle);
        }
        match self.operating_point(duty, airspeed_m_s.max(0.0), soc) {
            Ok(op) => Ok(PowerPoint {
                thrust_n: self.installed_thrust(op.thrust_n, airspeed_m_s),
                battery_current_a: op.i_battery_a,
                battery_voltage_v: op.v_pack_v,
            }),
            // Windmilling or above the propeller data: no powered equilibrium.
            Err(PowerError::NoSolution(_)) => Ok(idle),
            Err(e) => Err(PerfError::Powertrain(e.to_string())),
        }
    }
}
