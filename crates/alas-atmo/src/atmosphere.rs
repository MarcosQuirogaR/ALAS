// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A point in the standard atmosphere: pressure, temperature and every
//! quantity derived from them.
//!
//! One model serves the whole program: the U.S. Standard Atmosphere, 1976
//! (NOAA, NASA, USAF; NOAA-S/T 76-1562), evaluated by [`crate::us1976`].
//! [`Atmosphere`] is the object form of that model and
//! [`crate::us1976_compute_values`] its function form; both return the same
//! numbers for the same input, bit for bit, so a true airspeed formed on one
//! side of an interface converts back to the same Mach number on the other.
//!
//! # Altitude convention
//!
//! [`Atmosphere::altitude_m`] is the **geopotential** altitude `H`, in
//! metres: the altitude of the model's layer table and, on a standard day,
//! the pressure altitude. That is what a flight level, a cruise altitude, a
//! ceiling, a cabin altitude or an operating-limit table states, so those
//! are passed as they stand ([`Atmosphere::new`], or
//! [`Atmosphere::from_geopotential`] where the call site should say it). A
//! temperature deviation keeps the pressure of the level: the altitude
//! remains a pressure altitude on a warm or a cold day.
//!
//! A **geometric** altitude `Z` above mean sea level (an aerodrome or
//! terrain elevation) enters through [`Atmosphere::from_geometric`], which
//! converts with `H = r0 Z / (r0 + Z)` and the standard's effective Earth
//! radius `r0 = 6 356 766 m` (equation 18). At 11 km the two differ by
//! 19 m, 0.3 % in pressure; at an aerodrome 610 m up, by 0.06 m.
//!
//! # Smoothness
//!
//! Inside a layer every property is analytic in altitude. At a layer
//! boundary temperature is continuous with a step in its lapse rate (from
//! -6.5 K/km to zero at 11 km geopotential), so density, speed of sound and
//! viscosity are continuous with a kink there, and pressure is continuous
//! with a continuous first derivative (`dp/dH = -g0 p / (R T)`). That kink
//! is the tropopause of the standard, not an artefact, and no smoothing is
//! applied: nothing in this program differentiates the atmosphere across a
//! layer boundary. The published base pressures of the layers are rounded to
//! six significant figures, which leaves a relative step of at most 3e-6 in
//! pressure and density at a boundary (22 632.04 Pa from the troposphere
//! formula against the tabulated 22 632.1 Pa at 11 km).
//!
//! # Validity
//!
//! Geopotential altitudes from -2 000 m to 84 852 m (86 000 m geometric,
//! the top of the standard's lower atmosphere). [`Atmosphere::new`]
//! clamps a request outside that band to its edge and logs a warning;
//! [`Atmosphere::try_new`] rejects it.

use crate::us1976::{self, GAS_CONSTANT_AIR};

pub use crate::us1976::{geometric_from_geopotential, geopotential_from_geometric};

/// Standard gravity `g0`, in m/s^2 (3rd CGPM, 1901; ISO 80000-3; ICAO Doc 7488).
///
/// The program's single source of truth is [`alas_units::STANDARD_GRAVITY`];
/// this is the same value, re-exported here for the crates that depend on
/// the atmosphere but not on the unit table. It is the gravity of the
/// model's hydrostatic equation.
pub const STANDARD_GRAVITY: f64 = alas_units::STANDARD_GRAVITY;

/// Universal gas constant `R*`, in J/(mol*K): the value the U.S. Standard
/// Atmosphere, 1976 adopts (section 1.2.1, adopted constants), not the
/// current CODATA 8.314 46.
pub const GAS_CONSTANT_UNIVERSAL: f64 = 8.31432;

/// Mean molecular mass of air at sea level `M0`, in kg/mol (U.S. Standard
/// Atmosphere, 1976, section 1.2.1, adopted constants).
pub const MOLECULAR_MASS_AIR: f64 = 28.9644e-3;

