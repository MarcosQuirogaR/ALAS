// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Place the sized box and the empirical non-box remainder separately.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;

use crate::wing_reconciliation::{clean_sheet_secondary, main_wing};
use crate::wingbox_feedback::{SizedWingboxMass, WingExtent};

pub(super) fn complete_group_centroid(
    config: &AlasConfig,
    plane: &Airplane,
    complete_mass_kg: f64,
    primary: SizedWingboxMass,
) -> Result<[f64; 3], String> {
    let box_mass = match primary.extent {
        WingExtent::FullWing => primary.mass_kg,
        WingExtent::SymmetricSemiWing => 2.0 * primary.mass_kg,
    };
    if !complete_mass_kg.is_finite() || complete_mass_kg <= 0.0 {
        return Err("complete wing mass must be positive and finite".to_owned());
    }
    if box_mass >= complete_mass_kg {
        // No nonnegative remainder exists. Keep a finite diagnostic position;
        // product reconciliation and delivery explicitly mark the current
        // candidate inventory incomplete; no negative remainder is placed.
        return Ok(primary.centroid_m);
    }
    let wing = main_wing(plane).ok_or("main wing is unavailable")?;
    let secondary = clean_sheet_secondary(config, wing, primary)
        .map_err(|error| format!("secondary wing position could not be placed: {error}"))?;
    // Torenbeek (1982), App. C: installed flaps/spoilers; NASA/TM-2017-219627
    // Vol. I, Eqs. 35-37: slat/aileron increments and fixed non-box structure.
    // Use those existing item proportions and declared geometric centroids
    // only to place the FLOPS remainder. FLOPS total and the sized box mass
    // are unchanged; no structural mass calibration is performed here.
    Ok(combine(
        complete_mass_kg,
        box_mass,
        primary.centroid_m,
        secondary.centroid_m,
    ))
}

fn combine(total: f64, box_mass: f64, box_cg: [f64; 3], secondary_cg: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|axis| {
        (box_mass * box_cg[axis] + (total - box_mass) * secondary_cg[axis]) / total
    })
}

#[cfg(test)]
mod tests {
    use super::combine;

    #[test]
    fn split_preserves_the_complete_wing_first_moment() {
        let total = 100.0;
        let box_mass = 60.0;
        let box_cg = [10.0, 0.0, -1.0];
        let secondary_cg = [15.0, 0.0, 0.5];
        let cg = combine(total, box_mass, box_cg, secondary_cg);
        for axis in 0..3 {
            assert_eq!(
                total * cg[axis],
                box_mass * box_cg[axis] + (total - box_mass) * secondary_cg[axis]
            );
        }
        assert!(cg[0] > box_cg[0] && cg[0] < secondary_cg[0]);
    }

    #[test]
    fn split_is_translation_invariant_and_has_the_correct_mass_limits() {
        let box_cg = [10.0, 0.0, -1.0];
        let secondary_cg = [15.0, 0.0, 0.5];
        assert_eq!(combine(100.0, 100.0, box_cg, secondary_cg), box_cg);
        assert_eq!(combine(100.0, 0.0, box_cg, secondary_cg), secondary_cg);
        let shifted = combine(
            100.0,
            60.0,
            box_cg.map(|x| x + 3.0),
            secondary_cg.map(|x| x + 3.0),
        );
        assert_eq!(
            shifted,
            combine(100.0, 60.0, box_cg, secondary_cg).map(|x| x + 3.0)
        );
    }

