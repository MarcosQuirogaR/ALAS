// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/settings.py
// Reference: alas @ rust-port-baseline.

//! The one object that fully specifies a run.
//!
//! Every other module in this crate describes one group of settings. This is
//! the aggregate the pipeline is handed: fifteen groups, the name of the
//! aircraft preset the user started from, and the two airports the route is
//! flown between. Nothing downstream reaches past it for a value, so what a
//! run was configured with is exactly what one of these held.
//!
//! # Loading is where a file becomes a run
//!
//! [`AlasConfig::from_value`] is the whole loading path, and it does two
//! things in an order that matters. It first applies the named preset --
//! copying that aircraft's geometry and requirements in, and, where the preset
//! carries them, its mass-model and field-performance calibrations. Then it
//! lays the file's own keys over the result, so a file may start from a real
//! aircraft and change three fields of it.
//!
//! Applying the calibrations is the part most easily lost. Upstream's
//! graphical front end applies them when a preset is chosen from the dropdown,
//! which makes them look like an interface concern; they are not. A headless
//! run of the A220-300 that skipped them would silently revert to the
//! widebody-calibrated mass fractions its entry exists to correct -- about
//! 2.8 t of operating empty weight -- and to a generic narrowbody's high-lift
//! system, worth fifteen to twenty knots on every V-speed. Both would produce
//! a plausible aircraft and a wrong one.
//!
//! # Where the file formats went
//!
//! Upstream this module also opens and writes YAML and JSON files. Both are
//! thin codecs over the single dictionary representation below -- its own
//! comment says so -- and that representation is what lives here, as
//! `serde_json::Value`. Reading and writing a path is the concern of the crate
//! that owns paths, and putting a serialization-format dependency in the
//! configuration model would put it in every crate that reads a setting.

use serde::{Deserialize, Serialize};

use crate::{
    overlay, AnalysisConfig, CabinConfig, ConfigNode, ControlSurfacesConfig, DesignRequirements,
    DragModelConfig, GeometryConfig, LandingGearConfig, MassModelConfig, MissionConfig, MsesConfig,
    OptimizerConfig, OverlayError, PerformanceConfig, PropulsionCycleConfig, StructuresConfig,
};

/// Everything one run is configured with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct AlasConfig {
    /// The aircraft this configuration started from, if any.
    #[config(
        help = "Name of the aircraft preset this configuration was started from, or blank for one built from the defaults. Selecting one replaces the geometry and requirements below, and applies that aircraft's own mass-model and high-lift calibration where it has them."
    )]
    pub preset: String,

    /// What the aircraft has to do.
    #[config(
        nested,
        help = "The mission targets the design must meet -- the inputs, as distinct from the modelling assumptions in every other group."
    )]
    pub requirements: DesignRequirements,

    /// The shape the design vector is hung on.
    #[config(
        nested,
        help = "The geometry scaffold: everything about the airframe's shape that the optimizer's design vector does not itself vary."
    )]
    pub geometry: GeometryConfig,

    /// What the search is looking for, and how hard it looks.
    #[config(
        nested,
        help = "What the design search optimizes for and how it is run -- the objective weights and the differential-evolution solver settings."
    )]
    pub optimizer: OptimizerConfig,

    /// How finely each discipline is resolved.
    #[config(
        nested,
        help = "Analysis fidelity: which disciplines run, how finely each one is resolved, and how long they are allowed to take."
    )]
    pub analysis: AnalysisConfig,

    /// The parasite and induced drag build-up.
    #[config(
        nested,
        help = "Drag model assumptions -- form factors, interference factors and the margins applied to the parasite-drag buildup."
    )]
    pub drag_model: DragModelConfig,

    /// The high-lift system and the field-performance rules.
    #[config(
        nested,
        help = "Field-performance and high-lift assumptions: the maximum lift coefficients, the thrust lapse, and the certification speed schedule the takeoff and landing distances follow."
    )]
    pub performance: PerformanceConfig,

    /// How the empty weight is estimated.
    #[config(
        nested,
        help = "Mass-model calibration: the empirical fractions the structural, systems and furnishings weights are estimated from, and the centre-of-gravity limits the balance check uses."
    )]
    pub mass_model: MassModelConfig,

    /// The undercarriage.
    #[config(
        nested,
        help = "Landing-gear sizing: strut and tire selection, and the geometry the tip-over and clearance checks are run against."
    )]
    pub landing_gear: LandingGearConfig,

    /// What is carried, and how it is arranged.
    #[config(
        nested,
        help = "Cabin and payload layout: the seating classes, their pitch and abreast counts, and how the lower-deck hold is filled."
    )]
    pub cabin: CabinConfig,

    /// The flight the fuel burn is computed over.
    #[config(
        nested,
        help = "Mission profile: the segments flown, their speeds and altitudes, and the reserves carried on top of them."
    )]
    pub mission: MissionConfig,

    /// The viscous airfoil solver.
    #[config(
        nested,
        help = "MSES settings: the operating points the airfoil sections are analysed at, and the convergence limits the solver is run under."
    )]
    pub mses: MsesConfig,

    /// The moving surfaces.
    #[config(
        nested,
        help = "Control-surface sizing: the chord and span fractions of the ailerons, elevator and rudder, and their deflection limits."
    )]
    pub control_surfaces: ControlSurfacesConfig,

    /// The engine's thermodynamic cycle.
    #[config(
        nested,
        help = "Propulsion cycle assumptions: the component efficiencies and pressure losses the on-design turbofan analysis is run with."
    )]
    pub propulsion_cycle: PropulsionCycleConfig,

    /// The wing box, and how it is sized and meshed.
    #[config(
        nested,
        help = "Structural sizing and meshing: the spar and rib layout, the materials, the minimum gauges, the main-wing structural mass centroid, and the finite-element model built from them."
    )]
    pub structures: StructuresConfig,

    /// Where the route starts.
    ///
    /// Saved with the configuration rather than chosen per run, so reloading
    /// a file reproduces the same route without the user re-selecting it.
    #[config(
        help = "Departure aerodrome, by display name. Saved with the configuration so a reloaded file flies the same route; the field-performance check is run against its elevation, runway length and hot-day temperature."
    )]
    pub departure_airport: String,

    /// Where it ends.
    #[config(
        help = "Arrival aerodrome, by display name. Together with the departure aerodrome this fixes the range the mission is flown over and the landing field length that has to be met."
    )]
    pub arrival_airport: String,
}

