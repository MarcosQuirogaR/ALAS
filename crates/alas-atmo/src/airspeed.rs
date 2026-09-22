// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Conversions between calibrated (CAS), equivalent (EAS) and true (TAS)
//! airspeed, and the Mach number that links them, for subsonic isentropic
//! pitot-static flow.
//!
//! Calibrated airspeed is defined by evaluating the compressible impact
//! -pressure relation at standard sea-level static pressure `p0` and speed of
//! sound `a0`:
//!
//! `qc / p0 = (1 + (gamma - 1)/2 * (Vc / a0)^2) ^ (gamma / (gamma - 1)) - 1`
//!
//! This is the isentropic total-to-static pressure ratio for subsonic flow
//! ahead of a pitot tube (no bow shock forms below Mach 1), applied to a
//! fictitious sea-level flight at `Vc`. The actual Mach number at the
//! aircraft's ambient static pressure `p` is recovered by inverting the same
//! relation with the impact pressure `qc` carried over unchanged and `p0`
//! replaced by `p`:
//!
//! `M = sqrt( 2/(gamma - 1) * ( (qc/p + 1)^((gamma - 1)/gamma) - 1 ) )`
//!
//! True airspeed follows from the local speed of sound, `TAS = M * a(T)`, and
//! equivalent airspeed from the dynamic-pressure equivalence
//! `EAS = TAS * sqrt(rho / rho0)`: a different, incompressible-referenced
//! quantity from CAS, not an alternate name for it.
//!
//! Primary source, read directly (not via a secondary reproduction): NASA
//! Glenn Research Center, "Isentropic Flow Equations",
//! <https://www.grc.nasa.gov/www/k-12/airplane/isentrop.html>, equation 2
//! (`a = sqrt(gamma R T)`) and equation 6, the isentropic total-to-static
//! pressure ratio for subsonic flow,
//! `p / pt = [1 + M^2 (gamma - 1)/2] ^ [-gamma / (gamma - 1)]`, which that
//! page states holds only ahead of a shock. Rearranged for pressure ratio
//! `pt/p` and written as an impact-pressure ratio `qc/p = pt/p - 1`, this is
//! `qc/p = [1 + (gamma-1)/2 * M^2]^(gamma/(gamma-1)) - 1` above. Defining
//! calibrated airspeed by evaluating that same NASA relation at standard
//! sea-level pressure and speed of sound (`p -> p0`, `M -> Vc/a0`), and then
//! reading the true Mach number back out of the resulting `qc` at the
//! aircraft's actual ambient pressure, is this module's own derivation built
//! on the cited NASA relation: NASA's page does not itself define, or use
//! the term, calibrated airspeed. This derivation and the resulting
//! `gamma = 1.4` numeric form are the standard one given in airspeed
//! textbooks (e.g. Clancy, L. J. (1975), *Aerodynamics*, SS3.12-3.13;
//! Houghton, E. L. and Carpenter, P. W. (1993), *Aerodynamics for
//! Engineering Students*, SS2.3.1); this module's derivation was checked
//! against the NASA primary source above, not against those textbooks
//! directly, and the module documentation below cites Wikipedia pages that
//! reproduce them only as a pointer to where their exact wording can be
//! read, not as evidence this implementation was checked against the
//! textbooks themselves.
//!
//! The equivalent-airspeed definition `EAS = TAS * sqrt(rho/rho0)` is the
//! standard incompressible dynamic-pressure equivalence (`0.5 rho0 EAS^2 =
//! 0.5 rho TAS^2`), consistent with NASA Glenn's equation 5
//! (`q = rho V^2 / 2`) applied once at the ambient state and once at the
//! sea-level reference state; it is also given, with the same textbook
//! citations as above, at
//! <https://en.wikipedia.org/wiki/Equivalent_airspeed>.
//!
//! `gamma = 1.4` matches [`crate::Atmosphere`]'s existing
//! `ratio_of_specific_heats()`, and every sea-level reference value below is
//! derived from [`crate::isa::GAS_CONSTANT_AIR`] and the same `p0`, `T0`
//! this crate's ISA model uses at zero altitude, rather than restated as
//! independently rounded literals, see [`sea_level_density_kg_m3`] for why
//! that matters for exact sea-level self-consistency.
//!
//! # Validity domain
//!
//! Subsonic only: an ambient state or calibrated speed that implies a Mach
//! number at or above 1 is rejected with [`AirspeedError::Supersonic`] rather
//! than evaluated, because a bow shock forms ahead of the pitot tube above
//! Mach 1 and the isentropic relation above no longer describes the flow (a
//! different, shock-corrected relation would be needed, and is not
//! implemented). Every input is checked finite, and every ambient pressure,
//! ambient temperature and input speed magnitude is checked positive (a
//! magnitude of exactly zero is the one accepted boundary: still air is a
//! valid, unremarkable input, and every conversion of it returns zero).
//! Nothing here clamps an out-of-domain input to the boundary and continues;
//! an invalid input is always a typed [`AirspeedError`], never a silently
//! repaired value.

