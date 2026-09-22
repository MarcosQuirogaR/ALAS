// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/payload.py (`apply_cabin_preset`)
// Reference: alas @ rust-port-baseline.

//! Named cabin layouts, and what selecting one writes into the configuration.
//!
//! A cabin preset is not a saved seat count: seats depend on pitch, seats
//! abreast and the real fuselage, so a preset states the *seat geometry* of
//! each class and the share of cabin length it gets, and the counts are solved
//! from those against whatever body the design vector produced. Selecting
//! "Iberia" on a narrowbody therefore gives a sensible narrowbody three-class
//! cabin rather than a 787's seat count in an A320.
//!
//! The three passenger presets are named after the operators whose published
//! layouts they reproduce, and their seat geometry is what makes them differ:
//! high-density economy comes from a tight pitch, not from an extra seat
//! abreast, since real high-density operators keep the aircraft's normal
//! abreast; and lie-flat business is declared as one-two-one rather than left
//! to the automatic width calculation, which would pack six abreast into the
//! same floor and roughly double the seat count.
//!
//! # References
//!
//! Economy's 0.7112 m (28 in) pitch is the regulatory and industry floor. The
//! 0.46 m seat footprint including armrests is the realistic minimum: a
//! narrower one lets the automatic calculation squeeze a tenth seat into a 787
//! that the real fuselage cannot fit.

use alas_config::{AlasConfig, CertifiedExitLayout, DesignVector};
use alas_geom::builder::{AircraftBuilder, BuildError};

use super::{
    registered_source_capacity_cap, registered_source_exit_layout, simulate_passenger_counts,
    simulate_passenger_counts_for_seat_mix_with_source_cap, PassengerCounts,
};
use crate::cargo::CargoLoadManager;
use crate::geometry::{CabinGeometry, CabinGeometryError};

/// What a first-time "Custom" freighter is filled to, as a fraction of the
/// positions' geometric capacity.
const CUSTOM_CARGO_FILL: f64 = 0.70;
/// The same fraction, as the "Dense payload" preset uses it.
const DENSE_PAYLOAD_FILL: f64 = 0.70;

