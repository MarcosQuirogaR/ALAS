// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one seam every export, figure and diagnostic resolves landing-gear
//! stations through.
//!
//! Each of those consumers used to rebuild the model-derived fallback itself
//! (`mac_le + mlg_x_fraction_mac * MAC`, a fraction of fuselage length for the
//! nose) and hand it straight to
//! [`alas_config::LandingGearConfig::resolved_station_positions`]. That rule
//! is a *wing-mounted gear* rule with a stated domain, so every independent
//! copy of it was a place where an aircraft with no wing-root gear bay could
//! still be drawn, plotted or exported with gear at a station the mass model
//! refuses to supply.
//!
//! The applicability gate itself now lives in
//! [`alas_config::LandingGearConfig::resolved_station_positions_checked`]; the
//! geometric measurement that feeds it lives in `alas_mass::stations`, which
//! is where the wing root and the fuselage crown are already compared. This
//! module only joins the two, so neither the rule nor its boundary is
//! restated here.
//!
//! Numerically this is a no-op on every aircraft that has a main-gear
//! station: the resolved positions are exactly what
//! `resolved_station_positions` returned before, including the retained
//! low-wing fallback for the presets that use it.

use alas_config::{AlasConfig, LandingGearStationPositions, MainGearFallbackRefusal};
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::stations::{component_stations_with_gear, StationError};

/// Whether the wing-mounted main-gear fallback applies to this built
/// aircraft.
///
/// The question is put to `alas_mass::stations`, the single owner of the
/// comparison between the wing root leading edge and the fuselage crown at
/// the same longitudinal station, so this crate neither restates the rule nor
/// re-derives the fuselage loft. Only
/// [`StationError::MainGearStationNotMeasured`] is a verdict about the
/// fallback's domain; every other station failure (a missing surface, a
/// non-finite extent) is left to each consumer's existing checks, exactly as
/// before, so this gate cannot turn an unrelated geometry problem into a gear
/// refusal.
#[must_use]
pub fn wing_mounted_gear_domain(
    config: &AlasConfig,
    plane: &Airplane,
) -> alas_config::WingMountedGearDomain {
    match component_stations_with_gear(
        plane,
        &config.geometry,
        &config.requirements,
        &config.mass_model,
        &config.structures,
        &config.landing_gear,
    ) {
        Err(StationError::MainGearStationNotMeasured {
            wing_root_z_m,
            fuselage_crown_z_m,
        }) => alas_config::WingMountedGearDomain::WingRootAboveFuselageCrown {
            wing_root_z_m,
            fuselage_crown_z_m,
        },
        _ => alas_config::WingMountedGearDomain::Applicable,
    }
}

/// Resolve the longitudinal gear stations for a consumer holding built
/// geometry, refusing a wing-mounted fallback outside its domain.
///
/// The four fallback arguments stay with the caller because each consumer
/// already computes the mean aerodynamic chord and its leading edge in its
/// own frame, and moving that here would silently change the station of every
/// aircraft that keeps the fallback. What moves here is the decision about
/// whether the fallback may be used at all.
///
/// # Errors
///
/// [`MainGearFallbackRefusal`] when this aircraft registers no source station
/// anchor and its wing root sits above the fuselage crown, carrying the two
/// heights (geometry frame, z up, m) that decided it.
pub fn resolved_gear_stations(
    config: &AlasConfig,
    plane: &Airplane,
    fallback_x_nlg_m: f64,
    fallback_x_mlg_m: f64,
    fuselage_start_x_m: f64,
    fuselage_length_m: f64,
) -> Result<LandingGearStationPositions, MainGearFallbackRefusal> {
    config.landing_gear.resolved_station_positions_checked(
        fallback_x_nlg_m,
        fallback_x_mlg_m,
        fuselage_start_x_m,
        fuselage_length_m,
        || wing_mounted_gear_domain(config, plane),
    )
}

