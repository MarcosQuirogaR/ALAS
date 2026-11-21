// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Evidence domain of the PW100-class turboprop surrogate, point by point.
//!
//! Each evaluation is judged on the ranges its sources cover, as the
//! empirical turbofan deck is judged on the ranges of its charts:
//!
//! * **Ratings**: certificated shaft power and flat-rating temperatures of
//!   the PW100 engine (EASA TCDS IM.E.041 section 5). They hold only on the
//!   TCDS-corner branch; a record that states its own thermodynamic power
//!   (the TP400-D6) is not a PW100 engine and none of this evidence covers it.
//! * **Power lapse and PSFC**: the PW120A cycle model and flight record of
//!   Majeed, *Parametric Specific Fuel Consumption Analysis of the PW120A
//!   Turboprop Engine*, SRS-TSD-002 Rev. 1, 2009: Tab. 4.2-4.3 run up to
//!   25,000 ft and over ISA-20 to ISA+20, and Tab. 4.1 gives the PSFC flat
//!   from 46 % to 100 % of take-off power. Above 25,000 ft, outside ISA+/-20,
//!   or below the 46 % knee (where the part-power rise is a flagged class
//!   shape) the fuel flow is extrapolated.
//! * **Propeller**: the blade efficiency is the sourced static figure of
//!   merit and forward-flight band (see `TurbopropThrustUncertainty`), whose
//!   uncertainty travels beside the thrust; the helical-tip-Mach loss above
//!   its critical value is a flagged factor, so such a point is extrapolated.
//! * **Flight idle** is the neutral zero-force hypothesis (no PW127 idle or
//!   windmilling map is public), and a governed point whose fuel flow had to
//!   be raised to the idle flow burns the idle surrogate's fuel: both are
//!   extrapolated.

use super::*;

/// Highest pressure height of the PW120A cycle-model tables, m (25,000 ft;
/// Majeed 2009, Tab. 4.2-4.3).
const PSFC_EVIDENCE_CEILING_M: f64 = 25_000.0 * 0.3048;
/// Largest ISA temperature deviation of the PW120A cycle-model tables, K
/// (ISA-20 to ISA+20; Majeed 2009, Tab. 4.2-4.3).
const PSFC_EVIDENCE_ISA_DEVIATION_K: f64 = 20.0;
/// ISA sea-level static temperature, K, and tropospheric lapse rate, K/m
/// (ICAO Doc 7488).
const ISA_SEA_LEVEL_K: f64 = 288.15;
const ISA_LAPSE_K_M: f64 = 0.0065;
/// ISA tropopause height, m, and temperature, K.
const ISA_TROPOPAUSE_M: f64 = 11_000.0;
const ISA_TROPOPAUSE_K: f64 = 216.65;
/// Relative tolerance on the PSFC comparison that detects the part-power rise.
const PSFC_RISE_TOLERANCE: f64 = 1.0e-9;

/// ISA static temperature at a pressure height, K.
fn isa_temperature_k(pressure_height_m: f64) -> f64 {
    if pressure_height_m >= ISA_TROPOPAUSE_M {
        ISA_TROPOPAUSE_K
    } else {
        ISA_SEA_LEVEL_K - ISA_LAPSE_K_M * pressure_height_m
    }
}

impl Atr72TurbopropSystem {
    /// The validity of one evaluation: `Valid` inside every range above, else
    /// `Extrapolated` with the first range it leaves.
    pub(super) fn evidence_validity(
        &self,
        flight: &FlightCondition,
        output: &TurbopropOutput,
        mode: TurbopropMode,
        idle_fuel_floor_raised: bool,
    ) -> ValidityStatus {
        match self.outside_evidence(flight, output, mode, idle_fuel_floor_raised) {
            None => ValidityStatus::Valid,
            Some(reason) => ValidityStatus::Extrapolated { reason },
        }
    }

    fn outside_evidence(
        &self,
        flight: &FlightCondition,
        output: &TurbopropOutput,
        mode: TurbopropMode,
        idle_fuel_floor_raised: bool,
    ) -> Option<String> {
        let model = &self.unit_model;
        if model.thermodynamic_power_w > 0.0 {
            return Some(
                "the PW100 evidence (TCDS IM.E.041 ratings, PW120A PSFC and lapse) does not cover an engine with a stated thermodynamic power".to_owned(),
            );
        }
        if mode == TurbopropMode::FlightIdle {
            return Some(
                "flight idle: neutral zero-force surrogate, no PW127 idle or windmilling map"
                    .to_owned(),
            );
        }
        if idle_fuel_floor_raised {
            return Some(
                "fuel flow held at the flight-idle surrogate's flow, no PW127 idle fuel data"
                    .to_owned(),
            );
        }
        if flight.altitude_m > PSFC_EVIDENCE_CEILING_M {
            return Some(format!(
                "pressure height {:.0} m above the 25,000 ft top of the PW120A PSFC and lapse evidence",
                flight.altitude_m
            ));
        }
        let deviation_k = flight.temperature_k - isa_temperature_k(flight.altitude_m.max(0.0));
        if deviation_k.abs() > PSFC_EVIDENCE_ISA_DEVIATION_K {
            return Some(format!(
                "ambient temperature ISA{deviation_k:+.1} K outside the ISA+/-20 K of the PW120A PSFC evidence"
            ));
        }
        let flat_psfc = model.psfc_kg_kwh(flight.temperature_k);
        if output.psfc_kg_kwh > flat_psfc * (1.0 + PSFC_RISE_TOLERANCE) {
            return Some(
                "shaft power below 46 % of available take-off power: the part-power PSFC rise is a flagged class shape, no PW100 part-power deck".to_owned(),
            );
        }
        let tip_mach = model.helical_tip_mach(
            flight.velocity_m_s,
            model.governed_propeller_speed_rpm,
            flight.temperature_k,
        );
        if tip_mach > model.tip_mach_critical {
            return Some(format!(
                "helical tip Mach {tip_mach:.3} above {:.2}: the compressibility loss is a flagged factor",
                model.tip_mach_critical
            ));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isa_temperature_follows_the_troposphere_and_holds_above_it() {
        assert!((isa_temperature_k(0.0) - 288.15).abs() < 1e-12);
        assert!((isa_temperature_k(PSFC_EVIDENCE_CEILING_M) - 238.62).abs() < 0.01);
        assert_eq!(isa_temperature_k(12_000.0), 216.65);
    }
}
