// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

fn structural_failure() -> CandidateFailure {
    CandidateFailure {
        reason: "structural_sizing",
    }
}

/// Run the checked product mass buildup and then replace the empirical total
/// wing item with the reconciled structural box inventory.  The replacement
/// is followed by the same payload/fuel/CG closure used by the mass crate;
/// leaving the old fuel remainder in place would make a heavier wing appear
/// to have the same takeoff mass.
///
/// The lumped group points are then placed by
/// [`alas_mass::product_stations::product_mass_coordinates`], the same
/// authoritative product placement `alas-pipeline`'s final report uses.  The
/// buildup itself is still requested with
/// [`MassCoordinateModel::ReferenceCompatibility`] because that model is what
/// supplies the *fallback* points (payload seating, and the frozen wing point
/// used when no tank arrangement resolves); every station the product
/// placement can derive from geometry overrides it.  Deriving the search's
/// balance from those frozen fractions instead was what let the optimizer
/// accept a candidate whose centre of gravity the final report placed
/// several percent of the mean aerodynamic chord further forward.
pub(crate) fn mass_analysis_with_structural_feedback(
    config: &AlasConfig,
    dv: &DesignVector,
    plane: &Airplane,
    payload_summary: Option<&PayloadLayoutSummary>,
    reference: Option<ReferenceWingMass>,
) -> Result<StructuralMassAnalysis, CandidateFailure> {
    let analysis_mass_model = config.analysis_mass_model(config.requirements.mtow_kg);
    let (mut masses, mut coords, _) = run_mass_analysis_with_model_checked_product_with_gear(
        plane,
        &config.requirements,
        &config.geometry,
        &config.cabin,
        &config.control_surfaces,
        Some(&analysis_mass_model),
        payload_summary,
        MassCoordinateModel::ReferenceCompatibility,
        &config.landing_gear,
    )
    .map_err(|_| CandidateFailure {
        reason: "mass_coordinates",
    })?;

    // The reconciliation itself lives in `alas_mass::wing_reconciliation`, so
    // `alas-pipeline`'s report publishes the same wing group this search sizes
    // against rather than the bare empirical total.
    let reconciliation = alas_mass::wing_reconciliation::reconcile(config, dv, plane, reference)
        .map_err(|_| structural_failure())?;
    let feedback = reconciliation.feedback;
    let reference = reconciliation.reference;
    let inventory = reconciliation.inventory;
    masses.wing = feedback.total_wing_mass_kg;
    coords.wing = feedback.centroid_m;
    let (masses, coords, _) = reclose_mass(masses, coords, &config.requirements, payload_summary);
    // The fuel closure above is what the tank fill is placed from, so the
    // stations are resolved after it rather than beside it.
    let (coords, cg) =
        product_mass_coordinates(config, dv, plane, &masses, coords).map_err(|_| {
            CandidateFailure {
                reason: "mass_coordinates",
            }
        })?;
    Ok((masses, coords, cg, feedback, reference, inventory))
}

/// Build the strength-sized primary box and reconcile it with either a frozen
/// empirical reference or the explicitly modelled clean-sheet movable items.
#[cfg(test)]
mod structural_tests {
    use super::*;
    use alas_config::design_variables::DesignVector;
    use alas_config::optimizer::DesignMode;
    use alas_mass::wing_reconciliation::{clean_sheet_secondary, main_wing, sized_primary_wing};
    use alas_mass::wingbox_feedback::SizedWingboxMass;

    /// The default configuration is a clean-sheet design space, so this
    /// exercises the enumerated inventory rather than a frozen reference.
    fn clean_sheet_candidate() -> (AlasConfig, DesignVector, Airplane) {
        let config = AlasConfig::default();
        assert_eq!(config.optimizer.design_space.mode, DesignMode::CleanSheet);
        build_geometry(&config, &DesignVector::default().to_array())
            .unwrap_or_else(|failure| panic!("{}", failure.reason))
    }

