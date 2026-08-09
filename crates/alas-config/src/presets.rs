// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/presets.py
// Reference: alas @ rust-port-baseline.

//! Real aircraft, as complete starting points.
//!
//! The other three registries in this crate bundle a handful of overrides on
//! one configuration struct. An entry here is a whole aeroplane: a design
//! vector, a geometry scaffold, a set of requirements, an engine, and -- for
//! the types the global assumptions do not fit -- its own mass-model and
//! field-performance calibration. Selecting one is how a user starts from
//! something that flies rather than from a blank form, and it is how this
//! program is checked against reality: an A320 that comes out sixteen tonnes
//! heavy is a visible failure in a way that a notional design never is.
//!
//! Dimensions come from published specification sheets. Where a manufacturer
//! does not publish root and tip chords, they are estimated from wing area,
//! aspect ratio, taper and sweep by standard planform relations, which is why
//! the two chord fields of a real type are the least certain numbers here.
//!
//! # What a preset does not settle
//!
//! Two things are deliberately left to whatever consumes a preset.
//!
//! The first is the engine cycle. A preset names its engine and does not copy
//! the table entry in, so every one of them carries [`crate::EngineConfig`]'s
//! fallback cycle -- the GE9X's -- until something calls
//! [`crate::EngineConfig::apply_engine_spec`]. Upstream's geometry builder does
//! that as its first step, so nothing that goes through it ever sees the
//! stale numbers; anything that reads an unbuilt preset's thrust does. The
//! port reproduces that rather than resolving the spec at registration, since
//! resolving it here would make a preset disagree with the same preset loaded
//! from a saved file.
//!
//! Nor does a preset fit the design space it is offered in. The bounds in
//! [`crate::design_variables`] are one global set describing AVE's family, so
//! every published type here starts outside at least one of them -- an A320's
//! fuselage is twenty-eight metres shorter than the shortest the search will
//! consider. That is upstream's arrangement and not an oversight: whoever runs
//! a search narrows the bounds around the design it starts from, and the
//! optimizer reports an initial design that falls outside whatever bounds it
//! was handed.
//!
//! The second is the two per-aircraft calibrations. They are [`Option`]s, and
//! `None` means "use the global default" rather than "no calibration" --
//! [`crate::AlasConfig::from_value`] is where they are applied, because a
//! headless run that skipped them would silently revert an A220 to the
//! widebody-calibrated mass fractions its entry exists to correct.

mod narrowbody;
mod reference;
mod widebody;

use std::sync::OnceLock;

use crate::{DesignRequirements, DesignVector, GeometryConfig, MassModelConfig, PerformanceConfig};

/// An aircraft preset that was asked for and is not registered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown preset '{name}'; available: {}", available.join(", "))]
pub struct UnknownAircraftPreset {
    /// What was asked for.
    pub name: String,
    /// What there is, sorted.
    pub available: Vec<String>,
}

/// One complete aircraft configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct AircraftPreset {
    /// The key the configuration selects it by.
    pub name: &'static str,
    /// What the interface calls it.
    pub display_name: &'static str,
    /// What kind of aircraft it is, in one sentence.
    pub description: &'static str,
    /// Its position in the design space the optimizer searches.
    pub design_vector: DesignVector,
    /// Everything about its shape the design vector does not own.
    pub geometry: GeometryConfig,
    /// The mission it is sized for.
    pub requirements: DesignRequirements,
    /// Which engine it is fitted with.
    pub engine_name: &'static str,
    /// How many of them.
    pub n_engines: usize,
    /// A mass model calibrated for this type, where the global one misses.
    ///
    /// The Torenbeek fractions in [`MassModelConfig`] are calibrated around a
    /// modern widebody, and structural and furnishings mass does not scale
    /// linearly with weight: a small narrowbody carries a higher operating
    /// empty weight per unit of maximum takeoff weight than a widebody does.
    /// `None` means the global default already lands close enough.
    pub mass_model: Option<MassModelConfig>,
    /// Field-performance assumptions calibrated for this type.
    ///
    /// The high-lift system is what decides the takeoff and landing speeds,
    /// and the vortex-lattice analysis cannot see one. A widebody scored with
    /// a regional jet's flaps comes out ten to twenty knots fast on every
    /// V-speed. `None` means the global default fits.
    pub performance: Option<PerformanceConfig>,
}

