// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The preset registry: construction order, lookup and display names.

use std::sync::OnceLock;

use super::{
    c295, clean_lift, high_lift_chord, high_lift_system, landing_reference, lj60, military,
    narrowbody, reference, regional, takeoff_reference, widebody, AircraftPreset,
    UnknownAircraftPreset,
};
use crate::PerformanceConfig;

/// Every registered aircraft, in the order the interface lists them.
pub fn registry() -> &'static [AircraftPreset] {
    static REGISTRY: OnceLock<Vec<AircraftPreset>> = OnceLock::new();
    REGISTRY.get_or_init(build)
}

/// The maximum zero-fuel mass of the aircraft `config` describes:
/// `requirements.mzfw_kg` when set, else the registered preset's published
/// value for its own variant.
///
/// The preset number belongs to the registered variant: a file that turns the preset
/// into another variant, by changing its aircraft type or by declaring more
/// structural payload than the registered one, describes an aircraft the
/// published value was never certified for and inherits none.
#[must_use]
pub fn published_mzfw_kg(config: &crate::AlasConfig) -> Option<f64> {
    // An explicit requirement is the limit of the variant flown, whatever the
    // preset or the geometry.
    if let Some(mzfw_kg) = config.requirements.mzfw_kg {
        return Some(mzfw_kg);
    }
    let preset = get(&config.preset).ok()?;
    let same_type = config.requirements.aircraft_type == preset.requirements.aircraft_type;
    let same_or_less_payload = config.requirements.max_structural_payload_kg
        <= preset.requirements.max_structural_payload_kg;
    if same_type && same_or_less_payload {
        preset.reference.mzfw_kg
    } else {
        None
    }
}

/// Look one aircraft up by name.
///
/// # Errors
///
/// [`UnknownAircraftPreset`], carrying what is available. Upstream raises for
/// the same input.
pub fn get(name: &str) -> Result<&'static AircraftPreset, UnknownAircraftPreset> {
    registry()
        .iter()
        .find(|preset| preset.name == name)
        .ok_or_else(|| UnknownAircraftPreset {
            name: name.to_owned(),
            available: sorted_names(),
        })
}

/// Every preset's name, in registration order.
pub fn available() -> Vec<&'static str> {
    registry().iter().map(|preset| preset.name).collect()
}

/// Every preset's name paired with what to call it, in registration order.
pub fn display_names() -> Vec<(&'static str, &'static str)> {
    registry()
        .iter()
        .map(|preset| (preset.name, preset.display_name))
        .collect()
}

/// The named high-lift technology level a preset is scored with.
///
/// Every preset states one, so a `None` here is a name that does not exist
/// rather than a type happy with the generic default; the registry's own
/// tests are what catch that, since falling back silently would revert a
/// widebody to a narrowbody's flaps and its V-speeds with them.
pub(super) fn high_lift(name: &'static str) -> Option<PerformanceConfig> {
    match crate::performance_presets::get(name) {
        Ok(preset) => Some(preset.settings.clone()),
        Err(error) => {
            tracing::error!(%error, "an aircraft preset names an unregistered high-lift level");
            None
        }
    }
}

fn sorted_names() -> Vec<String> {
    let mut names: Vec<String> = registry()
        .iter()
        .map(|preset| preset.name.to_owned())
        .collect();
    names.sort();
    names
}

/// Record the built reference trapezoid of the registered geometry, the
/// denominators of the `Published` reference-wing mapping.
fn register_published_reference(preset: &mut AircraftPreset) {
    let x_wing_global_m = preset.geometry.wing.root_datum_x_m + preset.design_vector.wing_x_shift_m;
    match preset
        .geometry
        .wing
        .transport_planform(&preset.design_vector)
    {
        Ok(planform) => preset
            .geometry
            .wing
            .reference_wing
            .register(&planform, x_wing_global_m),
        Err(error) => {
            tracing::error!(%error, preset = preset.name, "the registered planform does not resolve");
        }
    }
}

