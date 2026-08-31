// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


/// Solve an ideal, adiabatic convergent nozzle with a supplied mass flow.
///
/// The unchoked exit closes to ambient pressure. The choked exit satisfies
/// `V=a` at the throat. Pressure thrust remains explicit; it is never hidden
/// in an equivalent velocity. Area follows from continuity after the state is
/// solved, which makes the routine useful while engine flow size is unknown.
pub fn convergent_nozzle(
    total_state: TotalState,
    ambient_pressure_pa: f64,
    mass_flow_kg_s: f64,
    composition: GasComposition,
) -> Result<NozzleResult, PhysicsError> {
    check_range(
        "total_state.temperature_k",
        total_state.temperature_k,
        MIN_TEMPERATURE_K,
        MAX_TEMPERATURE_K,
    )?;
    check_positive("total_state.pressure_pa", total_state.pressure_pa)?;
    check_positive("ambient_pressure_pa", ambient_pressure_pa)?;
    check_positive("mass_flow_kg_s", mass_flow_kg_s)?;
    if total_state.pressure_pa <= ambient_pressure_pa {
        return Err(PhysicsError::InsufficientTotalPressure {
            total_pressure_pa: total_state.pressure_pa,
            back_pressure_pa: ambient_pressure_pa,
        });
    }

    let total_properties = gas_properties(total_state.temperature_k, composition)?;
    let critical_temperature_k = bisect_temperature(
        MIN_TEMPERATURE_K.max(0.25 * total_state.temperature_k),
        total_state.temperature_k,
        |temperature_k| {
            let properties = gas_properties(temperature_k, composition)?;
            Ok(
                2.0 * (total_properties.specific_enthalpy_j_kg - properties.specific_enthalpy_j_kg)
                    - properties.gamma * properties.gas_constant_j_kg_k * temperature_k,
            )
        },
        "critical nozzle energy",
    )?;
    let critical_pressure_pa =
        isentropic_pressure(total_state, critical_temperature_k, composition)?;

    let (regime, exit_pressure_pa, exit_temperature_k) =
        if ambient_pressure_pa <= critical_pressure_pa {
            (
                NozzleRegime::Choked,
                critical_pressure_pa,
                critical_temperature_k,
            )
        } else {
            let temperature_k =
                temperature_at_isentropic_pressure(total_state, ambient_pressure_pa, composition)?;
            (NozzleRegime::Unchoked, ambient_pressure_pa, temperature_k)
        };
    let exit_properties = gas_properties(exit_temperature_k, composition)?;
    let exit_velocity_m_s = (2.0
        * (total_properties.specific_enthalpy_j_kg - exit_properties.specific_enthalpy_j_kg))
        .sqrt();
    let speed_of_sound_m_s =
        (exit_properties.gamma * exit_properties.gas_constant_j_kg_k * exit_temperature_k).sqrt();
    let exit_density_kg_m3 =
        exit_pressure_pa / (exit_properties.gas_constant_j_kg_k * exit_temperature_k);
    let required_area_m2 = mass_flow_kg_s / (exit_density_kg_m3 * exit_velocity_m_s);
    let reconstructed_mass_flow = exit_density_kg_m3 * exit_velocity_m_s * required_area_m2;
    let energy_residual_w = mass_flow_kg_s
        * (total_properties.specific_enthalpy_j_kg
            - exit_properties.specific_enthalpy_j_kg
            - 0.5 * exit_velocity_m_s.powi(2));

    Ok(NozzleResult {
        regime,
        exit_temperature_k,
        exit_pressure_pa,
        exit_velocity_m_s,
        exit_mach: exit_velocity_m_s / speed_of_sound_m_s,
        exit_density_kg_m3,
        required_area_m2,
        momentum_thrust_n: mass_flow_kg_s * exit_velocity_m_s,
        pressure_thrust_n: (exit_pressure_pa - ambient_pressure_pa) * required_area_m2,
        energy_residual_w,
        mass_residual_kg_s: reconstructed_mass_flow - mass_flow_kg_s,
    })
}

