// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from native aerodynamic model/atmosphere/atmosphere.py
// Upstream: native aerodynamic model 4.2.8, MIT.
// Reference: alas @ 7d1555c1f4db5110cf6cd187c156718e1a033b50.

//! A point in the atmosphere: pressure and temperature by one of two altitude
//! models, and every quantity derived from them.
//!
//! Upstream this is one class with a `method` string selecting between the
//! closed-form ISA and the fitted, differentiable model, and everything below
//! `pressure()` and `temperature()` is derived identically either way. The
//! same shape is kept here, as [`Method`], because the choice is not an
//! implementation detail a caller can ignore: the two disagree by up to 1.1%
//! in temperature, so which one a discipline uses is part of what that
//! discipline computes. A translated module picks the method its Python
//! counterpart picked, and [`Method::default`] is [`Method::Differentiable`]
//! for the same reason upstream's default is -- so that a call site that
//! names no method reproduces one that named no method.
//!
//! Two gravitational accelerations appear in this crate, deliberately not
//! unified: [`crate::BAROMETRIC_GRAVITY`] (9.81 m/s^2) is what the barometric
//! formula uses, because that is the constant `_isa_atmo_functions.py`
//! defines locally and reads as `g`. [`STANDARD_GRAVITY`] (9.80665 m/s^2) is
//! what [`Atmosphere::density_altitude`]'s approximate formula uses instead,
//! written as a literal in `atmosphere.py` rather than importing the other
//! module's constant. Using one where the other belongs would move every
//! pressure value by a part in 500,000, which is exactly the kind of error
//! this crate exists to make impossible.
//!
//! `atmosphere.py` also defines `effective_collision_diameter` at module
//! scope. No method in the class reads it -- it is dead code upstream -- so
//! it has no counterpart here; nothing this crate computes would depend on
//! it.

use crate::differentiable::{pressure_differentiable, temperature_differentiable};
use crate::isa::{
    pressure_isa, temperature_isa, GAS_CONSTANT_AIR, GAS_CONSTANT_UNIVERSAL, MOLECULAR_MASS_AIR,
};

/// Standard gravity, in m/s^2, used only by the approximate density-altitude
/// formula.
///
/// Written as a literal `9.80665` in `atmosphere.py`'s `density_altitude`
/// rather than importing [`crate::BAROMETRIC_GRAVITY`]; see the module
/// documentation.
pub const STANDARD_GRAVITY: f64 = 9.80665;

/// Sutherland's law constant, in kg/(m*s*sqrt(K)).
///
/// Cited upstream to <https://www.cfd-online.com/Wiki/Sutherland's_law>.
const SUTHERLAND_C1: f64 = 1.458e-6;

/// Sutherland's law reference temperature, in Kelvin.
const SUTHERLAND_S: f64 = 110.4;

/// Which altitude model an [`Atmosphere`] evaluates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Method {
    /// The cubic B-spline fit through the ISA, and upstream's default. See
    /// [`crate::differentiable`] for what it is and why it is not
    /// interchangeable with the closed form.
    #[default]
    Differentiable,
    /// The closed-form International Standard Atmosphere.
    Isa,
}

/// Which density-altitude formula [`Atmosphere::density_altitude`] should
/// evaluate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DensityAltitudeMethod {
    /// The incompressible approximation. The only method this crate
    /// implements.
    Approximate,
    /// An iterative match against the full density profile. native aerodynamic model
    /// raises `NotImplementedError` for this branch rather than providing
    /// it; see [`DensityAltitudeError::ExactNotImplemented`].
    Exact,
}

/// Why [`Atmosphere::density_altitude`] could not produce a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DensityAltitudeError {
    /// The caller asked for [`DensityAltitudeMethod::Exact`]. Upstream raises
    /// `NotImplementedError` for the same case; reporting this instead of
    /// silently substituting the approximate formula is the translation of
    /// that.
    ExactNotImplemented,
}