#[cfg(test)]
// The fixtures are registered presets; a failed build is the test failing,
// not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alas_config::presets;
    use alas_geom::builder::AircraftBuilder;

    fn preset_case(name: &str) -> (AlasConfig, Airplane) {
        let preset = presets::get(name).unwrap_or_else(|error| panic!("preset {name}: {error}"));
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
            .unwrap_or_else(|error| panic!("config {name}: {error}"));
        if name == "ATR72-600" {
            // Keep the refusal seam as an explicit unmeasured fixture. The
            // registered production ATR now carries its published anchors.
            config.landing_gear.reference_station_fuselage_length_m = None;
            config.landing_gear.reference_nlg_x_fraction = None;
            config.landing_gear.reference_mlg_x_fractions = None;
        }
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .unwrap_or_else(|error| panic!("geometry {name}: {error:?}"));
        (config, plane)
    }

    /// The fallback stations each consumer builds, in the convention they all
    /// share, so the test compares the seam against what they used to do.
    fn fallbacks(config: &AlasConfig, plane: &Airplane) -> (f64, f64, f64, f64) {
        let wing = plane
            .wings
            .iter()
            .find(|wing| wing.name == "Main Wing")
            .unwrap_or(&plane.wings[0]);
        let mac = plane.c_ref;
        let x_mac_le = wing.aerodynamic_center(0.25)[0] - 0.25 * mac;
        let fuselage = &plane.fuselages[0];
        let start_x = fuselage.xsecs[0].xyz_c[0];
        let end_x = fuselage.xsecs[fuselage.xsecs.len() - 1].xyz_c[0];
        (
            start_x + (end_x - start_x) * config.mass_model.nlg_x_fraction,
            x_mac_le + config.mass_model.mlg_x_fraction_mac * mac,
            start_x,
            end_x - start_x,
        )
    }

    #[test]
    fn an_aircraft_with_no_measured_main_gear_station_is_refused_at_the_shared_seam() {
        // This is the proof that no consumer routed through this module can
        // draw, plot or export gear for an ATR-like layout.
        let (config, plane) = preset_case("ATR72-600");
        let (x_nlg, x_mlg, start_x, length) = fallbacks(&config, &plane);
        let refusal = resolved_gear_stations(&config, &plane, x_nlg, x_mlg, start_x, length)
            .expect_err("an ATR-like layout has no main-gear station to resolve");
        assert!(
            refusal.wing_root_z_m > refusal.fuselage_crown_z_m,
            "the refusal must carry the wing root above the crown: {refusal}"
        );
        assert!(!wing_mounted_gear_domain(&config, &plane).applies());
    }

    #[test]
    fn low_wing_fallback_aircraft_resolve_the_station_they_already_had() {
        // B787-9 and DC-10 register no anchor and keep the wing-mounted
        // fallback. The seam must return it unchanged, bit for bit.
        for name in ["B787-9", "DC-10", "AVE"] {
            let (config, plane) = preset_case(name);
            let (x_nlg, x_mlg, start_x, length) = fallbacks(&config, &plane);
            let resolved = resolved_gear_stations(&config, &plane, x_nlg, x_mlg, start_x, length)
                .unwrap_or_else(|error| panic!("{name} must keep its fallback station: {error}"));
            let unchecked = config
                .landing_gear
                .resolved_station_positions(x_nlg, x_mlg, start_x, length);
            assert_eq!(resolved, unchecked, "{name}");
            assert!(!resolved.source_scaled, "{name}");
            assert_eq!(resolved.x_mlg_m, x_mlg, "{name}");
            assert!(
                wing_mounted_gear_domain(&config, &plane).applies(),
                "{name}"
            );
        }
    }

    #[test]
    fn source_scaled_aircraft_are_unaffected_by_the_gate() {
        for name in ["A320-200", "A220-300", "A340-300", "A380-800"] {
            let (config, plane) = preset_case(name);
            let (x_nlg, x_mlg, start_x, length) = fallbacks(&config, &plane);
            let resolved = resolved_gear_stations(&config, &plane, x_nlg, x_mlg, start_x, length)
                .unwrap_or_else(|error| panic!("{name} is source-scaled: {error}"));
            assert!(resolved.source_scaled, "{name}");
            assert_eq!(
                resolved,
                config
                    .landing_gear
                    .resolved_station_positions(x_nlg, x_mlg, start_x, length),
                "{name}"
            );
        }
    }
}
