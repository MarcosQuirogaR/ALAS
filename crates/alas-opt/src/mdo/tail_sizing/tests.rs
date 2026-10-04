// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A test asserts on values it built here, so a failed unwrap is the assertion
// failing rather than a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::mdo::build::build_geometry_with_fuselage_policy;
use crate::mdo::sizing::{run_candidate, SizingOutcome};

/// Zero-lift drag area `CD0 S_ref`, m^2, at the trimmed table's design Mach
/// and reference altitude.
fn parasite_area(outcome: &SizingOutcome) -> f64 {
    let table = outcome.sized.fuel_artifacts.drag.table().unwrap();
    table.cd0(table.design_mach(), table.reference_altitude_m()) * outcome.plane.s_ref
}

/// Presets covering a conventional narrowbody, a widebody and the T-tail
/// turboprop whose tailplane rides on the fin.
const PRESETS: [&str; 3] = ["A320-200", "B787-9", "ATR72-600"];

fn config(preset: &str, mode: &str) -> AlasConfig {
    AlasConfig::from_value(&serde_json::json!({
        "preset": preset,
        "optimizer": {"design_space": {"mode": mode}}
    }))
    .unwrap()
}

fn nominal(preset: &str) -> DesignVector {
    alas_config::presets::get(preset).unwrap().design_vector
}

/// The preset's design vector with its wing resized: span and chords scale
/// independently, so area, mean chord and span all move.
fn perturbed(preset: &str, span: f64, chord: f64) -> DesignVector {
    let mut dv = nominal(preset);
    dv.span_m *= span;
    dv.root_chord_m *= chord;
    dv.break_chord_m *= chord;
    dv.tip_chord_m *= chord;
    dv
}

fn built(preset: &str, dv: &DesignVector) -> (AlasConfig, DesignVector, Airplane) {
    build_geometry_with_fuselage_policy(
        &config(preset, "reference_adaptation"),
        &dv.to_array(),
        true,
    )
    .unwrap_or_else(|failure| panic!("{preset}: {}", failure.reason))
}

#[test]
fn tail_volumes_hold_the_nominal_values_for_a_resized_wing() {
    for preset in PRESETS {
        let (nominal_h, nominal_v) = nominal_volumes(preset).unwrap();
        for (span, chord) in [(1.0, 1.0), (1.06, 0.93), (0.94, 1.08), (1.10, 1.10)] {
            let (_, _, plane) = built(preset, &perturbed(preset, span, chord));
            let (vh, vv) = tail_volume_coefficients(&plane);
            let (vh, vv) = (vh.unwrap(), vv.unwrap());
            assert!(
                (vh / nominal_h - 1.0).abs() < 1e-6 && (vv / nominal_v - 1.0).abs() < 1e-6,
                "{preset} span x{span} chord x{chord}: V_H {vh} vs {nominal_h}, V_V {vv} vs {nominal_v}"
            );
        }
    }
}

#[test]
fn an_unperturbed_preset_keeps_its_own_tails() {
    for preset in PRESETS {
        let dv = nominal(preset);
        let (candidate, sized, _) = built(preset, &dv);
        assert!((sized.tail_scale - dv.tail_scale).abs() < 1e-6, "{preset}");
        assert!(
            (candidate.geometry.empennage.vstab_scale_ratio - 1.0).abs() < 1e-6,
            "{preset}"
        );
    }
}

#[test]
fn tail_areas_scale_with_wing_size_and_inversely_with_the_arm() {
    // V = S_tail l / (S_w ref), so at the nominal volume
    // S_H = V_H S_w MAC / l_H and S_V = V_V S_w b / l_V.
    for preset in PRESETS {
        let (nominal_h, nominal_v) = nominal_volumes(preset).unwrap();
        let (_, _, plane) = built(preset, &perturbed(preset, 1.08, 1.05));
        let x_ac = |wing: &alas_geom::aircraft::wing::Wing| wing.aerodynamic_center(0.25)[0];
        let wing_x = x_ac(&plane.wings[0]);
        let l_h = x_ac(&plane.wings[1]) - wing_x;
        let l_v = x_ac(&plane.wings[2]) - wing_x;
        let s_h = plane.wings[1].reference_area();
        let s_v = plane.wings[2].unfolded_area();
        let expected_h = nominal_h * plane.s_ref * plane.c_ref / l_h;
        let expected_v = nominal_v * plane.s_ref * plane.b_ref / l_v;
        assert!((s_h / expected_h - 1.0).abs() < 1e-6, "{preset} S_H");
        assert!((s_v / expected_v - 1.0).abs() < 1e-6, "{preset} S_V");
    }
}

#[test]
fn a_larger_wing_carries_heavier_and_draggier_tails() {
    // The same wing with the nominal tail (baseline sandbox: no auto-sizing)
    // against the sized tail: the sized tail is larger, so its mass and its
    // parasite drag area must be larger, and the closed CG must have moved.
    let preset = "A320-200";
    let x = perturbed(preset, 1.06, 1.10).to_array();
    let sized = run_candidate(&config(preset, "reference_adaptation"), &x).unwrap();
    let fixed = run_candidate(&config(preset, "baseline_sandbox"), &x).unwrap();
    assert!(sized.masses.h_stab > fixed.masses.h_stab);
    assert!(sized.masses.v_stab > fixed.masses.v_stab);
    assert!(parasite_area(&sized) > parasite_area(&fixed));
    assert!((sized.cg_x - fixed.cg_x).abs() > 1e-6);

    let smaller = perturbed(preset, 0.94, 0.92).to_array();
    let sized = run_candidate(&config(preset, "reference_adaptation"), &smaller).unwrap();
    let fixed = run_candidate(&config(preset, "baseline_sandbox"), &smaller).unwrap();
    assert!(sized.masses.h_stab < fixed.masses.h_stab);
    assert!(parasite_area(&sized) < parasite_area(&fixed));
}

#[test]
fn clean_sheet_mode_leaves_the_tail_scale_to_the_search() {
    let dv = perturbed("A320-200", 1.06, 0.93);
    let (candidate, sized, _) = build_geometry_with_fuselage_policy(
        &config("A320-200", "clean_sheet"),
        &dv.to_array(),
        true,
    )
    .unwrap();
    assert_eq!(sized.tail_scale, dv.tail_scale);
    assert_eq!(candidate.geometry.empennage.vstab_scale_ratio, 1.0);
}

#[test]
fn the_fin_ratio_is_recovered_from_the_built_geometry() {
    for preset in PRESETS {
        let (candidate, sized, plane) = built(preset, &perturbed(preset, 1.06, 0.93));
        let ratio = fin_scale_ratio(&plane, &candidate.geometry.empennage, &sized);
        assert!(
            (ratio / candidate.geometry.empennage.vstab_scale_ratio - 1.0).abs() < 1e-9,
            "{preset}"
        );
    }
}
