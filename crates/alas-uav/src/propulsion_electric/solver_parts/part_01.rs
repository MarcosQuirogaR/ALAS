// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use crate::catalog::ComponentKind;
use crate::optimizer::{PropulsionMap, PropulsionOperatingPoint};
use crate::Catalog;

use super::{apc_performance_map, ElectricPropulsionError, PropellerPerformanceMap};

const TAU: f64 = std::f64::consts::PI * 2.0;
const BISECTION_ITERATIONS: usize = 96;

/// Selected source-backed propulsion hardware.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CataloguePowertrainSelection {
    /// Battery catalogue identifier.
    pub battery_id: String,
    /// Motor catalogue identifier.
    pub motor_id: String,
    /// Electronic-speed-controller catalogue identifier.
    pub esc_id: String,
    /// Propeller catalogue identifier.
    pub propeller_id: String,
    /// Number of identical propulsors fed by the selected battery.
    pub motor_count: u16,
}

impl CataloguePowertrainSelection {
    /// Construct a single-propulsor fixed-wing selection.
    pub fn single_motor(
        battery_id: impl Into<String>,
        motor_id: impl Into<String>,
        esc_id: impl Into<String>,
        propeller_id: impl Into<String>,
    ) -> Self {
        Self {
            battery_id: battery_id.into(),
            motor_id: motor_id.into(),
            esc_id: esc_id.into(),
            propeller_id: propeller_id.into(),
            motor_count: 1,
        }
    }
}

/// Flight condition used to calculate one propulsion operating point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElectricFlightCondition {
    /// True airspeed.
    pub speed_m_s: f64,
    /// Local air density.
    pub air_density_kg_m3: f64,
    /// ESC command as a fraction from zero to one.
    pub throttle: f64,
}

impl ElectricFlightCondition {
    /// Full-throttle condition used for available-thrust maps.
    pub const fn full_power(speed_m_s: f64, air_density_kg_m3: f64) -> Self {
        Self {
            speed_m_s,
            air_density_kg_m3,
            throttle: 1.0,
        }
    }
}

/// Electrical parameters for a brushless motor winding.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElectricMotorModel {
    /// Back-EMF speed constant in RPM per volt.
    pub kv_rpm_per_v: f64,
    /// No-load current at the model's nominal condition.
    pub no_load_current_a: f64,
    /// Winding resistance.
    pub resistance_ohm: f64,
}

impl ElectricMotorModel {
    fn torque_constant_nm_per_a(self) -> f64 {
        60.0 / (TAU * self.kv_rpm_per_v)
    }
}

/// Pack model with an optional measured DC internal resistance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedVoltageBattery {
    /// Open-circuit or fixed nominal voltage chosen for the calculation.
    pub open_circuit_voltage_v: f64,
    /// Measured pack resistance. Zero represents the legacy fixed-voltage model.
    pub internal_resistance_ohm: f64,
}

/// Series-resistance approximation for an ESC.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EscElectricalModel {
    /// Aggregate conduction resistance. Zero represents the legacy lossless ESC.
    pub series_resistance_ohm: f64,
}

/// Complete source-bounded model for one or more identical propulsors.
#[derive(Debug, Clone, PartialEq)]
pub struct ElectricPowertrainModel {
    /// Battery terminal model.
    pub battery: FixedVoltageBattery,
    /// Motor winding model.
    pub motor: ElectricMotorModel,
    /// ESC loss model.
    pub esc: EscElectricalModel,
    /// Reviewed propeller performance table.
    pub propeller: PropellerPerformanceMap,
    /// Number of identical motors and propellers on the shared pack.
    pub motor_count: u16,
}

/// Per-motor values at a converged electrical and propeller equilibrium.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElectricalOperatingPoint {
    /// Shaft speed.
    pub rpm: f64,
    /// Propeller torque required at the shaft.
    pub propeller_torque_nm: f64,
    /// Thrust produced by one propeller.
    pub thrust_n: f64,
    /// Mechanical shaft power delivered to one propeller.
    pub shaft_power_w: f64,
    /// Current flowing in one motor winding.
    pub motor_current_a: f64,
    /// Motor terminal voltage after ESC drop.
    pub motor_terminal_voltage_v: f64,
    /// Electrical power at the motor terminals.
    pub motor_electrical_power_w: f64,
    /// Winding I-squared-R loss.
    pub motor_copper_loss_w: f64,
}

