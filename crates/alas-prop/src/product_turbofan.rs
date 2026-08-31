// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Physically closed Level-1 separate-flow turbofan screening model.
//!
//! This kernel is deliberately independent of the legacy mission and report
//! solvers. Compressor and turbine work use temperature-dependent enthalpy;
//! nozzles and thrust use dimensional control-volume balances that remain
//! regular at zero flight speed. Fixed component pressure ratios are an
//! on-design screening assumption, not an off-design engine deck.

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

impl ProductTurbofan {
    /// Size inlet mass flow so the model exactly reproduces certified static thrust.
    pub fn size_from_certified_static_thrust(
        config: ProductTurbofanConfig,
        certified_static_thrust_n: f64,
        sizing_source: impl Into<String>,
    ) -> Result<Self, ProductTurbofanError> {
        check_range(
            "certified_static_thrust_n",
            certified_static_thrust_n,
            f64::EPSILON,
            f64::INFINITY,
        )?;
        validate_config(&config)?;
        let mut engine = Self {
            config,
            design_total_air_mass_flow_kg_s: 1.0,
            certified_static_thrust_n,
            part_power_fuel_evidence: None,
            provenance: ModelProvenance {
                cycle_physics: "variable-enthalpy, separate-flow, two-spool Level-1 cycle",
                off_design_basis: "fixed pressure ratios and per-point required nozzle areas; no component maps, fixed geometry, or control schedule; not validated for mission prediction",
                fuel_interpolation: None,
                sizing_source: sizing_source.into(),
            },
        };
        let point = engine.design_static_point();
        let unit = engine.evaluate_with_mass_flow(point, 1.0)?;
        if !unit.net_thrust_n.is_finite() || unit.net_thrust_n <= 0.0 {
            return Err(ProductTurbofanError::InvalidSizingPoint);
        }
        engine.design_total_air_mass_flow_kg_s = certified_static_thrust_n / unit.net_thrust_n;
        Ok(engine)
    }

    /// Attach empirical fuel-flow anchors without presenting them as cycle physics.
    pub fn with_part_power_fuel_evidence(mut self, evidence: FuelFlowEvidenceAnchors) -> Self {
        self.part_power_fuel_evidence = Some(evidence);
        self.provenance.fuel_interpolation =
            Some("piecewise-linear empirical evidence interpolation");
        self
    }

    /// Evaluate the sized fixed-ratio cycle at one flight condition.
    pub fn evaluate(
        &self,
        point: TurbofanOperatingPoint,
    ) -> Result<ProductTurbofanResult, ProductTurbofanError> {
        self.evaluate_with_mass_flow(point, self.design_total_air_mass_flow_kg_s)
    }

    fn design_static_point(&self) -> TurbofanOperatingPoint {
        TurbofanOperatingPoint {
            ambient_temperature_k: self.config.design_ambient_temperature_k,
            ambient_pressure_pa: self.config.design_ambient_pressure_pa,
            true_airspeed_m_s: 0.0,
        }
    }

