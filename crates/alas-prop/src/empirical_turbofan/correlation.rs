// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Atmosphere, airspeed and domain helpers of the empirical turbofan deck.

use super::*;

/// Deck-constant anchors of the maximum-climb correlation, evaluated once at
/// construction instead of per request (each needs a US 1976 atmosphere call).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ClimbReference {
    /// Standard pressure at 3,048 m (10,000 ft), Pa.
    pub(super) p10_pa: f64,
    /// Standard pressure at the maximum-climb reference altitude, Pa.
    pub(super) pcr_pa: f64,
    /// Calibrated airspeed at the reference Mach and pressure (gamma 1.4), m/s.
    pub(super) cas_m_s: f64,
    /// Installed maximum-climb thrust at the reference point, the anchor the
    /// correlation's ratio multiplies, N ([super::max_climb]).
    pub(super) max_climb_thrust_n: f64,
}

pub(super) fn standard_pressure_pa(altitude_m: f64) -> f64 {
    // Use the same atmosphere implementation that supplies mission flight
    // conditions. A separate rounded troposphere fit moves the nominal
    // OpenAP anchor away from ratio 1 and breaks exact calibration closure.
    alas_atmo::us1976_compute_values(altitude_m, 0.0).pressure_pa
}

pub(super) fn cas_m_s(mach: f64, pressure_pa: f64, gamma: f64) -> f64 {
    let qc = pressure_pa
        * ((1.0 + 0.5 * (gamma - 1.0) * mach.powi(2)).powf(gamma / (gamma - 1.0)) - 1.0);
    let sea_level_mach = ((2.0 / 0.4) * ((qc / 101_325.0 + 1.0).powf(0.4 / 1.4) - 1.0))
        .max(0.0)
        .sqrt();
    340.294 * sea_level_mach
}

pub(super) fn validate_flight(flight: FlightCondition) -> Result<(), PropulsionError> {
    for (field, value) in [
        ("empirical turbofan altitude_m", flight.altitude_m),
        ("empirical turbofan mach", flight.mach),
        ("empirical turbofan pressure_pa", flight.pressure_pa),
        ("empirical turbofan temperature_k", flight.temperature_k),
        ("empirical turbofan gamma", flight.gamma),
        ("empirical turbofan velocity_m_s", flight.velocity_m_s),
        ("empirical turbofan gravity_m_s2", flight.gravity_m_s2),
    ] {
        if !value.is_finite() {
            return Err(PropulsionError::InvalidInput { field, value });
        }
    }
    if flight.altitude_m < 0.0
        // The transport presets include certified/observed cruise through
        // FL410 and ceilings near FL431. A hidden FL400 numerical cutoff made
        // otherwise valid step climbs fail as a propulsion NaN. Keep a
        // bounded 45,000-ft correlation domain and retain Extrapolated
        // validity outside the declared engine reference point.
        || flight.altitude_m > 13_716.0
        || !(0.0..=MAXIMUM_DECK_MACH).contains(&flight.mach)
        || flight.pressure_pa <= 0.0
        || flight.temperature_k <= 0.0
        || flight.gamma <= 1.0
        || flight.velocity_m_s < 0.0
        || flight.gravity_m_s2 <= 0.0
    {
        return Err(PropulsionError::InvalidInput {
            field: "empirical turbofan flight condition",
            value: flight.altitude_m,
        });
    }
    Ok(())
}
