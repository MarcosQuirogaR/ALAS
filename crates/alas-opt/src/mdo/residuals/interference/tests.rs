// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A test unwrap is the assertion failing on a registered preset or geometry.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

use super::*;
use crate::mdo::sizing::run_candidate;

fn pose(pitch_deg: f64, roll_deg: f64) -> GroundPose {
    GroundPose {
        ground_z_m: -3.0,
        x_main_gear_m: 20.0,
        y_outboard_wheel_m: 4.0,
        pitch_rad: pitch_deg.to_radians(),
        roll_rad: roll_deg.to_radians(),
    }
}

#[test]
fn an_unposed_point_stands_at_its_height_above_the_ground() {
    let height = pose(0.0, 0.0).height_m([25.0, 7.0, -1.0]);
    assert!((height - 2.0).abs() < 1.0e-12);
}

#[test]
fn pitching_nose_up_lowers_points_aft_of_the_wheels_and_raises_those_forward() {
    let aft = pose(10.0, 0.0).height_m([30.0, 0.0, -1.0]);
    let forward = pose(10.0, 0.0).height_m([10.0, 0.0, -1.0]);
    // h = 2 m above ground; dx = +/-10 m.
    let expected_aft = 2.0 * 10.0_f64.to_radians().cos() - 10.0 * 10.0_f64.to_radians().sin();
    let expected_forward = 2.0 * 10.0_f64.to_radians().cos() + 10.0 * 10.0_f64.to_radians().sin();
    assert!((aft - expected_aft).abs() < 1.0e-12);
    assert!((forward - expected_forward).abs() < 1.0e-12);
}

#[test]
fn rolling_lowers_points_outboard_of_the_wheel_and_raises_those_inboard() {
    let outboard = pose(0.0, 5.0).height_m([20.0, 14.0, -1.0]);
    let inboard = pose(0.0, 5.0).height_m([20.0, 1.0, -1.0]);
    let (s, c) = 5.0_f64.to_radians().sin_cos();
    assert!((outboard - (2.0 * c - 10.0 * s)).abs() < 1.0e-12);
    assert!((inboard - (2.0 * c + 3.0 * s)).abs() < 1.0e-12);
    // The mirror-image point on the other wing is the same.
    assert_eq!(
        pose(0.0, 5.0).height_m([20.0, -14.0, -1.0]),
        pose(0.0, 5.0).height_m([20.0, 14.0, -1.0])
    );
}

fn tube(name: &str, y: f64, z: f64, radius: f64) -> Fuselage {
    let section = |x: f64| FuselageXSec::new([x, y, z], Some(radius), None, None, 2.0).unwrap();
    Fuselage::new(name, vec![section(0.0), section(10.0)])
}

#[test]
// The boundaries are 64-point polylines: millimetres of polygon error.
fn the_gap_between_two_round_bodies_is_the_distance_less_both_radii() {
    let a = tube("a", 0.0, 0.0, 2.0);
    let b = tube("b", 5.0, 0.0, 1.0);
    assert!((body_gap_m(&a, &b).unwrap() - 2.0).abs() < 1.0e-9);
    let overlapping = tube("c", 2.5, 0.0, 1.0);
    assert!((body_gap_m(&a, &overlapping).unwrap() + 0.5).abs() < 5.0e-3);
    // Diagonal separation, centre distance 5 (3-4-5).
    let diagonal = tube("d", 3.0, 4.0, 1.0);
    assert!((body_gap_m(&a, &diagonal).unwrap() - 2.0).abs() < 5.0e-3);
}

#[test]
fn bodies_with_no_common_station_have_no_gap() {
    let a = tube("a", 0.0, 0.0, 1.0);
    let far = Fuselage::new(
        "far",
        vec![
            FuselageXSec::new([20.0, 5.0, 0.0], Some(1.0), None, None, 2.0).unwrap(),
            FuselageXSec::new([30.0, 5.0, 0.0], Some(1.0), None, None, 2.0).unwrap(),
        ],
    );
    assert!(body_gap_m(&a, &far).is_none());
}

/// Probe: the registered presets' interference rows.
#[test]
#[ignore = "diagnostic table of all registered presets; prints"]
fn registered_preset_interference_table() {
    for name in alas_config::presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let Ok(outcome) = run_candidate(&config, &design.to_array()) else {
            eprintln!("{name}: nominal candidate does not evaluate");
            continue;
        };
        let rows = interference_residuals(&outcome, &config, ResidualRole::Constraint);
        let text: Vec<String> = rows
            .iter()
            .map(|row| format!("{} {:.3}", row.id, row.actual))
            .collect();
        if let Some(pose) = ground_pose(
            &outcome.plane,
            &config,
            alas_mass::stations::GroundPlaneBasis::Nominal,
        ) {
            let flat = GroundPose {
                pitch_rad: 0.0,
                roll_rad: 0.0,
                ..pose
            };
            let hold = config.geometry.wing.nacelles_hold_ground_height;
            let rise = flight_rise_fn(&outcome.plane, &config);
            eprintln!(
                "   static nacelle low point {:?} m (hold {hold})",
                nacelle_clearance_m(&outcome.plane, &flat, &|y| if hold { 0.0 } else { rise(y) })
            );
            eprintln!(
                "{name}: pitch {:.2} deg, x_mlg {:.2}, wheel y {:.2}; {}",
                pose.pitch_rad.to_degrees(),
                pose.x_main_gear_m,
                pose.y_outboard_wheel_m,
                text.join("; ")
            );
        }
        let bodies: Vec<String> = outcome
            .plane
            .fuselages
            .iter()
            .map(|b| b.name.clone())
            .collect();
        let wings: Vec<String> = outcome.plane.wings.iter().map(|w| w.name.clone()).collect();
        eprintln!("   bodies {bodies:?} wings {wings:?}");
    }
}