/// Ratio of specific heats for air, held constant at 1.4.
///
/// Matches [`crate::Atmosphere::ratio_of_specific_heats`], which models no
/// temperature variation either; kept as this module's own constant rather
/// than requiring a caller to construct an [`crate::Atmosphere`] just to
/// read it, since every conversion here needs only an ambient pressure
/// and/or temperature value, not a full altitude model.
const AIR_GAMMA: f64 = 1.4;

/// Standard sea-level static pressure, in pascals (ICAO/ISA).
pub const SEA_LEVEL_PRESSURE_PA: f64 = 101_325.0;

/// Standard sea-level temperature, in kelvin (ICAO/ISA).
pub const SEA_LEVEL_TEMPERATURE_K: f64 = 288.15;

/// Commonly quoted rounded ICAO/ISA sea-level density, in kg/m^3, for
/// display and comparison only. **Not used by any conversion in this
/// module**: [`sea_level_density_kg_m3`] computes the value actually used,
/// from this module's own [`SEA_LEVEL_PRESSURE_PA`],
/// [`SEA_LEVEL_TEMPERATURE_K`] and [`crate::isa::GAS_CONSTANT_AIR`] via the
/// ideal gas law, which comes out to 1.224999..., not bit-identical to this
/// rounded literal. Using this rounded constant for the actual EAS/TAS
/// conversion instead would make `EAS != TAS` at the sea-level reference
/// state itself (a spurious ~1e-5 relative error at exactly `p0`, `T0`), so
/// the two are kept deliberately separate rather than one standing in for
/// the other.
pub const SEA_LEVEL_DENSITY_DISPLAY_KG_M3: f64 = 1.225;

/// Standard sea-level density, in kg/m^3: `p0 / (R * T0)`, from this
/// module's own [`SEA_LEVEL_PRESSURE_PA`], [`SEA_LEVEL_TEMPERATURE_K`] and
/// [`crate::isa::GAS_CONSTANT_AIR`], the values every EAS/TAS conversion
/// here actually uses, see [`SEA_LEVEL_DENSITY_DISPLAY_KG_M3`] for why this
/// is not simply the commonly quoted 1.225.
fn sea_level_density_kg_m3() -> f64 {
    SEA_LEVEL_PRESSURE_PA / (crate::isa::GAS_CONSTANT_AIR * SEA_LEVEL_TEMPERATURE_K)
}

/// Standard sea-level speed of sound, in m/s: `sqrt(gamma * R * T0)` at
/// [`SEA_LEVEL_TEMPERATURE_K`], using [`crate::isa::GAS_CONSTANT_AIR`].
fn sea_level_speed_of_sound_m_s() -> f64 {
    (AIR_GAMMA * crate::isa::GAS_CONSTANT_AIR * SEA_LEVEL_TEMPERATURE_K).sqrt()
}

