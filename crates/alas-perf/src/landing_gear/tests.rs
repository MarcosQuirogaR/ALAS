// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

#[test]
fn a_wider_track_lowers_the_turnover_angle() {
    let narrow = LandingGearConfig {
        track_diameter_factor: 1.0,
        ..Default::default()
    };
    let wide = LandingGearConfig {
        track_diameter_factor: 3.0,
        ..Default::default()
    };

    let common = |cfg: &LandingGearConfig| {
        size_landing_gear(120_000.0, 6.0, 12.0, 7.0, 11.0, 6.0, 6.0, cfg).turnover_angle_deg
    };
    assert!(common(&wide) < common(&narrow));
}

#[test]
fn a_three_leg_arrangement_keeps_the_centreline_bogie_and_each_count() {
    let config = LandingGearConfig {
        n_mlg_struts: 3,
        mlg_strut_bogie_wheels: Some(vec![4, 4, 2]),
        track_diameter_factor: 10.684 / 5.64,
        reference_track_m: Some(10.684),
        reference_wheelbase_m: Some(25.375),
        ..Default::default()
    };
    let layout = size_landing_gear(260_000.0, 6.0, 31.0, 8.0, 27.0, 5.64, 6.0, &config);

    assert_eq!(layout.n_mlg_struts, 3);
    assert_eq!(layout.mlg_wheels_per_strut, vec![4, 4, 2]);
    assert_eq!(
        layout
            .wheels
            .iter()
            .filter(|wheel| wheel.group == "MLG")
            .count(),
        10
    );
    assert_eq!(layout.track_width_m, 10.684);
    // The source wheelbase is metadata. Model-derived stations and the
    // reaction geometry continue to use the caller's x stations.
    assert_eq!(layout.reference_wheelbase_m, Some(25.375));
    assert_eq!(layout.wheelbase_m, 25.0);
    let centreline: Vec<&Wheel> = layout
        .wheels
        .iter()
        .filter(|wheel| wheel.strut_label == "MLG-Body-C")
        .collect();
    assert_eq!(centreline.len(), 2);
    assert!((centreline[0].y + centreline[1].y).abs() < 1e-12);
    assert!(centreline
        .iter()
        .all(|wheel| (wheel.x - 31.0).abs() < 1e-12));
}

#[test]
fn four_leg_source_topology_uses_wing_and_body_bogie_sizes() {
    let config = LandingGearConfig {
        n_mlg_struts: 4,
        mlg_strut_bogie_wheels: Some(vec![4, 4, 6, 6]),
        track_diameter_factor: 14.34 / 7.14,
        reference_track_m: Some(14.34),
        reference_wheelbase_m: Some(28.61),
        reference_body_wheelbase_m: Some(31.88),
        ..Default::default()
    };
    let layout = size_landing_gear(560_000.0, 6.0, 35.0, 8.0, 31.0, 7.14, 8.0, &config);
    assert_eq!(layout.mlg_wheels_per_strut, vec![4, 4, 6, 6]);
    assert_eq!(
        layout
            .wheels
            .iter()
            .filter(|wheel| wheel.group == "MLG")
            .count(),
        20
    );
    assert_eq!(layout.track_width_m, 14.34);
    assert_eq!(layout.reference_wheelbase_m, Some(28.61));
    assert_eq!(layout.reference_body_wheelbase_m, Some(31.88));
    assert_eq!(
        layout
            .wheels
            .iter()
            .filter(|wheel| wheel.strut_label == "MLG-Body-L")
            .count(),
        6
    );
    assert_eq!(
        layout
            .wheels
            .iter()
            .filter(|wheel| wheel.strut_label == "MLG-Body-R")
            .count(),
        6
    );
    // Declared (source-backed) bogie counts: the strength check is a
    // meaningful adequacy check on this layout.
    assert!(layout.capacity_basis_declared);
}