    fn evaluate_with_mass_flow(
        &self,
        point: TurbofanOperatingPoint,
        total_air_mass_flow_kg_s: f64,
    ) -> Result<ProductTurbofanResult, ProductTurbofanError> {
        validate_config(&self.config)?;
        check_range(
            "ambient_temperature_k",
            point.ambient_temperature_k,
            MINIMUM_TEMPERATURE_K,
            MAXIMUM_TEMPERATURE_K,
        )?;
        check_range(
            "ambient_pressure_pa",
            point.ambient_pressure_pa,
            f64::EPSILON,
            f64::INFINITY,
        )?;
        check_range("true_airspeed_m_s", point.true_airspeed_m_s, 0.0, 400.0)?;
        check_range(
            "total_air_mass_flow_kg_s",
            total_air_mass_flow_kg_s,
            f64::EPSILON,
            f64::INFINITY,
        )?;

        let ambient = stagnate(point)?;
        let inlet = adiabatic_inlet(ambient, self.config.inlet_pressure_recovery)?;
        let fan_exit = compress(
            inlet,
            self.config.fan_pressure_ratio,
            self.config.fan_isentropic_efficiency,
        )?;
        let compressor_exit = compress(
            fan_exit,
            self.config.core_compressor_pressure_ratio,
            self.config.core_compressor_isentropic_efficiency,
        )?;

        let core_air = total_air_mass_flow_kg_s / (1.0 + self.config.bypass_ratio);
        let bypass_air = total_air_mass_flow_kg_s - core_air;
        let bleed_air = core_air * self.config.bleed_fraction_of_core_air;
        let combustor_air = core_air - bleed_air;
        let fuel_air_ratio = solve_fuel_air_ratio(
            compressor_exit.temperature_k,
            self.config.turbine_inlet_temperature_k,
            self.config.combustion_efficiency,
            self.config.fuel_lower_heating_value_j_kg,
        )?;
        let product_composition = complete_kerosene_products(combustor_air, fuel_air_ratio)?;
        let fuel_flow = combustor_air * fuel_air_ratio;
        let hot_mass_flow = combustor_air + fuel_flow;
        let combustor_exit = TotalState {
            temperature_k: self.config.turbine_inlet_temperature_k,
            pressure_pa: compressor_exit.pressure_pa * self.config.combustor_pressure_ratio,
        };

        let fan_work_per_total_air = enthalpy(fan_exit.temperature_k, GasComposition::DRY_AIR)?
            - enthalpy(inlet.temperature_k, GasComposition::DRY_AIR)?;
        let compressor_work_per_core_air =
            enthalpy(compressor_exit.temperature_k, GasComposition::DRY_AIR)?
                - enthalpy(fan_exit.temperature_k, GasComposition::DRY_AIR)?;
        let high_required_power = core_air
            * (compressor_work_per_core_air + self.config.accessory_specific_work_j_kg_core_air);
        let high_turbine = expand_for_power(
            "high_pressure_turbine",
            combustor_exit,
            product_composition,
            high_required_power / hot_mass_flow / self.config.high_spool_mechanical_efficiency,
            self.config.high_pressure_turbine_isentropic_efficiency,
        )?;
        let low_required_power = total_air_mass_flow_kg_s * fan_work_per_total_air;
        let low_turbine = expand_for_power(
            "low_pressure_turbine",
            high_turbine,
            product_composition,
            low_required_power / hot_mass_flow / self.config.low_spool_mechanical_efficiency,
            self.config.low_pressure_turbine_isentropic_efficiency,
        )?;

        let bypass_nozzle = convergent_nozzle(
            fan_exit,
            point.ambient_pressure_pa,
            bypass_air,
            GasComposition::DRY_AIR,
        )?;
        let core_nozzle = convergent_nozzle(
            low_turbine,
            point.ambient_pressure_pa,
            hot_mass_flow,
            product_composition,
        )?;
        // Level-1 installation assumption: extracted compressor bleed is
        // discharged axially through a convergent overboard nozzle. This makes
        // the bleed port's mass and momentum explicit; a later installation
        // model must replace it for cabin/ECS routing or non-axial discharge.
        let bleed_discharge_nozzle = if bleed_air > 0.0 {
            Some(convergent_nozzle(
                compressor_exit,
                point.ambient_pressure_pa,
                bleed_air,
                GasComposition::DRY_AIR,
            )?)
        } else {
            None
        };
        let mut thrust_streams = vec![
            nozzle_stream(
                bypass_air,
                bypass_air,
                point.ambient_pressure_pa,
                bypass_nozzle,
            ),
            nozzle_stream(
                combustor_air,
                hot_mass_flow,
                point.ambient_pressure_pa,
                core_nozzle,
            ),
        ];
        if let Some(nozzle) = bleed_discharge_nozzle {
            thrust_streams.push(nozzle_stream(
                bleed_air,
                bleed_air,
                point.ambient_pressure_pa,
                nozzle,
            ));
        }
        let thrust = net_thrust(point.true_airspeed_m_s, fuel_flow, &thrust_streams)?;

        let high_extracted_power = hot_mass_flow
            * (enthalpy(combustor_exit.temperature_k, product_composition)?
                - enthalpy(high_turbine.temperature_k, product_composition)?)
            * self.config.high_spool_mechanical_efficiency;
        let low_extracted_power = hot_mass_flow
            * (enthalpy(high_turbine.temperature_k, product_composition)?
                - enthalpy(low_turbine.temperature_k, product_composition)?)
            * self.config.low_spool_mechanical_efficiency;
        let combustor_inlet_power =
            combustor_air * enthalpy(compressor_exit.temperature_k, GasComposition::DRY_AIR)?;
        let combustor_outlet_power =
            hot_mass_flow * enthalpy(combustor_exit.temperature_k, product_composition)?;
        let chemical_power = fuel_flow
            * self.config.combustion_efficiency
            * self.config.fuel_lower_heating_value_j_kg;
        let design_point = self.design_static_point();
        let at_design = relative_difference(
            point.ambient_temperature_k,
            design_point.ambient_temperature_k,
        ) < 1.0e-9
            && relative_difference(point.ambient_pressure_pa, design_point.ambient_pressure_pa)
                < 1.0e-9
            && point.true_airspeed_m_s == 0.0;

        Ok(ProductTurbofanResult {
            net_thrust_n: thrust.net_thrust_n,
            cycle_fuel_mass_flow_kg_s: fuel_flow,
            total_air_mass_flow_kg_s,
            core_air_mass_flow_kg_s: core_air,
            bypass_air_mass_flow_kg_s: bypass_air,
            bleed_air_mass_flow_kg_s: bleed_air,
            fuel_air_ratio,
            bypass_nozzle,
            core_nozzle,
            bleed_discharge_nozzle,
            ram_drag_n: thrust.ram_drag_n,
            nozzle_area_treatment: NozzleAreaTreatment::RequiredAreaSolvedPerPoint,
            stations: TurbofanStations {
                inlet_k: inlet.temperature_k,
                fan_exit_k: fan_exit.temperature_k,
                compressor_exit_k: compressor_exit.temperature_k,
                combustor_exit_k: combustor_exit.temperature_k,
                high_turbine_exit_k: high_turbine.temperature_k,
                low_turbine_exit_k: low_turbine.temperature_k,
            },
            residuals: TurbofanResiduals {
                mass_kg_s: thrust.mass_residual_kg_s,
                high_spool_power_w: high_extracted_power - high_required_power,
                low_spool_power_w: low_extracted_power - low_required_power,
                combustor_energy_w: combustor_outlet_power - combustor_inlet_power - chemical_power,
                nozzle_energy_w: bypass_nozzle.energy_residual_w
                    + core_nozzle.energy_residual_w
                    + bleed_discharge_nozzle.map_or(0.0, |nozzle| nozzle.energy_residual_w),
            },
            validity: if at_design {
                CycleValidity::OnDesignScreening
            } else {
                CycleValidity::FixedRatioVariableAreaExtrapolation
            },
            provenance: self.provenance.clone(),
        })
    }
}

