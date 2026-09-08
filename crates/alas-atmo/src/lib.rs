// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Atmosphere models: every altitude-dependent physics module in this program
//! needs pressure, temperature and the properties derived from them, and two
//! unrelated reference implementations supply them at different points in the
//! program's ancestry. Each gets its own submodule rather than its own crate,
//! for the same reason `alas-math` gives its numerical primitives one each:
//! neither is large enough to justify a crate boundary, and both belong under
//! "atmosphere" for a caller choosing between them.
//!
//! [`atmosphere`] is native aerodynamic model's `Atmosphere` class: one point in the air,
//! evaluated by one of two altitude models. [`isa`] is the closed-form
//! International Standard Atmosphere and [`differentiable`] is the cubic
//! B-spline fitted through it, which is the model upstream uses when a caller
//! names none. The two disagree by about a per cent in temperature, so
//! [`Method`] is a modeling choice a caller makes deliberately, not a
//! performance knob.
//!
//! [`us1976`] is mission analysis model's US Standard 1976 model, which every mission analysis model mission
//! segment uses instead of either.

pub mod airspeed;
pub mod atmosphere;
pub mod differentiable;
pub mod isa;
mod us1976;

pub use airspeed::{
    calibrated_from_true, equivalent_from_true, mach_from_calibrated, true_from_calibrated,
    true_from_equivalent, true_from_mach, AirspeedError, SEA_LEVEL_DENSITY_DISPLAY_KG_M3,
    SEA_LEVEL_PRESSURE_PA, SEA_LEVEL_TEMPERATURE_K,
};
pub use atmosphere::{
    Atmosphere, AtmosphereError, DensityAltitudeError, DensityAltitudeMethod, Method,
    STANDARD_GRAVITY,
};
pub use differentiable::{altitude_knots_m, pressure_differentiable, temperature_differentiable};
pub use isa::{
    pressure_isa, temperature_isa, BAROMETRIC_GRAVITY, GAS_CONSTANT_AIR, GAS_CONSTANT_UNIVERSAL,
    MOLECULAR_MASS_AIR,
};
pub use us1976::{
    compute_values as us1976_compute_values, try_compute_values as us1976_try_compute_values,
    Us1976Error, Values as Us1976Values,
};
