// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The lumped mass coordinates the product analysis balances on.
//!
//! The reference implementation places every group at a fraction of a
//! length that was chosen for one aircraft. The product analysis places
//! them at the stations the built geometry gives: the integrated wingbox
//! centroid, the tails on their own mean chords, the gear at its nose and
//! main stations, the engines at their nacelles and the fuel where the tank
//! arrangement holds it at the analyzed load. Those are the same stations
//! the item ledger is built from, so the trim anchor, the model envelope
//! and the ledger agree about where the aircraft balances.

use alas_config::design_variables::DesignVector;
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::breakdown::{calculate_physical_cg, MassBreakdown, MassCoordinates};
use alas_mass::product_stations::product_mass_coordinates;
use alas_mass::stations::{component_stations_with_gear, StationError};

use super::FullAnalysis;

/// Stable machine-readable classification of a failed product station
/// placement, in the convention
/// [`alas_mass::flops_transport::FlopsTransportUnverifiedReason::as_str`]
/// already uses for pipeline-visible blockers.
///
/// [`product_mass_coordinates`] reports a message, so by the time the failure
/// reaches the analysis the typed cause is already flattened and a reader
/// cannot tell a *missing datum* from a *degenerate geometry*. The two lead
/// to opposite actions: the first is closed by registering an aircraft's
/// published gear stations, the second by fixing a geometry builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StationPlacementFailure {
    /// This aircraft has no main-gear longitudinal station the model can
    /// supply: it registers no source station anchor and its wing root sits
    /// above the fuselage crown, so the wing-mounted fallback rule has no
    /// wing-root gear bay to place legs in.
    ///
    /// The analysis fails rather than continuing, and it fails *before* the
    /// feasibility findings, so the run produces no report at all rather than
    /// an infeasible one. That is deliberate: a report would have to publish
    /// a balance, a reaction load and a CG envelope taken about a station
    /// nothing measured.
    MainGearStationNotMeasured,
    /// Any other station placement failure: a missing lifting surface or
    /// fuselage, or a non-finite station or extent that only degenerate
    /// geometry produces.
    MassCoordinates,
}

impl StationPlacementFailure {
    /// Stable machine-readable name used by audits, tests and saved evidence.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MainGearStationNotMeasured => "main_gear_station_not_measured",
            Self::MassCoordinates => "mass_coordinates",
        }
    }

    /// Recover the typed cause of a station placement that has already
    /// failed.
    ///
    /// This re-resolves the same stations on the failure path only, so a run
    /// that places its stations pays nothing for the classification, and it
    /// asks `alas_mass::stations` rather than restating the applicability
    /// rule that decides the answer.
    #[must_use]
    pub fn classify(config: &alas_config::AlasConfig, plane: &Airplane) -> Self {
        match component_stations_with_gear(
            plane,
            &config.geometry,
            &config.requirements,
            &config.mass_model,
            &config.structures,
            &config.landing_gear,
        ) {
            Err(StationError::MainGearStationNotMeasured { .. }) => {
                Self::MainGearStationNotMeasured
            }
            _ => Self::MassCoordinates,
        }
    }
}

impl FullAnalysis {
    /// Replace the frozen group points with geometry-derived stations and
    /// recompute the centre of gravity, unless this analysis replays the
    /// reference implementation or the configuration keeps the frozen
    /// placement.
    ///
    /// The payload point is kept from `legacy`, where the detailed layout has
    /// already placed it. The fuel point is the centroid of the analyzed
    /// fuel in its tanks; when no tank can be resolved on this geometry the
    /// frozen wing point stands, because a missing tank arrangement is a
    /// reported limitation, not a reason to fail the analysis.
    pub(crate) fn station_coordinates(
        &self,
        design: &DesignVector,
        plane: &Airplane,
        masses: &MassBreakdown,
        legacy: MassCoordinates,
    ) -> Result<(MassCoordinates, [f64; 3]), String> {
        if self.reference_compatibility {
            let cg = calculate_physical_cg(masses, &legacy);
            return Ok((legacy, cg));
        }
        station_coordinates_for(&self.config, design, plane, masses, legacy)
    }
}