fn validate_config(config: &ProductTurbofanConfig) -> Result<(), ProductTurbofanError> {
    check_range("bypass_ratio", config.bypass_ratio, 0.1, 30.0)?;
    check_range("fan_pressure_ratio", config.fan_pressure_ratio, 1.001, 3.0)?;
    check_range(
        "core_compressor_pressure_ratio",
        config.core_compressor_pressure_ratio,
        1.001,
        80.0,
    )?;
    for (field, value) in [
        (
            "fan_isentropic_efficiency",
            config.fan_isentropic_efficiency,
        ),
        (
            "core_compressor_isentropic_efficiency",
            config.core_compressor_isentropic_efficiency,
        ),
        (
            "high_pressure_turbine_isentropic_efficiency",
            config.high_pressure_turbine_isentropic_efficiency,
        ),
        (
            "low_pressure_turbine_isentropic_efficiency",
            config.low_pressure_turbine_isentropic_efficiency,
        ),
        (
            "high_spool_mechanical_efficiency",
            config.high_spool_mechanical_efficiency,
        ),
        (
            "low_spool_mechanical_efficiency",
            config.low_spool_mechanical_efficiency,
        ),
        ("inlet_pressure_recovery", config.inlet_pressure_recovery),
        ("combustor_pressure_ratio", config.combustor_pressure_ratio),
        ("combustion_efficiency", config.combustion_efficiency),
    ] {
        check_range(field, value, 0.5, 1.0)?;
    }
    check_range(
        "turbine_inlet_temperature_k",
        config.turbine_inlet_temperature_k,
        800.0,
        MAXIMUM_TEMPERATURE_K,
    )?;
    check_range(
        "fuel_lower_heating_value_j_kg",
        config.fuel_lower_heating_value_j_kg,
        20.0e6,
        60.0e6,
    )?;
    check_range(
        "bleed_fraction_of_core_air",
        config.bleed_fraction_of_core_air,
        0.0,
        0.25,
    )?;
    check_range(
        "accessory_specific_work_j_kg_core_air",
        config.accessory_specific_work_j_kg_core_air,
        0.0,
        200_000.0,
    )?;
    check_range(
        "design_ambient_temperature_k",
        config.design_ambient_temperature_k,
        MINIMUM_TEMPERATURE_K,
        350.0,
    )?;
    check_range(
        "design_ambient_pressure_pa",
        config.design_ambient_pressure_pa,
        10_000.0,
        120_000.0,
    )
}

