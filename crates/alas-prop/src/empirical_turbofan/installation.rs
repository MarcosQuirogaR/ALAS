// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Installed-versus-uninstalled thrust-specific fuel consumption: customer
//! bleed and shaft-power extraction, and the part-power shape of TSFC.
//!
//! Manufacturer cruise SFC (the catalogue anchors) is an uninstalled value:
//! "the manufacturer's engine data include assumptions as to inlet efficiency
//! and distortion, engine bleed, engine power extraction, nozzle performance,
//! and other factors. These are usually optimistic, and in some cases the
//! losses are assumed to be zero" (Raymer, *Aircraft Design: A Conceptual
//! Approach*, 6th ed., Sec. 13.2, PDF p. 505 / book p. 471). Jenkinson,
//! *Civil Jet Aircraft Design*, defines installed engine data as "the engine
//! performance after allowance has been made for: (1) air and power offtakes
//! and (2) intake losses" (PDF p. 269). The payload-range charts the model is
//! checked against print the same convention ("normal power extraction and
//! air conditioning bleed", Boeing D6-58333 Rev Q Sec. 3.2.2, PDF p. 38).
//!
//! # Bleed
//!
//! The SFC increment is proportional to the bleed mass flow over the core
//! (gas-generator) mass flow. Torenbeek, *Synthesis of Subsonic Airplane
//! Design* (1982), Fig. 4-33 (book p. 127, PDF p. 147; M 0.8 at 30,000 ft,
//! `T_t4` 1450 K, OPR 24, BPR 5), read on the page: a straight line through
//! the origin with 8.7 % at 4.4 % bleed for a port behind the high-pressure
//! compressor (slope 1.98) and 4.6 % at 4.45 % bleed for the optimum port
//! (slope 1.03). Raymer Eq. (13.8), book p. 475: percent thrust loss =
//! `C_bleed` x percent bleed of the engine mass flow, "for initial analysis
//! `C_bleed` can be approximated as 2.0" (the high-pressure-port value, in
//! agreement with Torenbeek's 1.98); bleed "typically ranges from 1-5 % of
//! the engine mass flow".
//!
//! # Power extraction
//!
//! Raymer, book p. 475-476: extraction "is typically less than 200 hp
//! {150 kW} for a 30,000-lb-thrust {133-kN} engine ... if the power required
//! to drive the compressor is known, the SFC increase and thrust loss are
//! both slightly less than the percent of power that is extracted". The
//! compressor power is the core mass flow times `c_p` times the compressor
//! temperature rise of the cycle the deck carries.
//!
//! # Part-power shape
//!
//! The ICAO LTO fuel-flow ratios are measured at sea-level static only. They
//! say that TSFC at 85 % and 30 % thrust is 3 % and 8 % below the take-off
//! value there, and 28 % above it at 7 %. Nothing in the ICAO data or in
//! the handbooks carries that dip to cruise altitude and Mach (a real engine
//! has a shallow TSFC minimum near 80-90 % of its cruise rating and rises
//! below it). [`tsfc_shape`] therefore keeps the sourced part-power RISE at
//! every condition, and the sourced part-power DIP only at sea level, fading
//! it out linearly to none at the altitude of the cruise anchor.

use super::*;

/// Specific heat of air used for the compressor power estimate, J/(kg K).
const CP_AIR_J_KG_K: f64 = 1_005.0;
/// Standard sea-level pressure and temperature of the corrected-flow scaling.
const SEA_LEVEL_PRESSURE_PA: f64 = 101_325.0;
const SEA_LEVEL_TEMPERATURE_K: f64 = 288.15;
/// Thrust class (133.4 kN = 30,000 lbf) at which Raymer quotes 150 kW.
const RAYMER_THRUST_CLASS_N: f64 = 133_447.0;
/// Raymer's shaft power for that class, W (200 hp, printed {150 kW}).
const RAYMER_EXTRACTION_W: f64 = 150_000.0;

/// Altitude over which the standing service loads ramp from none (the ICAO LTO
/// ground rows, measured without bleed or extraction) to full, m. ASSUMPTION:
/// about 3,300 ft, the first part of the climb where the packs are on.
pub const STANDING_LOADS_FULL_ALTITUDE_M: f64 = 1_000.0;
/// Fractional SFC increase per unit bleed fraction of core flow, optimum
/// (intermediate) port at cruise: Torenbeek 1982 Fig. 4-33, PDF p. 147. The
/// port behind the high-pressure compressor reads 1.98 there and Raymer
/// Eq. (13.8) gives `C_bleed` = 2.0 (the conservative alternative).
pub const BLEED_SFC_SLOPE_OPTIMUM_PORT: f64 = 1.03;
/// Customer bleed of a conventional transport as a fraction of core mass
/// flow, an ESTIMATE inside Raymer's 1-5 % range: 14 CFR 25.831(a) requires
/// 0.55 lb/min (0.00417 kg/s) of fresh air per occupant, which is 0.77 kg/s
/// for the 186 occupants of an A320 (0.39 kg/s per engine) and 2.3 kg/s for
/// the 555 of an A380 (0.58 kg/s per engine); against a cruise core flow of
/// about 20 kg/s (CFM56-5B) and 25 kg/s (Trent 900) that is 2 %.
pub const TRANSPORT_BLEED_CORE_FRACTION: f64 = 0.02;

