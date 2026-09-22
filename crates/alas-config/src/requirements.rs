// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/requirements.py
// Reference: alas @ rust-port-baseline.

//! The stable wire group for mission targets and aircraft-level limits.
//!
//! The original configuration grouped mission requirements, discipline inputs
//! and a few study bounds here. The optimizer still reads this group as the
//! compatibility authority, but new code should classify each field explicitly
//! as a requirement, model input, preference or numerical setting. The audit
//! and migration ledger lives in `docs/design-constraints.md`; moving fields
//! requires a saved-file migration rather than silently changing their meaning.
//!
//! The defaults reproduce the long-range transport the reference
//! implementation was tuned against, so the program produces a real aircraft
//! before anything has been configured.
//!
//! Passenger capacity is always recomputed for each candidate shell. A saved
//! `optimize_passenger_capacity` value is still accepted for compatibility,
//! but no longer changes product behavior.
//!
//! Passenger and cargo targets are editable while the cabin preset is set to
//! `Custom`. A named preset computes its payload from geometry, and letting
//! both be edited would leave the two disagreeing with no indication of which
//! one the run used.

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
        help = "Design cruise Mach number: the primary speed target the optimizer sizes the aircraft around."
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
        help = "Target maximum take-off weight: anchors the whole weight & balance / sizing pipeline."
    )]
    pub mtow_kg: f64,

    /// Whether the aircraft carries passengers or freight.
    #[config(
        options = AircraftType,
        label = "Aircraft type",
        help = "'passenger' or 'cargo': switches which cabin-preset list and payload model apply."
    )]
    pub aircraft_type: String,

    /// The named layout the cabin is filled from.
    ///
    /// The serialized values are historical identifiers (originally named
    /// after the airline whose published layout each one reproduces, see
    /// `alas-payload/src/build/presets.rs`) and must not change; only the
    /// descriptive label the GUI shows for each one does
    /// (`alas-gui/src/views/form_options.rs::display_option`).
    #[config(
        options = CabinPreset,
        label = "Cabin preset",
        help = "Named seating/payload layout preset ('High-density single-class', 'Two-class (Business/Economy)', 'Three-class (First/Business/Economy)' for passenger; 'Max payload', 'Dense payload' for cargo). 'Custom' lets you hand-edit the Cabin & Payload tab."
    )]
    pub cabin_preset: String,

    /// Legacy passenger load-case switch, retained for saved-file compatibility.
    #[serde(default = "default_optimize_passenger_capacity", skip_serializing)]
    #[config(skip)]
    pub optimize_passenger_capacity: bool,

    /// Derived passenger capacity, retained in the serialized model.
    #[config(
        hidden,
        help = "Target passenger count (if aircraft_type is 'passenger'). Auto-recomputed when a cabin preset is active, only editable with cabin_preset set to 'Custom'."
    )]
    pub num_passengers: i64,

    /// How much freight the aircraft is sized for.
    #[config(
        label = "Cargo payload capacity",
        unit = "kg",
        readonly_unless(field = "cabin_preset", value = "Custom"),
        help = "Target cargo payload capacity (if aircraft_type is 'cargo'). Auto-recomputed when a cabin preset is active, only editable with cabin_preset set to 'Custom'."
    )]
    pub cargo_payload_kg: f64,

    /// The freight mass the design is asked to match, kg.
    ///
    /// The user's requested cargo objective: deliberately a different
    /// quantity from `cargo_payload_kg` (the capacity a cabin preset
    /// determines and the load case requests) and from the payload a
    /// candidate actually carries. A cabin preset writes the capacity and
    /// never this field, because a request that preset application
    /// overwrote would not be a requirement at all. Read it through
    /// [`Self::cargo_target_kg`], the mass the objective scores a
    /// candidate's deviation from.
    #[serde(default)]
    #[config(
        advanced,
        label = "Cargo capacity objective",
        unit = "kg",
        help = "Cargo capacity objective: the cargo payload mass requested of a freighter (aircraft_type 'cargo'), which the search is rewarded for getting closer to. It stays separate from the cargo payload capacity above and from the payload a candidate actually carries, so it redefines neither, and it is not a floor a candidate must clear: mass, volume, structural-loading and mission feasibility are judged exactly as before. 0 = disabled (the default), and the objective then follows the configured cargo payload capacity."
    )]
    pub cargo_objective_kg: f64,

    /// The most the airframe may carry, whatever the hold could hold.
    #[config(
        advanced,
        label = "Max structural payload",
        unit = "kg",
        help = "Maximum structural payload (= MZFW - OEW), i.e. the most the airframe may carry regardless of how much the belly could physically hold. In passenger mode the detailed layout fills the lower-deck belly with revenue freight (on top of passengers + checked bags) up to this structural limit, so the payload, and therefore the residual fuel (MTOW - OEW - payload), matches the real aircraft's max-payload point. A widebody belly can volumetrically hold far more than this structural cap, so without it 'fill the belly' overshoots. 0 = disabled (use the explicit Cabin & Payload belly_cargo_kg instead)."
    )]
    pub max_structural_payload_kg: f64,

    /// The fewest passengers a candidate's geometry-resolved cabin may carry.
    #[config(
        advanced,
        label = "Minimum passenger capacity",
        help = "Hard floor on the geometry-resolved passenger capacity: a candidate whose class-mix and geometry produce fewer than this many seats is scored infeasible under the configured geometry constraint policy. 0 = disabled (the default): capacity is otherwise always dynamic, whatever the configured cabin class-mix percentages and the candidate's actual fuselage/cabin geometry produce, with no minimum."
    )]
    pub min_passenger_capacity: i64,

    /// Limit load factor with the certification safety margin applied.
    #[config(
        advanced,
        label = "Ultimate load factor (n_ult)",
        help = "Structural screening input fed into the Torenbeek mass formulas. The shipped 3.75 is 1.5 x 2.5; verify the selected certification basis, amendment, aircraft category and load case before treating it as an airworthiness value."
    )]
    pub ultimate_load_factor: f64,

    /// The structural design dive speed.
    #[config(
        advanced,
        label = "Design dive speed (V_dive)",
        unit = "m/s",
        help = "Structural screening dive speed, fed into the Torenbeek mass formulas and the V-n diagram. The project may derive VC as VD/1.25 for this study; verify speed type, altitude/Mach envelope, certification basis and amendment before treating that relation as an airworthiness result."
    )]
    pub dive_speed_m_s: f64,

    /// The negative limit load factor.
    #[config(
        advanced,
        label = "Limit load factor, negative (n_lim,neg)",
        help = "Negative V-n screening input. The shipped -1.0 follows the large-aeroplane CS-25 reference case up to VC; verify the selected certification basis, amendment, speed range and category before using it for qualification. The positive limit value is derived as ultimate_load_factor / 1.5."
    )]
    pub limit_load_factor_neg: f64,

    /// Upper bound on projected wing reference/planform area.
    #[config(
        advanced,
        label = "Maximum wing area",
        unit = "m^2",
        help = "Upper bound on the wing planform projected onto the aircraft XY reference plane; the optimizer is penalised for exceeding it."
    )]
    pub max_wing_area_m2: f64,

    /// Lower bound on wing loading.
    #[config(
        advanced,
        label = "Minimum wing loading (MTOW/S)",
        unit = "kg/m^2",
        help = "Lower bound on wing loading (MTOW / wing area): keeps the wing from being sized too large for the mass it carries."
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
        help = "Combined average mass per occupant (body + baggage). The shipped 100 kg is a transparent project load-case default; FAA AC 120-27F is operator weight-and-balance guidance and does not establish a universal passenger mass. Record the operator, population, baggage method and date before using another value operationally. This remains the single load-case authority for report, GUI preview, pipeline, export and optimizer paths: cabin.passenger.checked_bag_mass_kg supplies the baggage share and the occupant slot the remainder."
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
            optimize_passenger_capacity: true,
            num_passengers: 350,
            cargo_payload_kg: 102_100.0,
            cargo_objective_kg: 0.0,
            max_structural_payload_kg: 0.0,
            min_passenger_capacity: 0,
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

