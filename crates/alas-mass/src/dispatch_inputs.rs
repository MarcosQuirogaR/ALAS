// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Input validation of the takeoff-mass fixed point.
use super::DispatchLimits;

pub(super) fn validate_inputs(
    zero_fuel_mass_kg: f64,
    initial_guess_kg: f64,
    range_m: f64,
    limits: &DispatchLimits,
    max_iterations: usize,
    tolerance_kg: f64,
) -> Result<(), String> {
    if !zero_fuel_mass_kg.is_finite() || zero_fuel_mass_kg <= 0.0 {
        return Err(format!(
            "zero-fuel mass {zero_fuel_mass_kg} kg must be finite and positive"
        ));
    }
    if !initial_guess_kg.is_finite() || initial_guess_kg < zero_fuel_mass_kg {
        return Err(format!(
            "initial guess {initial_guess_kg} kg must be finite and at least the zero-fuel mass {zero_fuel_mass_kg} kg"
        ));
    }
    if !range_m.is_finite() || range_m < 0.0 {
        return Err(format!("range {range_m} m must be finite and nonnegative"));
    }
    if !limits.mtow_kg.is_finite() || limits.mtow_kg < zero_fuel_mass_kg {
        return Err(format!(
            "MTOW {} kg must be finite and at least the zero-fuel mass {zero_fuel_mass_kg} kg",
            limits.mtow_kg
        ));
    }
    for (name, value) in [
        ("MZFW", limits.mzfw_kg),
        ("MLW", limits.mlw_kg),
        ("usable tank capacity", limits.usable_capacity_kg),
    ] {
        if let Some(value) = value {
            if !value.is_finite() || value <= 0.0 {
                return Err(format!("{name} {value} kg must be finite and positive"));
            }
        }
    }
    if !tolerance_kg.is_finite() || tolerance_kg <= 0.0 {
        return Err(format!(
            "tolerance {tolerance_kg} kg must be finite and positive"
        ));
    }
    if max_iterations == 0 {
        return Err("max_iterations must be at least one".to_owned());
    }
    Ok(())
}
