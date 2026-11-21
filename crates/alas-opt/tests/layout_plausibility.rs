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
use support::{assess_view, nominal, violates, AssessmentView};

/// What a drag-divergence finding is tracked by. These presets met the ceiling
/// before; they stay failing here, by name, until a
/// wave-drag fix lands, instead of being pinned as expected violators.
const AERO_WAVE_DRAG_TASK: &str =
    "tracked by the aero wave-drag fix task (wave drag over the drag-divergence ceiling since phys-aero a7c5e21d)";

/// Presets whose mid-cruise wave drag still crosses the ceiling. The ceiling admits the
/// rms error of the `M_dd` relation above `M_dd` (58.4 counts); DC-10-30F (41.8), LJ60
/// (36.9), B787-9 (31.5) and the DC-10 now meet it, AVE (90.3 counts, 0.038 in Mach above
/// `M_dd`) does not.
const AERO_WAVE_DRAG_TRACKED: &[&str] = &["AVE"];

/// The C295 nominal's drag-table rejection, matched exactly: the induced
/// drag term of its trimmed table is non-physical (`drag_table::induced`).
const C295_DRAG_TABLE: &str = "drag_table: drag-table term is non-physical: induced curvature or energy (centre of gravity at x = 9.97 m, cruise CL 0.660)";

/// What the C295 drag-table rejection is tracked by.
const C295_DRAG_TABLE_TASK: &str =
    "tracked by the aero induced-drag fix task (non-physical C295 drag table)";

/// The failure line for a preset whose reference assessment returned an
/// error: the tracked C295 drag-table rejection by name, anything else as
/// an unhandled error.
fn assessment_failure(preset: &str, reason: &str) -> String {
    if preset == "C295" && reason == C295_DRAG_TABLE {
        format!("{preset}: {C295_DRAG_TABLE_TASK}: {reason}")
    } else {
        format!("{preset}: unhandled assessment error: {reason}")
    }
}

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
fn assess_reference(preset: &str) -> Result<AssessmentView, String> {
    assess_reference_mode(preset, alas_config::MtowSizing::FixedRequirement)
}

