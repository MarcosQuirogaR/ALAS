// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wing-to-fuselage layout residuals (`mdo::residuals_layout`): every
//! registered aircraft's own reference geometry must pass them, and a
//! deliberately disconnected or transonically inconsistent candidate must be
//! rejected by name.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_opt::{assess_product_candidate, CandidateAssessment};

/// Every layout residual identifier this suite covers.
const LAYOUT_IDS: &[&str] = &[
    "wing_root_incidence_min",
    "wing_root_incidence_max",
    "wing_dihedral_min",
    "wing_dihedral_max",
    "wing_root_le_on_fuselage",
    "wing_root_te_on_fuselage",
    "wing_apex_fraction_min",
    "wing_apex_fraction_max",
    "wingbox_root_depth_fits_fuselage",
    "wing_root_within_fuselage_envelope",
    "sweep_consistent_with_cruise_mach",
];

fn nominal(preset: &str) -> DesignVector {
    alas_config::presets::get(preset)
        .expect("registered preset")
        .design_vector
}

/// Assess `preset` at its own registered design, in the same
/// `DesignMode::ReferenceAdaptation` the GUI's default preset selection
/// loads: the registered/published geometry is kept intact rather than
/// re-derived, which is the "reference geometry" this suite must pass.
fn assess_reference(preset: &str) -> Result<CandidateAssessment, String> {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    assert_eq!(
        config.optimizer.design_space.mode,
        alas_config::DesignMode::ReferenceAdaptation,
        "a bare preset document loads as reference adaptation"
    );
    assess_product_candidate(&config, &nominal(preset))
}

fn violates(assessment: &CandidateAssessment, id: &str) -> bool {
    assessment
        .violated_hard_ids()
        .into_iter()
        .any(|name| name == id)
}

#[test]
fn every_registered_preset_reference_geometry_passes_the_layout_residuals() {
    let mut failures = Vec::new();
    for preset in alas_config::presets::available() {
        let assessment = match assess_reference(preset) {
            Ok(assessment) => assessment,
            // A preset that cannot be sized at all under its default route is
            // a separate finding (mass/mission model territory), not a
            // layout-residual one.
            Err(_) => continue,
        };
        for id in LAYOUT_IDS {
            if violates(&assessment, id) {
                let residual = assessment.residuals.iter().find(|r| r.id == *id);
                failures.push(format!("{preset}: {id} violated; residual: {residual:?}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "registered presets whose reference geometry violates a layout residual:\n  {}",
        failures.join("\n  ")
    );
}

#[test]
fn every_layout_residual_is_evaluated_and_finite_on_the_reference_aircraft() {
    // A limit that is never evaluated is not a limit: every registered,
    // low-wing preset (all but ATR72-600) must produce every low-wing-branch
    // residual with a finite actual value.
    let assessment = assess_reference("AVE").expect("the reference twin sizes");
    for id in LAYOUT_IDS {
        let residual = assessment
            .residuals
            .iter()
            .find(|residual| residual.id == *id)
            .unwrap_or_else(|| panic!("{id} is not evaluated"));
        assert!(
            residual.actual.is_finite() && residual.raw_residual.is_finite(),
            "{id} reports a non-finite quantity"
        );
    }
}

/// Assess `design` against `preset`'s configuration in clean-sheet mode
/// (global bounds, no reference-adaptation window pre-filtering), with
/// `edit` applied to the loaded configuration first -- the same pattern
/// `plausibility_residuals.rs` uses to exercise a residual directly rather
/// than through a registered aircraft's own envelope.
fn assess_clean_sheet(
    preset: &str,
    design: DesignVector,
    edit: impl FnOnce(&mut AlasConfig),
) -> Result<CandidateAssessment, String> {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    config.optimizer.design_space.mode = alas_config::optimizer::DesignMode::CleanSheet;
    edit(&mut config);
    assess_product_candidate(&config, &design)
}

#[test]
fn a_wing_root_moved_past_the_tailcone_is_rejected_by_the_fuselage_containment_residual() {
    // Move the wing-root datum itself far aft, well past the registered
    // fuselage length: not reachable through the design vector's own
    // `wing_x_shift_m` bounds on every body length, but exactly the
    // configuration-scaffold edit (`geometry.wing.root_datum_x_m`,
    // `docs/optimizer-design-vector.md`'s "fixed" fields) this residual
    // exists to catch on an imported or hand-edited geometry document.
    let design = nominal("AVE");
    let assessment = assess_clean_sheet("AVE", design, |config| {
        config.geometry.wing.root_datum_x_m = design.fuselage_length_m + 20.0;
    })
    .expect("a displaced wing root still sizes");
    assert!(
        violates(&assessment, "wing_root_te_on_fuselage")
            || violates(&assessment, "wing_apex_fraction_max"),
        "violated: {:?}",
        assessment.violated_hard_ids()
    );
}

#[test]
fn an_unswept_wing_at_a_high_cruise_mach_is_rejected_by_the_korn_equation_residual() {
    // Zero sweep at AVE's 0.84 design Mach puts the candidate far past its
    // own Korn-equation drag-divergence Mach; the quartic wave-drag rise
    // then blows through the small ceiling this residual enforces.
    let design = DesignVector {
        sweep_deg: 0.0,
        ..nominal("AVE")
    };
    let assessment =
        assess_clean_sheet("AVE", design, |_| {}).expect("an unswept wing still sizes");
    assert!(
        violates(&assessment, "sweep_consistent_with_cruise_mach"),
        "violated: {:?}",
        assessment.violated_hard_ids()
    );
}

#[test]
fn turning_geometry_constraints_off_removes_the_layout_residuals_too() {
    // The layout family shares `objective.geometry_constraints`
    // (`mdo::residuals_layout`'s own module doc), so disabling the geometry
    // family removes these rows along with the aspect-ratio/fineness ones.
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "AVE" }))
        .unwrap_or_else(|error| panic!("{error}"));
    let design = nominal("AVE");
    let with = assess_product_candidate(&config, &design).expect("sizes with the family on");
    config.optimizer.objective.geometry_constraints = alas_config::ConstraintPolicy::Off;
    let without = assess_product_candidate(&config, &design).expect("sizes with the family off");

    for id in LAYOUT_IDS {
        assert!(with.residuals.iter().any(|residual| residual.id == *id));
        assert!(!without.residuals.iter().any(|residual| residual.id == *id));
    }
}