fn build() -> Vec<AircraftPreset> {
    let mut presets = vec![
        reference::ave(),
        widebody::a340_300(),
        widebody::a350_900(),
        widebody::a380_800(),
        widebody::b787_9(),
        narrowbody::a320_200(),
        narrowbody::a220_300(),
        regional::atr72_600(),
        widebody::dc_10(),
        narrowbody::e195_e2(),
        narrowbody::c919(),
        widebody::b747_400(),
        military::a400m(),
        lj60::lj60(),
        c295::c295(),
    ];

    // The engine name is stated on the preset and read off the geometry, and
    // two of the seven state it in both places. Copying it down here is what
    // makes the two agree for the other five, and is upstream's `__post_init__`.
    //
    // Copying the name is not enough on its own. `EngineConfig::default()` is
    // a materialized GE9X (thrust, cycle, nacelle and all) because a bare
    // config has to describe some real engine before anything reads it. A
    // preset that only renamed that default therefore declared a CFM56 and
    // was still weighed, drawn and flown as a 467 kN GE9X: the propulsion
    // group came out at 10.3 t per engine for every turbofan in the registry,
    // and the A320 carried a 2.1 m-radius nacelle. Materializing the named
    // entry here is what makes the declared engine the one the disciplines
    // actually see. `..._if_uninitialized` rather than the unconditional form
    // so the ATR, which binds PW127M in its own constructor and then moves
    // the nacelle onto its wing, keeps that installation.
    for preset in &mut presets {
        anchor_tip_rake(preset);
        preset.geometry.engine.engine_name = preset.engine_name.to_owned();
        preset.geometry.engine.apply_engine_spec_if_uninitialized();
        register_published_reference(preset);
        landing_reference::apply(preset);
        declare_main_gear_truck(preset);
        takeoff_reference::apply(preset);
        high_lift_chord::apply(preset);
        clean_lift::apply(preset);
        high_lift_system::apply(preset);
        if preset.geometry.fuselage.hold_interior_end_x_m.is_none() {
            preset.geometry.fuselage.hold_interior_end_x_m =
                super::lower_holds::declared_hold_interior_end_x_m(preset.name);
        }
        // Converted presets build their fuselage from the embedded fitted loft.
        if let Some(id) = crate::preset_loft_id(preset.name) {
            preset.geometry.fuselage.loft = Some(crate::LoftRef::embedded(id));
        }
        // Last: it reads the finished mission profile.
        apply_design_dive_speed(preset);
    }
    presets
}

/// A drawn swept-back tip was read against the preset's own straight-edge tip chord:
/// record that chord, so the rake keeps its ratio to the tip chord of any other
/// candidate and is the drawn chord, bit for bit, at the preset's own design vector.
fn anchor_tip_rake(preset: &mut AircraftPreset) {
    let tip_chord_m = preset.design_vector.tip_chord_m;
    let wing = &mut preset.geometry.wing;
    if wing.tip_rake_tip_chord_m.is_some() && wing.tip_rake_reference_tip_chord_m.is_none() {
        wing.tip_rake_reference_tip_chord_m = Some(tip_chord_m);
    }
}

/// The design dive speed of a preset whose mission profile declares a VMO (type certificate
/// data sheet or sourced ladder): CS 25.335(b) on VC = VMO and MC = MMO (CS 25.1505), the one
/// rule of [`crate::design_speeds::dive_speed_from_operating_limits_for`] for every such preset,
/// under (b)(1)(ii) where the certification basis carries a high speed protection function
/// (`reference.high_speed_protection`) and (b)(1)(i) otherwise. The value the constructor
/// declared is replaced; a preset without a VMO keeps its own declared VD, from which VC is
/// derived by the inverse of the same rule.
fn apply_design_dive_speed(preset: &mut AircraftPreset) {
    preset.requirements.high_speed_protection = preset.reference.high_speed_protection.is_some();
    let criterion = preset.requirements.upset_criterion();
    let profile = preset.operational_mission_defaults().profile;
    if let Some(speeds) =
        crate::design_speeds::dive_speed_from_operating_limits_for(&profile, criterion)
    {
        preset.requirements.dive_speed_m_s = speeds.dive_eas_m_s;
    }
    preset.requirements.max_operating_mach =
        crate::design_speeds::declared_max_operating_mach(&profile);
}

/// Copy the declared main-gear truck size (first strut of the bogie list, else
/// the uniform wheels per strut) into the performance block, where it selects
/// the braked-run deceleration of the landing-distance model.
fn declare_main_gear_truck(preset: &mut AircraftPreset) {
    let gear = &preset.landing_gear;
    let wheels = gear
        .mlg_strut_bogie_wheels
        .as_ref()
        .and_then(|counts| counts.first().copied())
        .filter(|&count| count > 0)
        .unwrap_or(gear.wheels_per_mlg_strut);
    if wheels > 0 {
        preset
            .performance
            .get_or_insert_with(Default::default)
            .main_gear_truck_wheels = wheels;
    }
}
