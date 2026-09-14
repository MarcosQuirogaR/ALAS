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
//! Applying the aircraft data is the part most easily lost. Upstream's
//! graphical front end applies it when a preset is chosen from the dropdown,
//! which makes it look like an interface concern; it is part of the run
//! definition. A headless run that skips a preset's source-backed FLOPS
//! transport inputs, wing-box material family, tank arrangement or high-lift
//! system can produce a plausible aircraft with the wrong mass method or
//! performance domain. The historical mass fractions carried by some presets
//! remain available only to the explicit reference-compatible comparison path.
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
    DragModelConfig, FuelPolicyConfig, FuelTankLayoutConfig, GeometryConfig, LandingGearConfig,
    MassModelConfig, MissionConfig, MsesConfig, OptimizerConfig, OverlayError, PerformanceConfig,
    PropulsionCycleConfig, StructuresConfig,
};

/// Top-level key under which the desktop application stores its workspace
/// session (mode, sandbox design and window layout) beside the aircraft
/// configuration in one saved file. [`AlasConfig::from_value`] removes it
/// before decoding, so such a file remains a valid configuration everywhere.
pub const WORKSPACE_ENVELOPE_KEY: &str = "alas_workspace";

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
        help = "Mass architecture and weight-and-balance inputs. Pure FLOPS transport is the product default and owns every production mass group; the legacy reference-compatible fractions remain available only through an explicit comparison path."
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

    /// The operating rule the mission fuel is planned under.
    #[serde(default, skip_serializing_if = "FuelPolicyConfig::is_default")]
    #[config(
        nested,
        help = "Fuel-planning policy: which operating rule supplies taxi, contingency, alternate and final-reserve fuel, and the operator assumptions those rules leave open."
    )]
    pub fuel_policy: FuelPolicyConfig,

    /// Where the fuel is carried.
    #[serde(default, skip_serializing_if = "FuelTankLayoutConfig::is_default")]
    #[config(
        nested,
        help = "Fuel-tank arrangement: which wing, centre, trim and auxiliary tanks the aircraft has, the semispan stations that bound them, and the published capacities of a registered aircraft."
    )]
    pub fuel_tanks: FuelTankLayoutConfig,

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
            fuel_policy: FuelPolicyConfig::default(),
            fuel_tanks: FuelTankLayoutConfig::default(),
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
        Self::from_value_with_migration(data).map(|(config, _)| config)
    }

    /// [`Self::from_value`], also reporting what loading did to the mass
    /// method.
    ///
    /// The mass architecture is the one configuration decision whose
    /// migration changes a published number -- operating empty mass -- so it
    /// is returned rather than logged. A front end shows it, an export
    /// records it, and a headless run can assert on it.
    ///
    /// # Errors
    ///
    /// As [`Self::from_value`].
    pub fn from_value_with_migration(
        data: &serde_json::Value,
    ) -> Result<(Self, crate::MassArchitectureMigration), OverlayError> {
        // A workspace file carries the desktop session envelope next to the
        // aircraft configuration. The envelope is not aircraft data, so it is
        // removed before the strict overlay sees the document; a file with no
        // envelope is unchanged by this step.
        let without_envelope;
        let data = match data.as_object() {
            Some(map) if map.contains_key(WORKSPACE_ENVELOPE_KEY) => {
                let mut map = map.clone();
                map.remove(WORKSPACE_ENVELOPE_KEY);
                without_envelope = serde_json::Value::Object(map);
                &without_envelope
            }
            _ => data,
        };
        let mut instance = Self::default();

        if let Some(name) = data.get("preset").and_then(serde_json::Value::as_str) {
            match crate::presets::get(name) {
                Ok(preset) => {
                    let operational = preset.operational_mission_defaults();
                    instance.preset = name.to_owned();
                    instance.geometry = preset.geometry.clone();
                    // A preset selection is the one product boundary where
                    // an engine name intentionally loads database values.
                    // The overlay below then applies saved/live edits, which
                    // every downstream solver must preserve verbatim.
                    instance.geometry.engine.apply_engine_spec();
                    instance.requirements = preset.requirements.clone();
                    // A preset's published passenger target is a load-case
                    // input, not an operator LOPA. Seed the generic cabin
                    // with the profile that can physically represent that
                    // target; the file overlay below remains authoritative
                    // for deliberate cabin edits.
                    instance.cabin = preset.planning_cabin_config();
                    instance.landing_gear = preset.landing_gear.clone();
                    if let Some(mass_model) = &preset.mass_model {
                        instance.mass_model = mass_model.clone();
                    }
                    // A registered aircraft's declared FLOPS architecture is
                    // aircraft data with its own provenance, exactly like its
                    // tanks and its wing-box material family. It is applied
                    // after the preset's own mass model so a preset that
                    // carries both keeps both, and before the file overlay so
                    // a saved file can still change any of it.
                    if let Some(flops) = crate::preset_flops::inputs_for(name) {
                        instance.mass_model.flops_transport = flops.transport;
                        instance.mass_model.flops_structure = flops.structure;
                    }
                    if let Some(performance) = &preset.performance {
                        instance.performance = performance.clone();
                    }
                    // A registered aircraft's tank arrangement is aircraft
                    // data, not a study assumption, so it travels with the
                    // preset the same way its gear and high-lift calibrations
                    // do; the overlay below still lets a file change it.
                    if let Some(tanks) = crate::preset_fuel_tanks::layout_for(name) {
                        instance.fuel_tanks = tanks;
                    }
                    // A registered aircraft's wing-box material family is aircraft data, not a
                    // global study default: the default's CFRP spar cap lands on metallic wings
                    // that no source describes that way. Only the material family travels here;
                    // gauges and spar stations stay with the study, because no source in
                    // .agent/evidence/ establishes them and the spars bound the fuel tank box.
                    // The overlay below still lets a file change any of it.
                    if let Some(structures) = crate::preset_structures::config_for(name) {
                        instance.structures = structures;
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

        // `MassModelConfig`'s serde representation repairs its derived group
        // selectors whenever a current architecture field is present.  The
        // overlay starts from the current defaults, so capture a pre-version-2
        // file's selectors before that repair; otherwise an old all-legacy
        // file would look like an old all-FLOPS file and the migration message
        // would be wrong.
        let legacy_group_selection = data
            .get("mass_model")
            .filter(|mass_model| mass_model.get("mass_architecture").is_none())
            .filter(|mass_model| {
                mass_model
                    .get("schema_version")
                    .and_then(serde_json::Value::as_u64)
                    .is_none_or(|version| version < crate::MASS_MODEL_SCHEMA_VERSION as u64)
            })
            .map(|mass_model| {
                (
                    mass_model
                        .get("systems_mass_method")
                        .cloned()
                        .and_then(|value| serde_json::from_value(value).ok())
                        .unwrap_or_default(),
                    mass_model
                        .get("structural_mass_method")
                        .cloned()
                        .and_then(|value| serde_json::from_value(value).ok())
                        .unwrap_or_default(),
                    mass_model
                        .get("propulsion_mass_method")
                        .cloned()
                        .and_then(|value| serde_json::from_value(value).ok())
                        .unwrap_or_default(),
                )
            });

        let mut loaded: Self = overlay(&instance, data)?;
        // The overlay merges `data` onto the *serialized defaults*, so the
        // schema version the merged document carries is this build's, not the
        // file's. Read the file's own claim instead, from `data` directly: a
        // `mass_model` block with no `schema_version` is by definition one
        // written before the version existed, and a file with no `mass_model`
        // block at all states no mass method to migrate.
        loaded.mass_model.schema_version =
            data.get("mass_model")
                .map_or(crate::MASS_MODEL_SCHEMA_VERSION, |mass_model| {
                    // The visible architecture field is the current UI
                    // selection. `schema_version` is hidden from that form, so
                    // its omission cannot turn an explicit legacy comparison
                    // choice back into a version-1 migration.
                    if mass_model.get("mass_architecture").is_some() {
                        crate::MASS_MODEL_SCHEMA_VERSION
                    } else {
                        mass_model
                            .get("schema_version")
                            .and_then(serde_json::Value::as_u64)
                            .and_then(|version| u32::try_from(version).ok())
                            .unwrap_or_else(crate::mass_architecture::legacy_schema_version)
                    }
                });
        if let Some((systems, structure, propulsion)) = legacy_group_selection {
            loaded.mass_model.mass_architecture = crate::MassArchitecture::default();
            loaded.mass_model.systems_mass_method = systems;
            loaded.mass_model.structural_mass_method = structure;
            loaded.mass_model.propulsion_mass_method = propulsion;
        }
        // The overlay is the last thing that can name a mass method, so the
        // architecture is reconciled here rather than in `MassModelConfig`'s
        // `Deserialize`: a version-1 file's three group selectors only mean
        // something once the preset defaults underneath them have been
        // applied. A caller that supplies the current architecture field
        // without the hidden schema-version field is already making an
        // explicit version-2 selection (as the settings form does), so it
        // must remain selectable rather than being mistaken for an old file.
        let migration = loaded.mass_model.normalize_architecture();
        Ok((loaded, migration))
    }

    /// The maximum landing mass to enforce for `candidate_mtow_kg`.
    ///
    /// In [`crate::optimizer::DesignMode::BaselineSandbox`], a valid declared
    /// reference MLW -- the registered preset's own certified limit -- governs
    /// as a fixed aircraft limit and does not scale with `candidate_mtow_kg`:
    /// BaselineSandbox replays that certified airframe unchanged, so its
    /// certified MLW does not move because a candidate MTOW does.
    /// [`crate::optimizer::DesignMode::ReferenceAdaptation`] applies the same
    /// declared value, but there it is a *reference-aircraft* limit rather
    /// than a certification of the adapted design: that mode may move some
    /// design variables inside configured windows, so the aircraft actually
    /// being evaluated is no longer necessarily the certified article the MLW
    /// was published for. Using the reference value there models "hold to the
    /// envelope of the aircraft this design is adapted from," not "this
    /// modified design is certified to that mass." In
    /// [`crate::optimizer::DesignMode::CleanSheet`], where there is no
    /// reference airframe to anchor to at all, the configured mass-model
    /// fraction of `candidate_mtow_kg` is used for sizing instead. A preset
    /// with no declared MLW, or no preset at all, falls back to the fraction
    /// in every mode; nothing here invents a value the registry does not
    /// carry.
    ///
    /// No configuration field currently lets an explicit TLAR override this
    /// (see [`DesignRequirements`]); if one is added later it must be checked
    /// ahead of the design-mode branch below, not folded into either fallback.
    pub fn landing_mass_limit_kg(&self, candidate_mtow_kg: f64) -> f64 {
        use crate::optimizer::DesignMode;

        let fraction_limit_kg = candidate_mtow_kg * self.mass_model.mlw_fraction_mtow;
        match self.optimizer.design_space.mode {
            DesignMode::CleanSheet => fraction_limit_kg,
            DesignMode::BaselineSandbox | DesignMode::ReferenceAdaptation => self
                .declared_reference_mlw_kg()
                .unwrap_or(fraction_limit_kg),
        }
    }

    /// The registered preset's own declared MLW, if it names one and it is a
    /// finite positive mass.
    fn declared_reference_mlw_kg(&self) -> Option<f64> {
        let mlw_kg = crate::presets::get(&self.preset).ok()?.reference.mlw_kg?;
        is_valid_declared_mass_kg(mlw_kg).then_some(mlw_kg)
    }

    /// This configuration's saved mass model, with `mlw_fraction_mtow`
    /// replaced by the ratio that reproduces [`Self::landing_mass_limit_kg`]
    /// for `candidate_mtow_kg`.
    ///
    /// Mass APIs below `AlasConfig` (e.g.
    /// `run_mass_analysis_with_model_checked_product_with_gear`) take a
    /// [`MassModelConfig`] and read a landing-mass fraction straight out of
    /// it; they have no `DesignMode`/preset identity to resolve a reference
    /// MLW themselves. `mlw_fraction_mtow` is this method's only way to carry
    /// [`Self::landing_mass_limit_kg`]'s mode-aware result through that
    /// existing slot -- it is not a new scalable design fraction. In
    /// BaselineSandbox/ReferenceAdaptation it is a fixed reference MLW
    /// divided by whatever `candidate_mtow_kg` is, so it changes if
    /// `candidate_mtow_kg` does and must be recomputed per call rather than
    /// cached. Every other field, and the caller's own saved configuration,
    /// is returned unchanged.
    ///
    /// For a non-finite or non-positive `candidate_mtow_kg` the configured
    /// fraction is left as-is: no ratio can be formed from it, and inventing
    /// one would manufacture a finite result for an input the caller's own
    /// design-requirements validation should already have rejected before it
    /// reaches a mass call.
    pub fn analysis_mass_model(&self, candidate_mtow_kg: f64) -> MassModelConfig {
        let mut model = self.mass_model.clone();
        if is_valid_declared_mass_kg(candidate_mtow_kg) {
            model.mlw_fraction_mtow =
                self.landing_mass_limit_kg(candidate_mtow_kg) / candidate_mtow_kg;
        }
        model
    }
}

/// Whether a declared reference mass (MLW, MTOW, ...) is usable as a fixed
/// limit rather than treated as missing data.
fn is_valid_declared_mass_kg(mass_kg: f64) -> bool {
    mass_kg.is_finite() && mass_kg > 0.0
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn baseline_sandbox_uses_the_declared_reference_mlw_not_the_fraction() {
        let mut config = AlasConfig::from_value(&json!({"preset": "ATR72-600"})).unwrap();
        config.optimizer.design_space.mode = crate::optimizer::DesignMode::BaselineSandbox;
        assert_eq!(config.requirements.mtow_kg, 23_000.0);
        // The generic fraction would give 23_000 * 0.92 = 21_160, not the
        // preset's declared certified MLW of 22_350.
        assert_eq!(
            config.landing_mass_limit_kg(config.requirements.mtow_kg),
            22_350.0
        );
    }

    #[test]
    fn reference_adaptation_also_uses_the_declared_reference_mlw() {
        let mut config = AlasConfig::from_value(&json!({"preset": "ATR72-600"})).unwrap();
        config.optimizer.design_space.mode = crate::optimizer::DesignMode::ReferenceAdaptation;
        assert_eq!(
            config.landing_mass_limit_kg(config.requirements.mtow_kg),
            22_350.0
        );
    }

    #[test]
    fn every_preset_with_a_declared_mlw_resolves_to_it_in_reference_modes() {
        for preset in crate::presets::registry() {
            let Some(mlw_kg) = preset.reference.mlw_kg else {
                continue;
            };
            let mut config = AlasConfig::from_value(&json!({"preset": preset.name})).unwrap();
            for mode in [
                crate::optimizer::DesignMode::BaselineSandbox,
                crate::optimizer::DesignMode::ReferenceAdaptation,
            ] {
                config.optimizer.design_space.mode = mode;
                assert_eq!(
                    config.landing_mass_limit_kg(config.requirements.mtow_kg),
                    mlw_kg,
                    "{} in {mode:?}",
                    preset.name
                );
            }
        }
    }

    #[test]
    fn clean_sheet_mode_keeps_the_mass_model_fraction_even_with_a_declared_mlw() {
        let mut config = AlasConfig::from_value(&json!({"preset": "ATR72-600"})).unwrap();
        config.optimizer.design_space.mode = crate::optimizer::DesignMode::CleanSheet;
        let candidate_mtow_kg = 24_000.0;
        assert_eq!(
            config.landing_mass_limit_kg(candidate_mtow_kg),
            candidate_mtow_kg * config.mass_model.mlw_fraction_mtow
        );
    }

    #[test]
    fn a_reference_mode_certified_limit_does_not_move_with_the_candidate_mtow() {
        let mut config = AlasConfig::from_value(&json!({"preset": "ATR72-600"})).unwrap();
        config.optimizer.design_space.mode = crate::optimizer::DesignMode::BaselineSandbox;
        // A search candidate's MTOW must not scale a real airframe's
        // certified landing-mass limit the way the fraction fallback would.
        assert_eq!(config.landing_mass_limit_kg(20_000.0), 22_350.0);
        assert_eq!(config.landing_mass_limit_kg(23_000.0), 22_350.0);
        assert_eq!(config.landing_mass_limit_kg(30_000.0), 22_350.0);
    }

    #[test]
    fn a_preset_without_a_declared_mlw_falls_back_to_the_fraction_in_reference_modes() {
        // AVE is the from-scratch default entry; it declares no reference data.
        let mut config = AlasConfig::from_value(&json!({"preset": "AVE"})).unwrap();
        assert_eq!(crate::presets::get("AVE").unwrap().reference.mlw_kg, None);
        config.optimizer.design_space.mode = crate::optimizer::DesignMode::BaselineSandbox;
        let mtow_kg = config.requirements.mtow_kg;
        assert_eq!(
            config.landing_mass_limit_kg(mtow_kg),
            mtow_kg * config.mass_model.mlw_fraction_mtow
        );
    }

    #[test]
    fn an_unregistered_preset_name_falls_back_to_the_fraction_rather_than_erroring() {
        let mut config = AlasConfig {
            preset: "not-a-real-preset".to_owned(),
            ..Default::default()
        };
        config.optimizer.design_space.mode = crate::optimizer::DesignMode::BaselineSandbox;
        let mtow_kg = config.requirements.mtow_kg;
        assert_eq!(
            config.landing_mass_limit_kg(mtow_kg),
            mtow_kg * config.mass_model.mlw_fraction_mtow
        );
    }

    #[test]
    fn analysis_mass_model_in_reference_modes_carries_the_declared_mlw_through_the_fraction_slot() {
        let mut config = AlasConfig::from_value(&json!({"preset": "ATR72-600"})).unwrap();
        for mode in [
            crate::optimizer::DesignMode::BaselineSandbox,
            crate::optimizer::DesignMode::ReferenceAdaptation,
        ] {
            config.optimizer.design_space.mode = mode;
            // Two different adapted MTOWs must still imply the same 22_350 kg
            // reference landing mass once the derived fraction is applied
            // back to the candidate it was built from -- the ratio itself is
            // only the transport format, not a new scalable constraint.
            for candidate_mtow_kg in [20_000.0, 23_000.0, 30_000.0] {
                let model = config.analysis_mass_model(candidate_mtow_kg);
                assert_eq!(
                    model.mlw_fraction_mtow,
                    22_350.0 / candidate_mtow_kg,
                    "{mode:?} at {candidate_mtow_kg}"
                );
                assert!(
                    (candidate_mtow_kg * model.mlw_fraction_mtow - 22_350.0).abs() < 1e-9,
                    "{mode:?} at {candidate_mtow_kg}"
                );
            }
        }
    }

    #[test]
    fn analysis_mass_model_in_clean_sheet_keeps_the_original_fraction() {
        let mut config = AlasConfig::from_value(&json!({"preset": "ATR72-600"})).unwrap();
        config.optimizer.design_space.mode = crate::optimizer::DesignMode::CleanSheet;
        let original_fraction = config.mass_model.mlw_fraction_mtow;
        let model = config.analysis_mass_model(24_000.0);
        assert_eq!(model.mlw_fraction_mtow, original_fraction);
    }

    #[test]
    fn analysis_mass_model_changes_only_the_landing_fraction_and_leaves_the_saved_config_alone() {
        let mut config = AlasConfig::from_value(&json!({"preset": "ATR72-600"})).unwrap();
        config.optimizer.design_space.mode = crate::optimizer::DesignMode::BaselineSandbox;
        let saved_mass_model = config.mass_model.clone();
        let model = config.analysis_mass_model(config.requirements.mtow_kg);
        assert_eq!(
            model,
            MassModelConfig {
                mlw_fraction_mtow: model.mlw_fraction_mtow,
                ..saved_mass_model.clone()
            }
        );
        // The call is read-only: the config's own saved mass model is
        // unchanged afterwards.
        assert_eq!(config.mass_model, saved_mass_model);
    }

    #[test]
    fn analysis_mass_model_with_an_invalid_candidate_leaves_the_configured_fraction() {
        let mut config = AlasConfig::from_value(&json!({"preset": "ATR72-600"})).unwrap();
        config.optimizer.design_space.mode = crate::optimizer::DesignMode::BaselineSandbox;
        let original_fraction = config.mass_model.mlw_fraction_mtow;
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -1.0] {
            let model = config.analysis_mass_model(invalid);
            assert_eq!(model.mlw_fraction_mtow, original_fraction, "{invalid}");
        }
    }

    #[test]
    fn non_finite_or_non_positive_declared_masses_are_invalid() {
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -1.0] {
            assert!(!is_valid_declared_mass_kg(invalid), "{invalid}");
        }
        assert!(is_valid_declared_mass_kg(22_350.0));
    }

    #[test]
    fn an_empty_file_is_the_defaults_rather_than_an_error() {
        assert_eq!(
            AlasConfig::from_value(&json!({})).unwrap(),
            AlasConfig::default()
        );
    }

    #[test]
    fn a_visible_architecture_selection_without_the_hidden_schema_version_stays_explicit() {
        let (config, migration) = AlasConfig::from_value_with_migration(&json!({
            "mass_model": {
                "mass_architecture": "legacy_reference_compatible_comparison"
            }
        }))
        .expect("the visible mass architecture is a valid overlay");
        assert_eq!(migration, crate::MassArchitectureMigration::None);
        assert_eq!(
            config.mass_model.mass_architecture,
            crate::MassArchitecture::LegacyReferenceCompatibleComparison
        );
        assert!(config.mass_model.architecture_is_coherent());
    }

    #[test]
    fn a_preset_replaces_the_geometry_and_the_requirements() {
        let config = AlasConfig::from_value(&json!({"preset": "A380-800"})).unwrap();
        let preset = crate::presets::get("A380-800").unwrap();
        let mut expected_geometry = preset.geometry.clone();
        expected_geometry.engine.apply_engine_spec();
        assert_eq!(config.geometry, expected_geometry);
        assert_eq!(config.requirements, preset.requirements);
        assert_eq!(config.landing_gear, preset.landing_gear);
        assert_eq!(config.preset, "A380-800");
    }

    #[test]
    fn a_preset_loads_a_representable_planning_cabin_without_changing_the_brief() {
        let narrowbody = AlasConfig::from_value(&json!({"preset": "A320-200"})).unwrap();
        assert_eq!(narrowbody.requirements.num_passengers, 150);
        assert_eq!(narrowbody.cabin.passenger.business.share_pct, 0.0);
        assert_eq!(narrowbody.cabin.passenger.economy.share_pct, 100.0);

        let widebody = AlasConfig::from_value(&json!({"preset": "B787-9"})).unwrap();
        assert_eq!(widebody.cabin.passenger.business.share_pct, 15.0);
        assert_eq!(widebody.cabin.passenger.economy.share_pct, 85.0);

        let regional = AlasConfig::from_value(&json!({"preset": "ATR72-600"})).unwrap();
        assert_eq!(regional.requirements.num_passengers, 72);
        assert_eq!(regional.cabin.passenger.min_exit_pair_spacing_m, 9.5);
    }

    #[test]
    fn a_preset_compatibility_calibrations_are_applied_on_the_headless_path_too() {
        // The graphical front end applies these historical comparison values
        // on selection, which makes them look like an interface concern. They
        // remain serialized for the explicit reference-compatible path; pure
        // FLOPS uses the preset's declared transport and structure inputs.
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
        let operational = preset.operational_mission_defaults();
        let atmosphere = alas_atmo::Atmosphere::new(operational.cruise_altitude_m);
        let expected = operational.cruise_mach * atmosphere.speed_of_sound();
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
    fn a_preset_loads_its_declared_pure_flops_inputs() {
        let config = AlasConfig::from_value(&json!({"preset": "B787-9"})).unwrap();
        let declared = crate::preset_flops::inputs_for("B787-9").expect("787 FLOPS inputs");
        assert_eq!(
            config.mass_model.mass_architecture,
            crate::MassArchitecture::PureFlopsTransportV1
        );
        assert_eq!(config.mass_model.flops_transport, declared.transport);
        assert_eq!(config.mass_model.flops_structure, declared.structure);
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
        assert_eq!(names.len(), 19);
        assert!(names.contains(&"fuel_policy"));
        assert!(names.contains(&"fuel_tanks"));
    }

    #[test]
    fn both_default_aerodromes_are_ones_the_table_carries() {
        // The field-performance check looks them up by display name, and a
        // default that is not in the table would fail on an unconfigured run.
        let config = AlasConfig::default();
        assert!(crate::airports::get(&config.departure_airport).is_ok());
        assert!(crate::airports::get(&config.arrival_airport).is_ok());
    }

    #[test]
    fn preset_selection_binds_preset_structures_config() {
        let a320 = AlasConfig::from_value(&json!({"preset": "A320-200"})).unwrap();
        assert_eq!(a320.structures.spar_cap_material, "Al 7075-T6");
        assert_eq!(a320.structures.skin_material, "Al 7075-T6");

        let b787 = AlasConfig::from_value(&json!({"preset": "B787-9"})).unwrap();
        assert_eq!(b787.structures.skin_material, "CFRP QI");
        assert_eq!(b787.structures.spar_web_material, "CFRP QI");
        assert_eq!(b787.structures.spar_cap_material, "CFRP QI");

        let ave = AlasConfig::from_value(&json!({"preset": "AVE"})).unwrap();
        assert_eq!(ave.structures, StructuresConfig::default());
    }

    #[test]
    fn explicit_structures_overlay_overrides_preset_structures() {
        let custom = AlasConfig::from_value(&json!({
            "preset": "A320-200",
            "structures": {
                "skin_material": "Ti-6Al-4V"
            }
        }))
        .unwrap();
        assert_eq!(custom.structures.skin_material, "Ti-6Al-4V");
        assert_eq!(custom.structures.spar_cap_material, "Al 7075-T6");
    }
}