/// Probe: tail geometry of the registered presets.
#[test]
#[ignore = "diagnostic table of all registered presets; prints"]
fn registered_preset_tail_table() {
    for name in alas_config::presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let Ok(outcome) = run_candidate(&config, &design.to_array()) else {
            continue;
        };
        let plane = &outcome.plane;
        let fus = &plane.fuselages[0];
        let end = fus.xsecs.last().unwrap();
        let htp = &plane.wings[1];
        let fin = &plane.wings[2];
        let root = &htp.xsecs[0];
        let tip = htp.xsecs.last().unwrap();
        let fin_root = &fin.xsecs[0];
        let fin_tip = fin.xsecs.last().unwrap();
        let at = section_at(fus, root.xyz_le[0]).unwrap_or(*end);
        eprintln!(
            "{name}: fus end x {:.2} (z {:.2} h {:.2}); HTP root LE ({:.2},{:.2},{:.2}) c {:.2}, tip ({:.2},{:.2},{:.2}); fuselage at root x: z {:.2} h {:.2} w {:.2}; fin root ({:.2},{:.2},{:.2}) c {:.2}, tip ({:.2},{:.2},{:.2}) c {:.2}",
            end.xyz_c[0], end.xyz_c[2], end.height,
            root.xyz_le[0], root.xyz_le[1], root.xyz_le[2], root.chord,
            tip.xyz_le[0], tip.xyz_le[1], tip.xyz_le[2],
            at.xyz_c[2], at.height, at.width,
            fin_root.xyz_le[0], fin_root.xyz_le[1], fin_root.xyz_le[2], fin_root.chord,
            fin_tip.xyz_le[0], fin_tip.xyz_le[1], fin_tip.xyz_le[2], fin_tip.chord,
        );
    }
}

/// Every registered preset is a clear, attached airframe: no interference
/// row is violated, and every row exists (a missing row would hide a check).
#[test]
fn every_registered_preset_passes_the_interference_checks() {
    const ROWS: [&str; 10] = [
        "nacelle_clear_of_fuselage",
        "nacelle_clear_of_nacelle",
        "wing_tip_ground_clearance_in_roll",
        "nacelle_ground_clearance_in_roll",
        "tail_ground_clearance_in_roll",
        "tail_root_attached",
        "nacelle_clear_of_wing",
        "main_gear_clear_of_nacelle",
        "propeller_ground_clearance",
        "propeller_ground_clearance_tyre_deflated",
    ];
    let mut failures = Vec::new();
    let mut evaluated = 0;
    for name in alas_config::presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let Ok(outcome) = run_candidate(&config, &design.to_array()) else {
            failures.push(format!("{name}: the nominal candidate does not evaluate"));
            continue;
        };
        evaluated += 1;
        let rows = interference_residuals(&outcome, &config, ResidualRole::Constraint);
        for id in &ROWS {
            // The engine-body pair row exists only with two or more engines.
            if *id == "nacelle_clear_of_nacelle" {
                continue;
            }
            // A turboprop nacelle is faired into the wing: no wing gap row.
            let turboprop = matches!(
                config.geometry.engine.active_model(),
                Ok(alas_config::ActiveEngineModel::Turboprop(_))
            );
            if *id == "nacelle_clear_of_wing" && turboprop {
                continue;
            }
            // A propeller row exists only for a turboprop.
            if id.starts_with("propeller_") && !turboprop {
                continue;
            }
            if !rows.iter().any(|row| row.id == *id) {
                failures.push(format!("{name}: no {id} row"));
            }
        }
        for row in rows.iter().filter(|row| row.violated()) {
            failures.push(format!("{name}: {} = {:.3} m", row.id, row.actual));
        }
    }
    // No registered preset violates a row. The E195-E2 used to fail the nacelle
    // roll row (-0.344 m) because it registered no `fuselage_ground_clearance_m`
    // and the 0.35 D fallback hung the ground plane 1.057 m under the belly,
    // 0.59 m above the plane its own APM readings imply: it now registers the
    // 1.65 m those readings give (nose B, nacelle F, tip G of APM 5824 Table 2.7
    // agree within 0.08 m), see the preset comment.
    assert_eq!(
        failures,
        Vec::<String>::new(),
        "registered-preset interference failures"
    );
    assert!(evaluated >= 8, "{evaluated} presets evaluated");
}

/// A detached tail root and a ground strike are violations; an overlapping
/// engine body is one too.
#[test]
fn a_moved_tail_a_lowered_wing_and_an_overlapping_engine_are_violations() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    let fresh = || run_candidate(&config, &design.to_array()).unwrap();
    let outcome = fresh();
    let violated = |outcome: &SizingOutcome, id: &str| {
        interference_residuals(outcome, &config, ResidualRole::Constraint)
            .into_iter()
            .find(|row| row.id == id)
            .unwrap()
            .violated()
    };
    for id in [
        "tail_root_attached",
        "wing_tip_ground_clearance_in_roll",
        "nacelle_clear_of_fuselage",
        "tail_ground_clearance_in_roll",
    ] {
        assert!(!violated(&outcome, id), "{id} on the registered A320");
    }
    // The tail slid 8 m aft of the fuselage end hangs in free air.
    let mut detached = fresh();
    for xsec in &mut detached.plane.wings[1].xsecs {
        xsec.xyz_le[0] += 8.0;
    }
    assert!(violated(&detached, "tail_root_attached"));
    // A wing hung 3 m lower strikes the ground in the 5 degree roll.
    let mut low = fresh();
    for xsec in &mut low.plane.wings[0].xsecs {
        xsec.xyz_le[2] -= 3.0;
    }
    assert!(violated(&low, "wing_tip_ground_clearance_in_roll"));
    // An engine body pulled onto the fuselage overlaps it.
    let mut overlapping = fresh();
    for body in overlapping.plane.fuselages.iter_mut().skip(1) {
        for xsec in &mut body.xsecs {
            xsec.xyz_c[1] *= 0.2;
        }
    }
    assert!(violated(&overlapping, "nacelle_clear_of_fuselage"));
}