/// One exhaust stream in the engine control-volume thrust balance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreamThrust {
    /// Freestream air captured by this stream, kg/s.
    pub inlet_air_mass_flow_kg_s: f64,
    /// Exhaust mass flow, including injected fuel where applicable, kg/s.
    pub exit_mass_flow_kg_s: f64,
    /// Axial exhaust velocity, m/s.
    pub exit_velocity_m_s: f64,
    /// Exhaust-plane static pressure, Pa.
    pub exit_pressure_pa: f64,
    /// Local ambient static pressure, Pa.
    pub ambient_pressure_pa: f64,
    /// Exhaust-plane flow area, m².
    pub exit_area_m2: f64,
}

/// Decomposed axial thrust and control-volume mass closure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThrustBalance {
    /// Sum of gross exhaust momentum fluxes, N.
    pub momentum_thrust_n: f64,
    /// Sum of exhaust pressure-thrust terms, N.
    pub pressure_thrust_n: f64,
    /// Captured freestream momentum flux, N.
    pub ram_drag_n: f64,
    /// Momentum plus pressure thrust minus ram drag, N.
    pub net_thrust_n: f64,
    /// Exhaust minus captured-air and fuel mass flows, kg/s.
    pub mass_residual_kg_s: f64,
}

/// Computes a dimensional multi-stream thrust balance.
///
/// Unlike formulations normalized by Mach, this control-volume equation is
/// regular at exactly zero freestream velocity. A non-positive result is
/// rejected because this primitive currently represents powered forward-thrust
/// operation; windmilling and reverse thrust require separate operating modes.
pub fn net_thrust(
    freestream_velocity_m_s: f64,
    fuel_mass_flow_kg_s: f64,
    streams: &[StreamThrust],
) -> Result<ThrustBalance, PhysicsError> {
    check_nonnegative("freestream_velocity_m_s", freestream_velocity_m_s)?;
    check_nonnegative("fuel_mass_flow_kg_s", fuel_mass_flow_kg_s)?;
    let mut inlet_air_mass_flow = 0.0;
    let mut exit_mass_flow = 0.0;
    let mut momentum_thrust = 0.0;
    let mut pressure_thrust = 0.0;
    for stream in streams {
        check_nonnegative(
            "stream.inlet_air_mass_flow_kg_s",
            stream.inlet_air_mass_flow_kg_s,
        )?;
        check_positive("stream.exit_mass_flow_kg_s", stream.exit_mass_flow_kg_s)?;
        check_nonnegative("stream.exit_velocity_m_s", stream.exit_velocity_m_s)?;
        check_positive("stream.exit_pressure_pa", stream.exit_pressure_pa)?;
        check_positive("stream.ambient_pressure_pa", stream.ambient_pressure_pa)?;
        check_positive("stream.exit_area_m2", stream.exit_area_m2)?;
        inlet_air_mass_flow += stream.inlet_air_mass_flow_kg_s;
        exit_mass_flow += stream.exit_mass_flow_kg_s;
        momentum_thrust += stream.exit_mass_flow_kg_s * stream.exit_velocity_m_s;
        pressure_thrust +=
            (stream.exit_pressure_pa - stream.ambient_pressure_pa) * stream.exit_area_m2;
    }
    let ram_drag = inlet_air_mass_flow * freestream_velocity_m_s;
    let net = momentum_thrust + pressure_thrust - ram_drag;
    if net <= 0.0 {
        return Err(PhysicsError::NonPositiveNetThrust { thrust_n: net });
    }
    Ok(ThrustBalance {
        momentum_thrust_n: momentum_thrust,
        pressure_thrust_n: pressure_thrust,
        ram_drag_n: ram_drag,
        net_thrust_n: net,
        mass_residual_kg_s: exit_mass_flow - inlet_air_mass_flow - fuel_mass_flow_kg_s,
    })
}

