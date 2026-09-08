// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The operational fuel-planning policy a design mission is sized under.
//!
//! A route that ends with zero usable fuel is not a flyable mission. Every
//! operating rule this program can be asked to respect -- ICAO Annex 6 Part I
//! 4.3.6, the EASA Air OPS basic fuel scheme (CAT.OP.MPA.181 and its AMC),
//! 14 CFR 121.639 for United States domestic operations and 14 CFR 121.645
//! for flag and supplemental turbine operations -- decomposes the fuel on
//! board into the same named quantities: taxi, trip, contingency, destination
//! alternate, final reserve, additional, extra and discretionary fuel. This
//! group selects which rule supplies each quantity and holds the operator
//! assumptions those rules leave open, such as the alternate distance and
//! the taxi time.
//!
//! Selecting a scheme is a study assumption, not a claim of compliance: the
//! policy sizes the fuel a conceptual mission carries, and the reported plan
//! names the rule every kilogram came from so a reviewer can check it.

use serde::{Deserialize, Serialize};

use crate::{ConfigNode, Kind, Leaf};

/// Which operating rule supplies the reserve quantities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FuelScheme {
    /// EASA Air OPS basic fuel scheme, AMC1 CAT.OP.MPA.181: contingency is
    /// the larger of a trip-fuel fraction and a five-minute hold, the final
    /// reserve is a thirty-minute hold at 1,500 ft, and a destination
    /// alternate is carried unless the no-alternate variation is selected.
    #[default]
    EasaBasic,
    /// 14 CFR 121.639, domestic operations: fuel to the destination, to the
    /// most distant alternate, and forty-five minutes at normal cruise
    /// consumption. There is no separate contingency quantity.
    FaaDomestic,
    /// 14 CFR 121.645(b), flag and supplemental turbine operations: trip fuel,
    /// ten percent of the flight time to the destination, the alternate, and
    /// thirty minutes holding at 1,500 ft above the alternate.
    FaaFlagSupplemental,
    /// A named conceptual-design convention (the FAST-OAD and N+3 studies use
    /// five percent of trip fuel, a fixed diversion and a fixed hold) whose
    /// terms are read from this group as given, with no regulatory claim.
    StudyConvention,
    /// No reserves at all. The mission carries trip fuel only, which is the
    /// frozen behaviour of the earlier maximum-available-fuel mission and is
    /// retained for comparison rather than for design use.
    TripFuelOnly,
}

impl FuelScheme {
    /// Stable serialized name, which is also what the settings form shows.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EasaBasic => "easa_basic",
            Self::FaaDomestic => "faa_domestic",
            Self::FaaFlagSupplemental => "faa_flag_supplemental",
            Self::StudyConvention => "study_convention",
            Self::TripFuelOnly => "trip_fuel_only",
        }
    }

    /// Whether the scheme carries any quantity beyond trip fuel.
    pub const fn carries_reserves(self) -> bool {
        !matches!(self, Self::TripFuelOnly)
    }
}