/// Why a checked atmosphere construction was rejected.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AtmosphereError {
    /// Altitude or temperature deviation was not finite.
    NonFiniteInput {
        /// Geopotential altitude supplied by the caller.
        altitude_m: f64,
        /// Temperature offset supplied by the caller.
        temperature_deviation_k: f64,
    },
    /// The requested altitude lies outside the checked physical atmosphere
    /// band. The legacy differentiable fan extends farther for optimizer
    /// robustness, but those values are not admitted at product boundaries.
    AltitudeOutsideModel {
        /// Requested geopotential altitude in metres.
        altitude_m: f64,
        /// Lowest altitude admitted by the checked physical model, in metres.
        min_m: f64,
        /// Highest altitude admitted by the checked physical model, in metres.
        max_m: f64,
    },
    /// The resulting thermodynamic state is not physically usable.
    NonPhysicalState {
        /// Temperature in kelvin.
        temperature_k: f64,
        /// Pressure in pascals.
        pressure_pa: f64,
        /// Density in kg/m^3.
        density_kg_m3: f64,
    },
}

impl std::fmt::Display for AtmosphereError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteInput {
                altitude_m,
                temperature_deviation_k,
            } => write!(
                f,
                "atmosphere inputs must be finite (altitude={altitude_m:?}, temperature deviation={temperature_deviation_k:?})"
            ),
            Self::AltitudeOutsideModel {
                altitude_m,
                min_m,
                max_m,
            } => write!(
                f,
                "altitude {altitude_m} m lies outside the checked physical atmosphere band [{min_m}, {max_m}] m"
            ),
            Self::NonPhysicalState {
                temperature_k,
                pressure_pa,
                density_kg_m3,
            } => write!(
                f,
                "atmosphere returned a nonphysical state (T={temperature_k:?} K, p={pressure_pa:?} Pa, rho={density_kg_m3:?} kg/m^3)"
            ),
        }
    }
}

impl std::error::Error for AtmosphereError {}

impl std::fmt::Display for DensityAltitudeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExactNotImplemented => {
                f.write_str("exact density altitude calculation not yet implemented")
            }
        }
    }
}

impl std::error::Error for DensityAltitudeError {}

/// A point in the atmosphere.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Atmosphere {
    /// Geopotential altitude above mean sea level, in metres.
    pub altitude_m: f64,
    /// Which altitude model to evaluate.
    pub method: Method,
    /// A deviation from the temperature model, in Kelvin (equivalently
    /// Celsius, since it is an offset rather than a scale). Added after the
    /// base temperature, so it shifts [`Atmosphere::temperature`] and
    /// everything computed from it, but leaves [`Atmosphere::pressure`]
    /// untouched -- matching the upstream model, which computes pressure and
    /// temperature independently rather than from each other.
    pub temperature_deviation_k: f64,
}

impl Default for Atmosphere {
    /// Sea level, the differentiable model, no temperature deviation -- the
    /// upstream class's defaults.
    fn default() -> Self {
        Self {
            altitude_m: 0.0,
            method: Method::default(),
            temperature_deviation_k: 0.0,
        }
    }
}

impl Atmosphere {
    /// A new atmosphere at `altitude_m` under the default (differentiable)
    /// model, reproducing `Atmosphere(altitude=...)`.
    pub fn new(altitude_m: f64) -> Self {
        Self {
            altitude_m,
            ..Self::default()
        }
    }

    /// Construct an atmosphere after checking the model domain and state.
    ///
    /// [`Self::new`] remains infallible for reference-fixture compatibility;
    /// new product/configuration boundaries should use this checked entry
    /// point so invalid finite inputs cannot enter a physics solve.
    pub fn try_new(altitude_m: f64) -> Result<Self, AtmosphereError> {
        let atmosphere = Self::new(altitude_m);
        atmosphere.validate()?;
        Ok(atmosphere)
    }

    /// A new atmosphere at `altitude_m` under the closed-form ISA,
    /// reproducing `Atmosphere(altitude=..., method="isa")`.
    pub fn isa(altitude_m: f64) -> Self {
        Self::new(altitude_m).with_method(Method::Isa)
    }

