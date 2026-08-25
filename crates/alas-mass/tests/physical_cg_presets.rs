// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cross-preset properties of the structural main-wing mass coordinate.
//!
//! These are physical/integration checks, not a second golden fixture. Frozen
//! Python agreement remains in `parity_breakdown`; this suite proves the
//! explicit product path is finite, conservative about failure, and exercised
//! by all seven registered aircraft configurations.

// A test unwrap is the assertion failing on a registered preset or geometry.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{run_mass_analysis, run_mass_analysis_with_model, MassCoordinateModel};

#[test]
fn every_preset_uses_a_finite_structural_wing_point_inside_its_wingbox() {
    let preset_names = presets::available();
    assert_eq!(preset_names.len(), 7, "the registered preset set changed");

    for preset_name in preset_names {
        let preset = presets::get(preset_name).expect("registered preset resolves");
        let mut config = AlasConfig {
            preset: preset.name.to_owned(),
            geometry: preset.geometry.clone(),
            requirements: preset.requirements.clone(),
            landing_gear: preset.landing_gear.clone(),
            ..Default::default()
        };
        if let Some(mass_model) = &preset.mass_model {
            config.mass_model = mass_model.clone();
        }
        config.geometry.engine.apply_engine_spec();

        let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("preset geometry builds");
        let main_wing = airplane
            .wings
            .iter()
            .find(|wing| wing.name == "Main Wing")
            .expect("preset has a main wing");
        if preset_name == "A220-300" {
            // Airbus A220 Aircraft Recovery Publication
            // BD500-3AB48-10400-00, May 2026:
            // J06-20-01 puts FS0 168.0 in ahead of the nose; J08-41-03-01
            // gives LEMAC FS 818.998 in and MAC 148.86 in. The planning
            // comparison owns that source frame; model aerodynamics retain
            // their independently built planform frame.
            let planning_reference = preset
                .reference
                .planning_cg_envelope
                .expect("A220 has a published planning envelope")
                .mac_reference;
            assert_eq!(planning_reference.lemac_from_aircraft_nose_m, 16.535_349_2);
            assert_eq!(planning_reference.mean_aerodynamic_chord_m, 3.781_044);
            assert!(
                (main_wing.reference_area() - 112.3).abs() < 0.01,
                "A220-300: projected reference area must reproduce Airbus Sref at published precision"
            );
            assert_eq!(
                airplane.s_ref,
                main_wing.reference_area(),
                "aircraft s_ref must use the projected reference-plane area"
            );
            let reference_airplane =
                AircraftBuilder::new_reference_compatibility(Some(config.geometry.clone()))
                    .build(Some(&preset.design_vector), true)
                    .expect("A220 frozen reference geometry builds");
            let reference_wing = reference_airplane
                .wings
                .iter()
                .find(|wing| wing.name == "Main Wing")
                .expect("A220 frozen reference has a main wing");
            let aerodynamic_center_x = reference_wing.aerodynamic_center(0.0)[0];
            let mean_aerodynamic_chord = reference_wing.mean_aerodynamic_chord();
            assert!(
                (aerodynamic_center_x - 16.349_875_122_007_83).abs() < 1e-12,
                "A220 frozen-reference aerodynamic-center x drifted: {aerodynamic_center_x:.15}"
            );
            assert!(
                (mean_aerodynamic_chord - 3.720_242_534_584_891_3).abs() < 1e-12,
                "A220 frozen-reference MAC drifted: {mean_aerodynamic_chord:.15}"
            );
            assert!(
                (main_wing.aerodynamic_center(0.0)[0] - aerodynamic_center_x).abs() > 1e-6,
                "A220 product geometry must not silently use the frozen reference planform"
            );
            assert!(
                (main_wing.mean_aerodynamic_chord() - mean_aerodynamic_chord).abs() > 1e-6,
                "A220 product MAC must remain distinct from the frozen reference MAC"
            );
        }
        let (reference_masses, reference_coordinates, _) = run_mass_analysis(
            &airplane,
            &config.requirements,
            &config.geometry,
            Some(&config.mass_model),
            None,
        );
        let compatibility_result = run_mass_analysis_with_model(
            &airplane,
            &config.requirements,
            &config.geometry,
            Some(&config.mass_model),
            None,
            MassCoordinateModel::ReferenceCompatibility,
        )
        .expect("reference compatibility is infallible");
        assert_eq!(
            compatibility_result,
            run_mass_analysis(
                &airplane,
                &config.requirements,
                &config.geometry,
                Some(&config.mass_model),
                None,
            ),
            "{preset_name}: explicit compatibility must preserve frozen parity"
        );
        let (structural_masses, structural_coordinates, structural_cg) =
            run_mass_analysis_with_model(
                &airplane,
                &config.requirements,
                &config.geometry,
                Some(&config.mass_model),
                None,
                MassCoordinateModel::StructuralWingbox(&config.structures),
            )
            .expect("preset structural wingbox is valid");

        assert_eq!(
            structural_masses, reference_masses,
            "{preset_name}: a coordinate model must not retune mass"
        );
        assert!(
            structural_cg.into_iter().all(f64::is_finite),
            "{preset_name}: aircraft CG is finite"
        );
        assert_eq!(
            structural_coordinates.wing[1], 0.0,
            "{preset_name}: a symmetric main wing stays on the centerline"
        );

        let (spar_fractions, _) = config.structures.resolved_spars();
        let minimum_box_x_m = main_wing
            .xsecs
            .iter()
            .flat_map(|section| {
                spar_fractions
                    .iter()
                    .map(move |fraction| section.xyz_le[0] + fraction * section.chord)
            })
            .fold(f64::INFINITY, f64::min);
        let maximum_box_x_m = main_wing
            .xsecs
            .iter()
            .flat_map(|section| {
                spar_fractions
                    .iter()
                    .map(move |fraction| section.xyz_le[0] + fraction * section.chord)
            })
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(
            (minimum_box_x_m..=maximum_box_x_m).contains(&structural_coordinates.wing[0]),
            "{preset_name}: structural wing point must lie inside the lofted wingbox"
        );
        assert!(
            structural_coordinates.wing[0] < reference_coordinates.wing[0],
            "{preset_name}: the aft-biased compatibility point should move forward"
        );
    }
}

#[test]
fn published_tank_limits_are_not_inferred_for_the_notional_preset() {
    let ave = presets::get("AVE").expect("AVE is registered");
    assert_eq!(ave.reference.usable_fuel_mass_kg, None);

    for name in [
        "A340-300", "A380-800", "B787-9", "A320-200", "A220-300", "DC-10",
    ] {
        let capacity_kg = presets::get(name)
            .expect("published aircraft preset resolves")
            .reference
            .usable_fuel_mass_kg
            .expect("published aircraft has an explicit usable-fuel mass");
        assert!(capacity_kg.is_finite() && capacity_kg > 0.0, "{name}");
    }
}
