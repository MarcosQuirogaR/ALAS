// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests build every fixture they assert on, so a failed expect is the
// assertion failing rather than a library invariant breaking.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;

use super::*;
use crate::envelope::{
    assess_model_cg_envelope_with_ledger, ModelCgConstraint, ModelCgEnvelopeAssessment,
    ModelCgLoadingState,
};

/// A weak required pitch acceleration, deg/s^2, used as a fixture to
/// separate a gear-placement mechanism under test from the nose-wheel
/// lift-off conflict the class value (5 deg/s^2) can raise on some presets.
const ISOLATING_PITCH_ACCELERATION_DEG_S2: f64 = 1.0;

fn preset(name: &str) -> (AlasConfig, DesignVector) {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    let design = alas_config::presets::get(name).unwrap().design_vector;
    (config, design)
}

/// The ground mechanisms the rule places the gear against, violated in any
/// state of `assessment`: tip-back, the nose-load window and nose-wheel
/// liftoff at rotation.
fn ground_misses(assessment: &ModelCgEnvelopeAssessment) -> BTreeSet<String> {
    let mut misses = BTreeSet::new();
    for state in &assessment.loading_states {
        for constraint in &state.constraints {
            if constraint.violated
                && matches!(
                    constraint.constraint,
                    ModelCgConstraint::TipBack
                        | ModelCgConstraint::MinimumNoseGearLoad
                        | ModelCgConstraint::MaximumNoseGearLoadFraction
                )
            {
                misses.insert(format!("{:?}", constraint.constraint));
            }
        }
        if crate::envelope::PhaseLimits::for_state(state.state).rotation
            && state.cg_pct_mac < state.physical_limits.rotation_fwd_pct_mac
        {
            misses.insert("Rotation".to_owned());
        }
    }
    misses
}

#[test]
fn only_a_redesigned_candidate_has_its_gear_placed() {
    let (config, design) = preset("ATR72-600");
    assert!(!places_gear(&config, &design.to_array()));
    let mut moved = design;
    moved.wing_x_shift_m += 0.1;
    assert!(places_gear(&config, &moved.to_array()));
    let sandbox = AlasConfig::from_value(&serde_json::json!({
        "preset": "ATR72-600",
        "optimizer": {"design_space": {"mode": "baseline_sandbox"}}
    }))
    .unwrap();
    assert!(!places_gear(&sandbox, &moved.to_array()));
}

/// An A220 wing moved 1 m aft puts the CG too close to the published main
/// gear. The rule translates the main group, the candidate is re-sized with
/// it, and the rebuilt envelope (not the solver) confirms every ground
/// mechanism; the assessed state carries the translation for a replay.
#[test]
fn a_translated_main_gear_meets_every_ground_mechanism_on_the_rebuilt_candidate() {
    let (config, design) = preset("A220-300");
    let mut moved = design;
    moved.wing_x_shift_m += 1.0;
    let x = moved.to_array();
    let configured = crate::mdo::sizing::run_candidate(&config, &x).unwrap();
    let before = super::super::relative_balance::assess(&configured, &config).unwrap();
    assert!(
        !ground_misses(&before).is_empty(),
        "the published stations must miss a ground mechanism for this fixture"
    );

    let (outcome, placed) =
        size_with_main_gear_placement(&config, &x, None, false, None, SizingControls::default())
            .unwrap();
    let placed = placed.expect("a translation is placed");
    let translation = placed
        .landing_gear
        .derived_main_gear
        .expect("the placed configuration carries it")
        .translation_m;
    assert!(translation > 0.0, "the gear moves aft, got {translation} m");
    let after = super::super::relative_balance::assess(&outcome, &placed).unwrap();
    assert_eq!(ground_misses(&after), BTreeSet::new());

    // Nearest station: one millimetre less translation already misses a
    // mechanism on the same closure's rebuilt ledger.
    let mut shorter = placed.clone();
    shorter.landing_gear.derived_main_gear = Some(DerivedMainGearStation {
        translation_m: translation - 1.0e-3,
    });
    let mut probe_outcome = outcome;
    let gear_x = |candidate: &AlasConfig| {
        alas_mass::stations::component_stations_with_gear(
            &probe_outcome.plane,
            &candidate.geometry,
            &candidate.requirements,
            &candidate.mass_model,
            &candidate.structures,
            &candidate.landing_gear,
        )
        .unwrap()
        .gear_centroid_m()[0]
    };
    let shift = gear_x(&shorter) - gear_x(&placed);
    probe_outcome.coords.gear[0] += shift;
    let near = super::super::relative_balance::assess(&probe_outcome, &shorter).unwrap();
    assert!(!ground_misses(&near).is_empty());

    let assessment = crate::mdo::assess_product_candidate(&config, &moved).unwrap();
    assert_eq!(
        assessment.resolved.main_gear_placement,
        placed.landing_gear.derived_main_gear
    );
}