const fn default_optimize_passenger_capacity() -> bool {
    true
}

impl DesignRequirements {
    /// Whether the payload target is recomputed from each candidate's cabin.
    ///
    /// This is unconditional in the product model. The legacy serialized
    /// switch is deliberately ignored so an old file cannot restore partial
    /// floor use.
    pub fn resolves_payload_from_candidate_geometry(&self) -> bool {
        true
    }

    /// Reject an unknown aircraft type, and bring the cabin preset into line
    /// with the type.
    ///
    /// A freighter carrying a passenger preset, or the reverse, is not a
    /// configuration anything downstream can act on: the payload model
    /// selected by the type would be reading a layout meant for the other
    /// one. Upstream does this when a requirements object is constructed,
    /// and reproducing where it happens matters, laying a partial
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

    /// The cargo payload mass the objective scores a candidate against, kg.
    ///
    /// The requested [`Self::cargo_objective_kg`] when the user entered one,
    /// otherwise the configured [`Self::cargo_payload_kg`] capacity, which is
    /// the mass the objective used before a request could be entered: a study
    /// that enters none is scored exactly as it was. A non-finite or
    /// non-positive request is no request, since a payload mass is positive
    /// by definition and zero or below is outside the target's valid domain
    /// rather than a request to carry nothing.
    ///
    /// This resolves the target only; it writes neither field, so the request
    /// can redefine neither the capacity a cabin preset determined nor the
    /// payload the load case actually carries.
    pub fn cargo_target_kg(&self) -> f64 {
        if self.cargo_objective_kg.is_finite() && self.cargo_objective_kg > 0.0 {
            return self.cargo_objective_kg;
        }
        self.cargo_payload_kg
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
    fn the_cargo_objective_is_disabled_by_default_and_the_capacity_is_the_target() {
        // Nothing entered: the objective is scored against the configured
        // capacity, which is the mass it was scored against before a request
        // could be entered at all.
        let defaults = DesignRequirements::default();
        assert_eq!(defaults.cargo_objective_kg, 0.0);
        assert_eq!(defaults.cargo_target_kg(), defaults.cargo_payload_kg);
    }

    #[test]
    fn an_entered_cargo_objective_is_the_target_and_leaves_the_capacity_alone() {
        let requirements = DesignRequirements {
            aircraft_type: "cargo".to_owned(),
            cargo_payload_kg: 45_000.0,
            cargo_objective_kg: 60_000.0,
            ..Default::default()
        };
        assert_eq!(requirements.cargo_target_kg(), 60_000.0);
        // The request is the target and nothing else: the capacity, and so
        // the payload the load case asks the hold for, are untouched.
        assert_eq!(requirements.cargo_payload_kg, 45_000.0);
        assert_eq!(requirements.payload_kg(), 45_000.0);
    }

    #[test]
    fn a_cargo_objective_outside_its_valid_domain_is_no_request() {
        // A payload mass is positive and finite. Anything else is not a
        // smaller target, it is no target, and the capacity keeps the role.
        for request in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let requirements = DesignRequirements {
                cargo_objective_kg: request,
                ..Default::default()
            };
            assert_eq!(
                requirements.cargo_target_kg(),
                requirements.cargo_payload_kg,
                "request {request}"
            );
        }
    }