fn stagnate(point: TurbofanOperatingPoint) -> Result<TotalState, ProductTurbofanError> {
    let static_h = enthalpy(point.ambient_temperature_k, GasComposition::DRY_AIR)?;
    let total_h = static_h + 0.5 * point.true_airspeed_m_s.powi(2);
    let total_temperature = temperature_for_enthalpy(
        total_h,
        GasComposition::DRY_AIR,
        point.ambient_temperature_k,
        MAXIMUM_TEMPERATURE_K,
    )?;
    let entropy_integral = integrate_cp_over_t(
        point.ambient_temperature_k,
        total_temperature,
        GasComposition::DRY_AIR,
    )?;
    let gas_constant =
        gas_properties(total_temperature, GasComposition::DRY_AIR)?.gas_constant_j_kg_k;
    Ok(TotalState {
        temperature_k: total_temperature,
        pressure_pa: point.ambient_pressure_pa * (entropy_integral / gas_constant).exp(),
    })
}

fn compress(
    inlet: TotalState,
    pressure_ratio: f64,
    efficiency: f64,
) -> Result<TotalState, ProductTurbofanError> {
    let gas = GasComposition::DRY_AIR;
    let gas_constant = gas_properties(inlet.temperature_k, gas)?.gas_constant_j_kg_k;
    let target_entropy_integral = gas_constant * pressure_ratio.ln();
    let ideal_temperature =
        solve_temperature(inlet.temperature_k, MAXIMUM_TEMPERATURE_K, |temperature| {
            Ok(integrate_cp_over_t(inlet.temperature_k, temperature, gas)?
                - target_entropy_integral)
        })?;
    let inlet_h = enthalpy(inlet.temperature_k, gas)?;
    let ideal_h = enthalpy(ideal_temperature, gas)?;
    let outlet_h = inlet_h + (ideal_h - inlet_h) / efficiency;
    let outlet_temperature =
        temperature_for_enthalpy(outlet_h, gas, ideal_temperature, MAXIMUM_TEMPERATURE_K)?;
    Ok(TotalState {
        temperature_k: outlet_temperature,
        pressure_pa: inlet.pressure_pa * pressure_ratio,
    })
}