/// Why an airspeed conversion was rejected.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AirspeedError {
    /// The input speed was negative or not finite. Airspeed magnitude is
    /// never negative; zero is the valid still-air boundary.
    InvalidSpeed(f64),
    /// The input Mach number was negative or not finite.
    InvalidMach(f64),
    /// Ambient static pressure was not finite or not strictly positive.
    InvalidPressure(f64),
    /// Ambient temperature was not finite or not strictly positive.
    InvalidTemperature(f64),
    /// The conversion implies Mach at or above 1, outside the subsonic
    /// isentropic pitot relation this module implements.
    Supersonic {
        /// The Mach number the inputs implied.
        mach: f64,
    },
}

impl std::fmt::Display for AirspeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSpeed(value) => {
                write!(f, "airspeed must be finite and non-negative, got {value}")
            }
            Self::InvalidMach(value) => {
                write!(f, "Mach number must be finite and non-negative, got {value}")
            }
            Self::InvalidPressure(value) => {
                write!(f, "ambient pressure must be finite and positive, got {value} Pa")
            }
            Self::InvalidTemperature(value) => {
                write!(
                    f,
                    "ambient temperature must be finite and positive, got {value} K"
                )
            }
            Self::Supersonic { mach } => write!(
                f,
                "inferred Mach {mach} is at or above 1; the subsonic isentropic pitot relation does not apply"
            ),
        }
    }
}

impl std::error::Error for AirspeedError {}

fn check_speed(speed_m_s: f64) -> Result<f64, AirspeedError> {
    if speed_m_s.is_finite() && speed_m_s >= 0.0 {
        Ok(speed_m_s)
    } else {
        Err(AirspeedError::InvalidSpeed(speed_m_s))
    }
}

fn check_mach(mach: f64) -> Result<f64, AirspeedError> {
    if mach.is_finite() && mach >= 0.0 {
        Ok(mach)
    } else {
        Err(AirspeedError::InvalidMach(mach))
    }
}

fn check_pressure(pressure_pa: f64) -> Result<f64, AirspeedError> {
    if pressure_pa.is_finite() && pressure_pa > 0.0 {
        Ok(pressure_pa)
    } else {
        Err(AirspeedError::InvalidPressure(pressure_pa))
    }
}

fn check_temperature(temperature_k: f64) -> Result<f64, AirspeedError> {
    if temperature_k.is_finite() && temperature_k > 0.0 {
        Ok(temperature_k)
    } else {
        Err(AirspeedError::InvalidTemperature(temperature_k))
    }
}

fn check_subsonic(mach: f64) -> Result<f64, AirspeedError> {
    if mach < 1.0 {
        Ok(mach)
    } else {
        Err(AirspeedError::Supersonic { mach })
    }
}

/// `gamma / (gamma - 1)`, the impact-pressure exponent.
const GAMMA_EXPONENT: f64 = AIR_GAMMA / (AIR_GAMMA - 1.0);
/// `(gamma - 1) / 2`, the impact-pressure Mach coefficient.
const GAMMA_COEFFICIENT: f64 = (AIR_GAMMA - 1.0) / 2.0;

/// Impact-pressure ratio `qc / p` at Mach `mach`, via
/// `expm1(GAMMA_EXPONENT * ln1p(GAMMA_COEFFICIENT * mach^2))` rather than
/// `(1 + GAMMA_COEFFICIENT * mach^2).powf(GAMMA_EXPONENT) - 1`, so a small
/// Mach number (the low-speed case this module exists to get right for a
/// turboprop climb) does not lose precision to cancellation between two
/// close-to-1 terms.
fn impact_pressure_ratio(mach: f64) -> f64 {
    (GAMMA_EXPONENT * (GAMMA_COEFFICIENT * mach * mach).ln_1p()).exp_m1()
}

/// Inverse of [`impact_pressure_ratio`]: the Mach number at impact-pressure
/// ratio `qc_over_p`, via the same `ln1p`/`expm1` pairing.
fn mach_from_impact_pressure_ratio(qc_over_p: f64) -> f64 {
    (((qc_over_p.ln_1p()) / GAMMA_EXPONENT).exp_m1() / GAMMA_COEFFICIENT).sqrt()
}