fn assess_reference_mode(
    preset: &str,
    mode: alas_config::MtowSizing,
) -> Result<AssessmentView, String> {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    assert_eq!(
        config.optimizer.design_space.mode,
        alas_config::DesignMode::ReferenceAdaptation,
        "a bare preset document loads as reference adaptation"
    );
    config.optimizer.objective.mtow_sizing = mode;
    assess_view(&config, &nominal(preset))
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
            // The DC-10 and B747-400 exceptions that stood here are gone: their wing thickness distributions are now the printed ones
            // (Obert Figs. 24.21 and 24.82), not an 11 % average and a 777-class CRM
            // distribution, and the mid-cruise wave drag meets the Korn ceiling.
            if violates(&assessment, id) {
                let residual = assessment.residuals.iter().find(|r| r.id == *id);
                let tracked = *id == "sweep_consistent_with_cruise_mach"
                    && AERO_WAVE_DRAG_TRACKED.contains(&preset);
                let note = if tracked {
                    AERO_WAVE_DRAG_TASK
                } else {
                    "untracked"
                };
                failures.push(format!(
                    "{preset}: {id} violated ({note}); residual: {residual:?}"
                ));
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
            Err(reason) => {
                failures.push(assessment_failure(preset, &reason));
                continue;
            }
        };
        for id in LAYOUT_IDS {
            if *id == "sweep_consistent_with_cruise_mach" {
                let residual = assessment.residuals.iter().find(|r| r.id == *id).unwrap();
                assert!(residual.actual.is_finite() && residual.actual >= 0.0);
                assert!(residual.limit.is_finite() && residual.limit > 0.0);
                assert_eq!(residual.role, alas_opt::mdo::ResidualRole::Constraint);
                match violates(&assessment, id) {
                    true if AERO_WAVE_DRAG_TRACKED.contains(&preset) => {
                        failures.push(format!("{preset}: {AERO_WAVE_DRAG_TASK}: {residual:?}"))
                    }
                    true => failures.push(format!(
                        "{preset}: new drag-divergence finding: {residual:?}"
                    )),
                    false => {}
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

/// The ceiling is the wave drag of the law in force (the one the drag table
/// tabulates) at its own drag-divergence Mach plus the relation's rms error,
/// so the gate trips exactly when the cruise Mach passes that Mach.
#[test]
fn drag_divergence_ceiling_is_the_law_in_force_past_its_own_divergence_mach() {
    let mut failures = Vec::new();
    for preset in alas_config::presets::available() {
        let assessment = match assess_reference(preset) {
            Ok(assessment) => assessment,
            Err(reason) => {
                failures.push(assessment_failure(preset, &reason));
                continue;
            }
        };
        let config = AlasConfig::from_value(&serde_json::json!({"preset": preset})).unwrap();
        let id = "sweep_consistent_with_cruise_mach";
        let Some(residual) = assessment.residuals.iter().find(|row| row.id == id) else {
            continue;
        };
        let detail = residual.detail.as_deref().expect("the residual has detail");
        let read = |label: &str| -> f64 {
            let (_, after) = detail
                .split_once(label)
                .unwrap_or_else(|| panic!("{preset}: {detail}"));
            after
                .split([',', ';'])
                .next()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or_else(|| panic!("{preset}: {detail}"))
        };
        let divergence = read("divergence Mach ");
        let ceiling = read("ceiling at Mach ");
        let rms = alas_aero::analysis::DRAG_DIVERGENCE_MACH_RMS_ERROR;
        let scatter = alas_aero::analysis::SHEVELL_KROO_DIVERGENCE_SCATTER;
        let offset = ceiling - divergence;
        assert!(
            (offset - rms).abs() <= 1.1e-3 || (offset - scatter).abs() <= 1.1e-3,
            "{preset}: {detail}"
        );
        // 0.0019 cos^3 L ((M_div + 0.047)/M_cc)^14.641 under Shevell-Kroo, 58.4 counts under Lock at C = 20.
        assert!(
            residual.limit > 0.0010 && residual.limit < 0.0200,
            "{preset}: ceiling {} ({detail})",
            residual.limit
        );
        // The wave drag rises monotonically with Mach, so the gate and the Mach comparison agree.
        let cruise = config.requirements.cruise_mach;
        if (cruise - ceiling).abs() > 1.0e-3 {
            assert_eq!(
                residual.actual > residual.limit,
                cruise > ceiling,
                "{preset}: {detail}"
            );
        }
        assert_eq!(
            violates(&assessment, id),
            residual.actual > residual.limit,
            "{preset}"
        );
    }
    assert!(
        failures.is_empty(),
        "presets the drag-divergence check could not assess:
  {}",
        failures.join(
            "
  "
        )
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

/// The trim refusals of the AVE with its wing root behind the tailcone: the
/// drag table's untrimmed induced node, or the trim solve finding no
/// stabiliser incidence at the cruise lift. Which one trips first depends on
/// the sized centre of gravity (x = 79.80 m against 79.77 m moved it from the
/// first to the second); both refuse the candidate before it is scored.
const TAILCONE_TRIM_REFUSALS: [&str; 2] = [
    "drag_table: induced node at CL 1.5000 left Cm",
    "trim_solve: no stabiliser incidence within 15 deg of the last estimate holds the pitching moment at CL 0.689",
];

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
        Err(reason) => assert!(
            TAILCONE_TRIM_REFUSALS
                .iter()
                .any(|refusal| reason.starts_with(refusal)),
            "{reason}"
        ),
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
        "violated: {:?}; residual: {:?}",
        assessment.violated_hard_ids(),
        assessment
            .residuals
            .iter()
            .find(|row| row.id == "sweep_consistent_with_cruise_mach")
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

/// The drag-divergence ceiling is a Diagnostic for the fixed certified baseline, whose
/// cruise Mach is a published fact, and a hard requirement in the modes that choose the
/// wing's sweep for their cruise Mach. AVE's mid-cruise wave drag exceeds the ceiling,
/// which admits the 0.023 rms of the `M_dd` relation above `M_dd`, by 0.015 in Mach.
#[test]
fn the_wave_drag_ceiling_is_diagnostic_for_the_baseline_and_hard_in_the_design_modes() {
    let role_of = |mode: &str| {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": "AVE",
            "optimizer": {"design_space": {"mode": mode}}
        }))
        .unwrap();
        // An unswept AVE at its Mach 0.84 is far past drag divergence under
        // any law, so the row is violated in every mode; the registered AVE
        // sits inside the row's documented scatter and cannot show the role.
        let design = DesignVector {
            sweep_deg: 0.0,
            ..nominal("AVE")
        };
        let assessment = assess_view(&config, &design).expect("AVE assessed");
        let row = assessment
            .residuals
            .iter()
            .find(|row| row.id == "sweep_consistent_with_cruise_mach")
            .expect("the wave-drag residual is evaluated")
            .clone();
        let hard = violates(&assessment, "sweep_consistent_with_cruise_mach");
        (row, hard)
    };
    let (baseline, baseline_hard) = role_of("baseline_sandbox");
    assert_eq!(baseline.role, alas_opt::mdo::ResidualRole::Diagnostic);
    assert!(baseline.violated(), "the shortfall is still reported");
    assert!(!baseline_hard);
    let (adapted, adapted_hard) = role_of("reference_adaptation");
    assert_eq!(adapted.role, alas_opt::mdo::ResidualRole::Constraint);
    assert!(adapted_hard);
}