fn expand_for_power(
    turbine: &'static str,
    inlet: TotalState,
    composition: GasComposition,
    actual_specific_work_j_kg_hot_gas: f64,
    isentropic_efficiency: f64,
) -> Result<TotalState, ProductTurbofanError> {
    let inlet_h = enthalpy(inlet.temperature_k, composition)?;
    let outlet_h = inlet_h - actual_specific_work_j_kg_hot_gas;
    if outlet_h <= enthalpy(MINIMUM_TEMPERATURE_K, composition)? {
        return Err(ProductTurbofanError::InsufficientTurbineWork {
            turbine,
            required_work_j_kg_core_air: actual_specific_work_j_kg_hot_gas,
        });
    }
    let outlet_temperature = temperature_for_enthalpy(
        outlet_h,
        composition,
        MINIMUM_TEMPERATURE_K,
        inlet.temperature_k,
    )?;
    let ideal_h = inlet_h - actual_specific_work_j_kg_hot_gas / isentropic_efficiency;
    if ideal_h <= enthalpy(MINIMUM_TEMPERATURE_K, composition)? {
        return Err(ProductTurbofanError::InsufficientTurbineWork {
            turbine,
            required_work_j_kg_core_air: actual_specific_work_j_kg_hot_gas,
        });
    }
    let ideal_temperature = temperature_for_enthalpy(
        ideal_h,
        composition,
        MINIMUM_TEMPERATURE_K,
        outlet_temperature,
    )?;
    let entropy_integral =
        integrate_cp_over_t(ideal_temperature, inlet.temperature_k, composition)?;
    let gas_constant = gas_properties(inlet.temperature_k, composition)?.gas_constant_j_kg_k;
    Ok(TotalState {
        temperature_k: outlet_temperature,
        pressure_pa: inlet.pressure_pa * (-entropy_integral / gas_constant).exp(),
    })
}

fn complete_kerosene_products(
    air_mass_flow_kg_s: f64,
    fuel_air_ratio: f64,
) -> Result<GasComposition, ProductTurbofanError> {
    check_range(
        "combustor_air_mass_flow_kg_s",
        air_mass_flow_kg_s,
        f64::EPSILON,
        f64::INFINITY,
    )?;
    check_range("fuel_air_ratio", fuel_air_ratio, 0.0, 0.1)?;
    let fuel_mass = air_mass_flow_kg_s * fuel_air_ratio;
    let oxygen_required = fuel_mass
        * (KEROSENE_CARBON_MASS_FRACTION * OXYGEN_PER_CARBON_KG_KG
            + KEROSENE_HYDROGEN_MASS_FRACTION * OXYGEN_PER_HYDROGEN_KG_KG);
    let oxygen_available = air_mass_flow_kg_s * GasComposition::DRY_AIR.oxygen;
    if oxygen_required >= oxygen_available {
        return Err(ProductTurbofanError::RichCombustionUnsupported { fuel_air_ratio });
    }
    let total = air_mass_flow_kg_s + fuel_mass;
    Ok(GasComposition {
        nitrogen: air_mass_flow_kg_s * GasComposition::DRY_AIR.nitrogen / total,
        oxygen: (oxygen_available - oxygen_required) / total,
        carbon_dioxide: (air_mass_flow_kg_s * GasComposition::DRY_AIR.carbon_dioxide
            + fuel_mass * KEROSENE_CARBON_MASS_FRACTION * CARBON_DIOXIDE_PER_CARBON_KG_KG)
            / total,
        water_vapor: fuel_mass * KEROSENE_HYDROGEN_MASS_FRACTION * WATER_PER_HYDROGEN_KG_KG / total,
        argon: air_mass_flow_kg_s * GasComposition::DRY_AIR.argon / total,
    })
}

