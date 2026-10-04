// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shaft-power availability and fuel consumption of the PW127M class.
//!
//! Two class relations replace the former single-point calibration:
//!
//! * **Available power** is flat-rated at sea level to the EASA TCDS IM.E.041
//!   temperatures and lapses as `(rho / rho_corner)^n` above the corner.
//! * **PSFC** is flat in power setting and scales with ambient temperature as
//!   `sqrt(T / T_ref)` (Majeed 2009, SRS-TSD-002 Rev. 1).

use super::*;

/// ISA sea-level static temperature, K.
const ISA_SEA_LEVEL_TEMPERATURE_K: f64 = 288.15;
/// ISA tropopause temperature, K (11 km).
const ISA_TROPOPAUSE_TEMPERATURE_K: f64 = 216.65;
/// ISA tropospheric density-temperature exponent, `g / (L R) - 1`, with
/// `g = 9.80665 m/s^2`, `L = 0.0065 K/m`, `R = 287.053 J/(kg K)` (ICAO Doc
/// 7488 / US Standard Atmosphere 1976): `rho / rho_0 = (T / T_0)^4.25588`.
const ISA_TROPOSPHERE_DENSITY_EXPONENT: f64 = 4.255_88;

/// The ISA static temperature at which the standard atmosphere has
/// `density_kg_m3`, K, for a sea-level reference density `reference_kg_m3`.
///
/// Used only when a caller supplies a density without a temperature
/// ([`Pw127m568fModel::evaluate`]): the fuel flow then follows the ISA day at
/// that density. Above the tropopause density it holds 216.65 K.
pub(super) fn isa_temperature_from_density_k(density_kg_m3: f64, reference_kg_m3: f64) -> f64 {
    let ratio = density_kg_m3 / reference_kg_m3;
    let tropopause_ratio = (ISA_TROPOPAUSE_TEMPERATURE_K / ISA_SEA_LEVEL_TEMPERATURE_K)
        .powf(ISA_TROPOSPHERE_DENSITY_EXPONENT);
    if ratio <= tropopause_ratio {
        ISA_TROPOPAUSE_TEMPERATURE_K
    } else {
        ISA_SEA_LEVEL_TEMPERATURE_K * ratio.powf(1.0 / ISA_TROPOSPHERE_DENSITY_EXPONENT)
    }
}

impl Pw127m568fModel {
    /// The ambient density at which `rating` stops being flat-rated, kg/m^3.
    ///
    /// The TCDS gives the flat rating only at sea level, as a temperature:
    /// take-off power to 39 C and maximum-continuous power to 48 C (EASA TCDS
    /// IM.E.041 section 5, PW127M). At that corner the temperature-limited
    /// power equals the flat rating. Taking the temperature-limited power to
    /// follow the same sourced density lapse `rho^n` wherever the density
    /// comes from, hot day or altitude, the corner is the density of
    /// sea-level pressure at the flat-rating temperature,
    /// `rho_corner = rho_SL,ISA x 288.15 / T_flat`: 1.131 kg/m^3 for take-off
    /// (ISA about 810 m) and 1.099 kg/m^3 for maximum continuous (about
    /// 1,110 m). This is a derivation on sourced data, not a TCDS altitude.
    ///
    /// The climb and cruise ratings are installation limits with no published
    /// flat-rating temperature. They take the maximum-continuous corner, on
    /// this evidence: the PW120 maximum-cruise chart fit
    /// `P_CR = 0.9 P_TO sigma^0.728` (Nita 2008 eq. 3.5.11, from McCormick
    /// 1995) lies above the PW120A's own sea-level cruise/take-off ratio of
    /// 1,651/2,000 = 0.83 (Majeed 2009 section 2), so the cruise rating is
    /// flat up to `sigma = (0.83/0.9)^(1/0.728) = 0.89`, the same corner as the
    /// PW127M's maximum-continuous `sigma = 0.897` to within 1 %. The flight
    /// idle surrogate is a fraction of maximum-continuous power and follows
    /// it.
    #[must_use]
    pub fn flat_rating_corner_density_kg_m3(self, rating: Pw127mRating) -> f64 {
        let flat_rating_temperature_k = match rating {
            Pw127mRating::NormalTakeoff | Pw127mRating::MaximumTakeoffReserve => {
                self.takeoff_flat_rating_temperature_k
            }
            Pw127mRating::MaximumContinuous
            | Pw127mRating::MaximumClimb
            | Pw127mRating::MaximumCruise
            | Pw127mRating::FlightIdleSurrogate => {
                self.maximum_continuous_flat_rating_temperature_k
            }
        };
        self.power_lapse_reference_density_kg_m3 * ISA_SEA_LEVEL_TEMPERATURE_K
            / flat_rating_temperature_k
    }