    /// Construct a checked closed-form ISA atmosphere.
    pub fn try_isa(altitude_m: f64) -> Result<Self, AtmosphereError> {
        let atmosphere = Self::isa(altitude_m);
        atmosphere.validate()?;
        Ok(atmosphere)
    }

    /// This atmosphere evaluated under `method` instead.
    pub fn with_method(self, method: Method) -> Self {
        Self { method, ..self }
    }

    /// This atmosphere with `temperature_deviation_k` applied.
    pub fn with_temperature_deviation(self, temperature_deviation_k: f64) -> Self {
        Self {
            temperature_deviation_k,
            ..self
        }
    }

    /// Apply a temperature deviation and reject non-finite/nonphysical state.
    pub fn try_with_temperature_deviation(
        self,
        temperature_deviation_k: f64,
    ) -> Result<Self, AtmosphereError> {
        let atmosphere = self.with_temperature_deviation(temperature_deviation_k);
        atmosphere.validate()?;
        Ok(atmosphere)
    }

    /// Validate all primitive properties used by downstream Mach/Reynolds
    /// and force calculations.
    pub fn validate(&self) -> Result<(), AtmosphereError> {
        if !self.altitude_m.is_finite() || !self.temperature_deviation_k.is_finite() {
            return Err(AtmosphereError::NonFiniteInput {
                altitude_m: self.altitude_m,
                temperature_deviation_k: self.temperature_deviation_k,
            });
        }
        // Keep the broad differentiable fan available through `new` for
        // frozen reference/optimizer behavior, but make checked construction
        // use the tabulated physical-atmosphere band shared by the mission
        // model and the ISA layer table.
        const MIN_PHYSICAL_ALTITUDE_M: f64 = -2_000.0;
        const MAX_PHYSICAL_ALTITUDE_M: f64 = 84_852.0;
        let min_m = MIN_PHYSICAL_ALTITUDE_M;
        let max_m = MAX_PHYSICAL_ALTITUDE_M;
        if self.altitude_m < min_m || self.altitude_m > max_m {
            return Err(AtmosphereError::AltitudeOutsideModel {
                altitude_m: self.altitude_m,
                min_m,
                max_m,
            });
        }
        let pressure_pa = self.pressure();
        let temperature_k = self.temperature();
        let density_kg_m3 = self.density();
        if !pressure_pa.is_finite()
            || pressure_pa <= 0.0
            || !temperature_k.is_finite()
            || temperature_k <= 0.0
            || !density_kg_m3.is_finite()
            || density_kg_m3 <= 0.0
            || !self.speed_of_sound().is_finite()
            || !self.dynamic_viscosity().is_finite()
            || self.dynamic_viscosity() <= 0.0
        {
            return Err(AtmosphereError::NonPhysicalState {
                temperature_k,
                pressure_pa,
                density_kg_m3,
            });
        }
        Ok(())
    }

    /// Pressure, in pascals.
    pub fn pressure(&self) -> f64 {
        match self.method {
            Method::Differentiable => pressure_differentiable(self.altitude_m),
            Method::Isa => pressure_isa(self.altitude_m),
        }
    }

    /// Temperature, in Kelvin, including `temperature_deviation_k`.
    pub fn temperature(&self) -> f64 {
        let base = match self.method {
            Method::Differentiable => temperature_differentiable(self.altitude_m),
            Method::Isa => temperature_isa(self.altitude_m),
        };
        base + self.temperature_deviation_k
    }

    /// Density, in kg/m^3, from the ideal gas law.
    pub fn density(&self) -> f64 {
        self.pressure() / (self.temperature() * GAS_CONSTANT_AIR)
    }

    /// Speed of sound, in m/s.
    pub fn speed_of_sound(&self) -> f64 {
        (self.ratio_of_specific_heats() * GAS_CONSTANT_AIR * self.temperature()).powf(0.5)
    }

