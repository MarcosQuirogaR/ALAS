// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The preset registry: construction order, lookup and display names.

use std::sync::OnceLock;

use super::{
    landing_reference, military, narrowbody, reference, regional, takeoff_reference, widebody,
    AircraftPreset, UnknownAircraftPreset,
};
use crate::PerformanceConfig;

/// Every registered aircraft, in the order the interface lists them.
pub fn registry() -> &'static [AircraftPreset] {
    static REGISTRY: OnceLock<Vec<AircraftPreset>> = OnceLock::new();
    REGISTRY.get_or_init(build)
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

fn build() -> Vec<AircraftPreset> {
    let mut presets = vec![
        reference::ave(),
        widebody::a340_300(),
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
        preset.geometry.engine.engine_name = preset.engine_name.to_owned();
        preset.geometry.engine.apply_engine_spec_if_uninitialized();
        landing_reference::apply(preset);
        takeoff_reference::apply(preset);
    }
    presets
}

// A test asserts on values it constructed here directly, so a failed unwrap