/// Aircraft-service losses of the deck: standing loads charged at every
/// flight condition, per engine, and the SFC coefficients that turn them
/// into extra fuel at the same net thrust.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InstallationLosses {
    /// Customer bleed of each engine as a fraction of its core mass flow.
    pub bleed_core_flow_fraction: f64,
    /// Fractional SFC increase per unit bleed fraction of core flow.
    pub bleed_sfc_slope: f64,
    /// Shaft power extracted from each engine (accessories and generators), W.
    pub shaft_power_per_engine_w: f64,
    /// Fractional SFC increase per unit fraction of compressor power
    /// extracted (Raymer: "slightly less than" 1).
    pub power_sfc_slope: f64,
}

impl InstallationLosses {
    /// No standing loads: the uninstalled deck.
    pub const NONE: Self = Self {
        bleed_core_flow_fraction: 0.0,
        bleed_sfc_slope: BLEED_SFC_SLOPE_OPTIMUM_PORT,
        shaft_power_per_engine_w: 0.0,
        power_sfc_slope: 1.0,
    };

    /// Standing loads of a conventional transport: the bleed of
    /// [`TRANSPORT_BLEED_CORE_FRACTION`] at the optimum-port slope, and
    /// Raymer's 150 kW per 133.4 kN of rated thrust per engine.
    pub fn transport(rated_thrust_per_engine_n: f64) -> Self {
        Self {
            bleed_core_flow_fraction: TRANSPORT_BLEED_CORE_FRACTION,
            bleed_sfc_slope: BLEED_SFC_SLOPE_OPTIMUM_PORT,
            shaft_power_per_engine_w: RAYMER_EXTRACTION_W * rated_thrust_per_engine_n
                / RAYMER_THRUST_CLASS_N,
            power_sfc_slope: 1.0,
        }
    }

    /// Whether every term is finite and non-negative.
    pub(super) fn is_valid(&self) -> bool {
        [
            self.bleed_core_flow_fraction,
            self.bleed_sfc_slope,
            self.shaft_power_per_engine_w,
            self.power_sfc_slope,
        ]
        .iter()
        .all(|value| value.is_finite() && *value >= 0.0)
            && self.bleed_core_flow_fraction < 0.5
    }
}

/// What the deck needs of the engine cycle to scale loads to flight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct CoreFlowBasis {
    /// Core mass flow of one engine at sea-level static, kg/s.
    pub static_core_flow_kg_s: f64,
    /// Number of engines.
    pub engines: f64,
    /// `T_t3 / T_t2` of the core compressors.
    pub compressor_temperature_ratio: f64,
}

impl CoreFlowBasis {
    pub(super) fn new(fuel_model: &LegacyTurbofanModel) -> Result<Self, PropulsionError> {
        Ok(Self {
            static_core_flow_kg_s: fuel_model.static_core_mass_flow_kg_s()?,
            engines: fuel_model.engine_count(),
            compressor_temperature_ratio: fuel_model.compressor_temperature_ratio(),
        })
    }
}

impl EmpiricalTurbofanModel {
    /// Ratio of installed to uninstalled TSFC at `flight` under the standing
    /// loads plus the request's `extra` loads (system totals): bleed over core
    /// flow and extracted shaft power over compressor power, each times its
    /// SFC slope.
    ///
    /// Electrical power is counted as shaft power (generator efficiency 1, an
    /// upper bound of the shaft demand). The core mass flow scales with
    /// corrected flow `(p_t / p_ref) / sqrt(T_t / T_ref)` from its
    /// sea-level-static value.
    pub(super) fn installation_sfc_factor(
        &self,
        flight: FlightCondition,
        extra: PropulsionLoads,
    ) -> f64 {
        let losses = self.deck.installation;
        let basis = self.core_flow;
        let (total_pressure_pa, total_temperature_k) = total_conditions(flight);
        let corrected = (total_pressure_pa / SEA_LEVEL_PRESSURE_PA)
            / (total_temperature_k / SEA_LEVEL_TEMPERATURE_K).sqrt();
        let core_flow_kg_s = basis.static_core_flow_kg_s * corrected * basis.engines;
        let compressor_power_w = core_flow_kg_s
            * CP_AIR_J_KG_K
            * total_temperature_k
            * (basis.compressor_temperature_ratio - 1.0);
        if !(core_flow_kg_s > 0.0 && compressor_power_w > 0.0) {
            return 1.0;
        }
        // The standing loads come on over the first `STANDING_LOADS_FULL_ALTITUDE_M`:
        // the ICAO LTO ground rows (taxi, take-off) are certified without them.
        let on = (flight.altitude_m / STANDING_LOADS_FULL_ALTITUDE_M).clamp(0.0, 1.0);
        let bleed_kg_s =
            on * losses.bleed_core_flow_fraction * core_flow_kg_s + extra.bleed_mass_flow_kg_s;
        let power_w = on * losses.shaft_power_per_engine_w * basis.engines
            + extra.accessory_power_w
            + extra.electrical_power_w;
        1.0 + losses.bleed_sfc_slope * bleed_kg_s / core_flow_kg_s
            + losses.power_sfc_slope * power_w / compressor_power_w
    }
}

