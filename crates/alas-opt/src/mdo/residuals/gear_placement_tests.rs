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
    // The rule is exercised on the layout as given: a wing moved by the
    // fixture, the gear at its published stations (`with_layout_as_given`).
    crate::mdo::wing_position::with_layout_as_given(
        a_translated_main_gear_meets_every_ground_mechanism_on_the_rebuilt_candidate_body,
    );
}

fn a_translated_main_gear_meets_every_ground_mechanism_on_the_rebuilt_candidate_body() {
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

    // Nearest station: one centimetre less translation already misses a
    // mechanism on the same closure's rebuilt ledger. (Geometry round 2 left
    // the solver's own tolerance between 1 mm and 1 cm on this fixture: 1 mm
    // less still passes, 1 cm, 5 cm and more miss the nose-gear load window.)
    let mut shorter = placed.clone();
    shorter.landing_gear.derived_main_gear = Some(DerivedMainGearStation {
        translation_m: translation - 1.0e-2,
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
    // The rule is exercised on the layout as given: a wing moved by the
    // fixture, the gear at its published stations (`with_layout_as_given`).
    crate::mdo::wing_position::with_layout_as_given(
        a_redesigned_atr_gets_a_station_that_meets_every_ground_mechanism_body,
    );
}

fn a_redesigned_atr_gets_a_station_that_meets_every_ground_mechanism_body() {
    let (class_config, design) = preset("ATR72-600");
    // The class-acceleration check: the ATR candidate must have a station at
    // the class pitch acceleration (5 deg/s^2).
    let mut moved = design;
    moved.span_m *= 0.995;
    let x = moved.to_array();
    // Product-state update: the class acceleration (5 deg/s^2) and the
    // tail-down-aware tip-back requirement (8 deg model tail-down, not the
    // old 15 deg floor) leave this candidate a feasible station at the class
    // pitch acceleration; it was Infeasible at 7 deg/s^2 and 15 deg tip-back.
    let at_class = crate::mdo::sizing::run_candidate(&class_config, &x).unwrap();
    assert!(
        !matches!(
            solve(&at_class, &class_config),
            Ok(MainGearPlacement::Infeasible)
        ),
        "the class-acceleration ATR candidate has no station"
    );
    let mut config = class_config;
    config.landing_gear.rotation_pitch_acceleration_deg_s2 =
        Some(ISOLATING_PITCH_ACCELERATION_DEG_S2);
    // The fuselage group of a wing-mounted propeller aircraft sits at 39 % of
    // the fuselage length (Torenbeek Table 8-15), which moved the registered
    // ATR's CG 9 % MAC forward, so the registered wing position no longer needs
    // a translated station. A wing moved aft (the fixture is one candidate,
    // not a claim about the design) restores the aft-CG case this check is for.
    let mut found = None;
    for wing_shift_m in [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0] {
        let mut candidate = moved;
        candidate.wing_x_shift_m = wing_shift_m;
        let x = candidate.to_array();
        let outcome = crate::mdo::sizing::run_candidate(&config, &x).unwrap();
        if let Ok(MainGearPlacement::Translated(translation_m)) = solve(&outcome, &config) {
            found = Some((x, translation_m));
            break;
        }
    }
    let (x, translation_m) =
        found.expect("no wing position makes the ATR need a translated station");
    assert!(
        translation_m.abs() > 1e-6,
        "a Translated outcome must move the station, got {translation_m}"
    );
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

/// The DC-10 candidate of the three-leg fixture: the registered design with the
/// wing moved `wing_x_shift_m` and the fuselage lengthened `fuselage_extra_m`.
fn dc10_three_leg_candidate(
    nominal: DesignVector,
    wing_x_shift_m: f64,
    fuselage_extra_m: f64,
) -> DesignVector {
    DesignVector {
        span_m: 49.219_501_737_927_02,
        root_chord_m: 11.185_361_368_270_566,
        break_chord_m: 7.987_089_110_565_081,
        tip_chord_m: 2.827_541_478_958_002,
        sweep_deg: 40.926_177_529_939_146,
        tip_twist_deg: -1.737_388_828_312_142_1,
        wing_x_shift_m,
        fuselage_length_m: nominal.fuselage_length_m + fuselage_extra_m,
        tail_scale: 1.011_511_624_181_909_5,
        airfoil_thickness_scale: 0.971_979_582_161_175_3,
        airfoil_camber_scale: 0.989_585_468_423_623_5,
        ..nominal
    }
}

/// Whether the DC-10 candidate misses the nose-gear load minimum before any
/// gear placement, `None` when its sizing does not close.
fn dc10_three_leg_misses(
    config: &AlasConfig,
    nominal: DesignVector,
    wing_x_shift_m: f64,
) -> Option<bool> {
    let x = dc10_three_leg_candidate(nominal, wing_x_shift_m, 3.4).to_array();
    let configured = crate::mdo::sizing::run_candidate(config, &x).ok()?;
    let before = super::super::relative_balance::assess(&configured, config).ok()?;
    Some(ground_misses(&before).contains("MinimumNoseGearLoad"))
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
///
/// The fixture was a single knife-edge point (P2 m3): the nose-gear miss exists
/// only for wing shifts in a narrow band at a fixed fuselage extension. The
/// band's miss edge is located at test time by bisection on the miss indicator
/// between a shift that does not miss (-1.6 m) and one that does (-1.4 m), to
/// 0.01 m, and the fixture takes the miss side of that edge. Scan of the grid
/// at the time of the change (diagnostic run, fuselage +3.4 m): -1.6 no miss;
/// -1.4 and -1.2 miss and place (translations 0.028 and 0.136 m); -1.0 and -0.8
/// leave the centre leg ahead of the trailing edge.
///
/// Two model defects keep this fixture from being placed. The probe ledger
/// loses hold payload (see
/// `the_gear_probe_ledger_conserves_the_payload_of_the_three_leg_fixture`),
/// and `configured_stations_suffice` judges the stations at the clean neutral
/// point while the envelope judges them at the critical one, where the model
/// puts the DC-10 neutral point 0.59 m behind the main gear and still reports
/// the minimum nose-gear load.
#[test]
fn a_three_leg_group_translates_past_its_centre_leg_behind_the_wing_root() {
    // The rule is exercised on the layout as given: a wing moved by the
    // fixture, the gear at its published stations (`with_layout_as_given`).
    crate::mdo::wing_position::with_layout_as_given(
        a_three_leg_group_translates_past_its_centre_leg_behind_the_wing_root_body,
    );
}

fn a_three_leg_group_translates_past_its_centre_leg_behind_the_wing_root_body() {
    let (mut class_config, nominal) = preset("DC-10");
    // The knife edge below (P2 m3) was found with the DC-10's own published
    // aft-CG nose share at its empty weight, 6.2 % (Boeing DC/MD-10 ACAP
    // DAC-67803A Rev A, Figure 7.4.2: 6.6 % at 118,500 kg to 5.7 % at
    // 123,000 kg). The model's minimum is now the class value, 3.52 %, which
    // moves the edge, so the fixture sets the share it was located with.
    class_config.mass_model.pct_load_nlg_min = 0.062;
    let mut config = class_config.clone();
    config.landing_gear.rotation_pitch_acceleration_deg_s2 =
        Some(ISOLATING_PITCH_ACCELERATION_DEG_S2);
    let (mut no_miss, mut miss) = (-1.6_f64, -1.4_f64);
    assert_eq!(
        dc10_three_leg_misses(&config, nominal, no_miss),
        Some(false)
    );
    assert_eq!(dc10_three_leg_misses(&config, nominal, miss), Some(true));
    while miss - no_miss > 0.01 {
        let mid = 0.5 * (no_miss + miss);
        match dc10_three_leg_misses(&config, nominal, mid) {
            Some(true) => miss = mid,
            Some(false) => no_miss = mid,
            None => panic!("the DC-10 candidate does not size at wing shift {mid} m"),
        }
    }
    let candidate = dc10_three_leg_candidate(nominal, miss, 3.4);
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

/// Translating the main group moves no payload: the ledger of the probe
/// station must close with the payload of the configured one. The fixture is the
/// three-leg DC-10 candidate (wing moved -1.4 m, fuselage lengthened 3.4 m).
#[test]
fn the_gear_probe_ledger_conserves_the_payload_of_the_three_leg_fixture() {
    // The rule is exercised on the layout as given: a wing moved by the
    // fixture, the gear at its published stations (`with_layout_as_given`).
    crate::mdo::wing_position::with_layout_as_given(
        the_gear_probe_ledger_conserves_the_payload_of_the_three_leg_fixture_body,
    );
}

fn the_gear_probe_ledger_conserves_the_payload_of_the_three_leg_fixture_body() {
    let (mut config, nominal) = preset("DC-10");
    config.landing_gear.rotation_pitch_acceleration_deg_s2 =
        Some(ISOLATING_PITCH_ACCELERATION_DEG_S2);
    let x = dc10_three_leg_candidate(nominal, -1.4, 3.4).to_array();
    let outcome = crate::mdo::sizing::run_candidate(&config, &x).unwrap();
    let config = outcome.config_with_clmax(&config);
    let configured = balance_ledger::loading_basis(&outcome, &config);
    assert!(
        configured.is_ok(),
        "configured ledger: {:?}",
        configured.err()
    );
    let probe = probe_loading_basis(&outcome, &config);
    assert!(
        probe.is_ok(),
        "the probe ledger does not close: {:?}",
        probe.err()
    );
}

/// With a zero re-trim tolerance every centre-of-gravity move re-trims, so a
/// re-sizing that is offered the previous closure's trim and lattice cache
/// must reproduce a cold re-sizing: the cache is exact to the rounding of the
/// lattice factorization update (relative 1e-9; it was bitwise before the
/// Schur-complement update, `ALAS_VLM_SCHUR_LU=0` restores that).
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
    };
    assert_ne!(first.cg_x.to_bits(), cold.cg_x.to_bits());
    for (warm, cold) in key(&warm).into_iter().zip(key(&cold)) {
        assert!(
            (warm - cold).abs() <= 1.0e-9 * cold.abs(),
            "{warm} vs {cold}"
        );
    }
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

/// A rigid move of the gear group carries the centre of gravity the OEI and
/// VMC residuals read by the gear's moment over the loaded mass, no more.
#[test]
fn a_rigid_gear_move_carries_the_centre_of_gravity_by_the_gear_moment() {
    let (config, design) = preset("A220-300");
    let mut outcome = run_candidate_cancellable(
        &config,
        &design.to_array(),
        None,
        false,
        None,
        SizingControls::default(),
    )
    .unwrap();
    let cg_before = outcome.cg_x;
    let total_kg: f64 = outcome
        .masses
        .as_pairs()
        .into_iter()
        .map(|(name, kg)| {
            if name == alas_mass::breakdown::FUEL {
                outcome.masses.physical_fuel_mass_kg().unwrap_or(0.0)
            } else {
                kg.max(0.0)
            }
        })
        .sum();
    let shift_m = 0.30;
    move_gear_rigidly(&mut outcome, shift_m);
    let expected_m = outcome.masses.gear.max(0.0) * shift_m / total_kg;
    assert!(expected_m > 1e-3, "{expected_m}");
    assert!(
        ((outcome.cg_x - cg_before) - expected_m).abs() < 1e-9,
        "moved {} m, expected {expected_m} m",
        outcome.cg_x - cg_before
    );
}