/// A check of the ATR placement, independent of the solver's algebra.
///
/// Finding: with the model tail-down angle (8 deg) setting the tip-back
/// requirement and the class 5 deg/s^2 required acceleration, this candidate
/// has a feasible station at the class acceleration (the first assertion;
/// earlier, at 7 deg/s^2 and a 15 deg tip-back floor, it had none). The
/// algebra check below isolates the placement from the lift-off authority
/// with the optional
/// `rotation_pitch_acceleration_deg_s2` override, a weaker required
/// acceleration that is a test fixture, not a physical claim: the ledger
/// and envelope are rebuilt with the main group translated by the solved
/// amount, and every mechanism the rule places against (tip-back, the
/// nose-load window at every state, lift-off where the envelope gates it)
/// holds there. This is a check at one candidate, not a proof over the
/// design space.
#[test]
fn a_redesigned_atr_gets_a_station_that_meets_every_ground_mechanism() {
    let (class_config, design) = preset("ATR72-600");
    let mut moved = design;
    moved.span_m *= 0.995;
    let x = moved.to_array();
    // Product-state update: the class acceleration (5 deg/s^2) and the
    // tail-down-aware tip-back requirement (8 deg model tail-down, not the
    // old 15 deg floor) leave this candidate a feasible station at the class
    // pitch acceleration; it was Infeasible at 7 deg/s^2 and 15 deg tip-back.
    let mut at_class = crate::mdo::sizing::run_candidate(&class_config, &x).unwrap();
    assert!(
        !matches!(
            solve(&mut at_class, &class_config),
            Ok(MainGearPlacement::Infeasible)
        ),
        "the class-acceleration ATR candidate has no station"
    );
    let mut config = class_config;
    config.landing_gear.rotation_pitch_acceleration_deg_s2 =
        Some(ISOLATING_PITCH_ACCELERATION_DEG_S2);
    let mut outcome = crate::mdo::sizing::run_candidate(&config, &x).unwrap();
    let translation_m = match solve(&mut outcome, &config) {
        Ok(MainGearPlacement::Translated(translation_m)) => translation_m,
        other => panic!("expected a translated station, got {other:?}"),
    };
    let mut outcome = crate::mdo::sizing::run_candidate(&config, &x).unwrap();
    let configured = super::super::relative_balance::assess(&outcome, &config).unwrap();
    // The neutral point does not depend on the gear: back the critical one
    // out of the configured assessment's aerodynamic aft limit.
    let frame = outcome.plane.mac_frame().unwrap();
    let critical_np_x_m = frame.x_lemac_m
        + (configured.aerodynamic_aft_limit_pct_mac
            + 100.0 * config.requirements.min_physical_static_margin)
            / 100.0
            * outcome.mac;
    let gear = |c: &AlasConfig| {
        alas_mass::stations::component_stations_with_gear(
            &outcome.plane,
            &c.geometry,
            &c.requirements,
            &c.mass_model,
            &c.structures,
            &c.landing_gear,
        )
        .unwrap()
    };
    let mut placed = config.clone();
    placed.landing_gear.derived_main_gear = Some(DerivedMainGearStation { translation_m });
    let shift_m = gear(&placed).gear_centroid_m()[0] - gear(&config).gear_centroid_m()[0];
    outcome.coords.gear[0] += shift_m;
    let ledger = balance_ledger::loading_basis(&outcome, &placed).unwrap();
    let assessment = assess_model_cg_envelope_with_ledger(
        &outcome.plane,
        ledger,
        outcome.x_np,
        critical_np_x_m,
        outcome.mac,
        &placed,
    )
    .unwrap();
    assert!(
        ground_misses(&assessment).is_empty(),
        "translation {translation_m} m misses {:?}",
        ground_misses(&assessment)
    );
    let takeoff = assessment
        .loading_states
        .iter()
        .find(|state| state.state == ModelCgLoadingState::AnalyzedTakeoff)
        .unwrap();
    assert!(takeoff.cg_pct_mac >= takeoff.physical_limits.rotation_fwd_pct_mac);
}