    #[test]
    fn the_cargo_objective_survives_a_save_and_load_round_trip() {
        let requirements = DesignRequirements {
            aircraft_type: "cargo".to_owned(),
            cabin_preset: "Max payload".to_owned(),
            cargo_objective_kg: 62_500.0,
            ..Default::default()
        };
        let saved = serde_json::to_value(requirements.clone()).unwrap();
        assert_eq!(saved["cargo_objective_kg"], 62_500.0);
        let loaded: DesignRequirements = serde_json::from_value(saved).unwrap();
        assert_eq!(loaded, requirements);
        assert_eq!(loaded.cargo_target_kg(), 62_500.0);
    }

    #[test]
    fn a_file_written_before_the_cargo_objective_existed_loads_with_it_disabled() {
        let mut saved = serde_json::to_value(DesignRequirements::default()).unwrap();
        saved.as_object_mut().unwrap().remove("cargo_objective_kg");
        let loaded: DesignRequirements = serde_json::from_value(saved).unwrap();
        assert_eq!(loaded.cargo_objective_kg, 0.0);
        assert_eq!(loaded, DesignRequirements::default());
    }

    #[test]
    fn the_cargo_objective_is_an_advanced_field_in_kilograms() {
        // Advanced, so the guided requirements card keeps exactly the fields
        // it had: the objective is rendered there by the Inputs view itself,
        // and only for a freighter. It is a free input at all times, unlike
        // the capacity a cabin preset owns.
        let schema = DesignRequirements::default().schema();
        let field = schema
            .field("cargo_objective_kg")
            .expect("the cargo objective is in the schema");
        assert_eq!(field.unit, "kg");
        assert!(field.advanced);
        match &field.entry {
            crate::Entry::Leaf(leaf) => assert!(leaf.readonly_unless.is_none()),
            crate::Entry::Node(_) => panic!("a payload mass is not a group"),
        }
    }