/// Freestream stagnation pressure (Pa) and temperature (K) of `flight` from
/// its static state and Mach number, isentropic air with `gamma` = 1.4.
pub(super) fn total_conditions(flight: FlightCondition) -> (f64, f64) {
    let ram = 1.0 + 0.2 * flight.mach * flight.mach;
    (
        flight.pressure_pa * ram.powf(3.5),
        flight.temperature_k * ram,
    )
}

/// The request's `loads`, refused unless finite and non-negative.
pub(super) fn checked_loads(loads: PropulsionLoads) -> Result<PropulsionLoads, PropulsionError> {
    for (field, value) in [
        ("accessory power", loads.accessory_power_w),
        ("bleed mass flow", loads.bleed_mass_flow_kg_s),
        ("electrical power", loads.electrical_power_w),
    ] {
        if !(value.is_finite() && value >= 0.0) {
            return Err(PropulsionError::InvalidInput { field, value });
        }
    }
    Ok(loads)
}

/// Multiplier of the full-rating TSFC at rating utilization `u` (fraction of
/// the active rating, in `[0.07, 1]`) at `altitude_m`, relative to the TSFC
/// the anchors define (module documentation, "Part-power shape").
///
/// `r(u) = f(u) / u` is the ICAO LTO fuel-flow ratio over the thrust ratio.
/// Where `r > 1` (low power) it applies at every altitude; where `r < 1` the
/// sea-level-static dip applies with weight `1 - h / h_anchor` (sea level to
/// the altitude of the cruise anchor), so the anchor altitude carries none.
pub(super) fn tsfc_shape(
    utilization: f64,
    altitude_m: f64,
    anchor_altitude_m: f64,
    ratios: [f64; 4],
) -> f64 {
    let u = utilization.clamp(0.07, 1.0);
    let ratio = part_power_fuel_fraction(u, ratios) / u;
    if ratio >= 1.0 {
        ratio
    } else {
        let weight = (1.0 - altitude_m / anchor_altitude_m).clamp(0.0, 1.0);
        1.0 + weight * (ratio - 1.0)
    }
}

// A test asserts on values it constructed here directly.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    const ICAO_CFM56: [f64; 4] = [0.089_316_99, 0.276_707_53, 0.822_241_68, 1.0];

    #[test]
    fn shape_keeps_the_low_power_rise_everywhere_and_the_dip_only_near_sea_level() {
        for altitude_m in [0.0, 5_000.0, 10_668.0, 12_000.0] {
            let idle = tsfc_shape(0.07, altitude_m, 10_668.0, ICAO_CFM56);
            assert!((idle - 0.089_316_99 / 0.07).abs() < 1.0e-12, "{idle}");
        }
        let sea_level = tsfc_shape(0.85, 0.0, 10_668.0, ICAO_CFM56);
        assert!((sea_level - 0.822_241_68 / 0.85).abs() < 1.0e-12);
        assert!(sea_level < 1.0);
        for altitude_m in [10_668.0, 11_500.0] {
            assert_eq!(tsfc_shape(0.85, altitude_m, 10_668.0, ICAO_CFM56), 1.0);
            assert_eq!(tsfc_shape(0.60, altitude_m, 10_668.0, ICAO_CFM56), 1.0);
        }
        let half = tsfc_shape(0.85, 5_334.0, 10_668.0, ICAO_CFM56);
        assert!((half - (1.0 + 0.5 * (0.822_241_68 / 0.85 - 1.0))).abs() < 1.0e-12);
        assert_eq!(tsfc_shape(1.0, 0.0, 10_668.0, ICAO_CFM56), 1.0);
    }

    #[test]
    fn the_transport_losses_follow_the_printed_coefficients() {
        let losses = InstallationLosses::transport(133_447.0);
        assert!((losses.shaft_power_per_engine_w - 150_000.0).abs() < 1.0e-9);
        assert_eq!(losses.bleed_core_flow_fraction, 0.02);
        assert_eq!(losses.bleed_sfc_slope, BLEED_SFC_SLOPE_OPTIMUM_PORT);
        assert!(losses.is_valid());
        assert!(InstallationLosses::NONE.is_valid());
        let mut bad = losses;
        bad.shaft_power_per_engine_w = f64::NAN;
        assert!(!bad.is_valid());
    }
}