#[test]
fn reference_track_scales_with_active_fuselage_diameter() {
    let config = LandingGearConfig {
        n_mlg_struts: 3,
        mlg_strut_bogie_wheels: Some(vec![4, 4, 2]),
        track_diameter_factor: 10.684 / 5.64,
        reference_track_m: Some(10.684),
        ..Default::default()
    };

    let baseline = size_landing_gear(260_000.0, 6.0, 31.0, 8.0, 27.0, 5.64, 6.0, &config);
    let resized = size_landing_gear(260_000.0, 6.0, 31.0, 8.0, 27.0, 6.20, 6.0, &config);

    assert!((baseline.track_width_m - 10.684).abs() < 1e-12);
    assert!((resized.track_width_m - 6.20 * (10.684 / 5.64)).abs() < 1e-12);
    assert_eq!(resized.reference_track_m, Some(10.684));
    assert!(resized.track_width_m > baseline.track_width_m);
}

#[test]
fn malformed_bogie_list_falls_back_without_dropping_a_leg() {
    let config = LandingGearConfig {
        n_mlg_struts: 3,
        mlg_strut_bogie_wheels: Some(vec![4, 4]),
        ..Default::default()
    };
    let layout = size_landing_gear(120_000.0, 6.0, 25.0, 8.0, 22.0, 5.0, 5.0, &config);
    assert_eq!(layout.mlg_wheels_per_strut.len(), 3);
    assert_eq!(
        layout
            .wheels
            .iter()
            .filter(|wheel| wheel.strut_label == "MLG-Body-C")
            .count(),
        layout.mlg_wheels_per_strut[2] as usize
    );
}

#[test]
fn group_station_sizing_keeps_each_main_gear_axle_in_its_source_position() {
    let config = LandingGearConfig {
        n_mlg_struts: 4,
        mlg_strut_bogie_wheels: Some(vec![4, 4, 6, 6]),
        track_diameter_factor: 14.34 / 7.14,
        reference_track_m: Some(14.34),
        ..Default::default()
    };
    let main_gear_x_m = [33.58, 33.58, 36.85, 36.85];
    let layout = size_landing_gear_with_group_stations(
        560_000.0,
        4.97,
        33.58,
        20.0,
        30.0,
        7.14,
        8.0,
        &main_gear_x_m,
        0.0,
        &config,
    );
    assert_eq!(layout.main_gear_x_m, main_gear_x_m);
    assert!((layout.x_mlg_aft_axle_m - 36.85).abs() < 1e-12);
    let body_wheels: Vec<&Wheel> = layout
        .wheels
        .iter()
        .filter(|wheel| wheel.strut_label.starts_with("MLG-Body"))
        .collect();
    let body_wheel_centroid =
        body_wheels.iter().map(|wheel| wheel.x).sum::<f64>() / body_wheels.len() as f64;
    assert!((body_wheel_centroid - 36.85).abs() < 1.0e-12);
    assert!((layout.wheelbase_m - (33.58 - 4.97)).abs() < 1.0e-12);
    assert!((layout.effective_x_mlg_m - 35.542).abs() < 1.0e-12);
    assert!((layout.effective_wheelbase_m - (35.542 - 4.97)).abs() < 1.0e-12);
}

#[test]
fn the_dynamic_nose_reaction_is_reported_and_exceeds_the_static_one() {
    let config = LandingGearConfig::default();
    let layout = size_landing_gear(80_000.0, 5.0, 18.0, 8.0, 16.0, 4.0, 3.6, &config);
    assert!(layout.r_nlg_dynamic_kg > 0.0);
    assert!(layout.r_nlg_dynamic_kg > layout.r_nlg_design_kg);
}

#[test]
fn a_registered_maximum_ramp_weight_raises_the_main_gear_design_load() {
    let config = LandingGearConfig::default();
    let at_mtow = size_landing_gear_at_design_state(
        80_000.0,
        None,
        5.0,
        18.0,
        8.0,
        16.0,
        None,
        0.0,
        geometry::TipBackBasis::default(),
        4.0,
        3.6,
        &[18.0],
        &config,
    );
    let at_mrw = size_landing_gear_at_design_state(
        80_000.0,
        Some(82_000.0),
        5.0,
        18.0,
        8.0,
        16.0,
        None,
        0.0,
        geometry::TipBackBasis::default(),
        4.0,
        3.6,
        &[18.0],
        &config,
    );
    assert!(at_mrw.r_mlg_total_design_kg > at_mtow.r_mlg_total_design_kg);
}