impl Leaf for FuelScheme {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

/// The fuel-planning rule and the operator assumptions it leaves open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(default, deny_unknown_fields)]
pub struct FuelPolicyConfig {
    /// Which operating rule supplies the reserve quantities.
    #[config(
        options = FuelScheme,
        label = "Fuel scheme",
        help = "Operating rule that decomposes the fuel on board into taxi, trip, contingency, alternate, final reserve, additional, extra and discretionary fuel. Selecting one sizes the design mission under that rule; it is a study assumption, not a compliance finding."
    )]
    pub scheme: FuelScheme,

    /// Whether the flown mission carries the policy fuel or the maximum load.
    #[config(
        label = "Fly the policy load case",
        help = "When enabled the route is flown at the takeoff mass the fuel policy requires for it (taxi, trip, contingency, alternate and final reserve), bounded by the takeoff-mass limit and the tank capacity, and the reserves are checked against that flight. When disabled the mission burns the maximum available fuel, the frozen legacy load case, and the reserve plan is reported against it without being carried."
    )]
    pub fly_policy_load_case: bool,

    /// How long the aircraft taxies before takeoff and after landing.
    #[config(
        label = "Taxi time",
        unit = "min",
        help = "Ground time at idle fuel flow before takeoff, which the ramp mass carries above the takeoff mass. Ten to fifteen minutes is representative of a large hub; the value is an operator assumption every scheme leaves open."
    )]
    pub taxi_time_min: f64,

    /// Contingency fuel as a share of the planned trip fuel.
    #[config(
        label = "Contingency trip-fuel fraction",
        help = "Share of the planned trip fuel carried as contingency. The EASA basic scheme uses five percent, or three percent when an en-route alternate is available; the FAA flag rule uses ten percent of the flight time instead, and the domestic rule has no contingency quantity."
    )]
    pub contingency_trip_fraction: f64,

    /// Floor on contingency fuel expressed as holding time at the destination.
    #[config(
        label = "Contingency holding floor",
        unit = "min",
        help = "Minimum contingency fuel, expressed as holding time at the holding altitude above the destination at the estimated landing mass. The EASA basic scheme requires at least five minutes; set zero to remove the floor."
    )]
    pub contingency_minimum_hold_min: f64,

    /// Distance to the destination alternate.
    #[config(
        label = "Alternate distance",
        unit = "nmi",
        help = "Still-air distance from the destination to the alternate aerodrome, flown after a missed approach. Zero means no destination alternate, which under the EASA basic scheme adds a fifteen-minute hold at the destination instead. Two hundred nautical miles is the common conceptual-design convention."
    )]
    pub alternate_distance_nmi: f64,

    /// Holding time carried as final reserve fuel.
    #[config(
        label = "Final reserve hold",
        unit = "min",
        help = "Holding time at the holding altitude above the alternate (or destination without one) at the estimated landing mass. Thirty minutes is the turbine value in the EASA basic scheme and the FAA flag rule; the FAA domestic rule uses forty-five minutes at normal cruise consumption instead."
    )]
    pub final_reserve_hold_min: f64,

    /// Cruise time carried as reserve under the FAA domestic rule.
    #[config(
        label = "Domestic cruise reserve",
        unit = "min",
        help = "Flight time at normal cruise fuel consumption carried after the alternate under 14 CFR 121.639. Forty-five minutes is the rule; it is read only when the FAA domestic scheme is selected."
    )]
    pub domestic_cruise_reserve_min: f64,

    /// Share of the destination flight time carried under the FAA flag rule.
    #[config(
        label = "Flag flight-time fraction",
        help = "Share of the total flight time to the destination whose fuel is carried as reserve under 14 CFR 121.645(b). Ten percent is the rule; it is read only when the FAA flag/supplemental scheme is selected."
    )]
    pub flag_flight_time_fraction: f64,

    /// Height above the aerodrome at which holding fuel is evaluated.
    #[config(
        label = "Holding altitude above aerodrome",
        unit = "ft",
        help = "Height above aerodrome elevation at which contingency and final-reserve holding fuel is evaluated. Every scheme here states 1,500 ft in standard conditions."
    )]
    pub holding_altitude_ft: f64,

    /// Fuel carried for the specific cases the general reserves do not cover.
    #[config(
        label = "Additional fuel",
        unit = "kg",
        help = "Fuel carried for the specific cases the general reserves do not cover, such as an engine failure or depressurisation at the critical point of an extended-diversion route. Zero unless such a case is being studied."
    )]
    pub additional_fuel_kg: f64,

    /// Fuel the commander adds at their discretion.
    #[config(
        label = "Extra and discretionary fuel",
        unit = "kg",
        help = "Fuel added above the planned requirement, either for anticipated delays (extra) or at the commander's discretion. It is carried and burned last, so it only raises the takeoff mass."
    )]
    pub extra_fuel_kg: f64,

    /// Tank volume kept empty for thermal expansion.
    #[config(
        label = "Expansion space fraction",
        help = "Share of every tank's geometric volume that cannot be filled, held for thermal expansion of the fuel. CS 25.969 and 14 CFR 25.969 require at least two percent of the tank capacity."
    )]
    pub expansion_space_fraction: f64,

    /// Fuel that cannot be delivered to the engines.
    #[config(
        label = "Unusable fuel fraction",
        help = "Share of the usable tank capacity that remains undeliverable in the most adverse feed condition. CS 25.959 requires it to be established by test; between a half and one percent is representative of transport integral tanks. It belongs to the operating empty mass, not to the fuel load."
    )]
    pub unusable_fuel_fraction: f64,
}