/// Probe: ground-plane breakdown of one preset (set `G1_PRESET`).
#[test]
#[ignore = "diagnostic; prints"]
fn ground_plane_breakdown() {
    let name = std::env::var("G1_PRESET").unwrap_or_else(|_| "E195-E2".to_owned());
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": name,
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get(&name).unwrap().design_vector;
    let outcome = run_candidate(&config, &design.to_array()).unwrap();
    let plane = &outcome.plane;
    let fus = &plane.fuselages[0];
    let ground = alas_mass::stations::ground_plane_z_m(fus, &config.geometry, &config.landing_gear);
    let belly = fus
        .xsecs
        .iter()
        .map(|x| x.xyz_c[2] - x.height / 2.0)
        .fold(f64::INFINITY, f64::min);
    let nose = fus.xsecs.first().unwrap();
    let tip = plane.wings[0].xsecs.last().unwrap();
    let (nacelle_low_x, nacelle_low) = plane
        .fuselages
        .iter()
        .skip(1)
        .flat_map(|b| b.xsecs.iter())
        .map(|x| (x.xyz_c[0], x.xyz_c[2] - x.height / 2.0))
        .fold(
            (f64::NAN, f64::INFINITY),
            |a, b| if b.1 < a.1 { b } else { a },
        );
    eprintln!(
        "{name}: x stations: nose {:.2}, nacelle low {:.2}, tip LE {:.2}, main gear {:.2}",
        nose.xyz_c[0],
        nacelle_low_x,
        tip.xyz_le[0],
        ground_pose(
            plane,
            &config,
            alas_mass::stations::GroundPlaneBasis::Nominal
        )
        .map_or(f64::NAN, |p| p.x_main_gear_m),
    );
    eprintln!(
        "{name}: ground z {ground:.3} belly z {belly:.3} (clearance {:.3}); nose bottom above ground {:.3}; nacelle low above ground {:.3}; tip LE above ground {:.3}; body axis above ground {:.3}; diameter {:.3}",
        belly - ground,
        nose.xyz_c[2] - nose.height / 2.0 - ground,
        nacelle_low - ground,
        tip.xyz_le[2] - ground,
        fus.xsecs[fus.xsecs.len() / 2].xyz_c[2] - ground,
        config.geometry.fuselage.diameter_m,
    );
    let lower: Vec<String> = fus
        .xsecs
        .iter()
        .map(|x| {
            format!(
                "x {:.2}: {:.3}",
                x.xyz_c[0],
                x.xyz_c[2] - x.height / 2.0 - ground
            )
        })
        .collect();
    eprintln!(
        "{name}: fuselage lower contour above ground: {}",
        lower.join("; ")
    );
    let axes: Vec<String> = plane
        .fuselages
        .iter()
        .skip(1)
        .filter_map(|b| b.xsecs.first())
        .map(|x| {
            format!(
                "x {:.2} y {:.2}: {:.3}",
                x.xyz_c[0],
                x.xyz_c[1],
                x.xyz_c[2] - ground
            )
        })
        .collect();
    eprintln!(
        "{name}: nacelle front-section axis above ground: {}",
        axes.join("; ")
    );
}

/// A nacelle pushed up into the wing violates the wing-gap row, whatever the
/// position of its axis (no geometric exemption), all the way through the
/// wing, and clears it only once it rides wholly above the upper surface (the
/// aft-fuselage installation over a wing root); the registered A320 has a
/// positive gap.
#[test]
fn a_nacelle_raised_into_the_wing_violates_the_wing_gap() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    let mut outcome = run_candidate(&config, &design.to_array()).unwrap();
    let height_m = outcome
        .plane
        .fuselages
        .iter()
        .skip(1)
        .flat_map(|body| body.xsecs.iter().map(|xsec| xsec.height))
        .fold(0.0, f64::max);
    let mut raised_m = 0.0;
    let mut gap = |raise: f64| {
        for body in outcome.plane.fuselages.iter_mut().skip(1) {
            for xsec in &mut body.xsecs {
                xsec.xyz_c[2] += raise - raised_m;
            }
        }
        raised_m = raise;
        interference_residuals(&outcome, &config, ResidualRole::Constraint)
            .into_iter()
            .find(|row| row.id == "nacelle_clear_of_wing")
            .unwrap()
    };
    let registered = gap(0.0);
    assert!(registered.actual > 0.0 && !registered.violated());
    let near = gap(registered.actual + 0.3);
    assert!(near.violated() && near.actual < 0.0);
    // The whole crossing is violated, with no hole where the axis passes the
    // wing's mean surface: from the top meeting the lower surface to the
    // bottom leaving the upper one: more than the 0.3 m past first contact
    // the near case shows, less than the tallest section plus a wing depth.
    let raises: Vec<f64> = (0..=200).map(|k| 0.05 * k as f64).collect();
    let violated: Vec<f64> = raises
        .iter()
        .copied()
        .filter(|&raise| gap(raise).violated())
        .collect();
    let (first, last) = (violated[0], violated[violated.len() - 1]);
    assert!(
        last - first > 0.3 && last - first < height_m + 1.0,
        "band {first}..{last} m, body {height_m} m"
    );
    assert_eq!(violated.len(), ((last - first) / 0.05).round() as usize + 1);
    // Wholly above the wing it clears by the free distance.
    let above = gap(last + 1.0);
    assert!(!above.violated() && above.actual > 0.9, "{}", above.actual);
}

