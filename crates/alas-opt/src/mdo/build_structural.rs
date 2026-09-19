// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

fn structural_failure() -> CandidateFailure {
    CandidateFailure {
        reason: "structural_sizing",
    }
}

/// Name the typed cause of a failed product station placement.
///
/// [`product_mass_coordinates`] reports a message, so the cause a search log
/// needs — a *missing* main-gear datum against a *degenerate* geometry — is
/// already flattened by the time it arrives here. Re-resolving the same
/// stations recovers the typed [`alas_mass::stations::StationError`] without
/// restating the applicability rule that decides it, which belongs to
/// `alas_mass::stations`. This runs only on the failure path, so an accepted
/// candidate pays nothing for it.
///
/// A missing main-gear station rejects the candidate exactly as before; what
/// changes is that `OptimizationHistory::reject_reason_counts` can now
/// separate it from every other coordinate failure, which is what tells a
/// reviewer to register the aircraft's published gear stations rather than to
/// look for a geometry bug.
fn mass_coordinate_failure(config: &AlasConfig, plane: &Airplane) -> CandidateFailure {
    match alas_mass::stations::component_stations_with_gear(
        plane,
        &config.geometry,
        &config.requirements,
        &config.mass_model,
        &config.structures,
        &config.landing_gear,
    ) {
        Err(alas_mass::stations::StationError::MainGearStationNotMeasured { .. }) => {
            CandidateFailure {
                reason: "main_gear_station_not_measured",
            }
        }
        _ => CandidateFailure {
            reason: "mass_coordinates",
        },
    }
}