/// Which density-altitude formula [`Atmosphere::density_altitude`] should
/// evaluate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DensityAltitudeMethod {
    /// The incompressible approximation. The only method this crate
    /// implements.
    Approximate,
    /// An iterative match against the full density profile. Not
    /// implemented; see [`DensityAltitudeError::ExactNotImplemented`].
    Exact,
}

/// Why [`Atmosphere::density_altitude`] could not produce a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DensityAltitudeError {
    /// The caller asked for [`DensityAltitudeMethod::Exact`]. Reported
    /// instead of silently substituting the approximate formula.
    #[error("exact density altitude calculation not yet implemented")]
    ExactNotImplemented,
}

/// Why a checked atmosphere construction was rejected.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum AtmosphereError {
    /// Altitude or temperature deviation was not finite.
    #[error(
        "atmosphere inputs must be finite (altitude={altitude_m:?}, temperature deviation={temperature_deviation_k:?})"
    )]
    NonFiniteInput {
        /// Geopotential altitude supplied by the caller.
        altitude_m: f64,
        /// Temperature offset supplied by the caller.
        temperature_deviation_k: f64,
    },
    /// The requested altitude lies outside the tabulated band of the model.
    #[error(
        "altitude {altitude_m} m lies outside the standard atmosphere band [{min_m}, {max_m}] m of geopotential altitude"
    )]
    AltitudeOutsideModel {
        /// Requested geopotential altitude in metres.
        altitude_m: f64,
        /// Lowest geopotential altitude of the model, in metres.
        min_m: f64,
        /// Highest geopotential altitude of the model, in metres.
        max_m: f64,
    },
    /// The resulting thermodynamic state is not physically usable.
    #[error(
        "atmosphere returned a nonphysical state (T={temperature_k:?} K, p={pressure_pa:?} Pa, rho={density_kg_m3:?} kg/m^3)"
    )]
    NonPhysicalState {
        /// Temperature in kelvin.
        temperature_k: f64,
        /// Pressure in pascals.
        pressure_pa: f64,
        /// Density in kg/m^3.
        density_kg_m3: f64,
    },
}

/// A point in the standard atmosphere.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Atmosphere {
    /// Geopotential altitude, in metres: the pressure altitude of a
    /// standard day.
    pub altitude_m: f64,
    /// A deviation from the standard temperature, in Kelvin (equivalently
    /// Celsius, since it is an offset rather than a scale). It shifts
    /// [`Atmosphere::temperature`] and everything computed from it and
    /// leaves [`Atmosphere::pressure`] untouched: the altitude is a pressure
    /// level, on a warmer or colder day.
    pub temperature_deviation_k: f64,
}

impl Atmosphere {
    /// The standard atmosphere at the geopotential altitude `altitude_m`,
    /// in metres: a pressure altitude, a flight level in metres.
    pub fn new(altitude_m: f64) -> Self {
        Self {
            altitude_m,
            temperature_deviation_k: 0.0,
        }
    }

    /// The standard atmosphere at the geopotential altitude
    /// `geopotential_altitude_m`, in metres: [`Self::new`] under the name
    /// that states the convention, for a call site that reads a flight level
    /// or a pressure altitude.
    pub fn from_geopotential(geopotential_altitude_m: f64) -> Self {
        Self::new(geopotential_altitude_m)
    }

    /// The standard atmosphere at the geometric altitude
    /// `geometric_altitude_m` above mean sea level, in metres: an aerodrome
    /// or terrain elevation, a surveyed height.
    pub fn from_geometric(geometric_altitude_m: f64) -> Self {
        Self::new(geopotential_from_geometric(geometric_altitude_m))
    }