#[test]
fn a_most_aft_loading_state_can_widen_the_main_gear_design_envelope() {
    let config = LandingGearConfig::default();
    let aero_limit_only = size_landing_gear_at_design_state(
        80_000.0,
        None,
        5.0,
        18.0,
        8.0,
        16.0,
        None,
        0.0,
        geometry::TipBackBasis::default(),
        4.0,
        3.6,
        &[18.0],
        &config,
    );
    let with_aft_state = size_landing_gear_at_design_state(
        80_000.0,
        None,
        5.0,
        18.0,
        8.0,
        16.0,
        Some(17.5),
        0.0,
        geometry::TipBackBasis::default(),
        4.0,
        3.6,
        &[18.0],
        &config,
    );
    assert!(with_aft_state.r_mlg_total_design_kg >= aero_limit_only.r_mlg_total_design_kg);
}

#[test]
fn an_undersized_forced_wheel_count_is_flagged_overloaded_not_hidden() {
    let config = LandingGearConfig {
        n_nlg_wheels: 1,
        wheels_per_mlg_strut: 2,
        tire_class: "light".to_owned(),
        ..Default::default()
    };
    // A heavy aircraft forced onto a single light nose wheel and 2 light
    // main tires per strut: massively under-rated on both ends.
    let layout = size_landing_gear(300_000.0, 6.0, 35.0, 10.0, 30.0, 6.0, 5.0, &config);
    assert!(layout.tire_overloaded);
    assert!(layout.nlg_tire_margin < 1.0);
    assert!(layout.mlg_tire_margin < 1.0);
}

#[test]
fn tip_back_angle_and_floor_are_reported_on_the_layout() {
    let config = LandingGearConfig::default();
    let layout = size_landing_gear(80_000.0, 5.0, 18.0, 8.0, 16.0, 4.0, 3.6, &config);
    // atan((18 - 16)/3.6) = 29.05 deg > default 15 deg floor.
    assert!((layout.tip_back_angle_deg - 29.054_604).abs() < 1e-3);
    assert!(layout.tip_back_ok);
}

#[test]
fn a_nose_tire_over_its_dynamic_rating_is_an_overload_not_a_missing_design_margin() {
    // Rated 1000 kg, dynamic factor 1.5: a braking load of 1550 kg per wheel
    // is 1.033 of the dynamic rating while the static load (700 kg x 1.07 =
    // 749 kg) is far inside the rating. The design-load margin alone
    // (rated / max(749, 1550 / 1.5) = 0.97) cannot tell this from a tire that
    // merely lacks the 7 % design factor; the ratios can.
    let tire = tires::TireSpec {
        rated_load_kg: 1000.0,
        ..tires::NARROWBODY
    };
    let ratios = tires::nose_load_ratios(700.0, 1550.0, &tire, 1.5);
    assert!((ratios.nose_static - 0.7).abs() < 1e-12);
    assert!((ratios.nose_dynamic - 1550.0 / 1500.0).abs() < 1e-12);
    assert!(ratios.nose_dynamic > 1.0 && ratios.nose_static < 1.0);
}

#[test]
fn a_main_tire_at_97_percent_of_rating_lacks_only_the_design_margin() {
    let tire = tires::TireSpec {
        rated_load_kg: 1000.0,
        ..tires::NARROWBODY
    };
    let mut ratios = tires::nose_load_ratios(0.0, 0.0, &tire, 1.5);
    ratios.note_main(970.0, &tire);
    ratios.note_main(500.0, &tire);
    assert!((ratios.main_static - 0.97).abs() < 1e-12);
    // margin including the 1.07 design factor: 1000 / (970 x 1.07) = 0.9635.
    assert!(tires::margin(1000.0, 970.0 * 1.07) < 1.0);
}

#[test]
fn an_undersized_forced_layout_reports_real_overload_ratios() {
    let config = LandingGearConfig {
        n_nlg_wheels: 1,
        wheels_per_mlg_strut: 2,
        tire_class: "light".to_owned(),
        ..Default::default()
    };
    let layout = size_landing_gear(300_000.0, 6.0, 35.0, 10.0, 30.0, 6.0, 5.0, &config);
    let ratios = layout.tire_load_ratios;
    assert!(ratios.nose_static > 1.0 && ratios.main_static > 1.0);
}

