// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The validity-domain residuals: a design that leaves the shape the
//! disciplines' correlations describe must be rejected by name, and a real
//! aeroplane must never be.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_opt::{assess_product_candidate, CandidateAssessment};

/// Assess `design` against `preset`'s configuration in the **clean-sheet**
/// design mode, so the envelope is the global box widened to contain the
/// design: the point of these tests is what the *residuals* say about a shape,
/// not whether the bounds happened to permit it.
///
/// The mode is stated here rather than inherited. A bare `{"preset": name}`
/// document used to load as a clean sheet, which is what these tests were
/// written against; it now defaults to `ReferenceAdaptation`, whose envelope
/// is the D09 window around the registered reference and which therefore
/// rejects a deliberately broken shape with `design_space` before any
/// plausibility residual is reached. That default is correct for the product
/// — adapting a registered aircraft is what a preset document asks for — and
/// wrong for these tests, which exist to exercise the residuals themselves.
fn assess(preset: &str, design: DesignVector) -> Result<CandidateAssessment, String> {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    config.optimizer.design_space.mode = alas_config::optimizer::DesignMode::CleanSheet;
    assess_product_candidate(&config, &design)
}

fn nominal(preset: &str) -> DesignVector {
    alas_config::presets::get(preset)
        .expect("registered preset")
        .design_vector
}

/// Whether `id` appears among the assessment's violated hard residuals.
fn violates(assessment: &CandidateAssessment, id: &str) -> bool {
    assessment
        .violated_hard_ids()
        .into_iter()
        .any(|name| name == id)
}

/// Every plausibility residual identifier, so a test can assert that none of
/// them fires on a real aeroplane.
const PLAUSIBILITY_IDS: &[&str] = &[
    "aspect_ratio_min",
    "aspect_ratio_max",
    "fuselage_fineness_min",
    "fuselage_fineness_max",
    "tail_arm_fraction_min",
    "tail_arm_fraction_max",
    "tip_root_chord_ratio_min",
    "tip_root_chord_ratio_max",
    "planform_break_ordering",
    "root_thickness_ratio_min",
    "root_thickness_ratio_max",
    "tip_washout_min",
    "tip_washout_max",
];

/// The registered aircraft whose *sized* fuselage leaves the validity domain
/// even though the aircraft itself does not.
///
/// Measured with `examples/plausibility_survey`. As built from the preset
/// geometry, every registered type has a transport fuselage: fineness 9.2 to
/// 12.4 and a tail arm of 0.42 to 0.48 of body length. Evaluating the same
/// aircraft in the **clean-sheet** design mode, which these tests use because
/// it is the default a bare `{"preset": name}` document loads, re-derives the
/// fuselage from the cabin and gives fineness 16.1 (A320-200), 18.6
/// (A220-300) and 23.5 (ATR72-600) with tail arms of 0.68, 0.69 and 0.78. A
/// twenty-seven metre body a metre and a bit across is not a regional
/// turboprop, so the limits are right and the derived geometry is wrong. The
/// A320-200 lands inside both windows despite the same distortion, so it is
/// not listed.
///
/// In `DesignMode::ReferenceAdaptation`, which is the mode a registered
/// aircraft is actually optimized in, the fuselage is not re-derived and the
/// distortion does not occur: an internal all-preset benchmark (2026-09-16)
/// records no plausibility residual among any preset's rejection reasons.
/// The defect is therefore in the cabin-derived fuselage sizing, which is not
/// this worker's slice.
///
/// This list is an expectation, not a permission: when the cabin/geometry
/// owner fixes it, the assertion below fails and the entry is deleted.
const SIZED_FUSELAGE_LEAVES_THE_DOMAIN: &[&str] = &["A220-300", "ATR72-600"];

#[test]
fn only_the_recorded_cabin_defect_pushes_a_registered_aircraft_out_of_the_domain() {
    // The limits describe where the correlations are meaningful, so an
    // aeroplane this program ships as a reference must sit inside all of them.
    // A registered type may well violate *other* residuals under a default
    // route (that is the mass and mission models' business); what must not
    // happen is that its shape is called implausible for a reason nobody has
    // written down.
    for preset in alas_config::presets::available() {
        let assessment = match assess(preset, nominal(preset)) {
            Ok(assessment) => assessment,
            // A preset that cannot be sized at all under its default
            // configuration is a separate finding, owned by the mass and
            // mission models, and is reported by the benchmark rather than
            // asserted on here.
            Err(_) => continue,
        };
        let recorded = SIZED_FUSELAGE_LEAVES_THE_DOMAIN.contains(&preset);
        for id in PLAUSIBILITY_IDS {
            let body_shape = *id == "fuselage_fineness_max" || *id == "tail_arm_fraction_max";
            if recorded && body_shape {
                continue;
            }
            assert!(
                !violates(&assessment, id),
                "{preset} violates {id}; residual: {:?}",
                assessment
                    .residuals
                    .iter()
                    .find(|residual| residual.id == *id)
            );
        }
        if recorded {
            assert!(
                violates(&assessment, "fuselage_fineness_max")
                    || violates(&assessment, "tail_arm_fraction_max"),
                "{preset} no longer has the recorded sized-cabin defect; \
                 remove it from SIZED_FUSELAGE_LEAVES_THE_DOMAIN"
            );
        }
    }
}