    /// Dynamic viscosity (mu), in kg/(m*s), from Sutherland's law.
    ///
    /// Cited upstream to <https://www.cfd-online.com/Wiki/Sutherland's_law>.
    /// Rathakrishnan, E. (2013). *Theoretical Aerodynamics*. John Wiley &
    /// Sons: valid from 0.01 to 100 atm and between 0 and 3000 K. White, F.
    /// M., & Corfield, I. (2006). *Viscous Fluid Flow* (Vol. 3, pp. 433-434).
    /// McGraw-Hill: error no more than about 2% for air between 170 K and
    /// 1900 K.
    pub fn dynamic_viscosity(&self) -> f64 {
        let temperature_k = self.temperature();
        SUTHERLAND_C1 * temperature_k.powf(1.5) / (temperature_k + SUTHERLAND_S)
    }

    /// Kinematic viscosity (nu), in m^2/s. Definitional: dynamic viscosity
    /// over density.
    pub fn kinematic_viscosity(&self) -> f64 {
        self.dynamic_viscosity() / self.density()
    }

    /// Ratio of specific heats, held constant at 1.4 (no temperature
    /// variation modeled, matching an upstream `TODO` left unresolved).
    pub fn ratio_of_specific_heats(&self) -> f64 {
        1.4
    }

    /// Mean free path of an air molecule, in metres.
    ///
    /// Models a hard-sphere gas with the same viscosity as the actual gas.
    /// Vincenti, W. G. and Kruger, C. H. (1965). *Introduction to Physical
    /// Gas Dynamics*. Krieger Publishing Company, p. 414.
    pub fn mean_free_path(&self) -> f64 {
        self.dynamic_viscosity() / self.pressure()
            * (std::f64::consts::PI * GAS_CONSTANT_UNIVERSAL * self.temperature()
                / (2.0 * MOLECULAR_MASS_AIR))
                .sqrt()
    }

    /// Knudsen number for a body of characteristic `length_m`.
    pub fn knudsen(&self, length_m: f64) -> f64 {
        self.mean_free_path() / length_m
    }

    /// Density altitude, in metres, by `method`.
    ///
    /// See <https://en.wikipedia.org/wiki/Density_altitude>.
    ///
    /// # Errors
    ///
    /// Returns [`DensityAltitudeError::ExactNotImplemented`] for
    /// [`DensityAltitudeMethod::Exact`]; see the module documentation.
    pub fn density_altitude(
        &self,
        method: DensityAltitudeMethod,
    ) -> Result<f64, DensityAltitudeError> {
        match method {
            DensityAltitudeMethod::Approximate => Ok(self.density_altitude_approximate()),
            DensityAltitudeMethod::Exact => Err(DensityAltitudeError::ExactNotImplemented),
        }
    }