#[test]
fn the_safety_factor_applies_to_the_tire_selection_as_well_as_the_overload_flag() {
    // 6 wheels carry 137,667 kg: 22,944.5 kg per wheel is inside the
    // 24,000 kg widebody rating unfactored, 24,551 kg with the 1.07 factor
    // is not, so the heavy class is selected and the bogie is not
    // overloaded (TYRE107).
    let (wheels, tire, overloaded) = tires::size_bogie(137_667.0, 1.07, "auto", 6);
    assert_eq!((wheels, tire.code, overloaded), (6, "heavy", false));
    let (_, widebody, _) = tires::size_bogie(137_667.0, 1.0, "auto", 6);
    assert_eq!(widebody.code, "widebody");
}

#[test]
fn a_printed_braking_load_below_the_rule_factor_governs_the_dynamic_rating() {
    let tire = tires::TireSpec {
        rated_load_kg: 1000.0,
        max_braking_load_kg: Some(1450.0),
        ..tires::NARROWBODY
    };
    let ratios = tires::nose_load_ratios(0.0, 1450.0, &tire, 1.5);
    assert!((ratios.nose_dynamic - 1.0).abs() < 1e-12);
    let class = tires::nose_load_ratios(0.0, 1450.0, &tires::NARROWBODY, 1.5);
    assert!((class.nose_dynamic - 1450.0 / (1.5 * tires::NARROWBODY.rated_load_kg)).abs() < 1e-12);
}

#[test]
fn a_printed_braking_load_of_one_and_a_half_times_keeps_the_rule_factor_bit_for_bit() {
    // Every certified row printing 1.50 x rated (or above, e.g. 40x16.0R16:
    // 47,225 / 31,475 = 1.5004) is checked against the rule factor exactly,
    // whatever the last bit of its lb-to-kg quotient; only a lower printed
    // ratio (the 1.45 rows) replaces it.
    for tire in tires::CERTIFIED_TIRES {
        let Some(braking) = tire.max_braking_load_kg else {
            continue;
        };
        let printed = braking / tire.rated_load_kg;
        let ratios = tires::nose_load_ratios(0.0, 1000.0, &tire, 1.5);
        let rule = 1000.0 / (1.5 * tire.rated_load_kg);
        if printed > 1.5 - 1e-6 {
            assert_eq!(
                ratios.nose_dynamic.to_bits(),
                rule.to_bits(),
                "{}",
                tire.code
            );
        } else {
            assert!(printed < 1.5 && ratios.nose_dynamic > rule, "{}", tire.code);
        }
    }
}

/// Design weight 80 t, wheelbase 13 m (nose 5 m, main 18 m).
fn behind_the_gear(fraction: f64) -> LandingGearLayout {
    let config = LandingGearConfig::default();
    size_landing_gear_at_design_state(
        80_000.0,
        None,
        5.0,
        18.0,
        8.0,
        // Aerodynamic aft limit 2 m behind the main gear: the raw moment
        // balance gives a nose reaction of -12.3 t.
        20.0,
        None,
        fraction,
        geometry::TipBackBasis::default(),
        4.0,
        3.6,
        &[18.0],
        &config,
    )
}

#[test]
fn an_aft_design_cg_behind_the_main_gear_never_gives_a_main_load_above_the_weight() {
    let layout = behind_the_gear(0.0);
    assert!(layout.aero_aft_limit_pulled_back);
    assert!(layout.r_nlg_aft_design_kg >= 0.0);
    assert!(layout.r_mlg_total_design_kg <= 80_000.0 + 1e-9);
}

#[test]
fn the_main_design_reaction_is_the_weight_less_the_minimum_nose_share() {
    // 6.68 % is the Boeing 747-400 ACAP nose share at 877,000 lb.
    let layout = behind_the_gear(0.0668);
    assert!(layout.aero_aft_limit_pulled_back);
    assert!((layout.r_mlg_total_design_kg - 80_000.0 * (1.0 - 0.0668)).abs() < 1e-9);
    assert!((layout.r_nlg_aft_design_kg - 80_000.0 * 0.0668).abs() < 1e-9);
}