    #[test]
    fn product_coordinates_keep_the_box_and_secondary_first_moments_separate(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::breakdown::{run_product_mass_analysis_with_groups, MassCoordinateModel};
        use crate::wing_reconciliation::{clean_sheet_secondary, main_wing, sized_primary_wing};
        use alas_config::{presets, AlasConfig};
        use alas_geom::builder::AircraftBuilder;

        let config = AlasConfig::from_value(&serde_json::json!({"preset": "AVE"}))?;
        let design = presets::get("AVE")?.design_vector;
        let plane =
            AircraftBuilder::new(Some(config.geometry.clone())).build(Some(&design), true)?;
        let mut requirements = config.requirements.clone();
        requirements.mtow_kg = crate::wing_reconciliation::structural_design_mass_kg(&config);
        let analysis_mass = config.analysis_mass_model(requirements.mtow_kg);
        let (masses, fallback, _, _) = run_product_mass_analysis_with_groups(
            &plane,
            &requirements,
            &config.geometry,
            &config.cabin,
            &config.control_surfaces,
            Some(&analysis_mass),
            None,
            MassCoordinateModel::StructuralWingbox(&config.structures),
            &config.landing_gear,
        )?;
        let (primary, _) = sized_primary_wing(&config, &design, &plane, &requirements)?;
        let wing = main_wing(&plane).ok_or("fixture has no main wing")?;
        let secondary = clean_sheet_secondary(&config, wing, primary)?;
        let full_box_mass = 2.0 * primary.mass_kg;
        assert!(full_box_mass < masses.wing);
        let (coordinates, _) = crate::product_stations::product_mass_coordinates(
            &config, &design, &plane, &masses, fallback,
        )?;
        for axis in 0..3 {
            let moment = full_box_mass * primary.centroid_m[axis]
                + (masses.wing - full_box_mass) * secondary.centroid_m[axis];
            assert!((masses.wing * coordinates.wing[axis] - moment).abs() < 1.0e-7);
        }
        assert!((coordinates.wing[0] - primary.centroid_m[0]).abs() > 1.0e-3);
        Ok(())
    }

    #[test]
    fn a_shared_box_reproduces_both_unshared_consumers_bit_for_bit(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::breakdown::{run_product_mass_analysis_with_groups, MassCoordinateModel};
        use crate::product_stations::{
            product_component_stations, product_component_stations_with_box,
            product_mass_coordinates, product_mass_coordinates_with_box,
        };
        use crate::wing_reconciliation::{
            reconcile_with_design_box, reconcile_with_wing_mass, size_design_wing_box,
        };
        use alas_config::{presets, AlasConfig};
        use alas_geom::builder::AircraftBuilder;

        // A registered aircraft in reference adaptation and the clean-sheet
        // default: both inventory branches of the reconciliation.
        let mut reference = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"}))?;
        reference.optimizer.design_space.mode = alas_config::DesignMode::ReferenceAdaptation;
        for (config, design) in [
            (reference, presets::get("A320-200")?.design_vector),
            (AlasConfig::default(), alas_config::DesignVector::default()),
        ] {
            let plane =
                AircraftBuilder::new(Some(config.geometry.clone())).build(Some(&design), true)?;
            let analysis_mass = config.analysis_mass_model(config.requirements.mtow_kg);
            let (masses, fallback, _, _) = run_product_mass_analysis_with_groups(
                &plane,
                &config.requirements,
                &config.geometry,
                &config.cabin,
                &config.control_surfaces,
                Some(&analysis_mass),
                None,
                MassCoordinateModel::StructuralWingbox(&config.structures),
                &config.landing_gear,
            )?;
            let shared = size_design_wing_box(&config, &design, &plane)?;

            let alone = reconcile_with_wing_mass(&config, &design, &plane, None, masses.wing)?;
            let with_box = reconcile_with_design_box(&config, &plane, None, masses.wing, shared)?;
            assert_eq!(alone.feedback, with_box.feedback);
            assert_eq!(alone.reference, with_box.reference);
            assert_eq!(alone.primary_declaration, with_box.primary_declaration);
            assert_eq!(
                alone.inventory.is_complete(),
                with_box.inventory.is_complete()
            );

            assert_eq!(
                product_mass_coordinates(&config, &design, &plane, &masses, fallback)?,
                product_mass_coordinates_with_box(
                    &config, &design, &plane, &masses, fallback, &shared
                )?
            );
            let stations = product_component_stations(&config, &design, &plane, &masses)?;
            let stations_with_box =
                product_component_stations_with_box(&config, &design, &plane, &masses, &shared)?;
            assert_eq!(
                stations.wing.position_m.map(f64::to_bits),
                stations_with_box.wing.position_m.map(f64::to_bits)
            );
        }
        Ok(())
    }
}
