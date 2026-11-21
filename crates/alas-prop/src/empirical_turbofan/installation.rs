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
//! value there, and 28 % above it at 7 %. That dip is a static property: in
//! flight the inlet ram drag takes a growing share of the gross thrust as the
//! engine is throttled back, and Jenkinson's SFC loops (Figs. 9.19(e)-9.21(e),
//! [super::sfc_loops]) show TSFC rising below the bucket at every flight Mach
//! number (BPR 8.0, M 0.25: +21 % from the bucket at `FN/(delta FN*)` 0.55 to
//! 0.20). [`tsfc_shape`] therefore applies the ICAO shape at static
//! conditions, the flight loops from Mach 0.25 up, and blends the two
//! linearly in Mach number between. The earlier form faded the static dip
//! out with altitude instead of Mach number, so a 30 min hold at 1,500 ft and
//! Mach 0.35 (about 15 % of `delta FN*`) was priced 5-14 % BELOW the
//! full-rating TSFC where the loops put it 20-45 % above.

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
/// Rated thrust lost per unit customer bleed over core mass flow: Raymer
/// Eq. (13.8), book p. 475, `C_bleed` "approximated as 2.0" (module
/// documentation, "Bleed"). Charged on the take-off rating only, where a
/// stated condition (packs on) asks for it; at the other ratings bleed costs
/// fuel at the same thrust (the SFC slope above).
pub const BLEED_THRUST_LOSS_SLOPE: f64 = 2.0;
/// Fresh air the packs deliver per occupant, kg/s: 14 CFR 25.831(a),
/// 0.55 lb/min.
pub const FRESH_AIR_PER_OCCUPANT_KG_S: f64 = 0.55 * 0.453_592_37 / 60.0;

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

impl EmpiricalTurbofanModel {
    /// `thrust_n` less the take-off-rating thrust the request's customer bleed
    /// takes: `1 - BLEED_THRUST_LOSS_SLOPE x bleed / core flow`, the core flow
    /// scaled with corrected flow as in `installation_sfc_factor`. Other
    /// demands, and a request without bleed, keep `thrust_n`.
    pub(super) fn takeoff_bleed_thrust_n(&self, request: &PropulsionRequest, thrust_n: f64) -> f64 {
        let takeoff = matches!(
            request.demand,
            PropulsionDemand::Rating(PropulsionRating::TakeoffGoAround)
                | PropulsionDemand::RatedFraction {
                    rating: PropulsionRating::TakeoffGoAround,
                    ..
                }
        );
        let bleed_kg_s = request.loads.bleed_mass_flow_kg_s;
        if !takeoff || bleed_kg_s.is_nan() || bleed_kg_s <= 0.0 {
            return thrust_n;
        }
        let (total_pressure_pa, total_temperature_k) = total_conditions(request.flight);
        let corrected = (total_pressure_pa / SEA_LEVEL_PRESSURE_PA)
            / (total_temperature_k / SEA_LEVEL_TEMPERATURE_K).sqrt();
        let core_flow_kg_s =
            self.core_flow.static_core_flow_kg_s * corrected * self.core_flow.engines;
        if core_flow_kg_s.is_nan() || core_flow_kg_s <= 0.0 {
            return thrust_n;
        }
        thrust_n * (1.0 - BLEED_THRUST_LOSS_SLOPE * bleed_kg_s / core_flow_kg_s).max(0.0)
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
/// the active rating, in `[0.07, 1]`) at flight Mach number `mach`, relative
/// to the TSFC the anchors define (module documentation, "Part-power shape").
///
/// `full_rating_x` is the corrected thrust `FN/(delta FN*)` of the rating `u`
/// is a share of at this condition, so the flown corrected thrust is
/// `u x full_rating_x`; `None` (flight idle, whose fuel is the idle chart
/// flow) keeps the ICAO shape. `r(u) = f(u) / u`, the ICAO LTO fuel-flow
/// ratio over the thrust ratio, is the static shape; the flight shape is
/// [SfcLoopTable::part_power_ratio]; between Mach 0 and the lowest flight
/// loop (0.25) the two are blended linearly in Mach number.
pub(super) fn tsfc_shape(
    utilization: f64,
    mach: f64,
    full_rating_x: Option<f64>,
    loops: &SfcLoopTable,
    ratios: [f64; 4],
) -> f64 {
    let u = utilization.clamp(0.07, 1.0);
    let static_ratio = part_power_fuel_fraction(u, ratios) / u;
    let Some(full_rating_x) = full_rating_x.filter(|x| x.is_finite() && *x > 0.0) else {
        return static_ratio;
    };
    let flight_ratio =
        |at_mach: f64| loops.part_power_ratio(u * full_rating_x, full_rating_x, at_mach);
    if mach >= LOOP_MINIMUM_MACH {
        flight_ratio(mach)
    } else {
        let weight = (mach / LOOP_MINIMUM_MACH).clamp(0.0, 1.0);
        static_ratio + weight * (flight_ratio(LOOP_MINIMUM_MACH) - static_ratio)
    }
}

// A test asserts on values it constructed here directly.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    const ICAO_CFM56: [f64; 4] = [0.089_316_99, 0.276_707_53, 0.822_241_68, 1.0];

    #[test]
    fn shape_is_icao_static_and_the_flight_loops_in_flight() {
        let loops = SfcLoopTable::new(5.7);
        // Flight idle (no rating basis) and static conditions keep the ICAO
        // shape, dip included.
        let idle = tsfc_shape(0.07, 0.8, None, &loops, ICAO_CFM56);
        assert!((idle - 0.089_316_99 / 0.07).abs() < 1.0e-12, "{idle}");
        let static_85 = tsfc_shape(0.85, 0.0, Some(1.0), &loops, ICAO_CFM56);
        assert!((static_85 - 0.822_241_68 / 0.85).abs() < 1.0e-12);
        assert!(static_85 < 1.0);
        assert_eq!(tsfc_shape(1.0, 0.0, Some(1.0), &loops, ICAO_CFM56), 1.0);
        // Full rating carries its anchored TSFC at every Mach number.
        for mach in [0.25, 0.4, 0.6, 0.8, 0.9] {
            assert_eq!(tsfc_shape(1.0, mach, Some(0.6), &loops, ICAO_CFM56), 1.0);
        }
        // Cruise: utilization 0.85 of a rating at FN/(delta FN*) 0.85 is
        // above the M 0.8 bucket, so the TSFC is the anchored one.
        assert_eq!(tsfc_shape(0.85, 0.8, Some(0.85), &loops, ICAO_CFM56), 1.0);
        // Hold at 1,500 ft and M 0.35: about 26 % of maximum climb at
        // FN/(delta FN*) 0.58 rises from the bucket instead of dipping.
        let hold = tsfc_shape(0.26, 0.35, Some(0.58), &loops, ICAO_CFM56);
        assert!(hold > 1.15 && hold < 1.5, "{hold}");
        // Between static and M 0.25 the two shapes blend linearly in Mach.
        let low = tsfc_shape(0.26, 0.25, Some(0.58), &loops, ICAO_CFM56);
        let icao = part_power_fuel_fraction(0.26, ICAO_CFM56) / 0.26;
        let blended = tsfc_shape(0.26, 0.125, Some(0.58), &loops, ICAO_CFM56);
        assert!((blended - 0.5 * (low + icao)).abs() < 1.0e-12);
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
