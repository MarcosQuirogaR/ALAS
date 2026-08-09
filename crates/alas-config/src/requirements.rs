// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/requirements.py
// Reference: alas @ rust-port-baseline.

//! What the user is asking for: the mission targets the design must meet.
//!
//! This is the input, as distinct from every other module here, which is an
//! assumption. The optimizer searches the design space for the geometry that
//! best satisfies what is stated here, so a value in this module is a
//! requirement the search is trying to meet rather than a modelling choice
//! it is working under.
//!
//! The defaults reproduce the long-range transport the reference
//! implementation was tuned against, so the program produces a real aircraft
//! before anything has been configured.
//!
//! Two fields are only editable while the cabin preset is set to `Custom`:
//! a preset computes the passenger count and the cargo capacity from the
//! layout, and letting both be edited would leave the two disagreeing with no
//! indication of which one the run used.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Cabin presets valid for a passenger aircraft.
const PASSENGER_PRESETS: &[&str] = &["Ryanair", "Iberia", "Emirates", "Custom"];

/// Cabin presets valid for a freighter.
const CARGO_PRESETS: &[&str] = &["Max payload", "Dense payload", "Custom"];

/// Why a set of requirements could not be accepted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RequirementsError {
    /// The aircraft type was neither `passenger` nor `cargo`.
    ///
    /// Upstream raises for the same input. It is a `Result` here because
    /// library code in this workspace does not panic on bad input; the
    /// condition, and when it is checked, are the same.
    #[error("aircraft_type must be 'passenger' or 'cargo', got '{0}'")]
    UnknownAircraftType(String),
}