/// The Mach number a calibrated airspeed implies at ambient static pressure
/// `ambient_pressure_pa`.
///
/// # Errors
///
/// [`AirspeedError::InvalidSpeed`], [`AirspeedError::InvalidPressure`] or
/// [`AirspeedError::Supersonic`]; see the module documentation.
pub fn mach_from_calibrated(
    calibrated_m_s: f64,
    ambient_pressure_pa: f64,
) -> Result<f64, AirspeedError> {
    let cas = check_speed(calibrated_m_s)?;
    let ambient_pressure_pa = check_pressure(ambient_pressure_pa)?;
    let sea_level_mach_equivalent = cas / sea_level_speed_of_sound_m_s();
    let qc_over_p0 = impact_pressure_ratio(sea_level_mach_equivalent);
    let qc = qc_over_p0 * SEA_LEVEL_PRESSURE_PA;
    let mach = mach_from_impact_pressure_ratio(qc / ambient_pressure_pa);
    check_subsonic(mach)
}

/// True airspeed at Mach `mach` and ambient temperature `ambient_temperature_k`.
///
/// # Errors
///
/// [`AirspeedError::InvalidMach`], [`AirspeedError::InvalidTemperature`] or
/// [`AirspeedError::Supersonic`].
pub fn true_from_mach(mach: f64, ambient_temperature_k: f64) -> Result<f64, AirspeedError> {
    let mach = check_mach(mach)?;
    let mach = check_subsonic(mach)?;
    let ambient_temperature_k = check_temperature(ambient_temperature_k)?;
    let local_speed_of_sound_m_s =
        (AIR_GAMMA * crate::isa::GAS_CONSTANT_AIR * ambient_temperature_k).sqrt();
    Ok(mach * local_speed_of_sound_m_s)
}

/// True airspeed at ambient pressure `ambient_pressure_pa` and temperature
/// `ambient_temperature_k` for a given calibrated airspeed.
///
/// # Errors
///
/// See [`mach_from_calibrated`] and [`true_from_mach`].
pub fn true_from_calibrated(
    calibrated_m_s: f64,
    ambient_pressure_pa: f64,
    ambient_temperature_k: f64,
) -> Result<f64, AirspeedError> {
    let mach = mach_from_calibrated(calibrated_m_s, ambient_pressure_pa)?;
    true_from_mach(mach, ambient_temperature_k)
}

/// Calibrated airspeed at ambient pressure `ambient_pressure_pa` for a given
/// true airspeed at temperature `ambient_temperature_k`: the inverse of
/// [`true_from_calibrated`].
///
/// # Errors
///
/// [`AirspeedError::InvalidSpeed`], [`AirspeedError::InvalidPressure`],
/// [`AirspeedError::InvalidTemperature`] or [`AirspeedError::Supersonic`].
pub fn calibrated_from_true(
    true_m_s: f64,
    ambient_pressure_pa: f64,
    ambient_temperature_k: f64,
) -> Result<f64, AirspeedError> {
    let tas = check_speed(true_m_s)?;
    let ambient_pressure_pa = check_pressure(ambient_pressure_pa)?;
    let ambient_temperature_k = check_temperature(ambient_temperature_k)?;
    let local_speed_of_sound_m_s =
        (AIR_GAMMA * crate::isa::GAS_CONSTANT_AIR * ambient_temperature_k).sqrt();
    let mach = check_subsonic(tas / local_speed_of_sound_m_s)?;
    let qc_over_p = impact_pressure_ratio(mach);
    let qc = qc_over_p * ambient_pressure_pa;
    let sea_level_mach_equivalent = mach_from_impact_pressure_ratio(qc / SEA_LEVEL_PRESSURE_PA);
    Ok(sea_level_mach_equivalent * sea_level_speed_of_sound_m_s())
}