    /// The incompressible density-altitude approximation.
    fn density_altitude_approximate(&self) -> f64 {
        // ISA sea-level reference and tropospheric lapse rate, local to this
        // formula rather than read from the layer table.
        const TEMPERATURE_SEA_LEVEL_K: f64 = 288.15;
        const PRESSURE_SEA_LEVEL_PA: f64 = 101_325.0;
        const LAPSE_RATE_K_PER_M: f64 = 0.0065;

        let temperature_ratio = self.temperature() / TEMPERATURE_SEA_LEVEL_K;
        let pressure_ratio = self.pressure() / PRESSURE_SEA_LEVEL_PA;

        let exponent =
            (STANDARD_GRAVITY / (GAS_CONSTANT_AIR * LAPSE_RATE_K_PER_M) - 1.0).powf(-1.0);

        (TEMPERATURE_SEA_LEVEL_K / LAPSE_RATE_K_PER_M)
            * (1.0 - (pressure_ratio / temperature_ratio).powf(exponent))
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_method_is_the_fit_and_not_the_closed_form() {
        // The single most consequential default in this crate: a call site
        // that names no method gets the same model upstream would give it.
        assert_eq!(Atmosphere::new(0.0).method, Method::Differentiable);
        assert_eq!(Atmosphere::default().method, Method::Differentiable);
        assert_eq!(Atmosphere::isa(0.0).method, Method::Isa);
    }

    #[test]
    fn the_two_methods_disagree_by_about_a_per_cent_in_temperature() {
        // If these ever agreed to within the `closed` tier, the fit would
        // have stopped being a fit and every module that picks between them
        // would have stopped needing to.
        let worst = (0..=250)
            .map(|step| f64::from(step) * 100.0)
            .map(|altitude_m| {
                let isa = Atmosphere::isa(altitude_m).temperature();
                (Atmosphere::new(altitude_m).temperature() - isa).abs() / isa
            })
            .fold(0.0_f64, f64::max);
        assert!((1e-3..1e-1).contains(&worst), "worst disagreement {worst}");
    }

    #[test]
    fn temperature_deviation_shifts_temperature_and_density_but_not_pressure() {
        for method in [Method::Isa, Method::Differentiable] {
            let base = Atmosphere::new(5000.0).with_method(method);
            let warmer = base.with_temperature_deviation(10.0);

            assert_eq!(warmer.pressure(), base.pressure());
            assert_eq!(warmer.temperature(), base.temperature() + 10.0);
            // A warmer parcel at the same pressure is less dense.
            assert!(warmer.density() < base.density());
        }
    }

    #[test]
    fn density_matches_the_ideal_gas_law() {
        for method in [Method::Isa, Method::Differentiable] {
            let atmo = Atmosphere::new(3000.0).with_method(method);
            let recovered_pressure = atmo.density() * GAS_CONSTANT_AIR * atmo.temperature();
            assert!((recovered_pressure - atmo.pressure()).abs() < 1e-9);
        }
    }

    #[test]
    fn knudsen_scales_inversely_with_length() {
        let atmo = Atmosphere::isa(20_000.0);
        let mean_free_path = atmo.mean_free_path();
        assert!((atmo.knudsen(1.0) - mean_free_path).abs() < 1e-20);
        assert!((atmo.knudsen(2.0) - mean_free_path / 2.0).abs() < 1e-20);
    }

    #[test]
    fn density_altitude_at_isa_sea_level_is_zero() {
        let atmo = Atmosphere::isa(0.0);
        let density_altitude_m = atmo
            .density_altitude(DensityAltitudeMethod::Approximate)
            .expect("the approximate method is implemented");
        assert!(density_altitude_m.abs() < 1e-9);
    }

    #[test]
    fn exact_density_altitude_reports_unsupported_rather_than_a_number() {
        let atmo = Atmosphere::isa(1000.0);
        assert_eq!(
            atmo.density_altitude(DensityAltitudeMethod::Exact),
            Err(DensityAltitudeError::ExactNotImplemented)
        );
    }

    #[test]
    fn ratio_of_specific_heats_is_constant() {
        assert_eq!(Atmosphere::isa(0.0).ratio_of_specific_heats(), 1.4);
        assert_eq!(Atmosphere::isa(50_000.0).ratio_of_specific_heats(), 1.4);
    }

    #[test]
    fn default_is_sea_level_with_no_deviation() {
        let atmo = Atmosphere::default();
        assert_eq!(atmo.altitude_m, 0.0);
        assert_eq!(atmo.temperature_deviation_k, 0.0);
    }

    #[test]
    fn checked_construction_rejects_nonphysical_temperature_and_nonfinite_inputs() {
        assert!(matches!(
            Atmosphere::try_new(0.0).and_then(|atmo| atmo.try_with_temperature_deviation(-400.0)),
            Err(AtmosphereError::NonPhysicalState { .. })
        ));
        assert!(matches!(
            Atmosphere::try_new(f64::NAN),
            Err(AtmosphereError::NonFiniteInput { .. })
        ));
        assert!(matches!(
            Atmosphere::try_new(-10_000_000.0),
            Err(AtmosphereError::AltitudeOutsideModel { .. })
        ));
        assert!(Atmosphere::try_new(84_852.0).is_ok());
        assert!(matches!(
            Atmosphere::try_new(84_853.0),
            Err(AtmosphereError::AltitudeOutsideModel { .. })
        ));
        assert!(matches!(
            Atmosphere::try_isa(f64::INFINITY),
            Err(AtmosphereError::NonFiniteInput { .. })
        ));
    }
}