/// Run the checked pure-FLOPS product mass buildup and independently evaluate
/// the structural wing-sizing diagnostic.  The structural result is retained
/// for feasibility and reporting, but it never replaces the FLOPS wing mass:
/// replacing one group after the buildup would create a hybrid mass model and
/// would make the fuel remainder compensate for a second, unrelated wing
/// estimate.
///
/// The lumped group points are then placed by
/// [`alas_mass::product_stations::product_mass_coordinates`], the same
/// authoritative product placement `alas-pipeline`'s final report uses.  The
/// product mass path is evaluated with the structural-wingbox coordinate
/// model, while the structural sizing result remains a separate diagnostic.
pub(crate) fn mass_analysis_with_structural_feedback(
    config: &AlasConfig,
    dv: &DesignVector,
    plane: &Airplane,
    payload_summary: Option<&PayloadLayoutSummary>,
    reference: Option<ReferenceWingMass>,
) -> Result<StructuralMassAnalysis, CandidateFailure> {
    // The mission-sized optimizer is the production path.  The legacy
    // architecture has a separate explicit comparison constructor and must
    // never enter this evaluator through a silent branch.
    if !config.mass_model.mass_architecture.is_production() {
        return Err(CandidateFailure {
            reason: "legacy_mass_architecture",
        });
    }
    let analysis_mass_model = config.analysis_mass_model(config.requirements.mtow_kg);
    let (masses, coords, _) = run_mass_analysis_with_model_checked_product_with_gear(
        plane,
        &config.requirements,
        &config.geometry,
        &config.cabin,
        &config.control_surfaces,
        Some(&analysis_mass_model),
        payload_summary,
        MassCoordinateModel::StructuralWingbox(&config.structures),
        &config.landing_gear,
    )
    .map_err(|_| CandidateFailure {
        reason: "mass_coordinates",
    })?;

    // Structural sizing is an independent feasibility/diagnostic check.  Its
    // empirical/Torenbeek reconciliation is deliberately not applied to the
    // pure FLOPS mass ledger.
    let reconciliation = alas_mass::wing_reconciliation::reconcile(config, dv, plane, reference)
        .map_err(|_| structural_failure())?;
    let feedback = reconciliation.feedback;
    let reference = reconciliation.reference;
    let inventory = reconciliation.inventory;
    // The FLOPS buildup already closed payload and fuel against its own
    // component groups.  Keep those values untouched so the search and final
    // report share one authoritative ledger.
    let (coords, cg) = product_mass_coordinates(config, dv, plane, &masses, coords)
        .map_err(|_| mass_coordinate_failure(config, plane))?;
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

    /// An ATR-like candidate — high wing, fuselage sponsons, no registered
    /// gear-station anchor — is rejected, and the search log says *why*.
    ///
    /// The candidate was already rejected before this phase, but under the
    /// generic `mass_coordinates` label, which a reject-reason tally cannot
    /// tell apart from a degenerate geometry. The distinction matters because
    /// the two have opposite remedies: this one is closed by registering the
    /// aircraft's published gear stations, not by fixing a builder.
    #[test]
    fn a_missing_main_gear_datum_is_named_in_the_candidate_rejection() {
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "ATR72-600" }))
            .unwrap_or_else(|error| panic!("{error}"));
        // Keep this rejection fixture independent of the registered ATR
        // datum: the production preset now carries measured stations.
        config.landing_gear.reference_station_fuselage_length_m = None;
        config.landing_gear.reference_nlg_x_fraction = None;
        config.landing_gear.reference_mlg_x_fractions = None;
        let registered =
            alas_config::presets::get("ATR72-600").unwrap_or_else(|error| panic!("{error}"));
        let dv = registered.design_vector;
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&dv), true)
            .expect("the ATR builds");

        let failure = mass_analysis_with_structural_feedback(&config, &dv, &plane, None, None)
            .expect_err("an aircraft with no measured main-gear station is not evaluable");
        assert_eq!(failure.reason, "main_gear_station_not_measured");
    }

    /// The label is specific to the missing datum: a candidate whose stations
    /// resolve is not relabelled, and the clean-sheet default still evaluates.
    #[test]
    fn a_resolvable_candidate_is_not_labelled_a_missing_gear_datum() {
        let (config, dv, plane) = clean_sheet_candidate();
        mass_analysis_with_structural_feedback(&config, &dv, &plane, None, None)
            .unwrap_or_else(|failure| panic!("{}", failure.reason));
        assert_eq!(
            mass_coordinate_failure(&config, &plane).reason,
            "mass_coordinates",
            "a candidate whose stations resolve keeps the generic coordinate label"
        );
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
    fn structural_diagnostics_do_not_replace_the_pure_flops_wing_or_fuel_closure() {
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

        // The structural inventory is a diagnostic/feasibility result.  The
        // mass path must remain the pure FLOPS buildup, including its own
        // wing group, rather than replacing it with the structural estimate.
        let (pure_masses, _, _, _) = alas_mass::breakdown::run_product_mass_analysis_with_groups(
            &plane,
            &config.requirements,
            &config.geometry,
            &config.cabin,
            &config.control_surfaces,
            Some(&config.mass_model),
            None,
            MassCoordinateModel::StructuralWingbox(&config.structures),
            &config.landing_gear,
        )
        .unwrap_or_else(|error| panic!("pure FLOPS buildup: {error}"));
        assert!((masses.wing - pure_masses.wing).abs() < 1.0e-9);
        assert!((feedback.secondary_mass_kg - inventory.total_kg()).abs() < 1.0e-9);
        for axis in 0..3 {
            let moment = feedback.first_moment_kg_m[axis];
            let scale = moment.abs().max(1.0);
            // The structural feedback still closes its own diagnostic first
            // moment, independently of the FLOPS wing point published by the
            // mass ledger.
            assert!(
                (feedback.centroid_m[axis] * feedback.total_wing_mass_kg - moment).abs() / scale
                    < 1.0e-9
            );
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

        // The pure FLOPS buildup's fuel remainder closes the same ledger; the
        // structural diagnostic is not allowed to alter it.
        let oew: f64 = alas_mass::breakdown::OEW_KEYS
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
        // solve is enabled: the `enabled` flag controls the optional
        // report stage, not the acceptance gate.
        let (mut config, dv, plane) = clean_sheet_candidate();
        config.structures.spanwise_stations = 1;
        let enabled_failure = sized_primary_wing(&config, &dv, &plane, &config.requirements)
            .expect_err("one spanwise station cannot be strength-sized");
        assert_eq!(
            enabled_failure,
            alas_mass::wing_reconciliation::WingReconciliationError::StructuralSizing
        );

        config.structures.enabled = false;
        let disabled_failure = sized_primary_wing(&config, &dv, &plane, &config.requirements)
            .expect_err("disabling the downstream solve does not waive the strength gate");
        assert_eq!(
            disabled_failure,
            alas_mass::wing_reconciliation::WingReconciliationError::StructuralSizing
        );
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