/// Equivalent airspeed for a given true airspeed at ambient pressure and
/// temperature: `EAS = TAS * sqrt(rho / rho0)`, the incompressible
/// dynamic-pressure equivalence, not calibrated airspeed under another
/// name.
///
/// # Errors
///
/// [`AirspeedError::InvalidSpeed`], [`AirspeedError::InvalidPressure`] or
/// [`AirspeedError::InvalidTemperature`].
pub fn equivalent_from_true(
    true_m_s: f64,
    ambient_pressure_pa: f64,
    ambient_temperature_k: f64,
) -> Result<f64, AirspeedError> {
    let tas = check_speed(true_m_s)?;
    let ambient_pressure_pa = check_pressure(ambient_pressure_pa)?;
    let ambient_temperature_k = check_temperature(ambient_temperature_k)?;
    let density_kg_m3 =
        ambient_pressure_pa / (crate::isa::GAS_CONSTANT_AIR * ambient_temperature_k);
    Ok(tas * (density_kg_m3 / sea_level_density_kg_m3()).sqrt())
}

/// True airspeed for a given equivalent airspeed: the inverse of
/// [`equivalent_from_true`].
///
/// # Errors
///
/// [`AirspeedError::InvalidSpeed`], [`AirspeedError::InvalidPressure`] or
/// [`AirspeedError::InvalidTemperature`].
pub fn true_from_equivalent(
    equivalent_m_s: f64,
    ambient_pressure_pa: f64,
    ambient_temperature_k: f64,
) -> Result<f64, AirspeedError> {
    let eas = check_speed(equivalent_m_s)?;
    let ambient_pressure_pa = check_pressure(ambient_pressure_pa)?;
    let ambient_temperature_k = check_temperature(ambient_temperature_k)?;
    let density_kg_m3 =
        ambient_pressure_pa / (crate::isa::GAS_CONSTANT_AIR * ambient_temperature_k);
    Ok(eas * (sea_level_density_kg_m3() / density_kg_m3).sqrt())
}