/// Top-level mission targets and constraints supplied by the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct DesignRequirements {
    /// The speed the aircraft is sized around.
    #[config(
        label = "Cruise Mach number",
        help = "Design cruise Mach number -- the primary speed target the optimizer sizes the aircraft around."
    )]
    pub cruise_mach: f64,

    /// The altitude the cruise point is evaluated at.
    #[config(
        label = "Cruise altitude",
        unit = "m",
        help = "Design cruise altitude, used to compute air density/speed of sound for the cruise design point."
    )]
    pub cruise_altitude_m: f64,

    /// The weight the whole sizing pipeline is anchored to.
    #[config(
        label = "Max take-off weight (MTOW)",
        unit = "kg",
        help = "Target maximum take-off weight -- anchors the whole weight & balance / sizing pipeline."
    )]
    pub mtow_kg: f64,

    /// Whether the aircraft carries passengers or freight.
    #[config(
        options = AircraftType,
        label = "Aircraft type",
        help = "'passenger' or 'cargo' -- switches which cabin-preset list and payload model apply."
    )]
    pub aircraft_type: String,

    /// The named layout the cabin is filled from.
    #[config(
        options = CabinPreset,
        label = "Cabin preset",
        help = "Named seating/payload layout preset ('Ryanair', 'Iberia', 'Emirates' for passenger; 'Max payload', 'Dense payload' for cargo). 'Custom' lets you hand-edit the Cabin & Payload tab."
    )]
    pub cabin_preset: String,

    /// How many passengers the aircraft is sized for.
    #[config(
        label = "Passenger count",
        readonly_unless(field = "cabin_preset", value = "Custom"),
        help = "Target passenger count (if aircraft_type is 'passenger'). Auto-recomputed when a cabin preset is active -- only editable with cabin_preset set to 'Custom'."
    )]
    pub num_passengers: i64,

    /// How much freight the aircraft is sized for.
    #[config(
        label = "Cargo payload capacity",
        unit = "kg",
        readonly_unless(field = "cabin_preset", value = "Custom"),
        help = "Target cargo payload capacity (if aircraft_type is 'cargo'). Auto-recomputed when a cabin preset is active -- only editable with cabin_preset set to 'Custom'."
    )]
    pub cargo_payload_kg: f64,

    /// The most the airframe may carry, whatever the hold could hold.
    #[config(
        advanced,
        label = "Max structural payload",
        unit = "kg",
        help = "Maximum structural payload (= MZFW - OEW), i.e. the most the airframe may carry regardless of how much the belly could physically hold. In passenger mode the detailed layout fills the lower-deck belly with revenue freight (on top of passengers + checked bags) up to this structural limit, so the payload -- and therefore the residual fuel (MTOW - OEW - payload) -- matches the real aircraft's max-payload point. A widebody belly can volumetrically hold far more than this structural cap, so without it 'fill the belly' overshoots. 0 = disabled (use the explicit Cabin & Payload belly_cargo_kg instead)."
    )]
    pub max_structural_payload_kg: f64,

    /// Limit load factor with the certification safety margin applied.
    #[config(
        advanced,
        label = "Ultimate load factor (n_ult)",
        help = "Limit load factor times the 1.5 safety margin, fed into the Torenbeek structural mass formulas."
    )]
    pub ultimate_load_factor: f64,

    /// The structural design dive speed.
    #[config(
        advanced,
        label = "Design dive speed (V_dive)",
        unit = "m/s",
        help = "Structural design dive speed, fed into the Torenbeek structural mass formulas. Also VD on the V-n diagram; design cruise speed VC is derived as VD/1.25 (CS-25.335(b) minimum margin) rather than a separate field."
    )]
    pub dive_speed_m_s: f64,

    /// The negative limit load factor.
    #[config(
        advanced,
        label = "Limit load factor, negative (n_lim,neg)",
        help = "CS-25.337(c) negative limit load factor for the V-n diagram. The positive limit load factor is derived as ultimate_load_factor / 1.5 (CS-25.303) rather than a separate field."
    )]
    pub limit_load_factor_neg: f64,

    /// Upper bound on wing planform area.
    #[config(
        advanced,
        label = "Maximum wing area",
        unit = "m^2",
        help = "Upper bound on wing planform area; the optimizer is penalised for exceeding it."
    )]
    pub max_wing_area_m2: f64,

    /// Lower bound on wing loading.
    #[config(
        advanced,
        label = "Minimum wing loading (MTOW/S)",
        unit = "kg/m^2",
        help = "Lower bound on wing loading (MTOW / wing area) -- keeps the wing from being sized too large for the mass it carries."
    )]
    pub min_wing_loading_kg_m2: f64,

    /// How close to stall a cruise point may sit before it is rejected.
    #[config(
        advanced,
        label = "Maximum cruise CL (stall guard)",
        help = "Candidate designs whose required cruise CL exceeds this are rejected as infeasible (too close to stall)."
    )]
    pub max_cruise_cl: f64,

    /// The stability margin the aft centre-of-gravity limit is set by.
    #[config(
        advanced,
        label = "Target static margin",
        unit = "fraction of MAC",
        help = "Static margin at the Aft CG Limit: Aft CG Limit (%MAC) = Neutral Point (%MAC) - target_static_margin*100. A positive value ensures positive static stability when the CG is at the aft limit."
    )]
    pub target_static_margin: f64,

    /// How wide the centre-of-gravity envelope is.
    #[config(
        advanced,
        label = "CG envelope width",
        unit = "% MAC",
        help = "Width of the CG envelope. Forward CG Limit (%MAC) = Aft CG Limit (%MAC) - cg_range_pct_mac."
    )]
    pub cg_range_pct_mac: f64,

    /// The stability floor a design is hard-rejected below.
    #[config(
        advanced,
        label = "Minimum physical static margin",
        unit = "fraction of MAC",
        help = "Minimum static margin measured using the actual mass-model (physical) CG, not the aerodynamic reference point. Designs below this are hard-rejected as inherently unstable. 0.0 = bare stability; 0.05 = 5% MAC buffer (recommended)."
    )]
    pub min_physical_static_margin: f64,

    /// Average mass of an occupant and their baggage.
    #[config(
        advanced,
        label = "Mass per passenger",
        unit = "kg",
        help = "Combined average mass per occupant (body + baggage). FAA AC 120-27E standard is 100 kg; airlines may use 90-105 kg."
    )]
    pub passenger_mass_kg: f64,

    /// Gravitational acceleration.
    #[config(
        advanced,
        label = "Gravitational acceleration",
        unit = "m/s^2",
        help = "Standard gravity, used to turn every mass in the sizing into a weight. Configuration rather than a constant so a run's assumptions are all in one place and all visible."
    )]
    pub gravity_m_s2: f64,
}

impl Default for DesignRequirements {
    fn default() -> Self {
        Self {
            cruise_mach: 0.84,
            cruise_altitude_m: 11887.2,
            mtow_kg: 358_670.0,
            aircraft_type: "passenger".to_owned(),
            cabin_preset: "Ryanair".to_owned(),
            num_passengers: 350,
            cargo_payload_kg: 102_100.0,
            max_structural_payload_kg: 0.0,
            ultimate_load_factor: 3.75,
            dive_speed_m_s: 220.0,
            limit_load_factor_neg: -1.0,
            max_wing_area_m2: 535.0,
            min_wing_loading_kg_m2: 485.0,
            max_cruise_cl: 0.95,
            target_static_margin: 0.10,
            cg_range_pct_mac: 30.0,
            min_physical_static_margin: 0.05,
            passenger_mass_kg: 100.0,
            gravity_m_s2: 9.81,
        }
    }
}

