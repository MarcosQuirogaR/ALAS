// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The clean-sheet entry point: a new aircraft started from requirements
//! alone, separate from the registered presets and from the Sandbox.
//!
//! The brief (payload, range, cruise Mach and altitude, a take-off mass to
//! seed the sizing, the route and the engine) becomes a preset-less
//! configuration. Wing, body, tail and engine station are derived from class
//! relations by [`AlasConfig::apply_clean_sheet_commit`], exactly as a brief
//! loaded from a file is, and the existing sizing and optimisation chain
//! takes it from there. No registered aircraft is loaded. The derivation
//! overlays the shipped geometry defaults (the AVE reference twin's body
//! section, loft reference, wing break fractions and the empennage that
//! `scale_empennage` scales from it), so those are still AVE-derived until
//! a derivation sizes them from the brief.

use alas_config::{AlasConfig, DESIGN_VARIABLE_SPECS};
use serde_json::{json, Value};

use super::config_edit_design_space::BriefCommit;
use super::{apply_interactive_solver, full_config_values, preserve_machine_locations};
use crate::state::{AppState, LogKind};
use crate::views::tr;

/// What a clean-sheet aircraft is asked to do.
#[derive(Debug, Clone, PartialEq)]
pub struct CleanSheetBrief {
    /// `passenger` or `cargo`.
    pub aircraft_type: String,
    /// Passengers carried, for a passenger aircraft.
    pub passengers: i64,
    /// Cargo payload in kilograms, for a freighter.
    pub cargo_payload_kg: f64,
    /// Design range in nautical miles.
    pub design_range_nmi: f64,
    /// Cruise Mach number.
    pub cruise_mach: f64,
    /// Cruise altitude in metres.
    pub cruise_altitude_m: f64,
    /// A take-off mass in kilograms that seeds the mass closure.
    pub mtow_estimate_kg: f64,
    /// Departure airport name.
    pub departure: String,
    /// Arrival airport name.
    pub arrival: String,
    /// Catalogue engine name.
    pub engine_name: String,
}

impl Default for CleanSheetBrief {
    /// A neutral single-aisle starting entry the user edits; these are
    /// ordinary form values, not model constants.
    fn default() -> Self {
        Self {
            aircraft_type: "passenger".to_owned(),
            passengers: 150,
            cargo_payload_kg: 0.0,
            design_range_nmi: 2_000.0,
            cruise_mach: 0.78,
            cruise_altitude_m: 11_278.0,
            mtow_estimate_kg: 70_000.0,
            departure: String::new(),
            arrival: String::new(),
            engine_name: String::new(),
        }
    }
}

impl CleanSheetBrief {
    /// The configuration document the brief states.
    pub fn document(&self) -> Value {
        let mut requirements = json!({
            "aircraft_type": self.aircraft_type,
            "cruise_mach": self.cruise_mach,
            "cruise_altitude_m": self.cruise_altitude_m,
            "mtow_kg": self.mtow_estimate_kg,
        });
        if self.aircraft_type == "cargo" {
            requirements["cargo_payload_kg"] = json!(self.cargo_payload_kg);
        } else {
            requirements["cabin_preset"] = json!("Custom");
            requirements["num_passengers"] = json!(self.passengers);
        }
        let mut document = json!({
            "requirements": requirements,
            "optimizer": {
                "design_space": {"mode": "clean_sheet"},
                "objective": {
                    "mtow_sizing": "payload_adjusted",
                    "design_range_nmi": self.design_range_nmi,
                },
            },
        });
        if !self.departure.is_empty() {
            document["departure_airport"] = json!(self.departure);
        }
        if !self.arrival.is_empty() {
            document["arrival_airport"] = json!(self.arrival);
        }
        if !self.engine_name.is_empty() {
            document["geometry"] = json!({"engine": {"engine_name": self.engine_name}});
        }
        document
    }
}

impl AppState {
    /// Replace the case with a new clean-sheet aircraft built from `brief`.
    /// Returns `false` (the case is unchanged) when the brief is not valid.
    pub fn start_clean_sheet(&mut self, brief: &CleanSheetBrief) -> bool {
        let mut config = match AlasConfig::from_value(&brief.document()) {
            Ok(config) => config,
            Err(error) => {
                self.log(error.to_string(), LogKind::Error);
                return false;
            }
        };
        apply_interactive_solver(&mut config);
        if let Some(previous) = self.typed_config() {
            preserve_machine_locations(&mut config, &previous);
        }
        config.preset.clear();
        self.config_values = full_config_values(&config);
        self.bump_config_revision();
        self.active_preset.clear();
        self.files.active_aircraft = None;
        self.selected_aux_preset.clear();
        self.design_values.clear();
        self.bounds.clear();
        self.pipeline_result = None;
        self.clear_result_figures();
        self.mission_profile_manual_edit = false;
        self.mission_profile_route_signature.clear();
        self.mission_profile_regeneration_prompt = false;
        self.mission_profile_retained_validation = None;
        self.clean_sheet_brief_fingerprint = None;
        crate::views::mission_profile_inputs::initialize_route_profile(self);
        // Commit the brief: marks it, derives the class-sized geometry, the
        // start and the search box, and resolves the aerodrome code.
        let committed = self.commit_clean_sheet_brief(
            config.clone(),
            BriefCommit::NewAircraft { from_preset: false },
        );
        if !committed {
            let nominal = config.configured_nominal_design();
            for (spec, value) in DESIGN_VARIABLE_SPECS.iter().zip(nominal.to_array()) {
                self.design_values.insert(spec.name.to_owned(), value);
            }
            self.reset_design_space_bounds_to_mode();
        }
        self.run_options.optimize = true;
        self.run_options.compare_baseline = false;
        self.log(tr("Started a clean-sheet aircraft."), LogKind::Info);
        self.on_config_modified();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started(brief: &CleanSheetBrief) -> AppState {
        let mut state = AppState::default();
        assert!(state.start_clean_sheet(brief));
        state
    }

    #[test]
    fn a_clean_sheet_has_no_registered_aircraft_and_derives_its_geometry_from_the_brief() {
        let small = started(&CleanSheetBrief::default());
        let config = small.typed_config().expect("typed config");
        assert!(config.preset.is_empty());
        assert!(small.active_preset.is_empty());
        assert!(config.derives_clean_sheet_start());
        assert_eq!(config.requirements.num_passengers, 150);
        assert!((config.requirements.cruise_mach - 0.78).abs() < 1e-12);
        assert!(small.run_options.optimize);
        assert_eq!(small.design_values.len(), DESIGN_VARIABLE_SPECS.len());
        // A larger brief gives a larger derived wing: the geometry follows
        // the requirements rather than a fixed reference.
        let large = started(&CleanSheetBrief {
            passengers: 300,
            mtow_estimate_kg: 230_000.0,
            design_range_nmi: 6_000.0,
            ..CleanSheetBrief::default()
        });
        assert!(large.design_values["span_m"] > small.design_values["span_m"]);
    }

    #[test]
    fn a_cargo_brief_states_the_cargo_payload_and_no_passengers() {
        let document = CleanSheetBrief {
            aircraft_type: "cargo".to_owned(),
            cargo_payload_kg: 18_000.0,
            ..CleanSheetBrief::default()
        }
        .document();
        assert_eq!(document["requirements"]["cargo_payload_kg"], 18_000.0);
        assert!(document["requirements"].get("num_passengers").is_none());
    }
}