    #[test]
    fn candidate_geometry_always_resolves_payload_capacity() {
        let fixed = DesignRequirements {
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: false,
            num_passengers: 130,
            ..Default::default()
        };
        let capacity = DesignRequirements {
            optimize_passenger_capacity: true,
            ..fixed.clone()
        };

        assert!(!fixed.optimize_passenger_capacity);
        assert_eq!(fixed.num_passengers, 130);
        assert!(fixed.resolves_payload_from_candidate_geometry());
        assert!(capacity.resolves_payload_from_candidate_geometry());

        let cargo = DesignRequirements {
            aircraft_type: "cargo".to_owned(),
            optimize_passenger_capacity: false,
            ..fixed
        };
        assert!(cargo.resolves_payload_from_candidate_geometry());
    }

    #[test]
    fn historical_files_without_the_load_case_flag_keep_the_reference_default() {
        let value = serde_json::to_value(DesignRequirements::default()).unwrap();
        assert!(value.get("optimize_passenger_capacity").is_none());
        let requirements: DesignRequirements = serde_json::from_value(value).unwrap();

        assert!(requirements.optimize_passenger_capacity);
    }

    #[test]
    fn a_legacy_false_capacity_flag_loads_but_is_ignored_and_not_saved_again() {
        let mut value = serde_json::to_value(DesignRequirements::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("optimize_passenger_capacity".to_owned(), false.into());

        let requirements: DesignRequirements = serde_json::from_value(value).unwrap();
        assert!(!requirements.optimize_passenger_capacity);
        assert!(requirements.resolves_payload_from_candidate_geometry());
        assert!(serde_json::to_value(requirements)
            .unwrap()
            .get("optimize_passenger_capacity")
            .is_none());
    }

    #[test]
    fn the_legacy_load_case_choice_is_absent_from_the_settings_schema() {
        let schema = DesignRequirements::default().schema();
        assert!(schema.field("optimize_passenger_capacity").is_none());
        assert!(schema.field("num_passengers").is_none());
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
    fn cargo_payload_remains_editable_only_for_a_custom_preset() {
        let schema = DesignRequirements::default().schema();
        match &schema.field("cargo_payload_kg").unwrap().entry {
            crate::Entry::Leaf(leaf) => {
                let condition = leaf.readonly_unless.unwrap();
                assert_eq!(condition.field, "cabin_preset");
                assert_eq!(condition.value, "Custom");
            }
            crate::Entry::Node(_) => panic!("a passenger count is not a group"),
        }
    }

    #[test]
    fn the_passenger_capacity_floor_defaults_to_disabled_and_is_advanced() {
        assert_eq!(DesignRequirements::default().min_passenger_capacity, 0);
        let schema = DesignRequirements::default().schema();
        let field = schema
            .field("min_passenger_capacity")
            .expect("min_passenger_capacity is in the schema");
        assert!(field.advanced);
    }
}