impl AircraftPreset {
    /// Where each engine hangs along the span, in metres from the centerline.
    pub fn engine_spanwise_positions(&self) -> &[f64] {
        &self.geometry.engine.spanwise_positions_m
    }
}

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
fn high_lift(name: &'static str) -> Option<PerformanceConfig> {
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
        widebody::dc_10(),
    ];

    // The engine name is stated on the preset and read off the geometry, and
    // two of the seven state it in both places. Copying it down here is what
    // makes the two agree for the other five, and is upstream's `__post_init__`.
    for preset in &mut presets {
        preset.geometry.engine.engine_name = preset.engine_name.to_owned();
    }
    presets
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dropdown_lists_the_aircraft_in_registration_order() {
        assert_eq!(
            available(),
            vec!["AVE", "A340-300", "A380-800", "B787-9", "A320-200", "A220-300", "DC-10",]
        );
    }

    #[test]
    fn an_unknown_aircraft_is_an_error_that_says_what_there_is() {
        let error = get("Concorde").unwrap_err();
        assert!(error.available.contains(&"A320-200".to_owned()));
        assert!(error.to_string().contains("A320-200"));
    }

    #[test]
    fn every_preset_carries_its_engine_name_into_its_geometry() {
        // Five of the seven state it only on the preset, and everything
        // downstream reads it off the geometry.
        for preset in registry() {
            assert_eq!(
                preset.geometry.engine.engine_name, preset.engine_name,
                "{}",
                preset.name
            );
        }
    }

    #[test]
    fn a_preset_mounts_exactly_as_many_engines_as_it_claims_to_have() {
        for preset in registry() {
            assert_eq!(
                preset.engine_spanwise_positions().len(),
                preset.n_engines,
                "{}",
                preset.name
            );
        }
    }

    #[test]
    fn a_preset_still_carries_the_fallback_cycle_until_the_builder_resolves_it() {
        // Nothing applies the engine table at registration, so an A320's
        // thrust reads as a GE9X's until the geometry builder runs. Stated as
        // a test because it is surprising and load-bearing: a consumer that
        // reads an unbuilt preset's thrust gets a widebody's.
        let a320 = get("A320-200").unwrap();
        assert_eq!(a320.geometry.engine.thrust_kn, 467.0);

        let mut resolved = a320.geometry.engine.clone();
        resolved.apply_engine_spec();
        assert!(resolved.thrust_kn < 200.0, "{}", resolved.thrust_kn);
    }

    #[test]
    fn every_engine_the_presets_name_is_one_the_table_carries() {
        // The name is a selector, and one the table does not carry leaves the
        // GE9X fallback in place for good -- a silent widebody engine on
        // whatever type mistyped it.
        for preset in registry() {
            assert!(
                crate::engines::get(preset.engine_name).is_ok(),
                "{}: no engine called {}",
                preset.name,
                preset.engine_name
            );
        }
    }

    #[test]
    fn only_the_reference_twin_fits_inside_the_unnarrowed_design_space() {
        // The design space's bounds describe AVE's family and nothing else, so
        // a real type of any other size starts outside them. Stated as a test
        // because it looks like a defect and is not: whoever runs a search
        // narrows the bounds around the design it starts from.
        let inside = |preset: &AircraftPreset| {
            preset
                .design_vector
                .to_array()
                .iter()
                .zip(crate::DESIGN_VARIABLE_SPECS)
                .all(|(value, spec)| *value >= spec.lower && *value <= spec.upper)
        };
        let fitting: Vec<&str> = registry()
            .iter()
            .filter(|preset| inside(preset))
            .map(|preset| preset.name)
            .collect();
        assert_eq!(fitting, vec!["AVE"]);
    }

    #[test]
    fn only_the_types_the_global_assumptions_miss_carry_their_own_calibration() {
        // A calibration on every preset would mean the defaults describe
        // nothing; one on none of them would mean the small narrowbody comes
        // out several tonnes light.
        let calibrated: Vec<&str> = registry()
            .iter()
            .filter(|preset| preset.mass_model.is_some())
            .map(|preset| preset.name)
            .collect();
        assert_eq!(calibrated, vec!["A220-300"]);
    }

    #[test]
    fn every_preset_states_a_high_lift_system_rather_than_inheriting_one() {
        // The default performance configuration is a generic narrowbody, and
        // it fits none of these seven well enough to leave unstated.
        for preset in registry() {
            assert!(preset.performance.is_some(), "{}", preset.name);
        }
    }
}