impl Default for AlasConfig {
    fn default() -> Self {
        Self {
            preset: String::new(),
            requirements: DesignRequirements::default(),
            geometry: GeometryConfig::default(),
            optimizer: OptimizerConfig::default(),
            analysis: AnalysisConfig::default(),
            drag_model: DragModelConfig::default(),
            performance: PerformanceConfig::default(),
            mass_model: MassModelConfig::default(),
            landing_gear: LandingGearConfig::default(),
            cabin: CabinConfig::default(),
            mission: MissionConfig::default(),
            mses: MsesConfig::default(),
            control_surfaces: ControlSurfacesConfig::default(),
            propulsion_cycle: PropulsionCycleConfig::default(),
            structures: StructuresConfig::default(),
            // A long-haul pair, so an unconfigured run has a real route rather
            // than a zero-length one.
            departure_airport: "London Heathrow (EGLL)".to_owned(),
            arrival_airport: "Dubai (OMDB)".to_owned(),
        }
    }
}

impl AlasConfig {
    /// Build a configuration from what a saved file holds.
    ///
    /// `data` names a subset of the fields below, to any depth. A `preset` key
    /// is applied first, so the rest of the file is read as changes to that
    /// aircraft rather than to the defaults.
    ///
    /// A preset the registry does not carry is passed over rather than
    /// refused, and the overlay that follows then writes the unrecognized name
    /// into [`Self::preset`] regardless. Upstream does both, and the second is
    /// a consequence of the first rather than a decision: reproduced here so
    /// that a file naming a preset from a later version loads with its own
    /// values intact and says which preset it wanted.
    ///
    /// # Errors
    ///
    /// [`OverlayError`] when `data` is not a mapping, names a field that does
    /// not exist, or gives one a value of the wrong type.
    pub fn from_value(data: &serde_json::Value) -> Result<Self, OverlayError> {
        let mut instance = Self::default();

        if let Some(name) = data.get("preset").and_then(serde_json::Value::as_str) {
            match crate::presets::get(name) {
                Ok(preset) => {
                    let operational = preset.operational_mission_defaults();
                    instance.preset = name.to_owned();
                    instance.geometry = preset.geometry.clone();
                    instance.requirements = preset.requirements.clone();
                    instance.landing_gear = preset.landing_gear.clone();
                    if let Some(mass_model) = &preset.mass_model {
                        instance.mass_model = mass_model.clone();
                    }
                    if let Some(performance) = &preset.performance {
                        instance.performance = performance.clone();
                    }
                    instance.departure_airport = operational.departure_airport.to_owned();
                    instance.arrival_airport = operational.arrival_airport.to_owned();
                    instance.mission.profile = operational.profile;
                }
                Err(error) => {
                    tracing::debug!(%error, "configuration names an unregistered preset");
                }
            }
        }

        overlay(&instance, data)
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_empty_file_is_the_defaults_rather_than_an_error() {
        assert_eq!(
            AlasConfig::from_value(&json!({})).unwrap(),
            AlasConfig::default()
        );
    }

    #[test]
    fn a_preset_replaces_the_geometry_and_the_requirements() {
        let config = AlasConfig::from_value(&json!({"preset": "A380-800"})).unwrap();
        let preset = crate::presets::get("A380-800").unwrap();
        assert_eq!(config.geometry, preset.geometry);
        assert_eq!(config.requirements, preset.requirements);
        assert_eq!(config.landing_gear, preset.landing_gear);
        assert_eq!(config.preset, "A380-800");
    }

    #[test]
    fn a_presets_own_calibrations_are_applied_on_the_headless_path_too() {
        // The graphical front end applies these on selection, which makes
        // them look like an interface concern. Skipping them here is about
        // 2.8 t of operating empty weight and fifteen knots of V-speed.
        let config = AlasConfig::from_value(&json!({"preset": "A220-300"})).unwrap();
        assert_eq!(config.mass_model.systems_mass_fraction, 0.13);
        assert_eq!(config.mass_model.furnishings_mass_fraction, 0.12);
        assert_eq!(config.performance.cl_max_to, 2.10);
    }

    #[test]
    fn selecting_a_preset_loads_its_operational_route_and_cruise_schedule() {
        let config = AlasConfig::from_value(&json!({"preset": "A220-300"})).unwrap();
        let preset = crate::presets::get("A220-300").unwrap();
        assert_eq!(config.departure_airport, "Riga (EVRA)");
        assert_eq!(config.arrival_airport, "Stockholm Arlanda (ESSA)");
        let atmosphere = alas_atmo::Atmosphere::new(config.requirements.cruise_altitude_m);
        let expected = config.requirements.cruise_mach * atmosphere.speed_of_sound();
        assert!((config.mission.profile.cruise_1_air_speed_m_s - expected).abs() < 1e-9);
        assert_ne!(
            config.mission.profile.cruise_1_air_speed_m_s,
            crate::MissionProfileConfig::default().cruise_1_air_speed_m_s
        );
        assert_eq!(config.requirements, preset.requirements);
    }

    #[test]
    fn saved_route_and_profile_values_override_the_preset_defaults() {
        let config = AlasConfig::from_value(&json!({
            "preset": "A220-300",
            "departure_airport": "Paris CDG (LFPG)",
            "arrival_airport": "Frankfurt (EDDF)",
            "mission": {"profile": {"cruise_1_air_speed_m_s": 219.0}}
        }))
        .unwrap();
        assert_eq!(config.departure_airport, "Paris CDG (LFPG)");
        assert_eq!(config.arrival_airport, "Frankfurt (EDDF)");
        assert_eq!(config.mission.profile.cruise_1_air_speed_m_s, 219.0);
    }

    #[test]
    fn a_preset_without_its_own_calibration_keeps_the_global_mass_model() {
        let config = AlasConfig::from_value(&json!({"preset": "B787-9"})).unwrap();
        assert_eq!(config.mass_model, MassModelConfig::default());
    }

    #[test]
    fn the_file_is_laid_over_the_preset_and_not_the_other_way_round() {
        let config = AlasConfig::from_value(
            &json!({"preset": "B787-9", "requirements": {"cruise_mach": 0.80}}),
        )
        .unwrap();
        let preset = crate::presets::get("B787-9").unwrap();
        assert_eq!(config.requirements.cruise_mach, 0.80);
        // Everything the file did not name still comes from the preset.
        assert_eq!(config.requirements.mtow_kg, preset.requirements.mtow_kg);
    }

    #[test]
    fn an_unregistered_preset_loads_and_keeps_the_name_it_asked_for() {
        let config = AlasConfig::from_value(&json!({"preset": "Concorde"})).unwrap();
        assert_eq!(config.preset, "Concorde");
        assert_eq!(config.geometry, GeometryConfig::default());
    }

    #[test]
    fn a_misspelled_key_is_refused_rather_than_dropped() {
        let error =
            AlasConfig::from_value(&json!({"requirements": {"cruise_match": 0.8}})).unwrap_err();
        assert!(format!("{error}").contains("cruise_match"), "{error}");
    }

    #[test]
    fn a_configuration_round_trips_through_serialization() {
        let config = AlasConfig::from_value(&json!({"preset": "DC-10"})).unwrap();
        let text = serde_json::to_string(&config).unwrap();
        assert_eq!(serde_json::from_str::<AlasConfig>(&text).unwrap(), config);
    }

    #[test]
    fn removed_runtime_keys_are_rejected_as_unknown_settings() {
        let config = AlasConfig::from_value(&json!({
            "mission": {
                "suave_venv_dir": "old-venv",
                "suave_runner_dir": "old-runner"
            }
        }))
        .expect_err("removed external-runtime settings must not remain accepted");
        let message = format!("{config}");
        assert!(message.contains("suave_venv_dir") || message.contains("suave_runner_dir"));
    }

    #[test]
    fn the_settings_screen_lists_the_preset_then_the_groups_then_the_route() {
        let names: Vec<&str> = AlasConfig::default()
            .schema()
            .fields
            .iter()
            .map(|field| field.name)
            .collect();
        assert_eq!(names.first(), Some(&"preset"));
        assert_eq!(names.last(), Some(&"arrival_airport"));
        assert_eq!(names.len(), 17);
    }

    #[test]
    fn both_default_aerodromes_are_ones_the_table_carries() {
        // The field-performance check looks them up by display name, and a
        // default that is not in the table would fail on an unconfigured run.
        let config = AlasConfig::default();
        assert!(crate::airports::get(&config.departure_airport).is_ok());
        assert!(crate::airports::get(&config.arrival_airport).is_ok());
    }
}
