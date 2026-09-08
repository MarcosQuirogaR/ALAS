// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::fmt;

use super::turbofan_physics::{
    adiabatic_inlet, combustor_fuel_air_ratio, convergent_nozzle, gas_properties, net_thrust,
    GasComposition, NozzleResult, PhysicsError, StreamThrust, TotalState,
};

const MINIMUM_TEMPERATURE_K: f64 = 200.0;
const MAXIMUM_TEMPERATURE_K: f64 = 2_000.0;
const KEROSENE_CARBON_MASS_FRACTION: f64 = 0.861_6;
const KEROSENE_HYDROGEN_MASS_FRACTION: f64 = 1.0 - KEROSENE_CARBON_MASS_FRACTION;
const OXYGEN_PER_CARBON_KG_KG: f64 = 32.0 / 12.011;
const OXYGEN_PER_HYDROGEN_KG_KG: f64 = 15.999 / (2.0 * 1.008);
const CARBON_DIOXIDE_PER_CARBON_KG_KG: f64 = 1.0 + OXYGEN_PER_CARBON_KG_KG;
const WATER_PER_HYDROGEN_KG_KG: f64 = 1.0 + OXYGEN_PER_HYDROGEN_KG_KG;

/// Physical-domain or model-validity failure from the product turbofan.
#[derive(Clone, Debug, PartialEq)]
pub enum ProductTurbofanError {
    /// A lower-level thermodynamic primitive failed.
    Physics(PhysicsError),
    /// A finite input lies outside the model's supported interval.
    InvalidInput {
        /// Stable input name.
        field: &'static str,
        /// Supplied value.
        value: f64,
        /// Inclusive lower limit.
        minimum: f64,
        /// Inclusive upper limit.
        maximum: f64,
    },
    /// The requested turbine work would leave an invalid thermodynamic state.
    InsufficientTurbineWork {
        /// Turbine identifier.
        turbine: &'static str,
        /// Required specific work per unit core inlet air, J/kg.
        required_work_j_kg_core_air: f64,
    },
    /// Complete-combustion products cannot be formed with the available oxygen.
    RichCombustionUnsupported {
        /// Computed fuel-to-air ratio.
        fuel_air_ratio: f64,
    },
    /// The certified sizing point did not produce positive unit-flow thrust.
    InvalidSizingPoint,
}

impl fmt::Display for ProductTurbofanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ProductTurbofanError {}

impl From<PhysicsError> for ProductTurbofanError {
    fn from(error: PhysicsError) -> Self {
        Self::Physics(error)
    }
}

/// Evidence status attached to an evaluated state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CycleValidity {
    /// Closed on-design cycle with fixed component pressure ratios.
    OnDesignScreening,
    /// Fixed-ratio, variable-required-area extrapolation; not a mission deck.
    FixedRatioVariableAreaExtrapolation,
}

/// Treatment of nozzle geometry in a cycle result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NozzleAreaTreatment {
    /// Area is inferred from continuity independently at every evaluated point.
    /// It is not fixed engine geometry and cannot validate off-design operation.
    RequiredAreaSolvedPerPoint,
}

/// Traceable description of the equations and calibration basis.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelProvenance {
    /// Thermodynamic formulation.
    pub cycle_physics: &'static str,
    /// Component-map status.
    pub off_design_basis: &'static str,
    /// Fuel-flow interpolation status.
    pub fuel_interpolation: Option<&'static str>,
    /// Source supplied for the certified sizing datum.
    pub sizing_source: String,
}

/// Optional empirical part-power fuel evidence, separate from cycle physics.
#[derive(Clone, Debug, PartialEq)]
pub struct FuelFlowEvidenceAnchors {
    /// Monotonically increasing normalized thrust commands.
    pub thrust_fractions: Vec<f64>,
    /// Fuel flows at the corresponding evidence points, kg/s.
    pub fuel_flows_kg_s: Vec<f64>,
    /// Dataset or document identifier.
    pub source: String,
}

