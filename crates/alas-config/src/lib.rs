// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Everything a user can tune, and the settings interface generated from it.
//!
//! The computational crates read their numbers from here and hold none of
//! their own, so there is one place to look for what a run was configured
//! with and one place a value can be changed. Each module corresponds to one
//! module of the reference implementation's `alas.config` package, keeping
//! the file boundaries so the two can be read side by side.
//!
//! # A struct describes itself
//!
//! A configuration struct derives [`ConfigNode`], which gives it a
//! [`Node`]: its fields in declaration order, each with a label, a unit, an
//! explanation, and either its value or -- for a field that is itself a
//! group -- the same description one level down. That description is what the
//! settings interface renders, so a field is documented once, next to its
//! type, rather than in a form definition somewhere else that drifts.
//!
//! The derive refuses a field that says nothing about itself. See
//! CONTRIBUTING.md: an undocumented setting reaches the user as a blank row
//! they have to guess at, and the cost of guessing wrong is a wrong aircraft.
//! The reference implementation has fields in exactly that state, whose
//! descriptions this port supplies; that adds prose and changes no number.

// The derive expands to `impl ::alas_config::ConfigNode`, which has to resolve
// inside this crate as well as outside it.
extern crate self as alas_config;

mod leaf;
mod overlay;
mod schema;

pub mod airports;
pub mod analysis;
pub mod cabin;
pub mod control_surfaces;
pub mod design_variables;
pub mod engines;
pub mod fidelity_presets;
pub mod flops_structure;
pub mod fuel_policy;
pub mod fuel_tanks;
pub mod geometry;
pub mod landing_gear;
pub mod mass;
pub mod materials;
pub mod mission;
pub mod mses;
pub mod optimizer;
pub mod performance;
pub mod performance_presets;
pub mod physics;
pub mod preset_fuel_tanks;
pub mod presets;
pub mod propulsion;
pub mod requirements;
pub mod settings;
pub mod solver_presets;
pub mod structures;
pub mod systems_mass;
pub mod validation;

pub use alas_config_derive::ConfigNode;
pub use leaf::Leaf;
pub use overlay::{overlay, OverlayError};
pub use schema::{
    Entry, Field, Kind, LeafField, Node, Number, OptionSource, ReadonlyUnless, TranslatedEntry,
    TranslatedField, TranslatedNode,
};

pub use analysis::AnalysisConfig;
pub use cabin::{CabinConfig, CargoDeckConfig, PassengerCabinConfig, SeatClassConfig};
pub use control_surfaces::ControlSurfacesConfig;
pub use design_variables::{
    DesignVariableSpec, DesignVector, DesignVectorError, SPECS as DESIGN_VARIABLE_SPECS,
};
pub use engines::{
    PropulsionTechnology, TurbofanEngineSpec, TurbofanOffDesignSpec, TurbopropEngineSpec,
};
pub use fidelity_presets::{FidelityPreset, UnknownFidelityPreset};
pub use flops_structure::{
    FlopsStructureConfig, FlopsWingBendingMethod, PropulsionMassMethod, StructuralMassMethod,
};
pub use fuel_policy::{FuelPolicyConfig, FuelScheme};
pub use fuel_tanks::{
    AuxiliaryTankConfig, CenterTankConfig, FuelTankLayoutConfig, TrimTankConfig, WingTankConfig,
};
pub use geometry::{
    ActiveEngineModel, EmpennageConfig, EngineBindingError, EngineConfig, FuselageConfig,
    GeometryConfig, InboardAerodynamicStation, MainWingPanel, MainWingStation, MainWingStationKind,
    TransportPlanform, TransportPlanformError, WingConfig,
};
pub use landing_gear::LandingGearConfig;
pub use mass::MassModelConfig;
pub use mission::{MissionConfig, MissionProfileConfig};
pub use mses::MsesConfig;
pub use optimizer::{
    ConstraintPolicy, MtowSizing, ObjectiveConfig, ObjectiveKind, ObjectiveWeights,
    OptimizerConfig, SolverSettings,
};
pub use performance::PerformanceConfig;
pub use performance_presets::{PerformancePreset, UnknownPerformancePreset};
pub use physics::DragModelConfig;
pub use presets::{
    AircraftPreset, AircraftReferenceData, AircraftVariantIdentity, CgEnvelopeCondition,
    CgEnvelopeEvidence, CgEnvelopeSource, CgEnvelopeVertex, CgLimits, DesignMissionEvidence,
    DesignMissionReference, MissingDesignMissionDatum, MissionEvidenceApplicability,
    PartialDesignMissionEvidence, PartialMissionEvidenceKind, PlanningCgEnvelope,
    PlanningMacReference, PublishedMissionLoadCase, PublishedRange, PublishedReserveContract,
    UnknownAircraftPreset,
};
pub use propulsion::PropulsionCycleConfig;
pub use requirements::{DesignRequirements, RequirementsError};
pub use settings::AlasConfig;
pub use solver_presets::{SolverPreset, UnknownSolverPreset};
pub use structures::StructuresConfig;
pub use systems_mass::{
    FlopsInputProvenance, FlopsTransportConfig, FlopsTransportProvenance, SystemsMassMethod,
};
pub use validation::{validate, Severity, ValidationIssue};

/// A configuration struct that can describe its own fields.
///
/// Implemented by `#[derive(ConfigNode)]`; there is no reason to write one by
/// hand, and one written by hand would not be checked for the documentation
/// the derive insists on.
pub trait ConfigNode {
    /// This struct's fields, in declaration order, with their current values.
    fn schema(&self) -> Node;
}