/// A cabin preset that could not be applied.
#[derive(Debug, thiserror::Error)]
pub enum CabinPresetError {
    /// The temporary aircraft the preset sizes against did not build.
    #[error(transparent)]
    Build(#[from] BuildError),
    /// That aircraft has no cabin frame to lay a preset out in.
    #[error(transparent)]
    Geometry(#[from] CabinGeometryError),
}

/// Apply the selected cabin preset, writing the per-class seat counts and the
/// requirements' payload back into `config`.
///
/// A name no branch matches returns without touching anything, which is what
/// keeps a saved file naming a preset this build no longer ships from emptying
/// the cabin it describes.
///
/// # Errors
///
/// [`CabinPresetError`], when the temporary aircraft a preset sizes against
/// cannot be built or sampled.
pub fn apply_cabin_preset(
    config: &mut AlasConfig,
    design_vector: Option<&DesignVector>,
) -> Result<(), CabinPresetError> {
    apply_cabin_preset_with_semantics(
        config,
        design_vector,
        CabinPresetSemantics::RequirementsFirst,
    )
}

/// Apply a cabin preset with the frozen Python port's capacity semantics.
///
/// This boundary exists only for parity evidence generated before fixed
/// requirements-first passenger load cases were introduced. Product callers
/// must use [`apply_cabin_preset`], which treats an explicit passenger target
/// as authoritative.
///
/// # Errors
///
/// [`CabinPresetError`], when the temporary aircraft a preset sizes against
/// cannot be built or sampled.
pub fn apply_cabin_preset_reference_compatibility(
    config: &mut AlasConfig,
    design_vector: Option<&DesignVector>,
) -> Result<(), CabinPresetError> {
    apply_cabin_preset_with_semantics(config, design_vector, CabinPresetSemantics::FrozenPython)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CabinPresetSemantics {
    RequirementsFirst,
    FrozenPython,
}

impl CabinPresetSemantics {
    const fn uses_reference_geometry(self) -> bool {
        matches!(self, Self::FrozenPython)
    }
}

fn apply_cabin_preset_with_semantics(
    config: &mut AlasConfig,
    design_vector: Option<&DesignVector>,
    semantics: CabinPresetSemantics,
) -> Result<(), CabinPresetError> {
    if semantics == CabinPresetSemantics::RequirementsFirst
        && config.requirements.aircraft_type == "passenger"
        && config.cabin.passenger.class_mix_mode == "count"
    {
        // Count mode is an installed-cabin declaration. Preserve it across
        // direct preset calls as well as the geometry-boundary call below;
        // otherwise a named preset can replace user counts before payload or
        // the optimizer gets a chance to resolve the canonical FLOPS split.
        config.cabin.passenger = config.cabin.passenger.canonicalized_for_product();
        if config.cabin.passenger.total_seats() > 0 {
            config.requirements.num_passengers = config.cabin.passenger.total_seats();
            return Ok(());
        }
    }
    let preset = config.requirements.cabin_preset.clone();
    if preset == "Custom" {
        return apply_custom(config, design_vector, semantics);
    }

    let cg_geom = cabin_geometry(config, design_vector, semantics)?;
    if config.requirements.aircraft_type == "cargo" {
        // A freighter preset is a payload, so the deck configuration it is a
        // payload *of* has to be settled first.
        config.cabin.cargo.use_main_deck = true;
        config.cabin.cargo.main_deck_uld = "PMC".to_owned();
        config.cabin.cargo.lower_deck_uld = "LD3".to_owned();
        config.cabin.cargo.loading_strategy = "target_cg".to_owned();

        let manager = CargoLoadManager::new(&cg_geom, config.cabin.cargo.clone());
        let capacity = manager.total_capacity();
        // A cabin preset owns the *capacity*: what this deck configuration
        // can hold, and therefore what the load case asks the hold for. It
        // deliberately never writes `requirements.cargo_objective_kg`, the
        // mass the user asked the design to match (clarified ledger App
        // Features 2, decision D10): a request a preset overwrote would not
        // be a requirement, and the objective's deviation would collapse to
        // zero on every candidate. The two quantities stay separate here and
        // are only brought together in the scoring, by
        // `DesignRequirements::cargo_target_kg`.
        match preset.as_str() {
            "Max payload" => config.requirements.cargo_payload_kg = capacity,
            "Dense payload" => {
                config.requirements.cargo_payload_kg = DENSE_PAYLOAD_FILL * capacity;
            }
            _ => {}
        }
        return Ok(());
    }

    let Some(product_mix) = passenger_preset_mix(config, &preset) else {
        return Ok(());
    };
    let source_capacity_cap =
        registered_source_capacity_cap(config, semantics.uses_reference_geometry());
    let source_exit_layout =
        registered_source_exit_layout(config, semantics.uses_reference_geometry());
    let mix = if semantics == CabinPresetSemantics::FrozenPython {
        frozen_reference_mix(&preset).unwrap_or(product_mix)
    } else {
        product_mix
    };
    // Mirroring the preset's own mix into the per-class shares is what stops
    // the cabin page showing shares that did not produce the layout it is
    // displaying.
    if semantics == CabinPresetSemantics::FrozenPython {
        set_frozen_reference_mix(config, &mix);
    } else {
        config.cabin.passenger.set_length_share_mix(&mix);
    }
    if semantics == CabinPresetSemantics::FrozenPython {
        let counts = simulate_passenger_counts(&cg_geom, &config.cabin.passenger, &mix);
        write_counts(config, counts);
    } else {
        let counts = simulate_passenger_counts_for_seat_mix_with_source_cap(
            &cg_geom,
            &config.cabin.passenger,
            &mix,
            source_capacity_cap,
            source_exit_layout,
        );
        write_counts(config, counts);
    }
    Ok(())
}

/// Historical mixes used only to replay the frozen Python evidence fixture.
fn frozen_reference_mix(preset: &str) -> Option<Vec<(&'static str, f64)>> {
    match preset {
        "Ryanair" => Some(vec![("Economy", 1.0)]),
        "Iberia" => Some(vec![
            ("Business", 0.40),
            ("Premium", 0.09),
            ("Economy", 0.51),
        ]),
        "Emirates" => Some(vec![("First", 0.09), ("Business", 0.30), ("Economy", 0.61)]),
        _ => None,
    }
}

fn set_frozen_reference_mix(config: &mut AlasConfig, mix: &[(&str, f64)]) {
    let share_of = |name: &str| {
        mix.iter()
            .find(|(other, _)| *other == name)
            .map_or(0.0, |&(_, fraction)| fraction * 100.0)
    };
    let pax = &mut config.cabin.passenger;
    pax.first.share_pct = share_of("First");
    pax.business.share_pct = share_of("Business");
    pax.premium.share_pct = share_of("Premium");
    pax.economy.share_pct = share_of("Economy");
}

/// The "Custom" branches: a percent-mode cabin re-solved from its shares, and
/// the first-time fill that keeps a newly-custom cabin from being empty.
fn apply_custom(
    config: &mut AlasConfig,
    design_vector: Option<&DesignVector>,
    semantics: CabinPresetSemantics,
) -> Result<(), CabinPresetError> {
    let source_capacity_cap =
        registered_source_capacity_cap(config, semantics.uses_reference_geometry());
    let source_exit_layout =
        registered_source_exit_layout(config, semantics.uses_reference_geometry());
    // In percent mode the shares are the input and the counts are derived, so
    // a Custom cabin has to be re-solved whenever the shares change. Returning
    // early on "already has seats", which is right in count mode, where the
    // counts *are* the input, would freeze the layout at whatever the first
    // solve produced and silently ignore every later share edit.
    // A `count` cabin with seats declared is an input, not a seed: the
    // registered or user-declared per-class counts are the cabin the case
    // is about (a source-matched 12F/138Y A320, say), and the layout seats
    // exactly those. Whether they fit is the layout's finding to report.
    if config.requirements.aircraft_type != "cargo"
        && semantics == CabinPresetSemantics::RequirementsFirst
        && config.cabin.passenger.class_mix_mode == "count"
        && config.cabin.passenger.total_seats() > 0
    {
        config.requirements.num_passengers = config.cabin.passenger.total_seats();
        return Ok(());
    }
    if config.requirements.aircraft_type != "cargo"
        && (semantics == CabinPresetSemantics::RequirementsFirst
            || config.cabin.passenger.class_mix_mode == "percent")
    {
        let mix = config.cabin.passenger.length_share_mix();
        if mix.is_empty() {
            return Ok(());
        }
        let cg_geom = cabin_geometry(config, design_vector, semantics)?;
        let counts = if semantics == CabinPresetSemantics::FrozenPython {
            simulate_passenger_counts(&cg_geom, &config.cabin.passenger, &mix)
        } else {
            simulate_passenger_counts_for_seat_mix_with_source_cap(
                &cg_geom,
                &config.cabin.passenger,
                &mix,
                source_capacity_cap,
                source_exit_layout,
            )
        };
        write_counts(config, counts);
        return Ok(());
    }

    // Custom means hand-editing the per-class counts, which this leaves alone.
    // All it does is fill in a sensible non-zero starting point the first time
    // Custom is selected with nothing configured, so switching to it never
    // leaves the cabin silently empty with no way to change it. The repeated
    // case (every optimizer evaluation) returns here before building
    // anything, and has to stay that cheap.
    let already_configured = if config.requirements.aircraft_type == "cargo" {
        config.requirements.cargo_payload_kg > 0.0
    } else {
        config.cabin.passenger.total_seats() > 0
    };
    if already_configured {
        return Ok(());
    }

    let cg_geom = cabin_geometry(config, design_vector, semantics)?;
    if config.requirements.aircraft_type == "cargo" {
        let manager = CargoLoadManager::new(&cg_geom, config.cabin.cargo.clone());
        config.requirements.cargo_payload_kg = CUSTOM_CARGO_FILL * manager.total_capacity();
    } else {
        let counts = if semantics == CabinPresetSemantics::FrozenPython {
            simulate_passenger_counts(&cg_geom, &config.cabin.passenger, &[("Economy", 1.0)])
        } else {
            simulate_passenger_counts_for_seat_mix_with_source_cap(
                &cg_geom,
                &config.cabin.passenger,
                &[("Economy", 1.0)],
                source_capacity_cap,
                source_exit_layout,
            )
        };
        config.cabin.passenger.economy.count = counts.economy;
        config.requirements.num_passengers = counts.total();
    }
    Ok(())
}

/// Write a preset's seat geometry into the four class slots and return the
/// share of cabin length each of them gets, or `None` for a name no preset
/// matches.
fn passenger_preset_mix(config: &mut AlasConfig, preset: &str) -> Option<Vec<(&'static str, f64)>> {
    let pax = &mut config.cabin.passenger;
    match preset {
        "Ryanair" => {
            pax.first.count = 0;
            pax.business.count = 0;
            pax.premium.count = 0;
            pax.economy.pitch_m = 0.7112;
            pax.economy.width_m = 0.46;
            pax.economy.mass_per_pax_kg = 80.0;
            Some(vec![("Economy", 1.0)])
        }
        "Iberia" => {
            pax.first.count = 0;

            pax.business.abreast = 4;
            pax.business.pitch_m = 1.55;
            pax.business.width_m = 0.70;
            pax.business.mass_per_pax_kg = 90.0;

            pax.premium.abreast = 7;
            pax.premium.pitch_m = 0.97;
            pax.premium.width_m = 0.52;
            pax.premium.mass_per_pax_kg = 86.0;

            pax.economy.abreast = 0;
            pax.economy.pitch_m = 0.79;
            pax.economy.width_m = 0.46;
            pax.economy.mass_per_pax_kg = 84.0;

            // Representative Iberia A350-900: 31 Business, 24 Premium
            // Economy and 293 Economy. The product intentionally exposes the
            // requested three slots only, so Premium Economy is conservatively
            // merged into Economy rather than mislabelled as First.
            Some(vec![("Business", 31.0 / 348.0), ("Economy", 317.0 / 348.0)])
        }
        "Emirates" => {
            pax.first.abreast = 4;
            pax.first.pitch_m = 2.00;
            pax.first.width_m = 0.95;
            pax.first.mass_per_pax_kg = 96.0;

            pax.business.abreast = 4;
            pax.business.pitch_m = 1.55;
            pax.business.width_m = 0.70;
            pax.business.mass_per_pax_kg = 90.0;

            pax.premium.count = 0;

            pax.economy.abreast = 0;
            pax.economy.pitch_m = 0.81;
            pax.economy.width_m = 0.46;
            pax.economy.mass_per_pax_kg = 84.0;

            // Emirates' published 519-seat, three-class A380: 14 First,
            // 76 Business and 429 Economy.
            Some(vec![
                ("First", 14.0 / 519.0),
                ("Business", 76.0 / 519.0),
                ("Economy", 429.0 / 519.0),
            ])
        }
        _ => None,
    }
}

/// Write solved counts into the four class slots and the requested passenger
/// total.
fn write_counts(config: &mut AlasConfig, counts: PassengerCounts) {
    config.cabin.passenger.first.count = counts.first;
    config.cabin.passenger.business.count = counts.business;
    config.cabin.passenger.premium.count = counts.premium;
    config.cabin.passenger.economy.count = counts.economy;
    config.requirements.num_passengers = counts.total();
}

/// Materialize a named or custom passenger mix against an already-built
/// cabin. This is the shared product boundary used by every layout consumer,
/// so previews, reports, mass/CG and pipeline runs cannot silently disagree.
pub(super) fn apply_cabin_preset_to_geometry(
    config: &mut AlasConfig,
    g: &CabinGeometry,
    source_capacity_cap: Option<i64>,
    source_exit_layout: Option<CertifiedExitLayout>,
) {
    if config.requirements.aircraft_type == "cargo" {
        return;
    }
    if config.cabin.passenger.class_mix_mode == "count" {
        config.cabin.passenger = config.cabin.passenger.canonicalized_for_product();
        if config.cabin.passenger.total_seats() > 0 {
            config.requirements.num_passengers = config.cabin.passenger.total_seats();
            return;
        }
    }
    let preset = config.requirements.cabin_preset.clone();
    if preset == "Custom"
        && config.cabin.passenger.class_mix_mode == "count"
        && config.cabin.passenger.total_seats() > 0
    {
        // Declared counts are the cabin; see `apply_custom`.
        config.requirements.num_passengers = config.cabin.passenger.total_seats();
        return;
    }
    let mix = if preset == "Custom" {
        config.cabin.passenger.length_share_mix()
    } else {
        passenger_preset_mix(config, &preset).unwrap_or_default()
    };
    if mix.is_empty() {
        return;
    }
    config.cabin.passenger.set_length_share_mix(&mix);
    let counts = simulate_passenger_counts_for_seat_mix_with_source_cap(
        g,
        &config.cabin.passenger,
        &mix,
        source_capacity_cap,
        source_exit_layout,
    );
    write_counts(config, counts);
}

/// The cabin frame of the aircraft this configuration and design vector build.
fn cabin_geometry(
    config: &AlasConfig,
    design_vector: Option<&DesignVector>,
    semantics: CabinPresetSemantics,
) -> Result<CabinGeometry, CabinPresetError> {
    let builder = if semantics.uses_reference_geometry() {
        AircraftBuilder::new_reference_compatibility(Some(config.geometry.clone()))
    } else {
        AircraftBuilder::new(Some(config.geometry.clone()))
    };
    let plane = builder.build(design_vector, false)?;
    Ok(if semantics.uses_reference_geometry() {
        CabinGeometry::new_reference_compatibility(
            &plane,
            &builder.geometry,
            config.cabin.passenger.wall_thickness_m,
        )?
    } else {
        CabinGeometry::new(
            &plane,
            &builder.geometry,
            config.cabin.passenger.wall_thickness_m,
        )?
    })
}

// These are assertions over fixtures constructed in the test itself; a failed
// unwrap or expect is the assertion failing, not a library invariant.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_passenger_preset_always_resolves_capacity_from_geometry() {
        let mut config = AlasConfig::default();
        config.requirements.cabin_preset = "Iberia".to_owned();
        config.requirements.num_passengers = 80;
        config.requirements.optimize_passenger_capacity = false;

        apply_cabin_preset(&mut config, Some(&DesignVector::default()))
            .unwrap_or_else(|error| panic!("named passenger preset applies: {error}"));

        assert_ne!(config.requirements.num_passengers, 80);
        assert_eq!(
            config.cabin.passenger.total_seats(),
            config.requirements.num_passengers
        );
        assert!((config.cabin.passenger.business.share_pct - 100.0 * 31.0 / 348.0).abs() < 1e-9);
        assert_eq!(config.cabin.passenger.premium.share_pct, 0.0);
        assert!((config.cabin.passenger.economy.share_pct - 100.0 * 317.0 / 348.0).abs() < 1e-9);
    }

    #[test]
    fn product_named_preset_preserves_a_nonempty_count_cabin() {
        let mut config = AlasConfig::default();
        config.requirements.cabin_preset = "Emirates".to_owned();
        config.cabin.passenger.class_mix_mode = "count".to_owned();
        config.cabin.passenger.first.count = 12;
        config.cabin.passenger.business.count = 24;
        config.cabin.passenger.premium.count = 6;
        config.cabin.passenger.economy.count = 138;

        apply_cabin_preset(&mut config, Some(&DesignVector::default()))
            .unwrap_or_else(|error| panic!("count cabin preset applies: {error}"));

        // Premium is folded at the product boundary, and no named-preset
        // capacity may replace the installed total.
        assert_eq!(config.cabin.passenger.first.count, 12);
        assert_eq!(config.cabin.passenger.business.count, 24);
        assert_eq!(config.cabin.passenger.premium.count, 0);
        assert_eq!(config.cabin.passenger.economy.count, 144);
        assert_eq!(config.requirements.num_passengers, 180);
    }

    #[test]
    fn airline_presets_pin_sourced_three_class_seat_shares() {
        let mut config = AlasConfig::default();
        let ryanair = passenger_preset_mix(&mut config, "Ryanair").expect("Ryanair preset");
        assert_eq!(ryanair, vec![("Economy", 1.0)]);

        let emirates = passenger_preset_mix(&mut config, "Emirates").expect("Emirates preset");
        assert_eq!(emirates.len(), 3);
        assert!((emirates[0].1 - 14.0 / 519.0).abs() < 1e-12);
        assert!((emirates[1].1 - 76.0 / 519.0).abs() < 1e-12);
        assert!((emirates[2].1 - 429.0 / 519.0).abs() < 1e-12);
    }
}
