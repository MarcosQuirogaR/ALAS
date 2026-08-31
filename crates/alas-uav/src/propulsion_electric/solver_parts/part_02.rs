// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


fn resolve_catalogue_powertrain<'a>(
    catalog: &'a Catalog,
    selection: &'a CataloguePowertrainSelection,
) -> Result<ResolvedCataloguePowertrain<'a>, ElectricPropulsionError> {
    if selection.motor_count == 0 {
        return Err(ElectricPropulsionError::InvalidInput(
            "a powertrain selection needs at least one motor".to_owned(),
        ));
    }
    let battery_record = catalogue_record(catalog, &selection.battery_id)?;
    let motor_record = catalogue_record(catalog, &selection.motor_id)?;
    let esc_record = catalogue_record(catalog, &selection.esc_id)?;
    let propeller_record = catalogue_record(catalog, &selection.propeller_id)?;
    let ComponentKind::Battery(battery) = &battery_record.kind else {
        return Err(wrong_family(&selection.battery_id, "battery"));
    };
    let ComponentKind::Motor(motor) = &motor_record.kind else {
        return Err(wrong_family(&selection.motor_id, "motor"));
    };
    let ComponentKind::Esc(esc) = &esc_record.kind else {
        return Err(wrong_family(&selection.esc_id, "ESC"));
    };
    if !matches!(propeller_record.kind, ComponentKind::Propeller(_)) {
        return Err(wrong_family(&selection.propeller_id, "propeller"));
    }

    let series_cells = battery
        .series_cells
        .ok_or_else(|| missing_evidence(&selection.battery_id, "series-cell count"))?;
    let voltage_v = battery
        .nominal_voltage_v
        .ok_or_else(|| missing_evidence(&selection.battery_id, "nominal voltage"))?;
    validate_cells(
        &selection.motor_id,
        series_cells,
        motor.min_series_cells,
        motor.max_series_cells,
    )?;
    validate_cells(
        &selection.esc_id,
        series_cells,
        esc.min_series_cells,
        esc.max_series_cells,
    )?;
    let published_kv = motor
        .kv_rpm_per_v
        .ok_or_else(|| missing_evidence(&selection.motor_id, "Kv"))?;
    let motor_model = ElectricMotorModel {
        kv_rpm_per_v: published_kv,
        no_load_current_a: motor.no_load_current_a.unwrap_or(0.0),
        resistance_ohm: motor.winding_resistance_ohm.unwrap_or(0.0),
    };
    let propeller = apc_performance_map(&selection.propeller_id)?.clone();
    if let Some(diameter_m) = match &propeller_record.kind {
        ComponentKind::Propeller(spec) => spec.diameter_m,
        _ => None,
    } {
        if (diameter_m - propeller.diameter_m()).abs() > 1.0e-9 {
            return Err(ElectricPropulsionError::InvalidInput(format!(
                "catalogue diameter for '{}' disagrees with its APC performance table",
                selection.propeller_id
            )));
        }
    }
    Ok(ResolvedCataloguePowertrain {
        model: ElectricPowertrainModel {
            battery: FixedVoltageBattery {
                open_circuit_voltage_v: voltage_v,
                internal_resistance_ohm: 0.0,
            },
            motor: motor_model,
            esc: EscElectricalModel {
                series_resistance_ohm: 0.0,
            },
            propeller,
            motor_count: selection.motor_count,
        },
        battery_id: &selection.battery_id,
        motor_id: &selection.motor_id,
        esc_id: &selection.esc_id,
        battery,
        motor,
        esc,
    })
}

fn catalogue_checks(
    resolved: &ResolvedCataloguePowertrain<'_>,
    result: &ElectricPropulsionResult,
) -> Vec<ElectricalCheck> {
    let mut checks = Vec::new();
    check_current(
        &mut checks,
        resolved.motor_id,
        resolved.motor.max_current_a,
        result.per_motor.motor_current_a,
    );
    check_power(
        &mut checks,
        resolved.motor_id,
        resolved.motor.max_power_w,
        result.per_motor.motor_electrical_power_w,
    );
    check_current(
        &mut checks,
        resolved.esc_id,
        resolved.esc.continuous_current_a,
        result.per_motor.motor_current_a,
    );
    check_current(
        &mut checks,
        resolved.battery_id,
        resolved.battery.rated_discharge_current_a(),
        result.battery_current_a,
    );
    if resolved.motor.winding_resistance_ohm.is_none() {
        checks.push(ElectricalCheck::Unverified {
            component_id: resolved.motor_id.to_owned(),
            field: "winding resistance",
        });
    }
    if resolved.motor.no_load_current_a.is_none() {
        checks.push(ElectricalCheck::Unverified {
            component_id: resolved.motor_id.to_owned(),
            field: "no-load current",
        });
    }
    checks
}

fn check_current(
    checks: &mut Vec<ElectricalCheck>,
    component_id: &str,
    limit: Option<f64>,
    actual: f64,
) {
    match limit {
        Some(limit) if actual > limit => checks.push(ElectricalCheck::CurrentOverload {
            component_id: component_id.to_owned(),
            actual_a: actual,
            limit_a: limit,
        }),
        Some(_) => {}
        None => checks.push(ElectricalCheck::Unverified {
            component_id: component_id.to_owned(),
            field: "maximum current",
        }),
    }
}

