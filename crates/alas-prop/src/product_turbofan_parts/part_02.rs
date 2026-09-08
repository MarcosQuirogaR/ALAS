// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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