// Tests assert on `Result`s they just constructed, so a failed unwrap there
// is the assertion failing, not a panic escaping into a caller.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    const SEA_LEVEL_PRESSURE: f64 = SEA_LEVEL_PRESSURE_PA;
    const SEA_LEVEL_TEMPERATURE: f64 = SEA_LEVEL_TEMPERATURE_K;

    #[test]
    fn at_sea_level_calibrated_equivalent_and_true_airspeed_agree() {
        // At the reference state itself (p0, T0) the three speed definitions
        // collapse onto one number by construction: CAS is defined by
        // evaluating the impact-pressure relation at p0/a0, so recovering
        // Mach at ambient pressure p0 is the exact algebraic inverse of the
        // same relation (round-off only, no modeling approximation) and
        // TAS = CAS; EAS uses `sea_level_density_kg_m3()`, computed by the
        // same `p0/(R T0)` formula this test evaluates density at, so the
        // density ratio is exactly 1.0 in floating point, not merely close
        // to it; these tolerances are round-off-tied, not loosened to
        // paper over the 1.225-vs-1.224999 rounding gap documented on
        // `SEA_LEVEL_DENSITY_DISPLAY_KG_M3`.
        let cas = 90.0_f64;
        let tas = true_from_calibrated(cas, SEA_LEVEL_PRESSURE, SEA_LEVEL_TEMPERATURE).unwrap();
        let eas = equivalent_from_true(tas, SEA_LEVEL_PRESSURE, SEA_LEVEL_TEMPERATURE).unwrap();
        assert!(
            (tas - cas).abs() < 1e-9 * cas,
            "tas={tas} cas={cas} relative_error={}",
            (tas - cas).abs() / cas
        );
        assert_eq!(eas, tas, "eas={eas} tas={tas}");
    }

    #[test]
    fn zero_speed_is_zero_everywhere() {
        assert_eq!(mach_from_calibrated(0.0, SEA_LEVEL_PRESSURE).unwrap(), 0.0);
        assert_eq!(true_from_mach(0.0, SEA_LEVEL_TEMPERATURE).unwrap(), 0.0);
        assert_eq!(true_from_calibrated(0.0, 50_000.0, 250.0).unwrap(), 0.0);
        assert_eq!(calibrated_from_true(0.0, 50_000.0, 250.0).unwrap(), 0.0);
        assert_eq!(equivalent_from_true(0.0, 50_000.0, 250.0).unwrap(), 0.0);
        assert_eq!(true_from_equivalent(0.0, 50_000.0, 250.0).unwrap(), 0.0);
    }

    #[test]
    fn invalid_inputs_are_rejected_not_repaired() {
        assert_eq!(
            mach_from_calibrated(-1.0, SEA_LEVEL_PRESSURE),
            Err(AirspeedError::InvalidSpeed(-1.0))
        );
        assert!(matches!(
            mach_from_calibrated(f64::NAN, SEA_LEVEL_PRESSURE),
            Err(AirspeedError::InvalidSpeed(value)) if value.is_nan()
        ));
        assert_eq!(
            mach_from_calibrated(f64::INFINITY, SEA_LEVEL_PRESSURE),
            Err(AirspeedError::InvalidSpeed(f64::INFINITY))
        );
        assert_eq!(
            mach_from_calibrated(90.0, 0.0),
            Err(AirspeedError::InvalidPressure(0.0))
        );
        assert_eq!(
            mach_from_calibrated(90.0, -50_000.0),
            Err(AirspeedError::InvalidPressure(-50_000.0))
        );
        assert!(matches!(
            mach_from_calibrated(90.0, f64::NAN),
            Err(AirspeedError::InvalidPressure(value)) if value.is_nan()
        ));
        assert_eq!(
            true_from_mach(0.5, 0.0),
            Err(AirspeedError::InvalidTemperature(0.0))
        );
        assert_eq!(
            true_from_mach(0.5, f64::NEG_INFINITY),
            Err(AirspeedError::InvalidTemperature(f64::NEG_INFINITY))
        );
        assert_eq!(
            true_from_mach(-0.1, SEA_LEVEL_TEMPERATURE),
            Err(AirspeedError::InvalidMach(-0.1))
        );
    }

    #[test]
    fn a_supersonic_inferred_mach_is_rejected_not_clamped() {
        // 700 kt CAS at 12,000 m ambient pressure (~19.3 kPa) implies a true
        // Mach above 1: the aircraft's ambient pressure is far below the
        // sea-level pressure the impact pressure was collected at, so the
        // same impact pressure corresponds to a much higher Mach number
        // away from sea level.
        let cas_m_s = 700.0 * 0.514_444_444;
        let low_pressure_pa = 19_330.0;
        match mach_from_calibrated(cas_m_s, low_pressure_pa) {
            Err(AirspeedError::Supersonic { mach }) => assert!(mach >= 1.0),
            other => panic!("expected Supersonic, got {other:?}"),
        }
    }

    #[test]
    fn calibrated_and_equivalent_airspeed_diverge_at_altitude() {
        // A transport-category cruise point: FL350 (10,668 m), ISA. At this
        // altitude and Mach the compressible correction (CAS -> TAS) and the
        // density correction (TAS -> EAS) are each several percent, and CAS
        // and EAS are not the same quantity: this asserts they visibly
        // differ, not just that neither equals TAS.
        let pressure_pa = 23_842.0; // ISA pressure at 10,668 m (~35,000 ft).
        let temperature_k = 218.81; // ISA temperature at 10,668 m.
        let cas_m_s = 250.0 * 0.514_444_444; // 250 kt CAS.
        let tas = true_from_calibrated(cas_m_s, pressure_pa, temperature_k).unwrap();
        let eas = equivalent_from_true(tas, pressure_pa, temperature_k).unwrap();
        let cas_eas_difference_fraction = (cas_m_s - eas).abs() / cas_m_s;
        let tas_cas_ratio = tas / cas_m_s;
        // TAS is well above CAS at this altitude (low density, compressible
        // correction), and EAS visibly differs from CAS; they are not
        // interchangeable, even though both start from an incompressible
        // dynamic-pressure idea.
        assert!(tas_cas_ratio > 1.5, "tas/cas={tas_cas_ratio}");
        assert!(
            cas_eas_difference_fraction > 0.01,
            "cas={cas_m_s} eas={eas} diff_fraction={cas_eas_difference_fraction}"
        );
    }

    #[test]
    fn calibrated_to_true_and_back_round_trips() {
        for cas_kt in [0.0, 50.0, 116.0, 170.0, 250.0, 275.0] {
            for (pressure_pa, temperature_k) in [
                (SEA_LEVEL_PRESSURE, SEA_LEVEL_TEMPERATURE),
                (81_494.0, 268.66), // ISA at 1,829 m (~6,000 ft).
                (54_020.0, 249.19), // ISA at 5,182 m (~17,000 ft, ATR cruise).
            ] {
                let cas_m_s = cas_kt * 0.514_444_444;
                let tas = true_from_calibrated(cas_m_s, pressure_pa, temperature_k)
                    .unwrap_or_else(|error| panic!("{cas_kt} kt at {pressure_pa} Pa: {error}"));
                let round_tripped = calibrated_from_true(tas, pressure_pa, temperature_k)
                    .unwrap_or_else(|error| panic!("{cas_kt} kt at {pressure_pa} Pa: {error}"));
                assert!(
                    (round_tripped - cas_m_s).abs() < 1e-6 * cas_m_s.max(1.0),
                    "cas={cas_m_s} round_tripped={round_tripped} at p={pressure_pa} T={temperature_k}"
                );
            }
        }
    }

    #[test]
    fn true_to_equivalent_and_back_round_trips() {
        let tas_m_s = 141.5;
        let pressure_pa = 54_020.0;
        let temperature_k = 249.19;
        let eas = equivalent_from_true(tas_m_s, pressure_pa, temperature_k).unwrap();
        let round_tripped = true_from_equivalent(eas, pressure_pa, temperature_k).unwrap();
        assert!((round_tripped - tas_m_s).abs() < 1e-9);
    }

    /// Independent reference point, not produced by calling this module's
    /// own helpers backward: 250 kt CAS at 10,668 m (FL350) ISA. Computed by
    /// hand from the same textbook relation this module implements (Clancy
    /// 1975; Houghton & Carpenter 1993; see the module documentation),
    /// independently of this implementation's `ln1p`/`expm1` code path, using
    /// plain `powf` arithmetic and this crate's own `GAS_CONSTANT_AIR`
    /// (287.053, the pre-1986 CODATA value `8.31432/28.9644e-3` this crate
    /// deliberately reproduces; see `crate::isa`):
    /// `a0 = sqrt(1.4*287.053*288.15) = 340.294 m/s`;
    /// `M0 = (250*0.514444)/340.294 = 0.37794`;
    /// `qc/p0 = (1+0.2*0.37794^2)^3.5 - 1 = 0.103609`;
    /// `qc = 0.103609*101325 = 10498.2 Pa`;
    /// `qc/p = 10498.2/23842 = 0.440324`;
    /// `M = sqrt(5*((1+0.440324)^(1/3.5) - 1)) = 0.741201`;
    /// `a(218.81 K) = sqrt(1.4*287.053*218.81) = 296.537 m/s`;
    /// `TAS = 0.741201*296.537 = 219.793 m/s` (427.24 kt). Re-deriving with
    /// the modern `R = 287.058 J/(kg K)` instead changes this by about
    /// 4e-5 (a fifth significant figure), so the tolerance below is set to
    /// round-off on this hand computation, not to a gas-constant spread.
    #[test]
    fn matches_an_independently_hand_computed_reference_point() {
        let pressure_pa = 23_842.0;
        let temperature_k = 218.81;
        let cas_m_s = 250.0 * 0.514_444_444;
        let tas = true_from_calibrated(cas_m_s, pressure_pa, temperature_k).unwrap();
        let reference_tas_m_s = 219.793;
        let relative_error = (tas - reference_tas_m_s).abs() / reference_tas_m_s;
        assert!(
            relative_error < 1e-4,
            "tas={tas} reference={reference_tas_m_s} relative_error={relative_error}"
        );
    }
}