#[test]
fn an_aft_design_cg_ahead_of_the_minimum_nose_station_is_not_limited() {
    // Aft limit 16 m: raw nose share (18 - 16) / 13 = 15.4 %, above 5 %.
    let config = LandingGearConfig::default();
    let layout = size_landing_gear_at_design_state(
        80_000.0,
        None,
        5.0,
        18.0,
        8.0,
        16.0,
        None,
        0.05,
        geometry::TipBackBasis::default(),
        4.0,
        3.6,
        &[18.0],
        &config,
    );
    assert!(!layout.aero_aft_limit_pulled_back);
    let nose = 80_000.0 * (18.0 - 16.0) / 13.0;
    assert!((layout.r_nlg_aft_design_kg - nose).abs() < 1e-9);
    assert!((layout.r_mlg_total_design_kg - (80_000.0 - nose)).abs() < 1e-9);
}

#[test]
fn a_non_finite_minimum_nose_share_falls_back_to_zero_not_to_nan_loads() {
    let layout = behind_the_gear(f64::NAN);
    assert!(layout.r_mlg_total_design_kg.is_finite());
    assert!(layout.r_mlg_total_design_kg <= 80_000.0 + 1e-9);
}

#[test]
fn a_design_heavier_than_the_published_reference_keeps_the_aft_cg_share_not_the_load() {
    // Boeing 747-400 ACAP section 7.3: 4 x 204,600 lb at 877,000 lb (397,800
    // kg). A design 10 % heavier is sized at the same nose share, so its main
    // reaction is (1 - f) x 1.1 W and not the reference aircraft's 371,200 kg.
    let config = LandingGearConfig::default();
    let weight_kg = 1.1 * 397_800.0;
    let fraction = 1.0 - 4.0 * 204_600.0 / 877_000.0;
    let layout = size_landing_gear_at_design_state(
        weight_kg,
        None,
        7.75,
        33.0,
        20.0,
        40.0,
        None,
        fraction,
        geometry::TipBackBasis::default(),
        6.5,
        4.0,
        &[33.0],
        &config,
    );
    assert!(layout.aero_aft_limit_pulled_back);
    assert!((layout.r_mlg_total_design_kg - (1.0 - fraction) * weight_kg).abs() < 1e-6);
    assert!(layout.r_mlg_total_design_kg > 1.09 * 371_000.0);
}

#[test]
fn a_loading_state_behind_the_minimum_nose_station_is_not_pulled_forward() {
    // Nose 5 m, main 18 m, wheelbase 13 m; the 5 % station is 17.35 m. The
    // aerodynamic limit (20 m) is pulled back to it, but a loading state the
    // aircraft really reaches at 17.9 m is not: tip-back and the main
    // reaction are evaluated at the state.
    let config = LandingGearConfig::default();
    let layout = size_landing_gear_at_design_state(
        80_000.0,
        None,
        5.0,
        18.0,
        8.0,
        20.0,
        Some(17.9),
        0.05,
        geometry::TipBackBasis::default(),
        4.0,
        3.6,
        &[18.0],
        &config,
    );
    let nose_kg = 80_000.0 * (18.0 - 17.9) / 13.0;
    assert!(layout.aero_aft_limit_pulled_back);
    assert!((layout.r_nlg_aft_design_kg - nose_kg).abs() < 1e-9);
    assert!((layout.r_mlg_total_design_kg - (80_000.0 - nose_kg)).abs() < 1e-9);
    assert!(layout.r_nlg_aft_design_kg < 0.05 * 80_000.0);
    let expected = geometry::tip_back_angle_deg(18.0, 17.9, 3.6);
    assert!((layout.tip_back_angle_deg - expected).abs() < 1e-12);
}

/// Layout of the E195-E2 as the dump sizes it (nose 4.55 m, main axle 19.49 m,
/// analysed CG 18.602 m, 1.1 D = 3.32 m height estimate), 15 deg floor as the
/// presets set. The CG envelope's own ground-referenced height of the analysed
/// takeoff state is 2.70 m and its potato aft extreme is 19.20 m (34.0 % MAC).
fn tip_back_layout(basis: geometry::TipBackBasis) -> LandingGearLayout {
    let config = LandingGearConfig {
        min_tip_back_deg: 15.0,
        ..LandingGearConfig::default()
    };
    size_landing_gear_at_design_state(
        62_413.0,
        None,
        4.55,
        19.49,
        10.0,
        18.602,
        None,
        0.05,
        basis,
        3.5,
        3.32,
        &[19.49],
        &config,
    )
}