    /// Construct an atmosphere after checking the model domain and state.
    ///
    /// [`Self::new`] is infallible and clamps to the model band; product and
    /// configuration boundaries use this checked entry point so that invalid
    /// inputs cannot enter a physics solve.
    pub fn try_new(altitude_m: f64) -> Result<Self, AtmosphereError> {
        let atmosphere = Self::new(altitude_m);
        atmosphere.validate()?;
        Ok(atmosphere)
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

    /// Validate the inputs, the model band and every primitive property
    /// used by downstream Mach/Reynolds and force calculations.
    pub fn validate(&self) -> Result<(), AtmosphereError> {
        use us1976::Us1976Error;
        match us1976::try_compute_values(self.altitude_m, self.temperature_deviation_k) {
            Ok(_) => Ok(()),
            Err(Us1976Error::NonFiniteInput { .. }) => Err(AtmosphereError::NonFiniteInput {
                altitude_m: self.altitude_m,
                temperature_deviation_k: self.temperature_deviation_k,
            }),
            Err(Us1976Error::AltitudeOutsideModel { .. }) => {
                let (min_m, max_m) = us1976::geopotential_band_m();
                Err(AtmosphereError::AltitudeOutsideModel {
                    altitude_m: self.altitude_m,
                    min_m,
                    max_m,
                })
            }
            Err(Us1976Error::NonPhysicalState) => {
                let values = self.values();
                Err(AtmosphereError::NonPhysicalState {
                    temperature_k: values.temperature_k,
                    pressure_pa: values.pressure_pa,
                    density_kg_m3: values.density_kg_m3,
                })
            }
        }
    }

    /// Geometric altitude of this point above mean sea level, in metres.
    pub fn geometric_altitude_m(&self) -> f64 {
        geometric_from_geopotential(self.altitude_m)
    }

    /// Every property of the model at this point.
    pub fn values(&self) -> us1976::Values {
        us1976::compute_values(self.altitude_m, self.temperature_deviation_k)
    }

    /// Pressure, in pascals.
    pub fn pressure(&self) -> f64 {
        self.values().pressure_pa
    }

    /// Temperature, in Kelvin, including `temperature_deviation_k`.
    pub fn temperature(&self) -> f64 {
        self.values().temperature_k
    }

    /// Density, in kg/m^3, from the ideal gas law.
    pub fn density(&self) -> f64 {
        self.values().density_kg_m3
    }

    /// Speed of sound, in m/s.
    pub fn speed_of_sound(&self) -> f64 {
        self.values().speed_of_sound_m_s
    }

    /// Dynamic viscosity (mu), in kg/(m*s), from Sutherland's law with the
    /// constants of the U.S. Standard Atmosphere, 1976 (equation 51):
    /// `beta = 1.458e-6 kg/(m s K^0.5)`, `S = 110.4 K`.
    pub fn dynamic_viscosity(&self) -> f64 {
        self.values().dynamic_viscosity_pa_s
    }

    /// Kinematic viscosity (nu), in m^2/s. Definitional: dynamic viscosity
    /// over density.
    pub fn kinematic_viscosity(&self) -> f64 {
        self.values().kinematic_viscosity_m2_s
    }

    /// Ratio of specific heats, held constant at 1.4 (the value the
    /// standard uses for its speed of sound, equation 50).
    pub fn ratio_of_specific_heats(&self) -> f64 {
        crate::airspeed::GAMMA_AIR
    }

    /// Mean free path of an air molecule, in metres.
    ///
    /// Models a hard-sphere gas with the same viscosity as the actual gas.
    /// Vincenti, W. G. and Kruger, C. H. (1965). *Introduction to Physical
    /// Gas Dynamics*. Krieger Publishing Company, p. 414.
    pub fn mean_free_path(&self) -> f64 {
        let values = self.values();
        values.dynamic_viscosity_pa_s / values.pressure_pa
            * (std::f64::consts::PI * GAS_CONSTANT_UNIVERSAL * values.temperature_k
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
    /// [`DensityAltitudeMethod::Exact`].
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
        // Sea-level reference and tropospheric lapse rate of the standard.
        use crate::airspeed::{
            SEA_LEVEL_PRESSURE_PA as PRESSURE_SEA_LEVEL_PA,
            SEA_LEVEL_TEMPERATURE_K as TEMPERATURE_SEA_LEVEL_K,
        };
        const LAPSE_RATE_K_PER_M: f64 = 0.0065;

        let values = self.values();
        let temperature_ratio = values.temperature_k / TEMPERATURE_SEA_LEVEL_K;
        let pressure_ratio = values.pressure_pa / PRESSURE_SEA_LEVEL_PA;

        let exponent =
            (STANDARD_GRAVITY / (GAS_CONSTANT_AIR * LAPSE_RATE_K_PER_M) - 1.0).powf(-1.0);

        (TEMPERATURE_SEA_LEVEL_K / LAPSE_RATE_K_PER_M)
            * (1.0 - (pressure_ratio / temperature_ratio).powf(exponent))
    }
}

/// Standard pressure at the geopotential altitude `altitude_m`, in pascals:
/// [`Atmosphere::pressure`] as a free function.
pub fn pressure_isa(altitude_m: f64) -> f64 {
    Atmosphere::new(altitude_m).pressure()
}

/// Standard temperature at the geopotential altitude `altitude_m`, in Kelvin:
/// [`Atmosphere::temperature`] as a free function.
pub fn temperature_isa(altitude_m: f64) -> f64 {
    Atmosphere::new(altitude_m).temperature()
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_object_and_the_function_form_return_identical_values() {
        for step in 0..=80 {
            let altitude_m = -1_000.0 + f64::from(step) * 500.0;
            for deviation_k in [0.0, 15.0, -20.0] {
                let atmosphere =
                    Atmosphere::new(altitude_m).with_temperature_deviation(deviation_k);
                let values = us1976::compute_values(altitude_m, deviation_k);
                assert_eq!(atmosphere.pressure(), values.pressure_pa);
                assert_eq!(atmosphere.temperature(), values.temperature_k);
                assert_eq!(atmosphere.density(), values.density_kg_m3);
                assert_eq!(atmosphere.speed_of_sound(), values.speed_of_sound_m_s);
                assert_eq!(
                    atmosphere.dynamic_viscosity(),
                    values.dynamic_viscosity_pa_s
                );
                assert_eq!(
                    atmosphere.kinematic_viscosity(),
                    values.kinematic_viscosity_m2_s
                );
            }
        }
    }

    #[test]
    fn the_altitude_is_geopotential_and_the_geometric_entry_converts() {
        // 11 000 m geopotential is the tropopause of the layer table.
        let tropopause = Atmosphere::new(11_000.0);
        assert_eq!(tropopause, Atmosphere::from_geopotential(11_000.0));
        assert!((tropopause.temperature() - 216.65).abs() < 1e-9);
        let above_sea_level_m = tropopause.geometric_altitude_m();
        assert!(above_sea_level_m > 11_019.0 && above_sea_level_m < 11_019.1);
        // The same number read as a geometric altitude is 19 m lower in
        // geopotential terms, still in the troposphere and warmer.
        let geometric = Atmosphere::from_geometric(11_000.0);
        assert!((geometric.altitude_m - 10_981.0).abs() < 0.05);
        assert!(geometric.temperature() > 216.65);
        assert!(geometric.pressure() > tropopause.pressure());
        for altitude_m in [-1_500.0, 0.0, 3_000.0, 11_000.0, 20_000.0, 60_000.0] {
            let round_trip = geometric_from_geopotential(geopotential_from_geometric(altitude_m));
            assert!((round_trip - altitude_m).abs() < 1e-8);
        }
    }

    #[test]
    fn pressure_has_no_step_and_no_slope_break_at_the_tropopause() {
        // Central slopes just below and just above 11 km geopotential agree
        // to the size of the step, while the temperature slope changes from
        // -6.5 K/km to zero.
        let step_m = 1.0;
        let at = |h: f64| Atmosphere::from_geopotential(h);
        let below = (at(11_000.0 - step_m).pressure() - at(11_000.0 - 3.0 * step_m).pressure())
            / (2.0 * step_m);
        let above = (at(11_000.0 + 3.0 * step_m).pressure() - at(11_000.0 + step_m).pressure())
            / (2.0 * step_m);
        assert!(((below - above) / above).abs() < 2e-3, "{below} {above}");
        let jump = (at(11_000.0 + 1e-6).pressure() - at(11_000.0 - 1e-6).pressure()).abs();
        assert!(jump / 22_632.1 < 3e-6, "pressure step {jump} Pa");
        let lapse_below = (at(11_000.0 - step_m).temperature()
            - at(11_000.0 - 3.0 * step_m).temperature())
            / (2.0 * step_m);
        let lapse_above = (at(11_000.0 + 3.0 * step_m).temperature()
            - at(11_000.0 + step_m).temperature())
            / (2.0 * step_m);
        assert!((lapse_below + 0.0065).abs() < 1e-9);
        assert!(lapse_above.abs() < 1e-9);
    }

    #[test]
    fn temperature_deviation_shifts_temperature_and_density_but_not_pressure() {
        let base = Atmosphere::new(5000.0);
        let warmer = base.with_temperature_deviation(10.0);

        assert_eq!(warmer.pressure(), base.pressure());
        assert_eq!(warmer.temperature(), base.temperature() + 10.0);
        // A warmer parcel at the same pressure is less dense.
        assert!(warmer.density() < base.density());
    }

    #[test]
    fn density_matches_the_ideal_gas_law() {
        let atmo = Atmosphere::new(3000.0);
        let recovered_pressure = atmo.density() * GAS_CONSTANT_AIR * atmo.temperature();
        assert!((recovered_pressure - atmo.pressure()).abs() < 1e-9);
    }

    #[test]
    fn the_free_functions_are_the_same_model() {
        assert_eq!(pressure_isa(9_000.0), Atmosphere::new(9_000.0).pressure());
        assert_eq!(
            temperature_isa(9_000.0),
            Atmosphere::new(9_000.0).temperature()
        );
    }

    #[test]
    fn knudsen_scales_inversely_with_length() {
        let atmo = Atmosphere::new(20_000.0);
        let mean_free_path = atmo.mean_free_path();
        assert!((atmo.knudsen(1.0) - mean_free_path).abs() < 1e-20);
        assert!((atmo.knudsen(2.0) - mean_free_path / 2.0).abs() < 1e-20);
    }

    #[test]
    fn density_altitude_at_sea_level_is_zero() {
        let atmo = Atmosphere::new(0.0);
        let density_altitude_m = atmo
            .density_altitude(DensityAltitudeMethod::Approximate)
            .expect("the approximate method is implemented");
        assert!(density_altitude_m.abs() < 1e-9);
    }

    #[test]
    fn exact_density_altitude_reports_unsupported_rather_than_a_number() {
        let atmo = Atmosphere::new(1000.0);
        assert_eq!(
            atmo.density_altitude(DensityAltitudeMethod::Exact),
            Err(DensityAltitudeError::ExactNotImplemented)
        );
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
        // The band is 84 852 m of geopotential altitude, 86 000 m geometric.
        assert!(Atmosphere::try_new(84_852.0).is_ok());
        assert!(matches!(
            Atmosphere::try_new(84_853.0),
            Err(AtmosphereError::AltitudeOutsideModel { .. })
        ));
        assert!((geometric_from_geopotential(84_852.0) - 86_000.0).abs() < 0.5);
        assert!(matches!(
            Atmosphere::try_new(f64::INFINITY),
            Err(AtmosphereError::NonFiniteInput { .. })
        ));
    }
}
