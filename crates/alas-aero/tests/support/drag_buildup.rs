// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What `parity_drag_buildup.rs` reads: the shape of
//! `golden/aero/drag_buildup.json`.
//!
//! The per-component values are keyed by SUAVE's component tags rather than
//! positional, because that is how `drag_breakdown` is keyed upstream and
//! because the pylon entry has no geometry of its own to be positional
//! against. [`at`] is the one lookup, and it panics rather than defaulting: a
//! missing tag means the fixture and the port disagree about which components
//! exist, which is not a comparison to go on and make.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Fixture {
    pub settings: SettingsFixture,
    pub geometry: GeometryFixture,
    pub cases: Vec<CaseFixture>,
}

/// What `SUAVE.Analyses.Aerodynamics.Fidelity_Zero()` was holding.
///
/// The two efficiency factors are `Option` because `None` is their real value
/// and it selects a different branch of `induced_drag_aircraft`; deserializing
/// them as `f64` would have needed a stand-in number and lost that.
#[derive(Debug, Deserialize)]
pub struct SettingsFixture {
    pub wing_parasite_drag_form_factor: f64,
    pub fuselage_parasite_drag_form_factor: f64,
    pub viscous_lift_dependent_drag_factor: f64,
    pub trim_drag_correction_factor: f64,
    pub drag_coefficient_increment: f64,
    pub spoiler_drag_increment: f64,
    pub lift_to_drag_adjustment: f64,
    pub oswald_efficiency_factor: Option<f64>,
    pub span_efficiency: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct GeometryFixture {
    pub reference_area_m2: f64,
    pub wings: Vec<WingFixture>,
    pub fuselages: Vec<FuselageFixture>,
    pub nacelles: Vec<NacelleFixture>,
    pub network_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct WingFixture {
    pub tag: String,
    pub mean_aerodynamic_chord_m: f64,
    pub quarter_chord_sweep_rad: f64,
    pub thickness_to_chord: f64,
    pub reference_area_m2: f64,
    pub wetted_area_m2: f64,
    pub transition_x_upper: f64,
    pub transition_x_lower: f64,
    pub aspect_ratio: f64,
    /// How many `wing.Segments` entries the vehicle carried. The port
    /// translates only the branch taken when this is zero.
    pub segment_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct FuselageFixture {
    pub tag: String,
    pub length_m: f64,
    pub effective_diameter_m: f64,
    pub front_projected_area_m2: f64,
    pub wetted_area_m2: f64,
}

#[derive(Debug, Deserialize)]
pub struct NacelleFixture {
    pub tag: String,
    pub length_m: f64,
    pub diameter_m: f64,
    pub wetted_area_m2: f64,
    pub origin_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct CaseFixture {
    pub tag: String,
    pub mach: f64,
    pub freestream: FreestreamFixture,
    pub lift: LiftFixture,
    pub parasite: ParasiteFixture,
    pub induced: InducedFixture,
    pub compressible: CompressibleFixture,
    pub miscellaneous: MiscellaneousFixture,
    pub untrimmed: f64,
    pub trim_corrected: f64,
    pub spoiler: f64,
    pub total: f64,
}

/// The flow state the correlations were evaluated at, as SUAVE's
/// `US_Standard_1976` produced it. Read, never recomputed, see the test's
/// module documentation for what recomputing it cost.
#[derive(Debug, Deserialize)]
pub struct FreestreamFixture {
    pub temperature_k: f64,
    pub reynolds_number_per_m: f64,
    pub density_kg_m3: f64,
    pub speed_of_sound_m_s: f64,
    pub dynamic_viscosity_pa_s: f64,
    pub velocity_m_s: f64,
}

/// The vortex lattice's answer, which the drag buildup consumes rather than
/// computes.
#[derive(Debug, Deserialize)]
pub struct LiftFixture {
    pub total: f64,
    pub inviscid_wings: BTreeMap<String, f64>,
    pub compressible_wings: BTreeMap<String, f64>,
    pub inviscid_induced_wings: BTreeMap<String, f64>,
}

#[derive(Debug, Deserialize)]
pub struct ParasiteFixture {
    pub components: BTreeMap<String, f64>,
    pub skin_friction: BTreeMap<String, f64>,
    pub form_factor: BTreeMap<String, f64>,
    pub compressibility_factor: BTreeMap<String, f64>,
    pub reynolds_factor: BTreeMap<String, f64>,
    pub total: f64,
}

#[derive(Debug, Deserialize)]
pub struct InducedFixture {
    pub total: f64,
    pub viscous: f64,
    pub viscous_wings: BTreeMap<String, f64>,
}

#[derive(Debug, Deserialize)]
pub struct CompressibleFixture {
    pub wings: BTreeMap<String, f64>,
    pub crest_critical: BTreeMap<String, f64>,
    pub divergence_mach: BTreeMap<String, f64>,
    pub total: f64,
}

#[derive(Debug, Deserialize)]
pub struct MiscellaneousFixture {
    pub total_wetted_area_m2: f64,
    pub total: f64,
}

/// One component's value, by tag.
///
/// # Panics
///
/// When the fixture has no entry under `tag`, which means the two sides
/// disagree about which components the aircraft has.
pub fn at(map: &BTreeMap<String, f64>, tag: &str) -> f64 {
    *map.get(tag)
        .unwrap_or_else(|| panic!("the fixture has no entry for `{tag}`"))
}