fn check_power(
    checks: &mut Vec<ElectricalCheck>,
    component_id: &str,
    limit: Option<f64>,
    actual: f64,
) {
    match limit {
        Some(limit) if actual > limit => checks.push(ElectricalCheck::PowerOverload {
            component_id: component_id.to_owned(),
            actual_w: actual,
            limit_w: limit,
        }),
        Some(_) => {}
        None => checks.push(ElectricalCheck::Unverified {
            component_id: component_id.to_owned(),
            field: "maximum electrical power",
        }),
    }
}

fn catalogue_record<'a>(
    catalog: &'a Catalog,
    id: &str,
) -> Result<&'a crate::catalog::ComponentRecord, ElectricPropulsionError> {
    catalog
        .get(id)
        .ok_or_else(|| ElectricPropulsionError::MissingComponent { id: id.to_owned() })
}

fn selected_series_cells(catalog: &Catalog, id: &str) -> Result<u16, ElectricPropulsionError> {
    let record = catalogue_record(catalog, id)?;
    let ComponentKind::Battery(battery) = &record.kind else {
        return Err(wrong_family(id, "battery"));
    };
    battery
        .series_cells
        .ok_or_else(|| missing_evidence(id, "series-cell count"))
}

fn wrong_family(id: &str, expected: &'static str) -> ElectricPropulsionError {
    ElectricPropulsionError::WrongComponentFamily {
        id: id.to_owned(),
        expected,
    }
}

fn missing_evidence(id: &str, field: &'static str) -> ElectricPropulsionError {
    ElectricPropulsionError::MissingEvidence {
        id: id.to_owned(),
        field,
    }
}

fn validate_cells(
    id: &str,
    cells: u16,
    minimum: Option<u16>,
    maximum: Option<u16>,
) -> Result<(), ElectricPropulsionError> {
    if minimum.is_some_and(|minimum| cells < minimum)
        || maximum.is_some_and(|maximum| cells > maximum)
    {
        Err(ElectricPropulsionError::CellCountMismatch {
            id: id.to_owned(),
            cells,
        })
    } else {
        Ok(())
    }
}

fn validate_model(
    model: &ElectricPowertrainModel,
    condition: ElectricFlightCondition,
) -> Result<(), ElectricPropulsionError> {
    let positive = [
        model.battery.open_circuit_voltage_v,
        model.motor.kv_rpm_per_v,
        model.propeller.diameter_m(),
        condition.air_density_kg_m3,
    ];
    let non_negative = [
        model.battery.internal_resistance_ohm,
        model.motor.no_load_current_a,
        model.motor.resistance_ohm,
        model.esc.series_resistance_ohm,
        condition.speed_m_s,
        condition.throttle,
    ];
    if positive
        .into_iter()
        .any(|value| !value.is_finite() || value <= 0.0)
        || non_negative
            .into_iter()
            .any(|value| !value.is_finite() || value < 0.0)
        || condition.throttle > 1.0
        || model.motor_count == 0
    {
        Err(ElectricPropulsionError::InvalidInput(
            "the electric powertrain model has a non-finite or invalid value".to_owned(),
        ))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct PropellerLoad {
    thrust_n: f64,
    propeller_torque_nm: f64,
}

fn propeller_load(
    model: &ElectricPowertrainModel,
    condition: ElectricFlightCondition,
    rpm: f64,
) -> Result<PropellerLoad, ElectricPropulsionError> {
    let sample = model
        .propeller
        .coefficients_at(rpm, condition.speed_m_s)
        .ok_or(ElectricPropulsionError::PerformanceTableOutOfRange {
            speed_m_s: condition.speed_m_s,
        })?;
    let revolutions_per_s = rpm / 60.0;
    let diameter_m = model.propeller.diameter_m();
    let thrust_n = condition.air_density_kg_m3
        * sample.thrust_coefficient
        * revolutions_per_s.powi(2)
        * diameter_m.powi(4);
    let propeller_torque_nm = condition.air_density_kg_m3
        * sample.power_coefficient
        * revolutions_per_s.powi(2)
        * diameter_m.powi(5)
        / TAU;
    Ok(PropellerLoad {
        thrust_n,
        propeller_torque_nm,
    })
}

fn residual(
    model: &ElectricPowertrainModel,
    condition: ElectricFlightCondition,
    rpm: f64,
) -> Result<f64, ElectricPropulsionError> {
    let load = propeller_load(model, condition, rpm)?;
    let motor_current_a = load.propeller_torque_nm / model.motor.torque_constant_nm_per_a()
        + model.motor.no_load_current_a;
    let battery_current_a = f64::from(model.motor_count) * condition.throttle * motor_current_a;
    let battery_voltage_v = model.battery.open_circuit_voltage_v
        - battery_current_a * model.battery.internal_resistance_ohm;
    let available_motor_voltage_v =
        condition.throttle * battery_voltage_v - motor_current_a * model.esc.series_resistance_ohm;
    Ok(available_motor_voltage_v
        - rpm / model.motor.kv_rpm_per_v
        - motor_current_a * model.motor.resistance_ohm)
}

fn bisect(
    model: &ElectricPowertrainModel,
    condition: ElectricFlightCondition,
    mut lower_rpm: f64,
    mut upper_rpm: f64,
) -> Result<f64, ElectricPropulsionError> {
    for _ in 0..BISECTION_ITERATIONS {
        let midpoint = 0.5 * (lower_rpm + upper_rpm);
        if residual(model, condition, midpoint)? > 0.0 {
            lower_rpm = midpoint;
        } else {
            upper_rpm = midpoint;
        }
    }
    Ok(0.5 * (lower_rpm + upper_rpm))
}