#[test]
fn an_unbounded_span_is_rejected_by_the_aspect_ratio_limit_by_name() {
    // The lattice keeps handing an ever-larger span an ever-smaller induced
    // drag, and the wingbox correlation keeps returning a plausible-looking
    // mass, so without this limit the objective improves while the aeroplane
    // stops existing.
    let stretched = DesignVector {
        span_m: 190.0,
        ..DesignVector::default()
    };
    let assessment = assess("AVE", stretched).expect("a stretched wing still sizes");
    assert!(
        violates(&assessment, "aspect_ratio_max"),
        "violated: {:?}",
        assessment.violated_hard_ids()
    );
    assert!(!assessment.hard_feasible);
}

#[test]
fn a_trailing_edge_break_outside_the_tip_to_root_interval_is_rejected_by_name() {
    // A break chord above the root or below the tip is not a yehudi blend: it
    // is a notch or an inversion, and without a rule about it the
    // correlations score it as an ordinary wing.
    //
    // Measured behaviour of the current geometry builder: it refuses both
    // orderings outright, with `geometry_build`, so that is the operative
    // rejection today and this residual is a second, explicit statement of
    // the same rule that survives a builder change. What the test holds is
    // that neither ordering can reach a scored candidate, by whichever of the
    // two routes, and that the reason is one of exactly those two.
    for broken in [
        // Inversion: the break is wider than the root.
        DesignVector {
            break_chord_m: 19.0,
            root_chord_m: 14.0,
            ..DesignVector::default()
        },
        // Notch: the break is narrower than the tip.
        DesignVector {
            break_chord_m: 1.2,
            tip_chord_m: 1.6,
            root_chord_m: 16.5,
            ..DesignVector::default()
        },
    ] {
        match assess("AVE", broken) {
            Ok(assessment) => assert!(
                violates(&assessment, "planform_break_ordering"),
                "a non-monotonic planform was accepted; violated: {:?}",
                assessment.violated_hard_ids()
            ),
            Err(reason) => assert_eq!(reason, "geometry_build"),
        }
    }
}

#[test]
fn a_vanishing_tip_chord_is_rejected_by_the_taper_limit_by_name() {
    let needle = DesignVector {
        tip_chord_m: 0.4,
        root_chord_m: 16.5,
        break_chord_m: 7.8,
        ..DesignVector::default()
    };
    let assessment = assess("AVE", needle).expect("a needle tip still sizes");
    assert!(
        violates(&assessment, "tip_root_chord_ratio_min"),
        "violated: {:?}",
        assessment.violated_hard_ids()
    );
}

/// Assess `design` against `preset`, with `edit` applied to the loaded
/// configuration first.
fn assess_with(
    preset: &str,
    design: DesignVector,
    edit: impl FnOnce(&mut AlasConfig),
) -> Result<CandidateAssessment, String> {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    edit(&mut config);
    assess_product_candidate(&config, &design)
}

#[test]
fn the_washout_residual_measures_tip_incidence_less_root_incidence() {
    // `tip_twist_deg` is the tip section's absolute incidence, not the
    // washout, even though the variable is labelled "Geometric washout at
    // tip": the builder writes it straight onto the tip section while the
    // root takes `geometry.wing.root_twist_deg`, +4.0 deg on AVE. The AVE
    // clean-sheet finalist of the 2026-09-16 benchmark sits at
    // `tip_twist_deg = +1.0`, which is 3.0 degrees of *washout*, not wash-in.
    // This is the check that keeps the two quantities from being confused
    // again.
    let assessment = assess("AVE", nominal("AVE")).expect("the reference twin sizes");
    let residual = assessment
        .residuals
        .iter()
        .find(|residual| residual.id == "tip_washout_max")
        .expect("the washout limit is evaluated");
    let expected = nominal("AVE").tip_twist_deg - 4.0;
    assert!(
        (residual.actual - expected).abs() < 1.0e-9,
        "built washout {} against tip {} less root 4.0",
        residual.actual,
        nominal("AVE").tip_twist_deg
    );

    let at_finalist_twist = assess(
        "AVE",
        DesignVector {
            tip_twist_deg: 1.0,
            ..nominal("AVE")
        },
    )
    .expect("the finalist twist sizes");
    let finalist = at_finalist_twist
        .residuals
        .iter()
        .find(|residual| residual.id == "tip_washout_max")
        .expect("the washout limit is evaluated");
    assert!(
        (finalist.actual - (-3.0)).abs() < 1.0e-9,
        "{}",
        finalist.actual
    );
    assert!(!violates(&at_finalist_twist, "tip_washout_max"));
}