/// The propeller disc must clear the ground by the CS 25.925(a) 18 cm: the
/// registered turboprops pass, and a nacelle (hence its propeller axis) hung
/// lower by more than the margin violates the row.
#[test]
fn a_propeller_disc_below_the_cs_25_925_clearance_is_a_violation() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "ATR72-600",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get("ATR72-600")
        .unwrap()
        .design_vector;
    let mut outcome = run_candidate(&config, &design.to_array()).unwrap();
    let gap = |outcome: &SizingOutcome| {
        interference_residuals(outcome, &config, ResidualRole::Constraint)
            .into_iter()
            .find(|row| row.id == "propeller_ground_clearance")
            .expect("a turboprop has a propeller clearance row")
            .actual
    };
    let nominal = gap(&outcome);
    assert!(nominal >= 0.0, "registered ATR 72-600 gap {nominal:.3} m");
    for body in outcome.plane.fuselages.iter_mut().skip(1) {
        for xsec in &mut body.xsecs {
            xsec.xyz_c[2] -= nominal + 0.1;
        }
    }
    let lowered = gap(&outcome);
    assert!(
        (lowered + 0.1).abs() < 1.0e-6,
        "lowering the axis by gap + 0.1 m leaves {lowered:.4} m, expected -0.1"
    );
}

/// Probe: how the FLOPS main-gear oleo length (eq. 66) relates to the
/// registered ground plane, per preset. Prints; asserts nothing.
#[test]
#[ignore = "diagnostic; prints"]
fn flops_oleo_against_registered_ground_plane() {
    for name in alas_config::presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let Ok(outcome) = run_candidate(&config, &design.to_array()) else {
            continue;
        };
        let plane = &outcome.plane;
        let Some(pose) = ground_pose(
            plane,
            &config,
            alas_mass::stations::GroundPlaneBasis::Nominal,
        ) else {
            continue;
        };
        let wing = &plane.wings[0];
        let engines: Vec<&Fuselage> = plane
            .fuselages
            .iter()
            .skip(1)
            .filter(|b| is_engine_body(b))
            .collect();
        let registered = config.landing_gear.fuselage_ground_clearance_m;
        let half_track =
            0.5 * config.geometry.fuselage.diameter_m * config.landing_gear.track_diameter_factor;
        let diameter = engines
            .iter()
            .flat_map(|b| b.xsecs.iter())
            .map(|x| x.height.max(x.width))
            .fold(0.0, f64::max);
        let yee = engines
            .iter()
            .filter_map(|b| b.xsecs.first())
            .map(|x| x.xyz_c[1].abs())
            .fold(0.0, f64::max);
        let root = wing.xsecs.first().unwrap();
        let tip = wing.xsecs.last().unwrap();
        let dih = ((tip.xyz_le[2] - root.xyz_le[2]) / tip.xyz_le[1].abs())
            .atan()
            .to_degrees();
        let xmlg_in = 12.0 * diameter / 0.3048
            + (0.26 - dih.to_radians().tan())
                * (yee / 0.0254 - 6.0 * config.geometry.fuselage.diameter_m / 0.3048);
        let xmlg = xmlg_in * 0.0254;
        let attach = surfaces::wing_lower_z_m(wing, pose.x_main_gear_m, half_track, &|_| 0.0);
        let attach_above_registered = attach.map(|z| z - pose.ground_z_m);
        eprintln!(
            "{name}: registered clearance {registered:?}; engines {}; FNAC {diameter:.2} m, YEE {yee:.2} m, dih {dih:.2} deg, XMLG {xmlg:.3} m; wing lower surface at gear (x {:.2}, y {half_track:.2}) above registered ground {attach_above_registered:?}",
            engines.len(),
            pose.x_main_gear_m,
        );
    }
}

/// With the critical tyre flat and its strut bottomed the propeller must still
/// be clear of the ground (CS 25.925(a), second sentence): the registered
/// turboprops pass, the row sits below the static one by the bottomed drop,
/// and an axis hung lower by more than that margin violates it.
#[test]
fn a_propeller_that_touches_with_a_flat_tyre_and_a_bottomed_strut_is_a_violation() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "ATR72-600",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get("ATR72-600")
        .unwrap()
        .design_vector;
    let mut outcome = run_candidate(&config, &design.to_array()).unwrap();
    let rows = |outcome: &SizingOutcome| {
        let rows = interference_residuals(outcome, &config, ResidualRole::Constraint);
        let get = |id: &str| rows.iter().find(|row| row.id == id).unwrap().actual;
        (
            get("propeller_ground_clearance") + 0.18,
            get("propeller_ground_clearance_tyre_deflated"),
        )
    };
    let (static_height, deflated) = rows(&outcome);
    assert!(
        deflated > 0.0,
        "registered ATR 72-600 deflated gap {deflated:.3} m"
    );
    // Static height less the larger of the two drops (nose case 0.38 of the
    // nose drop at this geometry, main case 1.5 of the main drop) is the gap.
    assert!(
        deflated < static_height - 0.2 && deflated > static_height - 1.5,
        "static {static_height:.3} m, deflated {deflated:.3} m"
    );
    for body in outcome.plane.fuselages.iter_mut().skip(1) {
        for xsec in &mut body.xsecs {
            xsec.xyz_c[2] -= deflated + 0.1;
        }
    }
    assert!(rows(&outcome).1 < 0.0);
}