/// The product placement for any caller holding a configuration.
///
/// This is a thin seam over [`alas_mass::product_stations`], which is where
/// the placement itself lives so the optimizer's search-time balance gate
/// evaluates the same stations this report does.
///
/// # Errors
///
/// The message `product_mass_coordinates` reports, prefixed by the stable
/// [`StationPlacementFailure::as_str`] classification of its typed cause. The
/// prefix names *which* failure this is; the message that follows keeps the
/// evidence — for a missing main-gear station, the wing root and fuselage
/// crown heights (geometry frame, z up, m) that decided it. Neither is
/// dropped, because a classification without evidence cannot be checked and
/// evidence without a classification cannot be acted on.
pub(crate) fn station_coordinates_for(
    config: &alas_config::AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    masses: &MassBreakdown,
    legacy: MassCoordinates,
) -> Result<(MassCoordinates, [f64; 3]), String> {
    product_mass_coordinates(config, design, plane, masses, legacy).map_err(|message| {
        format!(
            "{}: {message}",
            StationPlacementFailure::classify(config, plane).as_str()
        )
    })
}

#[cfg(test)]
// The fixtures are registered presets; a failed build is the test failing,
// not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alas_config::{presets, AlasConfig};
    use alas_geom::builder::AircraftBuilder;

    fn preset_case(name: &str) -> (AlasConfig, DesignVector, Airplane) {
        let preset = presets::get(name).unwrap_or_else(|error| panic!("preset {name}: {error}"));
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
            .unwrap_or_else(|error| panic!("config {name}: {error}"));
        if name == "ATR72-600" {
            // This test owns the missing-datum path; the registered ATR has
            // measured anchors and must remain testable through the normal
            // production path elsewhere.
            config.landing_gear.reference_station_fuselage_length_m = None;
            config.landing_gear.reference_nlg_x_fraction = None;
            config.landing_gear.reference_mlg_x_fractions = None;
        }
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .unwrap_or_else(|error| panic!("geometry {name}: {error:?}"));
        (config, preset.design_vector, plane)
    }

    /// The lumped masses and frozen group points the product placement
    /// replaces, in the same coordinate model the analysis uses.
    fn lumped(config: &AlasConfig, plane: &Airplane) -> (MassBreakdown, MassCoordinates) {
        let (masses, coords, _) = alas_mass::breakdown::run_mass_analysis_with_model(
            plane,
            &config.requirements,
            &config.geometry,
            Some(&config.mass_model),
            None,
            alas_mass::breakdown::MassCoordinateModel::StructuralWingbox(&config.structures),
        )
        .expect("the lumped coordinate model resolves on a registered preset");
        (masses, coords)
    }

    #[test]
    fn a_missing_main_gear_station_is_classified_and_keeps_its_evidence() {
        // The analysis must not be able to report this as a generic
        // coordinate failure, and must not be able to drop the two heights
        // that decided it.
        let (config, design, plane) = preset_case("ATR72-600");
        assert_eq!(
            StationPlacementFailure::classify(&config, &plane),
            StationPlacementFailure::MainGearStationNotMeasured
        );

        let (masses, legacy) = lumped(&config, &plane);
        let error = station_coordinates_for(&config, &design, &plane, &masses, legacy)
            .expect_err("an ATR-like layout has no main-gear station to place");
        assert!(
            error.starts_with(StationPlacementFailure::MainGearStationNotMeasured.as_str()),
            "the classification must lead the message: {error}"
        );
        assert!(
            error.contains("no main-gear longitudinal station is available"),
            "the typed cause must survive the classification: {error}"
        );
        assert!(
            error.contains("above the fuselage crown"),
            "the evidence must survive the classification: {error}"
        );
    }

    #[test]
    fn a_low_wing_fallback_aircraft_still_places_its_stations() {
        // The presets that keep the wing-mounted fallback must be unaffected:
        // their placement succeeds, so no classification is reported at all.
        for name in ["B787-9", "DC-10"] {
            let (config, design, plane) = preset_case(name);
            assert_eq!(
                StationPlacementFailure::classify(&config, &plane),
                StationPlacementFailure::MassCoordinates,
                "{name} has a main-gear station, so nothing is refused"
            );
            let (masses, legacy) = lumped(&config, &plane);
            assert!(
                station_coordinates_for(&config, &design, &plane, &masses, legacy).is_ok(),
                "{name} must still place its product stations"
            );
        }
    }

    #[test]
    fn the_two_classifications_have_distinct_stable_names() {
        assert_eq!(
            StationPlacementFailure::MainGearStationNotMeasured.as_str(),
            "main_gear_station_not_measured"
        );
        assert_eq!(
            StationPlacementFailure::MassCoordinates.as_str(),
            "mass_coordinates"
        );
    }
}