/// The registered aircraft keeps its published gear: on every preset's own
/// design vector the placement step sizes once and places nothing, so the
/// nominal findings are those of the configured stations.
#[test]
fn every_registered_nominal_keeps_its_published_gear() {
    for name in alas_config::presets::available() {
        let (config, design) = preset(name);
        let (_, placed) = size_with_main_gear_placement(
            &config,
            &design.to_array(),
            None,
            false,
            None,
            SizingControls::default(),
        )
        .unwrap_or_else(|failure| panic!("{name}: {}", failure.reason));
        assert!(placed.is_none(), "{name}");
    }
}

/// A placed candidate is compared with the one nominal of its unplaced
/// configuration: the relative guards' limits are unchanged and no further
/// nominal is resolved for the translation.
#[test]
fn a_placed_candidate_shares_the_unplaced_nominal() {
    let (config, design) = preset("A220-300");
    assert_eq!(
        config.optimizer.design_space.mode,
        DesignMode::ReferenceAdaptation
    );
    let mut moved = design;
    moved.wing_x_shift_m += 1.0;
    let (outcome, placed) = size_with_main_gear_placement(
        &config,
        &moved.to_array(),
        None,
        false,
        None,
        SizingControls::default(),
    )
    .unwrap();
    let placed = placed.expect("the A220 fixture places its gear");
    let assessment = super::super::relative_balance::assess(&outcome, &placed).unwrap();
    let limits = |candidate: &AlasConfig| {
        super::super::relative_balance::residuals(
            &assessment,
            candidate,
            crate::mdo::ResidualRole::Constraint,
        )
        .into_iter()
        .map(|row| (row.id, row.limit.to_bits()))
        .collect::<Vec<_>>()
    };
    let unplaced = limits(&config);
    let before = super::super::relative_balance::NOMINAL.resolutions(&config);
    assert_eq!(before, 1);
    assert!(!unplaced.is_empty());
    assert_eq!(limits(&placed), unplaced);
    assert_eq!(
        super::super::relative_balance::NOMINAL.resolutions(&config),
        before
    );
    assert_eq!(
        super::super::relative_balance::NOMINAL.resolutions(&placed),
        before
    );
}

/// A DC-10 candidate whose empty aircraft sits behind the published minimum
/// nose-gear load. Its centre (body) leg stands 0.76 m aft of the wing
/// bogies, behind the trailing edge of the wing's centreline chord, as on
/// the real aircraft; it is a fuselage gear and does not pin the group. The
/// group translates aft and the rebuilt envelope meets every ground
/// mechanism.
///
/// Finding: at the built tail incidence (CL_h about -0.85 for the DC-10) this
/// candidate had no station at the class pitch acceleration: the translation
/// that clears the nose-load window left the nose wheel unable to lift off
/// at maximum-fuel takeoff. With the trimmable stabiliser at its takeoff
/// nose-up setting (CL_h about -0.98) it is placed at the class
/// acceleration too. The test states that, then isolates the centre-leg
/// geometry from the lift-off authority with the optional pitch-acceleration
/// override (a fixture, not a physical claim).
#[test]
fn a_three_leg_group_translates_past_its_centre_leg_behind_the_wing_root() {
    let (class_config, nominal) = preset("DC-10");
    let mut config = class_config.clone();
    config.landing_gear.rotation_pitch_acceleration_deg_s2 =
        Some(ISOLATING_PITCH_ACCELERATION_DEG_S2);
    let candidate = DesignVector {
        span_m: 49.219_501_737_927_02,
        root_chord_m: 11.185_361_368_270_566,
        break_chord_m: 7.987_089_110_565_081,
        tip_chord_m: 2.827_541_478_958_002,
        sweep_deg: 40.926_177_529_939_146,
        tip_twist_deg: -1.737_388_828_312_142_1,
        // Old -3.977 m -> new -3.5 m: with the measured 7.42 m DC-10 nose (was
        // 5.5 m) the cabin proxy and the empty centre of gravity sit further aft
        // and the old shift can no longer be placed (the placement finds no
        // translation that clears the minimum nose-gear load); -3.5 m is the
        // nearest shift (probed at 0.5 m steps) that can.
        wing_x_shift_m: -3.5,
        tail_scale: 1.011_511_624_181_909_5,
        airfoil_thickness_scale: 0.971_979_582_161_175_3,
        airfoil_camber_scale: 0.989_585_468_423_623_5,
        ..nominal
    };
    let x = candidate.to_array();
    let configured = crate::mdo::sizing::run_candidate(&config, &x).unwrap();
    let wing_te_at_centreline_m = configured
        .plane
        .wings
        .first()
        .and_then(|wing| wing.xsecs.first())
        .map(|root| root.xyz_le[0] + root.chord)
        .unwrap();
    let sections = &configured.plane.fuselages[0].xsecs;
    let start_m = sections[0].xyz_c[0];
    let length_m = sections[sections.len() - 1].xyz_c[0] - start_m;
    let centre_leg_m = config
        .landing_gear
        .resolved_station_positions(start_m, start_m, start_m, length_m)
        .main_gear_x_m[2];
    assert!(centre_leg_m > wing_te_at_centreline_m);
    let before = super::super::relative_balance::assess(&configured, &config).unwrap();
    assert!(ground_misses(&before).contains("MinimumNoseGearLoad"));

    let (class_outcome, class_placed) = size_with_main_gear_placement(
        &class_config,
        &x,
        None,
        false,
        None,
        SizingControls::default(),
    )
    .unwrap();
    let class_placed = class_placed.expect("the class-acceleration DC-10 candidate is placed");
    let class_after =
        super::super::relative_balance::assess(&class_outcome, &class_placed).unwrap();
    assert_eq!(ground_misses(&class_after), BTreeSet::new());
    let (outcome, placed) =
        size_with_main_gear_placement(&config, &x, None, false, None, SizingControls::default())
            .unwrap();
    let placed = placed.expect("the group is placed");
    let translation_m = placed.landing_gear.derived_main_gear.unwrap().translation_m;
    assert!(translation_m > 0.0, "{translation_m} m");
    let after = super::super::relative_balance::assess(&outcome, &placed).unwrap();
    assert_eq!(ground_misses(&after), BTreeSet::new());
}