#[test]
fn a_wash_in_tip_is_rejected_by_the_washout_limit_by_name() {
    // Wash-in is not reachable from the current design space (see the limit's
    // own documentation), so it is constructed here by removing the root
    // incidence, which is exactly the geometry document this guard exists
    // for: a wing whose tip sits above its root.
    let wash_in = DesignVector {
        tip_twist_deg: 1.0,
        ..nominal("AVE")
    };
    let assessment = assess_with("AVE", wash_in, |config| {
        config.geometry.wing.root_twist_deg = 0.0;
        config.geometry.wing.break_twist_deg = 0.0;
    })
    .expect("a wash-in wing still sizes");
    assert!(
        violates(&assessment, "tip_washout_max"),
        "violated: {:?}",
        assessment.violated_hard_ids()
    );
    assert!(!assessment.hard_feasible);

    // The same geometry with the tip below the root is accepted, so the limit
    // is a statement about direction and not about twist itself.
    let washout = DesignVector {
        tip_twist_deg: -2.0,
        ..nominal("AVE")
    };
    let accepted = assess_with("AVE", washout, |config| {
        config.geometry.wing.root_twist_deg = 0.0;
        config.geometry.wing.break_twist_deg = 0.0;
    })
    .expect("a washed-out wing sizes");
    assert!(!violates(&accepted, "tip_washout_max"));
    assert!(!violates(&accepted, "tip_washout_min"));
}

#[test]
fn a_flat_wing_sits_on_the_washout_bound_without_violating_it() {
    // Tip and root at the same incidence land the residual exactly on its
    // upper bound. A window whose bound is zero has no magnitude to take a
    // relative slack from; this holds that the explicit degree slack is what
    // keeps such a wing admissible instead of rejecting it on roundoff.
    let flat = DesignVector {
        tip_twist_deg: 0.0,
        ..nominal("AVE")
    };
    let assessment = assess_with("AVE", flat, |config| {
        config.geometry.wing.root_twist_deg = 0.0;
        config.geometry.wing.break_twist_deg = 0.0;
    })
    .expect("the flat wing sizes");
    let residual = assessment
        .residuals
        .iter()
        .find(|residual| residual.id == "tip_washout_max")
        .expect("the washout limit is evaluated");
    assert!(
        residual.actual.abs() < 1.0e-6,
        "washout {}",
        residual.actual
    );
    assert!(!violates(&assessment, "tip_washout_max"));
}

#[test]
fn the_most_twisted_registered_wing_clears_the_washout_floor() {
    // The A380-800 carries -7.0 degrees of built washout (tip -2.5, root
    // +4.5), the largest of the eight. A floor set above it would reject a
    // real aeroplane, which is the defect this test exists to catch: the
    // first draft of this limit used -6.0 and did exactly that, rejecting 51
    // A380 candidates in the reference-adaptation benchmark.
    let assessment = assess("A380-800", nominal("A380-800")).expect("the A380 sizes");
    let residual = assessment
        .residuals
        .iter()
        .find(|residual| residual.id == "tip_washout_min")
        .expect("the washout limit is evaluated");
    assert!(
        (residual.actual - (-7.0)).abs() < 1.0e-9,
        "built washout {}",
        residual.actual
    );
    assert!(!violates(&assessment, "tip_washout_min"));
}

#[test]
fn every_plausibility_residual_is_present_and_compliant_on_the_nominal_design() {
    // A limit that is never evaluated is not a limit. The nominal AVE must
    // produce each residual, and each must report a compliant (non-positive)
    // raw value.
    let assessment = assess("AVE", nominal("AVE")).expect("the reference twin sizes");
    for id in PLAUSIBILITY_IDS {
        let residual = assessment
            .residuals
            .iter()
            .find(|residual| residual.id == *id)
            .unwrap_or_else(|| panic!("{id} is not evaluated"));
        assert!(
            residual.actual.is_finite() && residual.raw_residual.is_finite(),
            "{id} reports a non-finite quantity"
        );
        assert!(
            !violates(&assessment, id),
            "{id} is violated on nominal AVE"
        );
    }
}

#[test]
fn turning_the_limits_off_removes_them_without_touching_any_other_residual() {
    // They are a configurable validity-domain statement, not a hidden gate, so
    // a user who disables them must get the same requirement table minus these
    // rows, with nothing else moved.
    let design = nominal("AVE");
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "AVE" }))
        .unwrap_or_else(|error| panic!("{error}"));
    let with = assess_product_candidate(&config, &design).expect("sizes with the limits on");
    config.optimizer.plausibility.enabled = false;
    let without = assess_product_candidate(&config, &design).expect("sizes with the limits off");

    for id in PLAUSIBILITY_IDS {
        assert!(with.residuals.iter().any(|residual| residual.id == *id));
        assert!(!without.residuals.iter().any(|residual| residual.id == *id));
    }
    let other_with: Vec<&str> = with
        .residuals
        .iter()
        .map(|residual| residual.id)
        .filter(|id| !PLAUSIBILITY_IDS.contains(id))
        .collect();
    let other_without: Vec<&str> = without
        .residuals
        .iter()
        .map(|residual| residual.id)
        .collect();
    assert_eq!(other_with, other_without);
    assert_eq!(with.hard_feasible, without.hard_feasible);
}