    #[test]
    fn the_clean_sheet_wing_inventory_is_complete_and_fully_enumerated() {
        let (config, dv, plane) = clean_sheet_candidate();
        let wing = main_wing(&plane).expect("built aircraft has a main wing");
        let (primary, _) = sized_primary_wing(&config, &dv, &plane, &config.requirements)
            .unwrap_or_else(|failure| panic!("{failure}"));
        let inventory = clean_sheet_secondary(&config, wing, primary)
            .unwrap_or_else(|failure| panic!("{failure}"));

        inventory
            .require_complete()
            .expect("the enumerated clean-sheet inventory is complete");
        assert!(inventory.items.iter().all(|item| item.mass_kg > 0.0
            && item.centroid_m.iter().all(|value| value.is_finite())
            && !item.source.is_empty()
            && !item.applicability.is_empty()));
        // Both empirical wing groups are evaluated on the same candidate and
        // both plausibility ratios are reported rather than clamped.
        let diagnostics = inventory.diagnostics;
        assert!(diagnostics.torenbeek_group_total_kg > 0.0);
        assert!(diagnostics.flops_group_total_kg > 0.0);
        assert!(diagnostics.total_to_torenbeek_group_ratio.is_finite());
        assert!(diagnostics.total_to_flops_group_ratio.is_finite());
    }

    #[test]
    fn the_inventory_reaches_the_reconciled_wing_mass_first_moment_and_fuel_closure() {
        let (config, dv, plane) = clean_sheet_candidate();
        let wing = main_wing(&plane).expect("built aircraft has a main wing");
        let (primary, _) = sized_primary_wing(&config, &dv, &plane, &config.requirements)
            .unwrap_or_else(|failure| panic!("{failure}"));
        let inventory = clean_sheet_secondary(&config, wing, primary)
            .unwrap_or_else(|failure| panic!("{failure}"));

        let (masses, _coords, _, feedback, reference, structural) =
            mass_analysis_with_structural_feedback(&config, &dv, &plane, None, None)
                .unwrap_or_else(|failure| panic!("{}", failure.reason));
        assert!(
            reference.is_none(),
            "clean sheet carries no frozen reference"
        );
        assert!(structural.is_complete());

        // The wing group the mass breakdown carries is exactly the sized box
        // plus the enumerated inventory, and its centroid is their first
        // moment divided by that mass.
        assert!((masses.wing - inventory.complete_wing_mass_kg).abs() < 1.0e-6);
        assert!((feedback.secondary_mass_kg - inventory.total_kg()).abs() < 1.0e-9);
        for axis in 0..3 {
            let moment = feedback.first_moment_kg_m[axis];
            let scale = moment.abs().max(1.0);
            // The reconciled wing centroid reproduces the reconciled moment.
            // The *published* `coords.wing` is no longer this point: the
            // product placement puts every group on its geometric station so
            // the search and the final report balance one aircraft, so the
            // reconciliation is checked at its own output.
            assert!((feedback.centroid_m[axis] * masses.wing - moment).abs() / scale < 1.0e-9);
            // The secondary part of that moment is the enumerated inventory's
            // own first moment, so no item is lost or double counted between
            // the inventory and the reconciliation.
            assert!(
                (feedback.secondary_first_moment_kg_m[axis] - inventory.first_moment_kg_m()[axis])
                    .abs()
                    / scale
                    < 1.0e-9
            );
            // Primary plus secondary closes the reconciled moment exactly.
            let primary_moment = moment - feedback.secondary_first_moment_kg_m[axis];
            assert!(
                (primary_moment + inventory.first_moment_kg_m()[axis] - moment).abs() / scale
                    < 1.0e-9
            );
        }

        // The fuel remainder is reclosed against the heavier wing, so the
        // inventory participates in the mass closure instead of sitting beside
        // it: OEW plus payload plus fuel is the takeoff-mass ceiling.
        let oew: f64 = OEW_KEYS
            .iter()
            .map(|&key| masses.get(key).unwrap_or(0.0))
            .sum();
        assert!((oew + masses.payload + masses.fuel - config.requirements.mtow_kg).abs() < 1.0e-6);
    }

