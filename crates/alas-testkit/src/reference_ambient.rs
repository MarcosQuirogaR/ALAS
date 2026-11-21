// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ambient state the reference implementation's fitted atmosphere
//! produced, read back from `golden/atmo/differentiable.json`.
//!
//! Several parity fixtures were generated with that fitted atmosphere and
//! record the altitude but not the temperature, pressure or density it
//! returned. The product evaluates the U.S. Standard Atmosphere, 1976, which
//! differs from the fit by up to 1.5 % in temperature and 0.15 % in pressure
//! below 15 km. A parity test that wants to compare a model with the
//! reference, and not one atmosphere with the other, reads the reference's
//! state here and drives the model with it through an explicit state
//! (`Atmosphere::new(h).with_temperature_deviation(dt)` holds the pressure of
//! the level and sets the temperature).
//!
//! The atmosphere fixture holds one row per altitude, on a 250 m grid inside
//! the flight envelope; each row carries the temperature deviation it was
//! generated with. [`reference_ambient`] undoes that deviation and returns the
//! standard-day state. The data is the crate's only input: no atmosphere model
//! is evaluated here.

use serde::Deserialize;

/// Sutherland's law constant `beta`, kg/(m s sqrt(K)), as the fixture was
/// generated with it (U.S. Standard Atmosphere, 1976, equation 51).
const SUTHERLAND_BETA: f64 = 1.458e-6;
/// Sutherland's constant `S`, K.
const SUTHERLAND_S: f64 = 110.4;

/// One recorded row of the atmosphere fixture.
#[derive(Debug, Deserialize)]
struct Row {
    altitude_m: f64,
    temperature_deviation_k: f64,
    temperature_k: f64,
    pressure_pa: f64,
    density_kg_m3: f64,
    // Recorded for the unit test that checks the reconstruction.
    #[cfg_attr(not(test), allow(dead_code))]
    speed_of_sound_m_s: f64,
    #[cfg_attr(not(test), allow(dead_code))]
    dynamic_viscosity_pa_s: f64,
    ratio_of_specific_heats: f64,
}

#[derive(Debug, Deserialize)]
struct File {
    cases: Vec<Row>,
}

/// The standard-day state of the reference's fitted atmosphere at one
/// altitude. Every field is a recorded value or follows from recorded values
/// by the ideal gas law and Sutherland's law (see the module note).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReferenceAmbient {
    /// The altitude the reference was asked for, m.
    pub altitude_m: f64,
    /// Static temperature, K.
    pub temperature_k: f64,
    /// Static pressure, Pa.
    pub pressure_pa: f64,
    /// Density, kg/m^3.
    pub density_kg_m3: f64,
    /// Speed of sound, m/s.
    pub speed_of_sound_m_s: f64,
    /// Dynamic viscosity, kg/(m s).
    pub dynamic_viscosity_pa_s: f64,
    /// Specific gas constant the reference used, J/(kg K): `p / (rho T)`.
    pub gas_constant_j_kg_k: f64,
}

/// The reference's standard-day ambient state at `altitude_m`.
///
/// # Panics
///
/// When the fixture has no row at exactly that altitude: the fixture is a
/// grid, and a test asking for an altitude off it has no recorded state to
/// use. [`try_reference_ambient`] reports that as `None`.
pub fn reference_ambient(altitude_m: f64) -> ReferenceAmbient {
    try_reference_ambient(altitude_m)
        .unwrap_or_else(|| panic!("golden/atmo/differentiable.json has no row at {altitude_m} m"))
}

/// The reference's standard-day ambient state at `altitude_m`, or `None` when
/// the fixture records no row at exactly that altitude.
pub fn try_reference_ambient(altitude_m: f64) -> Option<ReferenceAmbient> {
    let file: File = crate::load("atmo", "differentiable");
    let row = file.cases.iter().find(|row| row.altitude_m == altitude_m)?;
    let gas_constant = row.pressure_pa / (row.density_kg_m3 * row.temperature_k);
    let temperature = row.temperature_k - row.temperature_deviation_k;
    let a2 = row.ratio_of_specific_heats * gas_constant * temperature;
    Some(ReferenceAmbient {
        altitude_m,
        temperature_k: temperature,
        pressure_pa: row.pressure_pa,
        density_kg_m3: row.pressure_pa / (gas_constant * temperature),
        speed_of_sound_m_s: a2.sqrt(),
        dynamic_viscosity_pa_s: SUTHERLAND_BETA * temperature.powf(1.5)
            / (temperature + SUTHERLAND_S),
        gas_constant_j_kg_k: gas_constant,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The standard-day reconstruction reproduces the rows recorded without a
    /// temperature deviation, and the recorded viscosity of every row follows
    /// from Sutherland's law at its recorded temperature.
    #[test]
    fn the_reconstruction_agrees_with_every_recorded_row() {
        let file: File = crate::load("atmo", "differentiable");
        let mut checked = 0;
        for row in file
            .cases
            .iter()
            .filter(|row| (-1000.0..=20_000.0).contains(&row.altitude_m))
        {
            let mu =
                SUTHERLAND_BETA * row.temperature_k.powf(1.5) / (row.temperature_k + SUTHERLAND_S);
            assert!(
                (mu / row.dynamic_viscosity_pa_s - 1.0).abs() < 1e-12,
                "viscosity at {} m",
                row.altitude_m
            );
            let state = reference_ambient(row.altitude_m);
            let a_row =
                (row.ratio_of_specific_heats * state.gas_constant_j_kg_k * row.temperature_k)
                    .sqrt();
            assert!(
                (a_row / row.speed_of_sound_m_s - 1.0).abs() < 1e-12,
                "speed of sound at {} m",
                row.altitude_m
            );
            if row.temperature_deviation_k == 0.0 {
                assert_eq!(state.temperature_k, row.temperature_k);
                assert!(
                    (state.density_kg_m3 / row.density_kg_m3 - 1.0).abs() < 1e-14,
                    "density at {} m",
                    row.altitude_m
                );
            }
            checked += 1;
        }
        assert!(checked > 50, "only {checked} rows checked");
    }
}