impl FuelFlowEvidenceAnchors {
    /// Piecewise-linear evidence interpolation. This does not alter cycle states.
    pub fn interpolate(&self, thrust_fraction: f64) -> Result<f64, ProductTurbofanError> {
        check_range("thrust_fraction", thrust_fraction, 0.0, 1.0)?;
        if self.thrust_fractions.len() < 2
            || self.thrust_fractions.len() != self.fuel_flows_kg_s.len()
        {
            return Err(invalid(
                "fuel evidence length",
                self.thrust_fractions.len() as f64,
                2.0,
                f64::INFINITY,
            ));
        }
        for index in 0..self.thrust_fractions.len() {
            check_range(
                "evidence thrust fraction",
                self.thrust_fractions[index],
                0.0,
                1.0,
            )?;
            check_range(
                "evidence fuel flow",
                self.fuel_flows_kg_s[index],
                0.0,
                f64::INFINITY,
            )?;
            if index > 0 && self.thrust_fractions[index] <= self.thrust_fractions[index - 1] {
                return Err(invalid(
                    "evidence thrust ordering",
                    self.thrust_fractions[index],
                    0.0,
                    1.0,
                ));
            }
        }
        let bounded = thrust_fraction.clamp(
            self.thrust_fractions[0],
            *self.thrust_fractions.last().unwrap_or(&1.0),
        );
        let upper = self
            .thrust_fractions
            .partition_point(|value| *value < bounded)
            .clamp(1, self.thrust_fractions.len() - 1);
        let lower = upper - 1;
        let span = self.thrust_fractions[upper] - self.thrust_fractions[lower];
        let weight = (bounded - self.thrust_fractions[lower]) / span;
        Ok(self.fuel_flows_kg_s[lower]
            + weight * (self.fuel_flows_kg_s[upper] - self.fuel_flows_kg_s[lower]))
    }
}

/// Technology parameters for a two-spool, separate-flow turbofan.
#[derive(Clone, Debug, PartialEq)]
pub struct ProductTurbofanConfig {
    /// Bypass-to-core inlet air mass-flow ratio.
    pub bypass_ratio: f64,
    /// Fan total-pressure ratio.
    pub fan_pressure_ratio: f64,
    /// Core-compressor total-pressure ratio, downstream of the fan.
    pub core_compressor_pressure_ratio: f64,
    /// Fan isentropic efficiency.
    pub fan_isentropic_efficiency: f64,
    /// Core-compressor isentropic efficiency.
    pub core_compressor_isentropic_efficiency: f64,
    /// High-pressure-turbine isentropic efficiency.
    pub high_pressure_turbine_isentropic_efficiency: f64,
    /// Low-pressure-turbine isentropic efficiency.
    pub low_pressure_turbine_isentropic_efficiency: f64,
    /// High-spool mechanical efficiency.
    pub high_spool_mechanical_efficiency: f64,
    /// Low-spool mechanical efficiency.
    pub low_spool_mechanical_efficiency: f64,
    /// Inlet total-pressure recovery.
    pub inlet_pressure_recovery: f64,
    /// Combustor total-pressure ratio (outlet/inlet).
    pub combustor_pressure_ratio: f64,
    /// Combustion efficiency.
    pub combustion_efficiency: f64,
    /// Turbine inlet total temperature, K.
    pub turbine_inlet_temperature_k: f64,
    /// Fuel lower heating value, J/kg.
    pub fuel_lower_heating_value_j_kg: f64,
    /// Compressor delivery air extracted as bleed fraction.
    pub bleed_fraction_of_core_air: f64,
    /// Accessory work referred to core inlet air, J/kg.
    pub accessory_specific_work_j_kg_core_air: f64,
    /// Declared design-point static temperature, K.
    pub design_ambient_temperature_k: f64,
    /// Declared design-point static pressure, Pa.
    pub design_ambient_pressure_pa: f64,
}