    /// The share of `rating`'s sea-level power available at an ambient
    /// density: 1 down to the flat-rating corner, `(rho / rho_corner)^n`
    /// below it, never under the declared floor.
    ///
    /// The exponent is [`Self::power_lapse_density_exponent`] (Nita 2008 eq.
    /// 3.5.11). The density alone carries the ambient temperature, because at
    /// a given pressure a hotter day is a thinner one; a separate temperature
    /// derating slope above the flat rating is not sourced for this engine.
    #[must_use]
    pub fn power_lapse_fraction(self, rating: Pw127mRating, density_kg_m3: f64) -> f64 {
        let ratio = (density_kg_m3 / self.flat_rating_corner_density_kg_m3(rating)).min(1.0);
        ratio
            .powf(self.power_lapse_density_exponent)
            .max(self.minimum_power_lapse_fraction)
    }

    /// Power-specific fuel consumption at an ambient static temperature,
    /// kg/(kW h), on free-turbine shaft power.
    ///
    /// `PSFC = PSFC_ref sqrt(T / T_ref)`: Majeed (2009) SRS-TSD-002 Tab. 4.2
    /// and 4.3 (PW120A cycle model at maximum-cruise setting, 13,000-25,000 ft
    /// and ISA-20 to ISA+20) give ESFC proportional to ambient temperature to
    /// the power 0.41-0.64, and 0.5 is the class-level value, the same as
    /// Eshelby's turbofan `theta^0.5`.
    ///
    /// **Flat in power setting.** Majeed's cycle model gives ESFC flat to
    /// 0.3 % from 46 % to 60 % of take-off power at 25,000 ft (Tab. 4.1), and
    /// the flight data are at 62 % and 100 %. Below about 46 % no citable
    /// penalty coefficient was found (Saravanamuttoo Fig. 9.11 and a PW127
    /// deck were not retrieved), so the model **assumes** the same PSFC there.
    /// A real turboprop's PSFC rises as power falls, so descent, idle and
    /// hold fuel are a lower bound.
    #[must_use]
    pub fn psfc_kg_kwh(self, ambient_temperature_k: f64) -> f64 {
        self.psfc_reference_kg_kwh
            * (ambient_temperature_k / self.psfc_reference_temperature_k).sqrt()
    }

    /// The shaft power this rating actually delivers at an ambient density, W.
    ///
    /// The rating is a sea-level certificated number; this applies the flat
    /// rating and the density lapse of [`Self::power_lapse_fraction`].
    /// Exposed because a consumer sizing a field length or a climb gradient
    /// needs the available power, and back-computing it from a thrust and an
    /// efficiency would invert a surrogate.
    #[must_use]
    pub fn available_shaft_power_w(self, rating: Pw127mRating, density_kg_m3: f64) -> f64 {
        self.rated_shaft_power_w(rating) * self.power_lapse_fraction(rating, density_kg_m3)
    }

    /// Evaluate one engine and propeller on the ISA day at the condition's
    /// density.
    ///
    /// Shaft power and thrust depend on density alone; only the fuel flow
    /// needs a temperature, and this entry point takes the ISA temperature at
    /// that density. A caller that knows the ambient temperature should use
    /// [`Self::evaluate_at_temperature`]. No limit is silently clipped.
    pub fn evaluate(
        self,
        condition: TurbopropCondition,
        command: TurbopropCommand,
    ) -> Result<TurbopropOutput, TurbopropError> {
        let temperature_k = isa_temperature_from_density_k(
            condition.density_kg_m3,
            self.power_lapse_reference_density_kg_m3,
        );
        self.evaluate_at_temperature(condition, temperature_k, command)
    }
}