/// Probe: what the registered ground-plane clearance
/// moves, per preset, against the 0.35 D fallback the preset used before
/// (`CLEARANCE_PROBE_PRESETS`, comma separated; the default is every preset whose
/// clearance was registered from documents). Prints; asserts nothing.
#[test]
#[ignore = "diagnostic; prints"]
fn clearance_before_after_probe() {
    let names = std::env::var("CLEARANCE_PROBE_PRESETS")
        .unwrap_or_else(|_| "E195-E2,C919,DC-10,ATR72-600,A400M,AVE".to_owned());
    for name in names.split(',') {
        let base = AlasConfig::from_value(&serde_json::json!({
            "preset": name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let registered = base.landing_gear.fuselage_ground_clearance_m;
        let extra: Vec<f64> = std::env::var("CLEARANCE_PROBE_EXTRA")
            .ok()
            .map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect())
            .unwrap_or_default();
        let mut variants = vec![
            ("fallback".to_owned(), None),
            ("registered".to_owned(), registered),
        ];
        variants.extend(extra.iter().map(|c| (format!("override {c}"), Some(*c))));
        let diameter_m = base.geometry.fuselage.diameter_m;
        if let Ok(fractions) = std::env::var("CLEARANCE_PROBE_FRACTIONS") {
            for f in fractions.split(',').filter_map(|x| x.parse::<f64>().ok()) {
                variants.push((format!("{f} D"), Some(f * diameter_m)));
            }
        }
        for (label, clearance) in variants {
            let mut config = base.clone();
            config.landing_gear.fuselage_ground_clearance_m = clearance;
            let outcome = run_candidate(&config, &design.to_array()).unwrap();
            let fus = &outcome.plane.fuselages[0];
            let ground =
                alas_mass::stations::ground_plane_z_m(fus, &config.geometry, &config.landing_gear);
            let frame = outcome.plane.mac_frame().unwrap();
            let pct = |x: f64| 100.0 * (x - frame.x_lemac_m) / frame.chord_m;
            let (oew, oew_x) = alas_payload::oew::oew_and_cg(&outcome.masses, &outcome.coords);
            let (mut m_sum, mut mz) = (0.0, 0.0);
            let pairs = outcome.coords.as_pairs();
            for key in alas_mass::breakdown::OEW_KEYS {
                let mass = outcome.masses.get(key).unwrap_or(0.0).max(0.0);
                if let Some((_, xyz)) = pairs.iter().find(|(n, _)| *n == key) {
                    m_sum += mass;
                    mz += mass * xyz[2];
                }
            }
            let oew_z = mz / m_sum;
            let takeoff_z = (mz
                + outcome.masses.payload * outcome.coords.payload[2]
                + outcome.masses.fuel * outcome.coords.fuel[2])
                / (m_sum + outcome.masses.payload + outcome.masses.fuel);
            let gear_z = pairs
                .iter()
                .filter(|(n, _)| n.contains("gear"))
                .map(|(n, xyz)| format!("{n} x {:.3} z {:.3}", xyz[0], xyz[2] - ground))
                .collect::<Vec<_>>()
                .join(", ");
            let assessment = crate::envelope::assess_model_cg_envelope(
                &outcome.plane,
                &outcome.masses,
                &outcome.coords,
                outcome.cg_x,
                outcome.x_np,
                outcome.x_np,
                outcome.mac,
                &config,
            );
            let states = match &assessment {
                Ok(a) => a
                    .loading_states
                    .iter()
                    .map(|s| {
                        format!(
                            "{:?}: cg {:.2} % MAC, nose {:.2} %, tip-back aft {:.2} % MAC",
                            s.state,
                            s.cg_pct_mac,
                            100.0 * s.nose_gear_load_fraction,
                            s.physical_limits.tip_back_aft_pct_mac
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" | "),
                Err(e) => format!("envelope error {e:?}"),
            };
            if std::env::var("CLEARANCE_PROBE_LIMITS").is_ok() {
                if let Ok(a) = &assessment {
                    for s in &a.loading_states {
                        eprintln!(
                            "clearance {name} [{label}] limits {:?}: {:?}",
                            s.state, s.physical_limits
                        );
                    }
                }
            }
            let scrape = assessment.as_ref().ok().and_then(|a| a.scrape_angle_deg);
            let required = assessment
                .as_ref()
                .map_or(f64::NAN, |a| a.required_tip_back_deg);
            let stations = gear_stations(&outcome.plane, &config).unwrap();
            let aft_x = outcome.x_np - config.requirements.target_static_margin * outcome.mac;
            let fwd_x = aft_x
                - crate::envelope::usable_range::UsableRangeRequirement::of(&config)
                    .length_m(outcome.mtow_ceiling, outcome.mac);
            let diameter = config.geometry.fuselage.diameter_m;
            let layout = alas_perf::landing_gear::size_landing_gear_at_design_state(
                outcome.mtow_ceiling,
                None,
                stations.x_nlg_m,
                stations.x_mlg_m,
                fwd_x,
                aft_x,
                Some(outcome.cg_x),
                config.mass_model.pct_load_nlg_min,
                alas_perf::landing_gear::geometry::TipBackBasis::default(),
                diameter,
                takeoff_z - ground,
                &stations.main_gear_x_m,
                &config.landing_gear,
            );
            let rows = interference_residuals(&outcome, &config, ResidualRole::Constraint)
                .into_iter()
                .map(|row| format!("{} {:.3}", row.id, row.actual))
                .collect::<Vec<_>>()
                .join("; ");
            eprintln!("clearance {name} [{label}] rows: {rows}");
            eprintln!(
                "clearance {name} [{label}] clearance {:?}: ground z {ground:.3}; OEW {oew:.0} kg (gear {:.0} kg), OEW cg {:.2} % MAC x {oew_x:.2} m z {:.3} m above ground, takeoff cg z {:.3} m above ground; gear stations ({gear_z}); scrape {scrape:?} deg, required tip-back {required:.2} deg, layout tip-back {:.2} deg, turnover {:.2} deg; nlg tyre {:.3} m, mlg tyre {:.3} m; {states}",
                clearance,
                outcome.masses.gear,
                pct(oew_x),
                oew_z - ground,
                takeoff_z - ground,
                layout.tip_back_angle_deg,
                layout.turnover_angle_deg,
                layout.nlg_tire.diameter_m,
                layout.mlg_tire.diameter_m,
            );
        }
    }
}

/// The propeller rows of the two registered turboprops, pinned: the
/// static row and the tyre-deflated row, from a probe run (`clearance_before_after_probe`).
/// The static gap is the registered ground plane (ATR 0.64 m, A400M 1.21 m)
/// less the 18 cm requirement above the lowest blade tip; the deflated row is
/// that tip less the bottomed stroke and flat-tyre drop (Raymer Eq. 11.12
/// stroke, 0.34 of it below static, rim ratio of the 46x17.0R20 tyre) in the
/// worse of the nose-gear and one-main-gear cases. The registered presets size the
/// mains with their certified data-book tyres (ATR H34x10.0R16, 0.885 m; A400M
/// 43x15.5-17, 1.092 m) instead of the 1.17 m class tyre; by probe (all tyres on
/// auto reproduces the G2 pins) that alone moves the deflated rows: ATR 0.541 ->
/// 0.658 m, A400M 0.863 -> 0.915 m; the static rows do not move.
#[test]
fn the_registered_turboprop_propeller_rows_are_pinned() {
    for (name, static_gap, deflated) in [("ATR72-600", 0.970, 0.658), ("A400M", 1.665, 0.915)] {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let outcome = run_candidate(&config, &design.to_array()).unwrap();
        let rows = interference_residuals(&outcome, &config, ResidualRole::Constraint);
        let get = |id: &str| rows.iter().find(|row| row.id == id).unwrap().actual;
        assert!(
            (get("propeller_ground_clearance") - static_gap).abs() < 0.01,
            "{name} static row {:.3} m, pinned {static_gap}",
            get("propeller_ground_clearance")
        );
        assert!(
            (get("propeller_ground_clearance_tyre_deflated") - deflated).abs() < 0.01,
            "{name} deflated row {:.3} m, pinned {deflated}",
            get("propeller_ground_clearance_tyre_deflated")
        );
    }
}

/// The E195-E2 ground attitude on its APM-read plane: the tail-scrape
/// angle at the main gear and the loaded take-off CG height above ground. Both
/// moved with the plane registered from the APM (1.65 m belly clearance against the
/// 1.057 m 0.35 D fallback): the scrape angle 5.89 -> 8.79 deg (the belly
/// is 0.59 m higher at the same aft lower-contour points), the take-off CG 1.88
/// -> 2.46 m above ground, +0.57 m (the plane dropped 0.593 m, the CG moved
/// with the gear datum by 0.02 m). The 15 deg `min_tip_back_deg` floor
/// governs the requirement before and after. With the tail-cone refit and shoulder offset k of the
/// E195-E2 loft the scrape angle is 8.528 deg.
#[test]
fn the_e195_e2_ground_attitude_is_pinned() {
    let name = "E195-E2";
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": name,
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get(name).unwrap().design_vector;
    let outcome = run_candidate(&config, &design.to_array()).unwrap();
    let pose = ground_pose(
        &outcome.plane,
        &config,
        alas_mass::stations::GroundPlaneBasis::Nominal,
    )
    .unwrap();
    let scrape_deg = pose.pitch_rad.to_degrees();
    assert!(
        (scrape_deg - 8.528).abs() < 0.05,
        "tail-scrape angle {scrape_deg:.3} deg, pinned 8.528"
    );
    let pairs = outcome.coords.as_pairs();
    let (mut mass, mut moment) = (0.0, 0.0);
    for key in alas_mass::breakdown::OEW_KEYS {
        let m = outcome.masses.get(key).unwrap_or(0.0).max(0.0);
        if let Some((_, xyz)) = pairs.iter().find(|(n, _)| *n == key) {
            mass += m;
            moment += m * xyz[2];
        }
    }
    let takeoff_z = (moment
        + outcome.masses.payload * outcome.coords.payload[2]
        + outcome.masses.fuel * outcome.coords.fuel[2])
        / (mass + outcome.masses.payload + outcome.masses.fuel);
    let height = takeoff_z - pose.ground_z_m;
    assert!(
        (height - 2.459).abs() < 0.02,
        "take-off CG height {height:.3} m above ground, pinned 2.459"
    );
}

/// Probe: the cabin floor above the registered ground plane at the declared
/// door stations, and the tip-back boundary of the take-off state at the
/// required and scrape angles in the model MAC frame (`CLEARANCE_PROBE_PRESETS`).
/// Prints; asserts nothing.
#[test]
#[ignore = "diagnostic; prints"]
fn ground_plane_floor_and_tip_back_boundary() {
    let names = std::env::var("CLEARANCE_PROBE_PRESETS").unwrap_or_else(|_| "ATR72-600".to_owned());
    for name in names.split(',') {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let outcome = run_candidate(&config, &design.to_array()).unwrap();
        let fus = &outcome.plane.fuselages[0];
        let ground =
            alas_mass::stations::ground_plane_z_m(fus, &config.geometry, &config.landing_gear);
        let frame = outcome.plane.mac_frame().unwrap();
        let pct = |x: f64| 100.0 * (x - frame.x_lemac_m) / frame.chord_m;
        let stations = gear_stations(&outcome.plane, &config).unwrap();
        let pose = ground_pose(
            &outcome.plane,
            &config,
            alas_mass::stations::GroundPlaneBasis::Nominal,
        )
        .unwrap();
        let (mut mass, mut moment) = (0.0, 0.0);
        let pairs = outcome.coords.as_pairs();
        for key in alas_mass::breakdown::OEW_KEYS {
            let m = outcome.masses.get(key).unwrap_or(0.0).max(0.0);
            if let Some((_, xyz)) = pairs.iter().find(|(n, _)| *n == key) {
                mass += m;
                moment += m * xyz[2];
            }
        }
        let takeoff_z = (moment
            + outcome.masses.payload * outcome.coords.payload[2]
            + outcome.masses.fuel * outcome.coords.fuel[2])
            / (mass + outcome.masses.payload + outcome.masses.fuel);
        let h = takeoff_z - ground;
        // The tip-back boundary about the rotation pivot, as the envelope's.
        let x_main = stations.rotation_pivot_x_m();
        let boundary = |deg: f64| pct(x_main - h * deg.to_radians().tan());
        eprintln!(
            "floorprobe {name}: MAC {:.3} m, LEMAC x {:.3} m, main gear x {x_main:.3} m = {:.2} % MAC, take-off CG height {h:.3} m, tip-back boundary at 8 deg {:.2} / 15 deg {:.2} / scrape {:.2} deg {:.2} % MAC; tip-back angle of a CG at 34 % MAC {:.2} deg",
            frame.chord_m,
            frame.x_lemac_m,
            pct(x_main),
            boundary(8.0),
            boundary(15.0),
            pose.pitch_rad.to_degrees(),
            boundary(pose.pitch_rad.to_degrees()),
            ((x_main - (frame.x_lemac_m + 0.34 * frame.chord_m)) / h)
                .atan()
                .to_degrees(),
        );
        if let Ok(cabin) = alas_payload::build::product_cabin_geometry(&outcome.plane, &config) {
            let deck = &cabin.passenger_decks[0];
            let doors: Vec<String> = cabin
                .door_stations
                .iter()
                .map(|d| format!("{d:?}"))
                .collect();
            let floors: Vec<String> = [3.0, 5.0, 7.0, 9.0, 12.0, 15.0, 18.0, 20.0, 22.0]
                .iter()
                .map(|&x| format!("x {x:.1}: {:.3}", cabin.floor_z(deck, x) - ground))
                .collect();
            eprintln!(
                "floorprobe {name}: main floor above ground: {}; doors: {}",
                floors.join("; "),
                doors.join(" | ")
            );
        }
    }
}

/// Probe: the model's own heights of the points the Boeing 777-9 airport
/// planning ground-clearance table names (Table 2-2: A fuselage top, F engine
/// low point, K extended wing tip, P horizontal-tail tip, Q fin top), measured
/// from the belly line, and the belly clearance each one implies against the
/// table's minimum and maximum columns. Prints; asserts nothing.
#[test]
#[ignore = "diagnostic; prints"]
fn ave_777_9_rows_probe() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "AVE",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get("AVE").unwrap().design_vector;
    let outcome = run_candidate(&config, &design.to_array()).unwrap();
    let plane = &outcome.plane;
    let fuselage = &plane.fuselages[0];
    let crown = fuselage
        .xsecs
        .iter()
        .map(|x| x.xyz_c[2] + x.height / 2.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let belly = fuselage
        .xsecs
        .iter()
        .map(|x| x.xyz_c[2] - x.height / 2.0)
        .fold(f64::INFINITY, f64::min);
    let ground = GroundPose {
        ground_z_m: belly,
        x_main_gear_m: 0.0,
        y_outboard_wheel_m: 0.0,
        pitch_rad: 0.0,
        roll_rad: 0.0,
    };
    // Heights above the belly line, level, no roll (the table prints neither).
    let rise = flight_rise_fn(plane, &config);
    let hold = config.geometry.wing.nacelles_hold_ground_height;
    let nacelle_rise = |y: f64| if hold { 0.0 } else { rise(y) };
    let rows = [
        ("A fuselage top", crown - belly, 8.46, 8.79),
        (
            "F engine low point",
            nacelle_clearance_m(plane, &ground, &nacelle_rise).unwrap(),
            0.84,
            0.86,
        ),
        (
            "K wing tip leading edge",
            plane.wings[0].xsecs.last().unwrap().xyz_le[2]
                - belly
                - rise(plane.wings[0].xsecs.last().unwrap().xyz_le[1].abs()),
            7.87,
            8.13,
        ),
        (
            "P horizontal-tail tip",
            plane
                .wings
                .iter()
                .find(|w| w.name.contains("Horizontal"))
                .unwrap()
                .xsecs
                .iter()
                .map(|x| x.xyz_le[2] - belly)
                .fold(f64::NEG_INFINITY, f64::max),
            8.03,
            8.48,
        ),
    ];
    eprintln!(
        "ave rows: body height {:.3} m, belly z {belly:.3}, crown z {crown:.3}",
        crown - belly
    );
    for (label, above_belly, min, max) in rows {
        eprintln!(
            "ave row {label}: {above_belly:.3} m above the belly line; implied belly clearance min column {:.3} m, max column {:.3} m",
            min - above_belly,
            max - above_belly
        );
    }
    let fin_top = plane
        .wings
        .iter()
        .filter(|w| w.name.contains("Vertical"))
        .flat_map(|w| w.xsecs.iter().map(|x| x.xyz_le[2]))
        .fold(f64::NEG_INFINITY, f64::max);
    eprintln!(
        "ave row Q fin top: {:.3} m above the belly line; implied clearance min {:.3}, max {:.3}",
        fin_top - belly,
        19.28 - (fin_top - belly),
        19.74 - (fin_top - belly)
    );
}

/// The config with the belly clearance declared as `fraction` of the diameter.
fn declared_at(config: &AlasConfig, fraction: f64) -> AlasConfig {
    let mut declared = config.clone();
    declared.landing_gear.fuselage_ground_clearance_m =
        Some(fraction * config.geometry.fuselage.diameter_m);
    declared
}

/// A body that declares no clearance is judged at both ends of its class
/// sample, row by row, and keeps the smaller gap of each. The signs differ by
/// row: on the AVE the nacelle and wing-tip gaps are the small-clearance end's
/// and the tail gap the large-clearance end's (the scrape pitch grows with the
/// clearance and drops the aft surfaces), so neither single end bounds all rows.
#[test]
fn an_undeclared_clearance_keeps_the_smaller_gap_of_both_ends_for_every_row() {
    for (name, low, high) in [("AVE", 0.298, 0.546), ("ATR72-600", 0.216, 0.223)] {
        let mut config = AlasConfig::from_value(&serde_json::json!({
            "preset": name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let outcome = run_candidate(&config, &design.to_array()).unwrap();
        let at = |config: &AlasConfig| {
            interference_residuals(&outcome, config, ResidualRole::Constraint)
        };
        let end_low = at(&declared_at(&config, low));
        let end_high = at(&declared_at(&config, high));
        config.landing_gear.fuselage_ground_clearance_m = None;
        let kept = at(&config);
        let ground_rows = [
            "wing_tip_ground_clearance_in_roll",
            "nacelle_ground_clearance_in_roll",
            "tail_ground_clearance_in_roll",
            "propeller_ground_clearance",
            "propeller_ground_clearance_tyre_deflated",
        ];
        let get = |rows: &[ConstraintResidual], id: &str| {
            rows.iter().find(|row| row.id == id).map(|row| row.actual)
        };
        for id in ground_rows {
            let (Some(a), Some(b)) = (get(&end_low, id), get(&end_high, id)) else {
                assert!(get(&kept, id).is_none(), "{name} {id}");
                continue;
            };
            let actual = get(&kept, id).unwrap();
            assert!(
                (actual - a.min(b)).abs() < 1e-9,
                "{name} {id}: {actual} of {a}, {b}"
            );
        }
        if name == "AVE" {
            let row = |rows: &[ConstraintResidual], id: &str| get(rows, id).unwrap();
            assert!(
                row(&end_low, "nacelle_ground_clearance_in_roll")
                    < row(&end_high, "nacelle_ground_clearance_in_roll")
            );
            assert!(
                row(&end_low, "wing_tip_ground_clearance_in_roll")
                    < row(&end_high, "wing_tip_ground_clearance_in_roll")
            );
            assert!(
                row(&end_high, "tail_ground_clearance_in_roll")
                    < row(&end_low, "tail_ground_clearance_in_roll")
            );
        }
    }
}

/// The CG envelope of an undeclared clearance is what both ends of the class
/// sample support, limit by limit and constraint by constraint: the more
/// forward aft limit, the more aft forward limit and the larger violation of
/// each constraint in each state. The rotation forward boundary moves in
/// opposite directions on the E195-E2 (-3.615 % MAC at 0.298 D, -3.775 % at
/// 0.546 D) and the AVE (27.53 % and 29.06 %), so keeping one whole end would
/// widen the envelope on the other side. A mid-sample plane (0.35 D) must not
/// be more restrictive than the merged envelope: a check at the three sampled
/// planes, not a proof that each limit is monotone between them.
#[test]
fn an_undeclared_clearance_envelope_is_the_limit_by_limit_merge_of_both_ends() {
    for name in ["E195-E2", "AVE", "A320-200"] {
        let mut config = AlasConfig::from_value(&serde_json::json!({
            "preset": name,
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let outcome = run_candidate(&config, &design.to_array()).unwrap();
        let assess = |config: &AlasConfig| {
            crate::envelope::assess_model_cg_envelope(
                &outcome.plane,
                &outcome.masses,
                &outcome.coords,
                outcome.cg_x,
                outcome.x_np,
                outcome.x_np,
                outcome.mac,
                config,
            )
            .unwrap()
        };
        let end_low = assess(&declared_at(&config, 0.298));
        let end_high = assess(&declared_at(&config, 0.546));
        let nominal = assess(&declared_at(&config, 0.35));
        config.landing_gear.fuselage_ground_clearance_m = None;
        let kept = assess(&config);
        let eps = 1e-9;
        for end in [&end_low, &end_high, &nominal] {
            assert!(
                kept.configured_forward_limit_pct_mac >= end.configured_forward_limit_pct_mac - eps,
                "{name} forward limit"
            );
            assert!(
                kept.worst_aft_limit_pct_mac <= end.worst_aft_limit_pct_mac + eps,
                "{name} aft limit"
            );
            assert!(
                kept.required_tip_back_deg >= end.required_tip_back_deg - eps,
                "{name}"
            );
            for (k, e) in kept.loading_states.iter().zip(&end.loading_states) {
                let (kl, el) = (&k.physical_limits, &e.physical_limits);
                assert!(
                    kl.rotation_fwd_pct_mac >= el.rotation_fwd_pct_mac - eps,
                    "{name}"
                );
                assert!(
                    kl.tip_back_aft_pct_mac <= el.tip_back_aft_pct_mac + eps,
                    "{name}"
                );
                assert!(kl.aft_limit_pct_mac <= el.aft_limit_pct_mac + eps, "{name}");
                assert!(kl.fwd_limit_pct_mac >= el.fwd_limit_pct_mac - eps, "{name}");
                for c in &e.constraints {
                    let kept_c = k
                        .constraints
                        .iter()
                        .find(|x| x.constraint == c.constraint)
                        .unwrap();
                    assert!(
                        kept_c.normalized_exceedance >= c.normalized_exceedance - eps,
                        "{name} {:?}",
                        c.constraint
                    );
                }
            }
        }
        // Exactly the extreme of the two ends, not a third value.
        assert!(
            (kept.configured_forward_limit_pct_mac
                - end_low
                    .configured_forward_limit_pct_mac
                    .max(end_high.configured_forward_limit_pct_mac))
            .abs()
                < eps,
            "{name}"
        );
    }
}
