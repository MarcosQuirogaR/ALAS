// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The standard atmosphere: every altitude-dependent physics module in this
//! program needs pressure, temperature and the properties derived from them,
//! and all of them read one model, the U.S. Standard Atmosphere, 1976.
//!
//! [`us1976_compute_values`] is the model as a function of altitude and
//! temperature deviation; [`Atmosphere`] is the same evaluation as an object
//! with one accessor per property. The two return identical numbers, so a
//! speed, a Mach number or a dynamic pressure formed by one discipline means
//! the same thing to the next. The altitude input is geopotential altitude
//! everywhere: the pressure altitude of a standard day, which is what a
//! flight level states. A geometric altitude above mean sea level (an
//! aerodrome elevation) enters through [`Atmosphere::from_geometric`],
//! [`us1976_values_at_geometric`] or [`geopotential_from_geometric`].
//!
//! [`airspeed`] holds the calibrated, equivalent and true airspeed
//! conversions on top of that state.

pub mod airspeed;
pub mod atmosphere;
mod us1976;

pub use airspeed::{
    calibrated_from_true, equivalent_from_true, mach_from_calibrated, true_from_calibrated,
    true_from_equivalent, true_from_mach, AirspeedError, GAMMA_AIR,
    SEA_LEVEL_DENSITY_DISPLAY_KG_M3, SEA_LEVEL_DENSITY_ICAO_KG_M3, SEA_LEVEL_PRESSURE_PA,
    SEA_LEVEL_TEMPERATURE_K,
};
pub use atmosphere::{
    pressure_isa, temperature_isa, Atmosphere, AtmosphereError, DensityAltitudeError,
    DensityAltitudeMethod, GAS_CONSTANT_UNIVERSAL, MOLECULAR_MASS_AIR, STANDARD_GRAVITY,
};
pub use us1976::{
    bisect_pressure_altitude_m as us1976_bisect_pressure_altitude_m,
    compute_values as us1976_compute_values, geometric_from_geopotential,
    geopotential_from_geometric, pressure_pa as us1976_pressure_pa,
    try_compute_values as us1976_try_compute_values,
    values_at_geometric as us1976_values_at_geometric, Us1976Error, Values as Us1976Values,
    EFFECTIVE_EARTH_RADIUS_M, GAS_CONSTANT_AIR,
};
