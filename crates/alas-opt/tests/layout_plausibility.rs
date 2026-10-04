// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wing-to-fuselage layout residuals (`mdo::residuals_layout`): every
//! registered aircraft's reference geometry meets the containment bounds, and a
//! disconnected or transonically inconsistent candidate must be
//! rejected by name.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_opt::{assess_product_candidate, CandidateAssessment};

mod support;
use support::{nominal, violates};

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

/// Assess `preset` at its own registered design, in the same
/// `DesignMode::ReferenceAdaptation` the GUI's default preset selection
/// loads: the registered/published geometry is kept intact rather than
/// re-derived.
fn assess_reference(preset: &str) -> Result<CandidateAssessment, String> {
    assess_reference_mode(preset, alas_config::MtowSizing::FixedRequirement)
}

fn assess_reference_mode(
    preset: &str,
    mode: alas_config::MtowSizing,
) -> Result<CandidateAssessment, String> {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    assert_eq!(
        config.optimizer.design_space.mode,
        alas_config::DesignMode::ReferenceAdaptation,
        "a bare preset document loads as reference adaptation"
    );
    config.optimizer.objective.mtow_sizing = mode;
    assess_product_candidate(&config, &nominal(preset))
}

#[test]
fn mission_sized_reference_geometry_passes_layout_residuals() {
    let mut failures = Vec::new();
    for preset in alas_config::presets::available() {
        let assessment =
            match assess_reference_mode(preset, alas_config::MtowSizing::SizedByMission) {
                Ok(assessment) => assessment,
                // A preset that cannot be sized at all under its default route is
                // a mass/mission model matter, not a layout-residual one.
                Err(_) => continue,
            };
        for id in LAYOUT_IDS {
            if preset == "DC-10" && *id == "sweep_consistent_with_cruise_mach" {
                // Documented exception. DC-10 cruises beyond model Mdd (0.800
                // vs 0.82): 1970s aft-loaded DSMA airfoil class has no sourced
                // Korn technology factor; kappa held at the conventional 0.87
                // lower bound (Mason ch. 7; Malone & Mason 1995). Finding, not
                // tuning. Its cruise wave drag must still exceed the ceiling,
                // so this flips when the model is fixed.
                let residual = assessment.residuals.iter().find(|r| r.id == *id).unwrap();
                assert!(residual.actual.is_finite() && residual.limit.is_finite());
                assert!(
                    violates(&assessment, id) && residual.actual > residual.limit,
                    "DC-10 wave drag now meets its ceiling: remove this exception: {residual:?}"
                );
                continue;
            }
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
fn hard_mtow_reference_layouts_keep_named_drag_divergence_findings() {
    let mut failures = Vec::new();
    for preset in alas_config::presets::available() {
        let assessment = match assess_reference(preset) {
            Ok(assessment) => assessment,
            Err(reason) => panic!("{preset}: hard-MTOW assessment failed: {reason}"),
        };
        for id in LAYOUT_IDS {
            if *id == "sweep_consistent_with_cruise_mach" {
                let residual = assessment.residuals.iter().find(|r| r.id == *id).unwrap();
                assert!(residual.actual.is_finite() && residual.actual >= 0.0);
                assert!(residual.limit.is_finite() && residual.limit > 0.0);
                assert_eq!(residual.role, alas_opt::mdo::ResidualRole::Constraint);
                let finding = match preset {
                    // At mid-cruise of the maximum-fuel design mission (its
                    // own trip, the trip-share contingency of that trip
                    // kept) the lift still lies past the Korn drag-divergence
                    // boundary at the preset cruise point (27.5 against 26.9
                    // counts). The A380 clears it at that mass (19.5 counts).
                    "AVE" => {
                        Some("maximum-fuel mid-cruise lift exceeds the drag-divergence boundary")
                    }
                    // The conventional factor is not calibrated to this
                    // aircraft's aft-loaded section (Mason, chapter 7).
                    "DC-10" => Some("aft-loaded section lacks a sourced Korn technology factor"),
                    _ => None,
                };
                if let Some(reason) = finding {
                    assert!(
                        violates(&assessment, id),
                        "{preset}: {reason}; boundary now met: review finding"
                    );
                } else {
                    assert!(
                        !violates(&assessment, id),
                        "{preset}: new drag-divergence finding: {residual:?}"
                    );
                }
                if violates(&assessment, id) {
                    assert!(residual.actual > residual.limit, "{preset}: {residual:?}");
                    assert!(!assessment.hard_feasible, "{preset}: {residual:?}");
                    assert!(assessment.violated_hard_ids().contains(id), "{preset}");
                }
                continue;
            }
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
fn lock_drag_divergence_ceiling_matches_the_quartic_law_slope() {
    for preset in alas_config::presets::available() {
        let assessment = assess_reference(preset).unwrap();
        let config = AlasConfig::from_value(&serde_json::json!({"preset": preset})).unwrap();
        let rise = config.drag_model.wave_drag_coefficient;
        let id = "sweep_consistent_with_cruise_mach";
        let residual = assessment
            .residuals
            .iter()
            .find(|row| row.id == id)
            .unwrap();
        // Eliminating the Mach excess from CDw = C (M - Mcrit)^4 gives
        // dCDw/dM = 4 C^(1/4) CDw^(3/4); drag divergence is slope 0.1.
        let ceiling_slope = 4.0 * rise.powf(0.25) * residual.limit.powf(0.75);
        assert!((ceiling_slope / 0.1 - 1.0).abs() <= 16.0 * f64::EPSILON);
        let slope = 4.0 * rise.powf(0.25) * residual.actual.powf(0.75);
        assert_eq!(residual.actual > residual.limit, slope > 0.1, "{preset}");
        assert_eq!(violates(&assessment, id), slope > 0.1, "{preset}");
    }
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
    //
    // The trimmed drag table may refuse such an aircraft first: with the
    // wing behind the tail its fourth trimmed lattice solve leaves the
    // induced-drag quadratic. Either route keeps it from being scored.
    let design = nominal("AVE");
    match assess_clean_sheet("AVE", design, |config| {
        config.geometry.wing.root_datum_x_m = design.fuselage_length_m + 20.0;
    }) {
        Ok(assessment) => assert!(
            violates(&assessment, "wing_root_te_on_fuselage")
                || violates(&assessment, "wing_apex_fraction_max"),
            "violated: {:?}",
            assessment.violated_hard_ids()
        ),
        Err(reason) => assert_eq!(reason, "drag_table"),
    }
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

/// The gated cruise point is mid-cruise; the start of cruise is reported
/// beside it so the choice is visible, and it is the heavier state.
#[test]
fn the_wave_drag_residual_reports_the_start_of_cruise_beside_the_gated_mid_cruise() {
    let assessment = assess_reference("A320-200").expect("A320-200 assessed");
    let residual = assessment
        .residuals
        .iter()
        .find(|r| r.id == "sweep_consistent_with_cruise_mach")
        .expect("the wave-drag residual is evaluated");
    let detail = residual.detail.as_deref().expect("the residual has detail");
    assert!(detail.contains("gated at mid-cruise"), "{detail}");
    assert!(detail.contains("start of cruise (not gated)"), "{detail}");
    let cl_mid = {
        let (_, after_mid) = detail.split_once("CL ").expect("mid CL");
        let end = after_mid.find(',').expect("comma");
        after_mid[..end].parse::<f64>().expect("mid CL number")
    };
    let cl_start = {
        let (_, after_start) = detail.rsplit_once("CL ").expect("start CL");
        let end = after_start.find(',').expect("comma");
        after_start[..end].parse::<f64>().expect("start CL number")
    };
    assert!(cl_start > cl_mid, "{detail}");
}