/// Source-limited rating result accompanying a calculated operating point.
#[derive(Debug, Clone, PartialEq)]
pub enum ElectricalCheck {
    /// A published maximum current is exceeded.
    CurrentOverload {
        /// Component whose published limit is exceeded.
        component_id: String,
        /// Calculated current.
        actual_a: f64,
        /// Published maximum current.
        limit_a: f64,
    },
    /// A published maximum electrical power is exceeded.
    PowerOverload {
        /// Motor whose published limit is exceeded.
        component_id: String,
        /// Calculated electrical power.
        actual_w: f64,
        /// Published maximum electrical power.
        limit_w: f64,
    },
    /// A required source value is absent and has not been inferred.
    Unverified {
        /// Component lacking evidence.
        component_id: String,
        /// Missing source-backed value.
        field: &'static str,
    },
}

/// Full coupled result, including totals across the selected propulsor count.
#[derive(Debug, Clone, PartialEq)]
pub struct ElectricPropulsionResult {
    /// Flight condition at which the point was evaluated.
    pub condition: ElectricFlightCondition,
    /// Per-motor electrical and propeller result.
    pub per_motor: ElectricalOperatingPoint,
    /// Battery terminal voltage after modeled voltage sag.
    pub battery_terminal_voltage_v: f64,
    /// Total current drawn from the selected pack.
    pub battery_current_a: f64,
    /// Total electrical power drawn from the selected pack.
    pub battery_power_w: f64,
    /// Aggregate installed thrust.
    pub total_thrust_n: f64,
    /// Aggregate propeller shaft power.
    pub total_shaft_power_w: f64,
    /// Published-limit overloads and source-evidence gaps.
    pub checks: Vec<ElectricalCheck>,
    /// Assumptions that remain outside the selected source evidence.
    pub assumptions: Vec<String>,
}

/// Solve a generic coupled electrical model without catalogue-specific limits.
pub fn solve_powertrain(
    model: &ElectricPowertrainModel,
    condition: ElectricFlightCondition,
) -> Result<ElectricPropulsionResult, ElectricPropulsionError> {
    validate_model(model, condition)?;
    if condition.throttle == 0.0 {
        return Ok(ElectricPropulsionResult {
            condition,
            per_motor: ElectricalOperatingPoint {
                rpm: 0.0,
                propeller_torque_nm: 0.0,
                thrust_n: 0.0,
                shaft_power_w: 0.0,
                motor_current_a: 0.0,
                motor_terminal_voltage_v: 0.0,
                motor_electrical_power_w: 0.0,
                motor_copper_loss_w: 0.0,
            },
            battery_terminal_voltage_v: model.battery.open_circuit_voltage_v,
            battery_current_a: 0.0,
            battery_power_w: 0.0,
            total_thrust_n: 0.0,
            total_shaft_power_w: 0.0,
            checks: Vec::new(),
            assumptions: Vec::new(),
        });
    }

    let (minimum_rpm, maximum_rpm) = model
        .propeller
        .rpm_bounds_at_speed(condition.speed_m_s)
        .ok_or(ElectricPropulsionError::PerformanceTableOutOfRange {
            speed_m_s: condition.speed_m_s,
        })?;
    let lower = residual(model, condition, minimum_rpm)?;
    let upper = residual(model, condition, maximum_rpm)?;
    if lower < 0.0 || upper > 0.0 {
        return Err(ElectricPropulsionError::NoEquilibrium {
            speed_m_s: condition.speed_m_s,
            minimum_rpm,
            maximum_rpm,
        });
    }

    let rpm = if lower == 0.0 {
        minimum_rpm
    } else if upper == 0.0 {
        maximum_rpm
    } else {
        bisect(model, condition, minimum_rpm, maximum_rpm)?
    };
    let load = propeller_load(model, condition, rpm)?;
    let motor_current_a = load.propeller_torque_nm / model.motor.torque_constant_nm_per_a()
        + model.motor.no_load_current_a;
    let battery_current_a = f64::from(model.motor_count) * condition.throttle * motor_current_a;
    let battery_terminal_voltage_v = model.battery.open_circuit_voltage_v
        - battery_current_a * model.battery.internal_resistance_ohm;
    let motor_terminal_voltage_v = condition.throttle * battery_terminal_voltage_v
        - motor_current_a * model.esc.series_resistance_ohm;
    let shaft_power_w = load.propeller_torque_nm * rpm * TAU / 60.0;
    let motor_electrical_power_w = motor_terminal_voltage_v * motor_current_a;
    let motor_copper_loss_w = motor_current_a.powi(2) * model.motor.resistance_ohm;

    Ok(ElectricPropulsionResult {
        condition,
        per_motor: ElectricalOperatingPoint {
            rpm,
            propeller_torque_nm: load.propeller_torque_nm,
            thrust_n: load.thrust_n,
            shaft_power_w,
            motor_current_a,
            motor_terminal_voltage_v,
            motor_electrical_power_w,
            motor_copper_loss_w,
        },
        battery_terminal_voltage_v,
        battery_current_a,
        battery_power_w: battery_terminal_voltage_v * battery_current_a,
        total_thrust_n: load.thrust_n * f64::from(model.motor_count),
        total_shaft_power_w: shaft_power_w * f64::from(model.motor_count),
        checks: Vec::new(),
        assumptions: Vec::new(),
    })
}