/// With a zero re-trim tolerance every centre-of-gravity move re-trims, so a
/// re-sizing that is offered the previous closure's trim and lattice cache
/// must reproduce a cold re-sizing bit for bit: the cache is exact.
#[test]
fn a_re_sizing_offered_a_trim_it_cannot_keep_is_a_cold_sizing() {
    let (mut config, design) = preset("A220-300");
    config.optimizer.objective.retrim_cg_tolerance_pct_mac = 0.0;
    let mut moved = design;
    moved.wing_x_shift_m += 1.0;
    let x = moved.to_array();
    let first = crate::mdo::sizing::run_candidate(&config, &x).unwrap();
    let mut translated = config.clone();
    translated.landing_gear.derived_main_gear = Some(DerivedMainGearStation { translation_m: 0.3 });
    let cold = crate::mdo::sizing::run_candidate(&translated, &x).unwrap();
    let warm = crate::mdo::sizing::run_candidate_reusing(
        &translated,
        &x,
        None,
        false,
        None,
        SizingControls::default(),
        first.trim_reuse.as_ref(),
    )
    .unwrap();
    let key = |outcome: &crate::mdo::sizing::SizingOutcome| {
        [
            outcome.sized.takeoff_mass_kg,
            outcome.sized.block_fuel_kg,
            outcome.sized.lift_to_drag,
            outcome.x_np,
            outcome.cg_x,
        ]
        .map(f64::to_bits)
    };
    assert_ne!(first.cg_x.to_bits(), cold.cg_x.to_bits());
    assert_eq!(key(&warm), key(&cold));
}

/// Within the re-trim tolerance the re-sizing keeps the previous closure's
/// polar, as the closure itself does between its passes, and reports the
/// centre-of-gravity shift it accepted.
#[test]
fn a_re_sizing_within_the_retrim_tolerance_keeps_the_trimmed_polar() {
    let (config, design) = preset("A220-300");
    let mut moved = design;
    moved.wing_x_shift_m += 1.0;
    let x = moved.to_array();
    let first = crate::mdo::sizing::run_candidate(&config, &x).unwrap();
    let mut translated = config.clone();
    translated.landing_gear.derived_main_gear = Some(DerivedMainGearStation {
        translation_m: 0.01,
    });
    let warm = crate::mdo::sizing::run_candidate_reusing(
        &translated,
        &x,
        None,
        false,
        None,
        SizingControls::default(),
        first.trim_reuse.as_ref(),
    )
    .unwrap();
    assert_ne!(first.cg_x.to_bits(), warm.cg_x.to_bits());
    assert_eq!(first.x_np.to_bits(), warm.x_np.to_bits());
    assert_eq!(warm.sized.retrim_count, 0);
    let tolerance = config.optimizer.objective.retrim_cg_tolerance_pct_mac;
    assert!(
        warm.sized.cg_shift_pct_mac > 0.0 && warm.sized.cg_shift_pct_mac <= tolerance,
        "{}",
        warm.sized.cg_shift_pct_mac
    );
}
