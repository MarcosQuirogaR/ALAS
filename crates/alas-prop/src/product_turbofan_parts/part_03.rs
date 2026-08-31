// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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