/// Resolve a reviewed selection and calculate its electrical operating point.
pub fn solve_catalogue_powertrain(
    catalog: &Catalog,
    selection: &CataloguePowertrainSelection,
    condition: ElectricFlightCondition,
) -> Result<ElectricPropulsionResult, ElectricPropulsionError> {
    let resolved = resolve_catalogue_powertrain(catalog, selection)?;
    let mut result = solve_powertrain(&resolved.model, condition)?;
    result.checks = catalogue_checks(&resolved, &result);
    result.assumptions = vec![
        "The selected pack is held at its source-stated nominal voltage unless a measured internal resistance is supplied through ElectricPowertrainModel.".to_owned(),
        "The selected ESC uses the legacy zero-loss model because no reviewed switching or conduction-loss curve is available.".to_owned(),
        "Thermal state, battery state of charge, wiring resistance, and propeller installation effects are not represented by this source table.".to_owned(),
    ];
    if !super::has_winding_model(resolved.motor) {
        result.assumptions.push(
            "The motor has Kv but no complete winding model; missing resistance or no-load current is represented as zero, so this is an unverified ideal-motor estimate.".to_owned(),
        );
    }
    Ok(result)
}

/// Produce a full-throttle, per-propulsor map for the preliminary optimizer.
///
/// Each stored point is one motor/ESC/propeller's demand; `motor_count` on
/// the returned map identifies how many identical propulsors share the pack.
/// This keeps component ratings per motor while allowing the optimizer to
/// derive aircraft thrust, mass, and battery demand without ambiguity.
pub fn native_propulsion_map(
    catalog: &Catalog,
    selection: &CataloguePowertrainSelection,
    speeds_m_s: &[f64],
    air_density_kg_m3: f64,
) -> Result<PropulsionMap, ElectricPropulsionError> {
    if !air_density_kg_m3.is_finite() || air_density_kg_m3 <= 0.0 || speeds_m_s.is_empty() {
        return Err(ElectricPropulsionError::InvalidInput(
            "a native propulsion map needs a positive density and at least one speed".to_owned(),
        ));
    }
    let mut speeds = speeds_m_s.to_vec();
    speeds.sort_by(f64::total_cmp);
    if speeds
        .windows(2)
        .any(|window| !window[0].is_finite() || window[0] < 0.0 || window[0] == window[1])
        || speeds
            .last()
            .is_some_and(|speed| !speed.is_finite() || *speed < 0.0)
    {
        return Err(ElectricPropulsionError::InvalidInput(
            "propulsion-map speeds must be distinct finite non-negative values".to_owned(),
        ));
    }
    let series_cells = selected_series_cells(catalog, &selection.battery_id)?;
    let resolved = resolve_catalogue_powertrain(catalog, selection)?;
    let performance_file = resolved.model.propeller.source_file().to_owned();
    let mut points = Vec::with_capacity(speeds.len());
    for speed_m_s in speeds {
        let result = solve_catalogue_powertrain(
            catalog,
            selection,
            ElectricFlightCondition::full_power(speed_m_s, air_density_kg_m3),
        )?;
        points.push(PropulsionOperatingPoint {
            speed_m_s,
            thrust_n: result.per_motor.thrust_n,
            motor_current_a: result.per_motor.motor_current_a,
            motor_power_w: result.per_motor.motor_electrical_power_w,
        });
    }
    Ok(PropulsionMap {
        motor_id: selection.motor_id.clone(),
        propeller_id: selection.propeller_id.clone(),
        series_cells,
        motor_count: selection.motor_count,
        evidence: format!(
            "native coupled electric solver; catalogue motor electrical inputs{} and APC {} coefficient table",
            super::ideal_motor_note(resolved.motor),
            performance_file
        ),
        points,
    })
}

struct ResolvedCataloguePowertrain<'a> {
    model: ElectricPowertrainModel,
    battery_id: &'a str,
    motor_id: &'a str,
    esc_id: &'a str,
    battery: &'a crate::catalog::BatterySpec,
    motor: &'a crate::catalog::MotorSpec,
    esc: &'a crate::catalog::EscSpec,
}