    #[test]
    fn a_disabled_downstream_solve_still_sizes_the_same_wing_mass_and_centroid() {
        // `structures.enabled = false` only skips the optional downstream
        // NASTRAN/report stage (`alas_config::StructuresConfig::enabled`'s
        // help text; `alas-pipeline` is the caller that actually skips that
        // stage). The mass/CG input this module computes must be identical
        // whether or not that flag is set.
        let (mut config, dv, plane) = clean_sheet_candidate();
        let enabled = mass_analysis_with_structural_feedback(&config, &dv, &plane, None, None)
            .unwrap_or_else(|failure| panic!("{}", failure.reason));
        config.structures.enabled = false;
        let disabled = mass_analysis_with_structural_feedback(&config, &dv, &plane, None, None)
            .unwrap_or_else(|failure| panic!("{}", failure.reason));

        let (enabled_masses, enabled_coords, _, enabled_feedback, ..) = enabled;
        let (disabled_masses, disabled_coords, _, disabled_feedback, ..) = disabled;
        assert!((enabled_masses.wing - disabled_masses.wing).abs() < 1.0e-9);
        for axis in 0..3 {
            assert!((enabled_coords.wing[axis] - disabled_coords.wing[axis]).abs() < 1.0e-9);
        }
        assert!(
            (enabled_feedback.total_wing_mass_kg - disabled_feedback.total_wing_mass_kg).abs()
                < 1.0e-9
        );
    }

    #[test]
    fn a_genuinely_invalid_structure_still_rejects_regardless_of_the_downstream_solve_flag() {
        // Fewer than two spanwise stations cannot be strength-sized; this
        // must still reject the candidate whether or not the downstream
        // solve is enabled -- the `enabled` flag controls the optional
        // report stage, not the acceptance gate.
        let (mut config, dv, plane) = clean_sheet_candidate();
        config.structures.spanwise_stations = 1;
        let enabled_failure = sized_primary_wing(&config, &dv, &plane, &config.requirements)
            .expect_err("one spanwise station cannot be strength-sized");
        assert_eq!(enabled_failure, alas_mass::wing_reconciliation::WingReconciliationError::StructuralSizing);

        config.structures.enabled = false;
        let disabled_failure = sized_primary_wing(&config, &dv, &plane, &config.requirements)
            .expect_err("disabling the downstream solve does not waive the strength gate");
        assert_eq!(disabled_failure, alas_mass::wing_reconciliation::WingReconciliationError::StructuralSizing);
    }

    #[test]
    fn a_heavier_sized_box_makes_a_heavier_clean_sheet_wing() {
        let (config, dv, plane) = clean_sheet_candidate();
        let wing = main_wing(&plane).expect("built aircraft has a main wing");
        let (primary, _) = sized_primary_wing(&config, &dv, &plane, &config.requirements)
            .unwrap_or_else(|failure| panic!("{failure}"));
        let nominal = clean_sheet_secondary(&config, wing, primary)
            .unwrap_or_else(|failure| panic!("{failure}"));

        let heavier_box = SizedWingboxMass {
            mass_kg: primary.mass_kg * 1.10,
            ..primary
        };
        let heavier = clean_sheet_secondary(&config, wing, heavier_box)
            .unwrap_or_else(|failure| panic!("{failure}"));

        // The inventory is independent of the box, so the whole box increment
        // reaches the wing group; nothing cancels it.
        assert!((heavier.total_kg() - nominal.total_kg()).abs() < 1.0e-9);
        let expected = nominal.complete_wing_mass_kg + 0.10 * 2.0 * primary.mass_kg;
        assert!((heavier.complete_wing_mass_kg - expected).abs() < 1.0e-6);
    }
}