#[test]
fn the_tip_back_flag_uses_the_envelopes_height_and_the_most_aft_reachable_cg() {
    // The 1.1 D height estimate alone: 14.98 deg at the analysed CG, not OK.
    let estimate = tip_back_layout(geometry::TipBackBasis::default());
    assert!(!estimate.tip_back_ok, "{}", estimate.tip_back_angle_deg);
    // The envelope's ground-referenced height (2.70 m): the analysed CG clears.
    let analysed = tip_back_layout(geometry::TipBackBasis {
        cg_height_m: Some(2.70),
        ..geometry::TipBackBasis::default()
    });
    assert!(analysed.tip_back_ok, "{}", analysed.tip_back_angle_deg);
    // The potato extreme 0.6 m further aft fails at that same height (6.1 deg).
    let reachable = tip_back_layout(geometry::TipBackBasis {
        most_aft_ground_cg_x_m: Some(19.20),
        scrape_angle_deg: None,
        cg_height_m: Some(2.70),
    });
    assert!(!reachable.tip_back_ok);
    assert!((reachable.tip_back_angle_deg - 6.1).abs() < 0.1);
    // A reachable CG ahead of the design CG never relaxes the check.
    let ahead = tip_back_layout(geometry::TipBackBasis {
        most_aft_ground_cg_x_m: Some(15.0),
        scrape_angle_deg: None,
        cg_height_m: Some(2.70),
    });
    assert_eq!(ahead.tip_back_angle_deg, analysed.tip_back_angle_deg);
}

#[test]
fn an_a380_at_its_potato_extreme_clears_tip_back_with_the_envelope_height() {
    // A380 dump: aft axle 36.85 m, potato extreme 34.84 m (49.4 % MAC), the
    // envelope's analysed-takeoff CG height 5.26 m: 20.9 deg, OK. The earlier
    // "14.4 deg" used the 1.1 D height estimate (7.85 m), not the envelope's.
    let config = LandingGearConfig {
        min_tip_back_deg: 15.0,
        ..LandingGearConfig::default()
    };
    let layout = size_landing_gear_at_design_state(
        560_000.0,
        None,
        4.97,
        36.85,
        28.0,
        33.55,
        None,
        0.05,
        geometry::TipBackBasis {
            most_aft_ground_cg_x_m: Some(34.84),
            scrape_angle_deg: Some(11.23),
            cg_height_m: Some(5.26),
        },
        7.14,
        7.85,
        &[36.85],
        &config,
    );
    assert!(layout.tip_back_ok, "{}", layout.tip_back_angle_deg);
    assert!((layout.tip_back_angle_deg - 20.9).abs() < 0.2);
}

#[test]
fn the_tail_scrape_angle_raises_the_required_tip_back_angle() {
    let basis = geometry::TipBackBasis {
        most_aft_ground_cg_x_m: None,
        scrape_angle_deg: Some(21.0),
        cg_height_m: Some(2.70),
    };
    let layout = tip_back_layout(basis);
    assert_eq!(layout.required_tip_back_deg, 21.0);
    assert!(!layout.tip_back_ok, "18.2 deg against a 21 deg scrape");
}

#[test]
fn the_layout_flag_and_the_envelope_limit_station_cannot_disagree() {
    // tip_back_angle >= required  <=>  x_cg <= tip_back_limit_x.
    for (axle, h, required) in [(36.85, 9.24, 15.0), (20.4, 8.45, 18.0), (12.5, 3.15, 8.0)] {
        let limit = geometry::tip_back_limit_x_m(axle, h, required);
        for dx in [-0.5, -1e-6, 1e-6, 0.5] {
            let x = limit + dx;
            let by_angle = geometry::tip_back_angle_deg(axle, x, h) >= required;
            assert_eq!(
                by_angle,
                x <= limit,
                "axle {axle} h {h} req {required} dx {dx}"
            );
        }
    }
    assert_eq!(geometry::required_tip_back_deg(15.0, Some(8.8)), 15.0);
    assert_eq!(geometry::required_tip_back_deg(15.0, Some(21.0)), 21.0);
    assert_eq!(geometry::required_tip_back_deg(15.0, None), 15.0);
}