fn solve_fuel_air_ratio(
    inlet_temperature_k: f64,
    outlet_temperature_k: f64,
    combustion_efficiency: f64,
    fuel_lower_heating_value_j_kg: f64,
) -> Result<f64, ProductTurbofanError> {
    let residual = |fuel_air_ratio: f64| -> Result<f64, ProductTurbofanError> {
        let products = complete_kerosene_products(1.0, fuel_air_ratio)?;
        Ok(combustor_fuel_air_ratio(
            inlet_temperature_k,
            outlet_temperature_k,
            combustion_efficiency,
            fuel_lower_heating_value_j_kg,
            GasComposition::DRY_AIR,
            products,
        )? - fuel_air_ratio)
    };
    let mut lower = 0.0;
    let mut upper = 0.06;
    let mut lower_value = residual(lower)?;
    if lower_value * residual(upper)? > 0.0 {
        return Err(ProductTurbofanError::Physics(PhysicsError::NoConvergence {
            equation: "FAR-derived combustion composition",
        }));
    }
    for _ in 0..80 {
        let midpoint = 0.5 * (lower + upper);
        let midpoint_value = residual(midpoint)?;
        if midpoint_value.abs() < 1.0e-12 || upper - lower < 1.0e-12 {
            return Ok(midpoint);
        }
        if lower_value * midpoint_value <= 0.0 {
            upper = midpoint;
        } else {
            lower = midpoint;
            lower_value = midpoint_value;
        }
    }
    Err(ProductTurbofanError::Physics(PhysicsError::NoConvergence {
        equation: "FAR-derived combustion composition",
    }))
}

fn nozzle_stream(
    inlet_air: f64,
    exit_mass: f64,
    ambient_pressure: f64,
    nozzle: NozzleResult,
) -> StreamThrust {
    StreamThrust {
        inlet_air_mass_flow_kg_s: inlet_air,
        exit_mass_flow_kg_s: exit_mass,
        exit_velocity_m_s: nozzle.exit_velocity_m_s,
        exit_pressure_pa: nozzle.exit_pressure_pa,
        ambient_pressure_pa: ambient_pressure,
        exit_area_m2: nozzle.required_area_m2,
    }
}

fn enthalpy(temperature_k: f64, composition: GasComposition) -> Result<f64, ProductTurbofanError> {
    Ok(gas_properties(temperature_k, composition)?.specific_enthalpy_j_kg)
}

fn integrate_cp_over_t(
    lower: f64,
    upper: f64,
    composition: GasComposition,
) -> Result<f64, ProductTurbofanError> {
    const INTERVALS: usize = 64;
    if upper == lower {
        return Ok(0.0);
    }
    let step = (upper - lower) / INTERVALS as f64;
    let integrand = |temperature| -> Result<f64, ProductTurbofanError> {
        Ok(gas_properties(temperature, composition)?.specific_heat_cp_j_kg_k / temperature)
    };
    let mut sum = integrand(lower)? + integrand(upper)?;
    for index in 1..INTERVALS {
        sum += if index % 2 == 0 { 2.0 } else { 4.0 } * integrand(lower + index as f64 * step)?;
    }
    Ok(sum * step / 3.0)
}

fn temperature_for_enthalpy(
    target_enthalpy: f64,
    composition: GasComposition,
    lower: f64,
    upper: f64,
) -> Result<f64, ProductTurbofanError> {
    solve_temperature(lower, upper, |temperature| {
        Ok(enthalpy(temperature, composition)? - target_enthalpy)
    })
}

fn solve_temperature<F>(
    mut lower: f64,
    mut upper: f64,
    function: F,
) -> Result<f64, ProductTurbofanError>
where
    F: Fn(f64) -> Result<f64, ProductTurbofanError>,
{
    let mut lower_value = function(lower)?;
    let upper_value = function(upper)?;
    if lower_value * upper_value > 0.0 {
        return Err(ProductTurbofanError::Physics(PhysicsError::NoConvergence {
            equation: "product turbofan temperature",
        }));
    }
    for _ in 0..80 {
        let midpoint = 0.5 * (lower + upper);
        let midpoint_value = function(midpoint)?;
        if midpoint_value.abs() < 1.0e-7 || upper - lower < 1.0e-8 {
            return Ok(midpoint);
        }
        if lower_value * midpoint_value <= 0.0 {
            upper = midpoint;
        } else {
            lower = midpoint;
            lower_value = midpoint_value;
        }
    }
    Err(ProductTurbofanError::Physics(PhysicsError::NoConvergence {
        equation: "product turbofan temperature",
    }))
}