/// Computes fuel/air ratio from `(1+f) h4 = h3 + f eta_b LHV`.
///
/// Both enthalpies are sensible enthalpies relative to 298.15 K. Chemical
/// energy is represented once, through the supplied lower heating value.
pub fn combustor_fuel_air_ratio(
    inlet_temperature_k: f64,
    outlet_temperature_k: f64,
    combustion_efficiency: f64,
    fuel_lower_heating_value_j_kg: f64,
    inlet_composition: GasComposition,
    outlet_composition: GasComposition,
) -> Result<f64, PhysicsError> {
    check_range(
        "combustion_efficiency",
        combustion_efficiency,
        f64::EPSILON,
        1.0,
    )?;
    check_positive(
        "fuel_lower_heating_value_j_kg",
        fuel_lower_heating_value_j_kg,
    )?;
    let inlet_h = gas_properties(inlet_temperature_k, inlet_composition)?.specific_enthalpy_j_kg;
    let outlet_h = gas_properties(outlet_temperature_k, outlet_composition)?.specific_enthalpy_j_kg;
    let denominator = combustion_efficiency * fuel_lower_heating_value_j_kg - outlet_h;
    if denominator <= 0.0 || outlet_h <= inlet_h {
        return Err(PhysicsError::OutOfRange {
            field: "combustor enthalpy balance",
            value: denominator,
            minimum: f64::EPSILON,
            maximum: f64::INFINITY,
        });
    }
    Ok((outlet_h - inlet_h) / denominator)
}

fn isentropic_pressure(
    total_state: TotalState,
    static_temperature_k: f64,
    composition: GasComposition,
) -> Result<f64, PhysicsError> {
    let integral_cp_over_t = integrate_simpson(
        static_temperature_k,
        total_state.temperature_k,
        |temperature_k| {
            let properties = gas_properties(temperature_k, composition)?;
            Ok(properties.specific_heat_cp_j_kg_k / temperature_k)
        },
    )?;
    let gas_constant = gas_properties(total_state.temperature_k, composition)?.gas_constant_j_kg_k;
    Ok(total_state.pressure_pa * (-integral_cp_over_t / gas_constant).exp())
}

fn temperature_at_isentropic_pressure(
    total_state: TotalState,
    static_pressure_pa: f64,
    composition: GasComposition,
) -> Result<f64, PhysicsError> {
    bisect_temperature(
        MIN_TEMPERATURE_K.max(0.25 * total_state.temperature_k),
        total_state.temperature_k,
        |temperature_k| {
            Ok(isentropic_pressure(total_state, temperature_k, composition)? - static_pressure_pa)
        },
        "isentropic pressure",
    )
}

fn integrate_simpson<F>(lower: f64, upper: f64, function: F) -> Result<f64, PhysicsError>
where
    F: Fn(f64) -> Result<f64, PhysicsError>,
{
    const INTERVALS: usize = 64;
    let step = (upper - lower) / INTERVALS as f64;
    let mut sum = function(lower)? + function(upper)?;
    for index in 1..INTERVALS {
        let weight = if index % 2 == 0 { 2.0 } else { 4.0 };
        sum += weight * function(lower + index as f64 * step)?;
    }
    Ok(sum * step / 3.0)
}

fn bisect_temperature<F>(
    mut lower: f64,
    mut upper: f64,
    function: F,
    equation: &'static str,
) -> Result<f64, PhysicsError>
where
    F: Fn(f64) -> Result<f64, PhysicsError>,
{
    let mut lower_value = function(lower)?;
    let upper_value = function(upper)?;
    if lower_value * upper_value > 0.0 {
        return Err(PhysicsError::NoConvergence { equation });
    }
    for _ in 0..80 {
        let midpoint = 0.5 * (lower + upper);
        let midpoint_value = function(midpoint)?;
        if midpoint_value.abs() < 1.0e-8 || (upper - lower) < 1.0e-9 {
            return Ok(midpoint);
        }
        if lower_value * midpoint_value <= 0.0 {
            upper = midpoint;
        } else {
            lower = midpoint;
            lower_value = midpoint_value;
        }
    }
    Err(PhysicsError::NoConvergence { equation })
}

fn check_positive(field: &'static str, value: f64) -> Result<(), PhysicsError> {
    check_range(field, value, f64::EPSILON, f64::INFINITY)
}

fn check_nonnegative(field: &'static str, value: f64) -> Result<(), PhysicsError> {
    check_range(field, value, 0.0, f64::INFINITY)
}

fn check_range(
    field: &'static str,
    value: f64,
    minimum: f64,
    maximum: f64,
) -> Result<(), PhysicsError> {
    if !value.is_finite() {
        return Err(PhysicsError::NonFinite { field, value });
    }
    if value < minimum || value > maximum {
        return Err(PhysicsError::OutOfRange {
            field,
            value,
            minimum,
            maximum,
        });
    }
    Ok(())
}