impl Default for FuelPolicyConfig {
    fn default() -> Self {
        Self {
            scheme: FuelScheme::EasaBasic,
            fly_policy_load_case: true,
            taxi_time_min: 12.0,
            contingency_trip_fraction: 0.05,
            contingency_minimum_hold_min: 5.0,
            alternate_distance_nmi: 200.0,
            final_reserve_hold_min: 30.0,
            domestic_cruise_reserve_min: 45.0,
            flag_flight_time_fraction: 0.10,
            holding_altitude_ft: 1_500.0,
            additional_fuel_kg: 0.0,
            extra_fuel_kg: 0.0,
            expansion_space_fraction: 0.02,
            unusable_fuel_fraction: 0.007,
        }
    }
}

impl FuelPolicyConfig {
    /// Whether every quantity is finite and inside its admissible range.
    pub fn validate(&self) -> Result<(), String> {
        let nonnegative = [
            ("taxi_time_min", self.taxi_time_min),
            ("contingency_trip_fraction", self.contingency_trip_fraction),
            (
                "contingency_minimum_hold_min",
                self.contingency_minimum_hold_min,
            ),
            ("alternate_distance_nmi", self.alternate_distance_nmi),
            ("final_reserve_hold_min", self.final_reserve_hold_min),
            (
                "domestic_cruise_reserve_min",
                self.domestic_cruise_reserve_min,
            ),
            ("flag_flight_time_fraction", self.flag_flight_time_fraction),
            ("holding_altitude_ft", self.holding_altitude_ft),
            ("additional_fuel_kg", self.additional_fuel_kg),
            ("extra_fuel_kg", self.extra_fuel_kg),
        ];
        for (name, value) in nonnegative {
            if !value.is_finite() || value < 0.0 {
                return Err(format!("fuel policy {name} must be finite and nonnegative"));
            }
        }
        // CS 25.969 states a minimum: a smaller expansion space is a rule
        // violation, not a study choice, so it is refused rather than allowed.
        if !self.expansion_space_fraction.is_finite()
            || !(0.02..0.5).contains(&self.expansion_space_fraction)
        {
            return Err("fuel policy expansion_space_fraction must lie in [0.02, 0.5)".to_owned());
        }
        if !self.unusable_fuel_fraction.is_finite()
            || !(0.0..0.5).contains(&self.unusable_fuel_fraction)
        {
            return Err("fuel policy unusable_fuel_fraction must lie in [0, 0.5)".to_owned());
        }
        Ok(())
    }

    /// Whether the serialized group equals the defaults, so an unchanged
    /// policy is omitted from saved files and parity fixtures alike.
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_policy_is_the_easa_basic_scheme_with_its_published_numbers() {
        let policy = FuelPolicyConfig::default();
        assert_eq!(policy.scheme, FuelScheme::EasaBasic);
        assert_eq!(policy.contingency_trip_fraction, 0.05);
        assert_eq!(policy.contingency_minimum_hold_min, 5.0);
        assert_eq!(policy.final_reserve_hold_min, 30.0);
        assert_eq!(policy.holding_altitude_ft, 1_500.0);
        assert_eq!(policy.expansion_space_fraction, 0.02);
        assert!(policy.validate().is_ok());
    }

    #[test]
    fn scheme_names_are_stable_in_saved_configuration() {
        for scheme in [
            FuelScheme::EasaBasic,
            FuelScheme::FaaDomestic,
            FuelScheme::FaaFlagSupplemental,
            FuelScheme::StudyConvention,
            FuelScheme::TripFuelOnly,
        ] {
            let serialized = serde_json::to_value(scheme).ok();
            assert_eq!(serialized, Some(serde_json::json!(scheme.as_str())));
        }
        assert!(!FuelScheme::TripFuelOnly.carries_reserves());
        assert!(FuelScheme::EasaBasic.carries_reserves());
    }

    #[test]
    fn an_impossible_expansion_space_is_rejected() {
        let policy = FuelPolicyConfig {
            expansion_space_fraction: 0.8,
            ..Default::default()
        };
        assert!(policy.validate().is_err());
        // Below the CS 25.969 minimum is a rule violation, not a choice.
        let below_minimum = FuelPolicyConfig {
            expansion_space_fraction: 0.01,
            ..Default::default()
        };
        assert!(below_minimum.validate().is_err());
        let negative = FuelPolicyConfig {
            taxi_time_min: -1.0,
            ..Default::default()
        };
        assert!(negative.validate().is_err());
    }

    #[test]
    fn an_unchanged_policy_is_detectable_so_it_can_be_omitted_from_files() {
        assert!(FuelPolicyConfig::default().is_default());
        let changed = FuelPolicyConfig {
            alternate_distance_nmi: 0.0,
            ..Default::default()
        };
        assert!(!changed.is_default());
    }
}