fn relative_difference(left: f64, right: f64) -> f64 {
    (left - right).abs() / left.abs().max(right.abs()).max(f64::EPSILON)
}

fn check_range(
    field: &'static str,
    value: f64,
    minimum: f64,
    maximum: f64,
) -> Result<(), ProductTurbofanError> {
    if !value.is_finite() || value < minimum || value > maximum {
        return Err(invalid(field, value, minimum, maximum));
    }
    Ok(())
}

fn invalid(field: &'static str, value: f64, minimum: f64, maximum: f64) -> ProductTurbofanError {
    ProductTurbofanError::InvalidInput {
        field,
        value,
        minimum,
        maximum,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn representative_config() -> ProductTurbofanConfig {
        ProductTurbofanConfig {
            bypass_ratio: 5.5,
            fan_pressure_ratio: 1.60,
            core_compressor_pressure_ratio: 18.0,
            fan_isentropic_efficiency: 0.89,
            core_compressor_isentropic_efficiency: 0.88,
            high_pressure_turbine_isentropic_efficiency: 0.91,
            low_pressure_turbine_isentropic_efficiency: 0.92,
            high_spool_mechanical_efficiency: 0.99,
            low_spool_mechanical_efficiency: 0.99,
            inlet_pressure_recovery: 0.995,
            combustor_pressure_ratio: 0.95,
            combustion_efficiency: 0.995,
            turbine_inlet_temperature_k: 1_650.0,
            fuel_lower_heating_value_j_kg: 43.0e6,
            bleed_fraction_of_core_air: 0.03,
            accessory_specific_work_j_kg_core_air: 12_000.0,
            design_ambient_temperature_k: 288.15,
            design_ambient_pressure_pa: 101_325.0,
        }
    }

    fn representative_engine() -> ProductTurbofan {
        ProductTurbofan::size_from_certified_static_thrust(
            representative_config(),
            120_000.0,
            "representative certified sea-level-static datum",
        )
        .unwrap()
    }

    #[test]
    fn true_static_sizing_has_no_mach_workaround_and_closes_thrust() {
        let engine = representative_engine();
        let result = engine.evaluate(engine.design_static_point()).unwrap();
        assert_eq!(engine.design_static_point().true_airspeed_m_s, 0.0);
        assert!((result.net_thrust_n - 120_000.0).abs() < 1.0e-6);
        assert_eq!(result.validity, CycleValidity::OnDesignScreening);
    }

    #[test]
    fn mass_energy_and_spool_balances_close() {
        let result = representative_engine()
            .evaluate(TurbofanOperatingPoint {
                ambient_temperature_k: 288.15,
                ambient_pressure_pa: 101_325.0,
                true_airspeed_m_s: 0.0,
            })
            .unwrap();
        assert!(result.residuals.mass_kg_s.abs() < 1.0e-10);
        assert!(
            result.residuals.high_spool_power_w.abs() < 1.0e-3,
            "high-spool residual = {} W",
            result.residuals.high_spool_power_w
        );
        assert!(
            result.residuals.low_spool_power_w.abs() < 1.0e-3,
            "low-spool residual = {} W",
            result.residuals.low_spool_power_w
        );
        assert!(
            result.residuals.combustor_energy_w.abs() < 1.0e-2,
            "combustor residual = {} W",
            result.residuals.combustor_energy_w
        );
        assert!(
            result.residuals.nozzle_energy_w.abs() < 1.0e-3,
            "nozzle residual = {} W",
            result.residuals.nozzle_energy_w
        );
    }

    #[test]
    fn station_temperatures_follow_cold_and_hot_section_physics() {
        let stations = representative_engine()
            .evaluate(TurbofanOperatingPoint {
                ambient_temperature_k: 288.15,
                ambient_pressure_pa: 101_325.0,
                true_airspeed_m_s: 0.0,
            })
            .unwrap()
            .stations;
        assert!(stations.inlet_k < stations.fan_exit_k);
        assert!(stations.fan_exit_k < stations.compressor_exit_k);
        assert!(stations.compressor_exit_k < stations.combustor_exit_k);
        assert!(stations.combustor_exit_k > stations.high_turbine_exit_k);
        assert!(stations.high_turbine_exit_k > stations.low_turbine_exit_k);
    }

    #[test]
    fn rejects_invalid_component_domains_and_rich_combustion() {
        let mut invalid_config = representative_config();
        invalid_config.fan_isentropic_efficiency = 1.1;
        assert!(matches!(
            ProductTurbofan::size_from_certified_static_thrust(invalid_config, 100_000.0, "test"),
            Err(ProductTurbofanError::InvalidInput { .. })
        ));
        assert!(matches!(
            complete_kerosene_products(1.0, 0.1),
            Err(ProductTurbofanError::RichCombustionUnsupported { .. })
        ));
    }

    #[test]
    fn representative_transport_cruise_is_finite_and_marked_extrapolated() {
        let result = representative_engine()
            .evaluate(TurbofanOperatingPoint {
                ambient_temperature_k: 216.65,
                ambient_pressure_pa: 22_632.0,
                true_airspeed_m_s: 230.0,
            })
            .unwrap();
        assert!(result.net_thrust_n > 0.0);
        assert!(result.cycle_fuel_mass_flow_kg_s > 0.0);
        assert!((0.01..0.06).contains(&result.fuel_air_ratio));
        assert_eq!(
            result.validity,
            CycleValidity::FixedRatioVariableAreaExtrapolation
        );
        assert_eq!(
            result.nozzle_area_treatment,
            NozzleAreaTreatment::RequiredAreaSolvedPerPoint
        );
        assert!(result
            .provenance
            .off_design_basis
            .contains("no component maps"));
    }

    #[test]
    fn nonzero_speed_bleed_stream_closes_mass_and_ram_drag() {
        let point = TurbofanOperatingPoint {
            ambient_temperature_k: 250.0,
            ambient_pressure_pa: 54_000.0,
            true_airspeed_m_s: 180.0,
        };
        let result = representative_engine().evaluate(point).unwrap();
        assert!(result.bleed_air_mass_flow_kg_s > 0.0);
        assert!(result.bleed_discharge_nozzle.is_some());
        assert!(result.residuals.mass_kg_s.abs() < 1.0e-10);
        let expected_ram_drag = result.total_air_mass_flow_kg_s * point.true_airspeed_m_s;
        assert!(
            (result.ram_drag_n - expected_ram_drag).abs() < 1.0e-8,
            "ram drag = {}, expected = {} N",
            result.ram_drag_n,
            expected_ram_drag
        );
    }

    #[test]
    fn empirical_fuel_schedule_is_explicitly_separate() {
        let engine =
            representative_engine().with_part_power_fuel_evidence(FuelFlowEvidenceAnchors {
                thrust_fractions: vec![0.07, 0.30, 0.85, 1.0],
                fuel_flows_kg_s: vec![0.10, 0.25, 0.70, 0.90],
                source: "certification evidence".into(),
            });
        let interpolated = engine
            .part_power_fuel_evidence
            .as_ref()
            .unwrap()
            .interpolate(0.50)
            .unwrap();
        assert!((0.25..0.70).contains(&interpolated));
        assert!(engine
            .provenance
            .fuel_interpolation
            .unwrap()
            .contains("empirical"));
    }
}