impl DesignRequirements {
    /// Reject an unknown aircraft type, and bring the cabin preset into line
    /// with the type.
    ///
    /// A freighter carrying a passenger preset, or the reverse, is not a
    /// configuration anything downstream can act on: the payload model
    /// selected by the type would be reading a layout meant for the other
    /// one. Upstream does this when a requirements object is constructed,
    /// and reproducing where it happens matters -- laying a partial
    /// configuration over an existing one does *not* re-run it there, so a
    /// file that sets only the aircraft type leaves a mismatched preset in
    /// place, and this port must not quietly fix that.
    ///
    /// # Errors
    ///
    /// [`RequirementsError::UnknownAircraftType`] when the type is neither
    /// `passenger` nor `cargo`.
    pub fn normalized(mut self) -> Result<Self, RequirementsError> {
        let presets = match self.aircraft_type.as_str() {
            "passenger" => PASSENGER_PRESETS,
            "cargo" => CARGO_PRESETS,
            other => return Err(RequirementsError::UnknownAircraftType(other.to_owned())),
        };
        if !presets.contains(&self.cabin_preset.as_str()) {
            // The two fall back differently upstream: a freighter with an
            // unrecognized preset gets the default layout, a passenger
            // aircraft gets `Custom`. Reproduced rather than unified.
            self.cabin_preset = if self.aircraft_type == "cargo" {
                "Max payload".to_owned()
            } else {
                "Custom".to_owned()
            };
        }
        Ok(self)
    }

    /// Total payload mass, from whichever of the two payload models the
    /// aircraft type selects.
    pub fn payload_kg(&self) -> f64 {
        if self.aircraft_type == "cargo" {
            return self.cargo_payload_kg;
        }
        self.num_passengers as f64 * self.passenger_mass_kg
    }

    /// The lift coefficient needed to hold level flight: `CL = W / (q * S)`.
    pub fn required_cruise_cl(&self, dynamic_pressure_pa: f64, wing_area_m2: f64) -> f64 {
        let weight_n = self.mtow_kg * self.gravity_m_s2;
        weight_n / (dynamic_pressure_pa * wing_area_m2)
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_defaults_need_no_normalizing() {
        // If they did, the defaults and what a run actually uses would differ,
        // and the parity fixture records the defaults.
        let defaults = DesignRequirements::default();
        assert_eq!(defaults.clone().normalized().unwrap(), defaults);
    }

    #[test]
    fn a_freighter_carrying_a_passenger_preset_is_brought_into_line() {
        let requirements = DesignRequirements {
            aircraft_type: "cargo".to_owned(),
            cabin_preset: "Emirates".to_owned(),
            ..Default::default()
        };
        assert_eq!(
            requirements.normalized().unwrap().cabin_preset,
            "Max payload"
        );
    }

    #[test]
    fn a_passenger_aircraft_with_an_unknown_preset_falls_back_to_custom() {
        // The two fall back to different things upstream, and the difference
        // is reproduced rather than tidied up.
        let requirements = DesignRequirements {
            cabin_preset: "Max payload".to_owned(),
            ..Default::default()
        };
        assert_eq!(requirements.normalized().unwrap().cabin_preset, "Custom");
    }

    #[test]
    fn an_unknown_aircraft_type_is_an_error_not_a_panic() {
        let requirements = DesignRequirements {
            aircraft_type: "seaplane".to_owned(),
            ..Default::default()
        };
        assert_eq!(
            requirements.normalized().unwrap_err(),
            RequirementsError::UnknownAircraftType("seaplane".to_owned())
        );
    }

    #[test]
    fn the_payload_comes_from_whichever_model_the_type_selects() {
        let passenger = DesignRequirements::default();
        assert_eq!(passenger.payload_kg(), 350.0 * 100.0);

        let freighter = DesignRequirements {
            aircraft_type: "cargo".to_owned(),
            ..Default::default()
        };
        assert_eq!(freighter.payload_kg(), freighter.cargo_payload_kg);
    }

    #[test]
    fn the_required_lift_coefficient_falls_with_speed_and_with_wing_area() {
        let requirements = DesignRequirements::default();
        let base = requirements.required_cruise_cl(15_000.0, 500.0);
        assert!(requirements.required_cruise_cl(30_000.0, 500.0) < base);
        assert!(requirements.required_cruise_cl(15_000.0, 1000.0) < base);
        // W / (q S), checked against the arithmetic rather than a recorded
        // number, since this is the one formula in this module.
        let expected = 358_670.0 * 9.81 / (15_000.0 * 500.0);
        assert!((base - expected).abs() < 1e-12);
    }

    #[test]
    fn a_preset_driven_field_is_marked_read_only_against_its_sibling() {
        let schema = DesignRequirements::default().schema();
        match &schema.field("num_passengers").unwrap().entry {
            crate::Entry::Leaf(leaf) => {
                let condition = leaf.readonly_unless.unwrap();
                assert_eq!(condition.field, "cabin_preset");
                assert_eq!(condition.value, "Custom");
            }
            crate::Entry::Node(_) => panic!("a passenger count is not a group"),
        }
    }
}