/// Flight condition supplied in SI units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurbofanOperatingPoint {
    /// Ambient static temperature, K.
    pub ambient_temperature_k: f64,
    /// Ambient static pressure, Pa.
    pub ambient_pressure_pa: f64,
    /// True airspeed, m/s; exactly zero is supported.
    pub true_airspeed_m_s: f64,
}

/// Engine sized from one certified static-thrust datum.
#[derive(Clone, Debug, PartialEq)]
pub struct ProductTurbofan {
    /// Technology configuration.
    pub config: ProductTurbofanConfig,
    /// Sized total inlet air mass flow, kg/s.
    pub design_total_air_mass_flow_kg_s: f64,
    /// Certified static thrust used for sizing, N.
    pub certified_static_thrust_n: f64,
    /// Optional evidence-only part-power fuel schedule.
    pub part_power_fuel_evidence: Option<FuelFlowEvidenceAnchors>,
    /// Traceable model provenance.
    pub provenance: ModelProvenance,
}

/// Principal total-temperature stations, K.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurbofanStations {
    /// Inlet/fan entry.
    pub inlet_k: f64,
    /// Fan exit.
    pub fan_exit_k: f64,
    /// Core compressor exit.
    pub compressor_exit_k: f64,
    /// Combustor exit / HPT inlet.
    pub combustor_exit_k: f64,
    /// HPT exit.
    pub high_turbine_exit_k: f64,
    /// LPT exit.
    pub low_turbine_exit_k: f64,
}

/// Conservation diagnostics in dimensional units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurbofanResiduals {
    /// Whole-engine mass residual including extracted bleed, kg/s.
    pub mass_kg_s: f64,
    /// High-spool power residual, W.
    pub high_spool_power_w: f64,
    /// Low-spool power residual, W.
    pub low_spool_power_w: f64,
    /// Combustor energy residual, W.
    pub combustor_energy_w: f64,
    /// Sum of nozzle energy residuals, W.
    pub nozzle_energy_w: f64,
}

/// Closed cycle and thrust result for one engine.
#[derive(Clone, Debug, PartialEq)]
pub struct ProductTurbofanResult {
    /// Net uninstalled thrust including the declared axial bleed discharge, N.
    /// Off-design values use per-point required nozzle areas and are screening
    /// outputs, not validated mission predictions.
    pub net_thrust_n: f64,
    /// Fuel mass flow from cycle physics, kg/s.
    pub cycle_fuel_mass_flow_kg_s: f64,
    /// Total captured air mass flow, kg/s.
    pub total_air_mass_flow_kg_s: f64,
    /// Core inlet air mass flow, kg/s.
    pub core_air_mass_flow_kg_s: f64,
    /// Bypass air mass flow, kg/s.
    pub bypass_air_mass_flow_kg_s: f64,
    /// Compressor-delivery bleed discharged by the axial-nozzle assumption, kg/s.
    pub bleed_air_mass_flow_kg_s: f64,
    /// Fuel-to-core-air ratio.
    pub fuel_air_ratio: f64,
    /// Bypass nozzle solution.
    pub bypass_nozzle: NozzleResult,
    /// Core nozzle solution.
    pub core_nozzle: NozzleResult,
    /// Axial bleed-discharge nozzle solution from compressor-delivery conditions.
    pub bleed_discharge_nozzle: Option<NozzleResult>,
    /// Captured-air momentum flux subtracted from gross thrust, N.
    pub ram_drag_n: f64,
    /// Nozzle geometry treatment used by this result.
    pub nozzle_area_treatment: NozzleAreaTreatment,
    /// Thermodynamic station temperatures.
    pub stations: TurbofanStations,
    /// Conservation diagnostics.
    pub residuals: TurbofanResiduals,
    /// Appropriate interpretation of this fixed-ratio state.
    pub validity: CycleValidity,
    /// Model and evidence provenance.
    pub provenance: ModelProvenance,
}
