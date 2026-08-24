// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! W6.5 evidence for planform area and the planform renderer's scale.
//!
//! Each case follows the reference path in causal order: preset/config,
//! engine resolution, builder stations, reference axes, projected/planform
//! areas, then renderer scale.  The test deliberately stops at the first
//! disagreement; it collects evidence for the diagnosis owner and does not
//! classify or repair a geometry or plotting difference.

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use alas_config::{AlasConfig, DesignVector};
use alas_geom::asb::airplane::Airplane;
use alas_geom::asb::wing::Wing;
use alas_geom::builder::AircraftBuilder;
use alas_report::families::geometry::figure_planform_comparison;
use alas_report::{Color, Scene, SceneElement};
use alas_testkit::{agrees, load_json, Tier};
use serde_json::Value;

fn number(value: &Value, key: &str) -> f64 {
    value[key].as_f64().unwrap()
}

fn close(label: &str, actual: f64, expected: f64) {
    assert!(
        agrees(actual, expected, Tier::Closed),
        "first W6.5 disagreement at {label}: got {actual:.17e}, reference {expected:.17e}"
    );
}

fn array(value: &Value, key: &str) -> Vec<f64> {
    value[key]
        .as_array()
        .unwrap()
        .iter()
        .map(Value::as_f64)
        .map(Option::unwrap)
        .collect()
}

fn close_array(label: &str, actual: &[f64], expected: &[f64]) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "first W6.5 disagreement at {label} length"
    );
    for (index, (&a, &e)) in actual.iter().zip(expected).enumerate() {
        close(&format!("{label}[{index}]"), a, e);
    }
}

fn compare_engine(actual: &Value, expected: &Value, label: &str) {
    for key in [
        "engine_name",
        "nacelle_profile",
        "radius_scale_m",
        "spanwise_positions_m",
        "z_m",
        "inlet_x_offset_m",
        "thrust_kn",
        "bypass_ratio",
        "overall_pressure_ratio",
        "fan_pressure_ratio",
        "turbine_inlet_temp_k",
        "cruise_tsfc_kg_kgf_hr",
        "fan_diameter_m",
    ] {
        assert_eq!(
            actual[key], expected[key],
            "first W6.5 disagreement at {label}.engine.{key}"
        );
    }
}

fn compare_station(actual: &Value, expected: &Value, label: &str) {
    close_array(
        &format!("{label}.xyz_le_m"),
        &array(actual, "xyz_le_m"),
        &array(expected, "xyz_le_m"),
    );
    close(
        &format!("{label}.chord_m"),
        number(actual, "chord_m"),
        number(expected, "chord_m"),
    );
    close(
        &format!("{label}.twist_deg"),
        number(actual, "twist_deg"),
        number(expected, "twist_deg"),
    );
}

fn compare_wing(actual: &Wing, expected: &Value, index: usize) {
    assert_eq!(
        actual.name,
        expected["name"].as_str().unwrap(),
        "first W6.5 disagreement at wings[{index}].name"
    );
    assert_eq!(
        actual.symmetric,
        expected["symmetric"].as_bool().unwrap(),
        "first W6.5 disagreement at wings[{index}].symmetric"
    );
    let xsecs = expected["xsecs"].as_array().unwrap();
    assert_eq!(
        actual.xsecs.len(),
        xsecs.len(),
        "first W6.5 disagreement at wings[{index}].xsecs length"
    );
    for (station_index, (actual_station, expected_station)) in
        actual.xsecs.iter().zip(xsecs).enumerate()
    {
        let actual_value = serde_json::json!({
            "xyz_le_m": actual_station.xyz_le,
            "chord_m": actual_station.chord,
            "twist_deg": actual_station.twist,
        });
        compare_station(
            &actual_value,
            expected_station,
            &format!("wings[{index}].xsecs[{station_index}]"),
        );
    }
}

fn top_outline_area(wing: &Wing) -> f64 {
    let polygon_area = |points: &[(f64, f64)]| {
        0.5 * points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .take(points.len())
            .map(|(&(x0, y0), &(x1, y1))| x0 * y1 - x1 * y0)
            .sum::<f64>()
            .abs()
    };
    let le: Vec<(f64, f64)> = wing
        .xsecs
        .iter()
        .map(|xsec| (xsec.xyz_le[1], xsec.xyz_le[0]))
        .collect();
    let te: Vec<(f64, f64)> = wing
        .xsecs
        .iter()
        .map(|xsec| (xsec.xyz_le[1], xsec.xyz_le[0] + xsec.chord))
        .collect();
    let half: Vec<(f64, f64)> = le.iter().copied().chain(te.iter().rev().copied()).collect();
    if !wing.symmetric {
        return polygon_area(&half);
    }
    let mirrored: Vec<(f64, f64)> = le
        .iter()
        .map(|&(y, x)| (-y, x))
        .chain(te.iter().rev().map(|&(y, x)| (-y, x)))
        .collect();
    polygon_area(&half) + polygon_area(&mirrored)
}

fn planform_limits(plane: &Airplane) -> ((f64, f64), (f64, f64)) {
    let (mut span_min, mut span_max) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut longitudinal_min, mut longitudinal_max) = (f64::INFINITY, f64::NEG_INFINITY);
    for wing in &plane.wings {
        for xsec in &wing.xsecs {
            let [x, y, _] = xsec.xyz_le;
            span_min = span_min.min(y);
            span_max = span_max.max(y);
            longitudinal_min = longitudinal_min.min(x);
            longitudinal_max = longitudinal_max.max(x + xsec.chord);
            if wing.symmetric {
                span_min = span_min.min(-y);
                span_max = span_max.max(-y);
            }
        }
    }
    ((span_min, span_max), (longitudinal_min, longitudinal_max))
}

fn autoscaled_range(minimum: f64, maximum: f64) -> (f64, f64) {
    let span = (maximum - minimum).abs();
    let margin = if span > 0.0 { span * 0.05 } else { 0.05 };
    (minimum - margin, maximum + margin)
}

fn axes_frame(scene: &Scene) -> (f64, f64, f64, f64) {
    scene
        .elements
        .iter()
        .find_map(|element| match element {
            SceneElement::Rect {
                x,
                y,
                width,
                height,
                fill: None,
                stroke: Some(_),
                ..
            } => Some((*x, *y, *width, *height)),
            _ => None,
        })
        .expect("planform scene contains its axis frame")
}

fn baseline_outline(scene: &Scene) -> &[[f64; 2]] {
    scene
        .elements
        .iter()
        .find_map(|element| match element {
            SceneElement::Polyline { points, stroke }
                if stroke.color == Color::from_hex("tab:blue")
                    && stroke.dash_array.is_some()
                    && points.len() > 2 =>
            {
                Some(points.as_slice())
            }
            _ => None,
        })
        .expect("planform scene contains the dashed baseline main-wing outline")
}

fn compare_case(case: &Value) {
    let preset = case["preset"].as_str().unwrap();
    let overlay = serde_json::json!({"preset": preset});
    let mut config = AlasConfig::from_value(&overlay).unwrap();
    // W6.5 isolates the planform-area/rendering chain and its fixture predates
    // the independently sourced A320-214 engine correction. Replay the same
    // LEAP input on both sides here; corrected-engine propagation is pinned by
    // the preset and mission-vehicle correction ledgers.
    if preset == "A320-200" {
        config.geometry.engine.engine_name = "LEAP-1A".to_owned();
    }
    assert_eq!(
        serde_json::to_value(&config.geometry).unwrap(),
        case["effective_geometry_config_before_builder"],
        "first W6.5 disagreement at {preset}.effective_geometry_config"
    );

    let design: DesignVector =
        serde_json::from_value(case["input"]["design_vector"].clone()).unwrap();
    let before = config.geometry.engine.clone();
    compare_engine(
        &serde_json::to_value(&before).unwrap(),
        &case["engine"]["before_builder"],
        &format!("{preset}.before_builder"),
    );
    let builder = AircraftBuilder::new(Some(config.geometry.clone()));
    compare_engine(
        &serde_json::to_value(&builder.geometry.engine).unwrap(),
        &case["engine"]["after_builder"],
        &format!("{preset}.after_builder"),
    );
    let plane = builder.build(Some(&design), true).unwrap();
    let expected_plane = &case["airplane"];
    assert_eq!(
        plane.name,
        expected_plane["name"].as_str().unwrap(),
        "first W6.5 disagreement at {preset}.airplane.name"
    );
    close_array(
        &format!("{preset}.airplane.xyz_ref_m"),
        &plane.xyz_ref,
        &array(expected_plane, "xyz_ref_m"),
    );
    for (field, actual) in [
        ("s_ref_m2", plane.s_ref),
        ("c_ref_m", plane.c_ref),
        ("b_ref_m", plane.b_ref),
    ] {
        close(
            &format!("{preset}.airplane.{field}"),
            actual,
            number(expected_plane, field),
        );
    }
    let expected_wings = expected_plane["wings"].as_array().unwrap();
    assert_eq!(
        plane.wings.len(),
        expected_wings.len(),
        "first W6.5 disagreement at {preset}.airplane.wings length"
    );
    for (index, (actual, expected)) in plane.wings.iter().zip(expected_wings).enumerate() {
        compare_wing(actual, expected, index);
    }
    let expected_fuselages = expected_plane["fuselages"].as_array().unwrap();
    assert_eq!(
        plane.fuselages.len(),
        expected_fuselages.len(),
        "first W6.5 disagreement at {preset}.airplane.fuselages length"
    );
    for (fuselage_index, (actual, expected)) in
        plane.fuselages.iter().zip(expected_fuselages).enumerate()
    {
        assert_eq!(
            actual.name,
            expected["name"].as_str().unwrap(),
            "first W6.5 disagreement at {preset}.fuselages[{fuselage_index}].name"
        );
        let expected_xsecs = expected["xsecs"].as_array().unwrap();
        assert_eq!(
            actual.xsecs.len(),
            expected_xsecs.len(),
            "first W6.5 disagreement at {preset}.fuselages[{fuselage_index}] stations"
        );
        for (station_index, (actual_xsec, expected_xsec)) in
            actual.xsecs.iter().zip(expected_xsecs).enumerate()
        {
            close_array(
                &format!("{preset}.fuselages[{fuselage_index}].xsecs[{station_index}].xyz_c_m"),
                &actual_xsec.xyz_c,
                &array(expected_xsec, "xyz_c_m"),
            );
            for (field, actual_value) in [
                ("width_m", actual_xsec.width),
                ("height_m", actual_xsec.height),
            ] {
                close(
                    &format!("{preset}.fuselages[{fuselage_index}].xsecs[{station_index}].{field}"),
                    actual_value,
                    number(expected_xsec, field),
                );
            }
        }
    }

    let renderer_axes = &case["renderer"]["axes"];
    let (span, longitudinal) = planform_limits(&plane);
    let x_range = autoscaled_range(span.0, span.1);
    let longitudinal_range = autoscaled_range(longitudinal.0, longitudinal.1);
    let y_range = (-longitudinal_range.1, -longitudinal_range.0);
    let python_ylim = array(renderer_axes, "ylim_m");
    let python_y_range = [-python_ylim[0], -python_ylim[1]];
    close_array(
        &format!("{preset}.renderer.axes.xlim_m"),
        &[x_range.0, x_range.1],
        &array(renderer_axes, "xlim_m"),
    );
    close_array(
        &format!("{preset}.renderer.axes.ylim_m"),
        &[y_range.0, y_range.1],
        &python_y_range,
    );
    assert_eq!(
        renderer_axes["xlabel"], "span Y [m]",
        "first W6.5 disagreement at {preset}.renderer.axes.xlabel"
    );
    assert_eq!(
        renderer_axes["ylabel"], "longitudinal X [m]",
        "first W6.5 disagreement at {preset}.renderer.axes.ylabel"
    );

    for (index, wing) in plane.wings.iter().enumerate() {
        let expected = &expected_plane["wings"][index]["areas_m2"];
        close(
            &format!("{preset}.wings[{index}].planform_area_m2"),
            wing.area(),
            number(expected, "planform"),
        );
        close(
            &format!("{preset}.wings[{index}].projected_top_outline_area_m2"),
            top_outline_area(wing),
            number(expected, "rendered_top_outline"),
        );
    }

    let scene =
        figure_planform_comparison(&plane, &plane, ("baseline", "optimized"), Some("light"));
    assert_eq!(scene.width, 1100.0);
    assert_eq!(scene.height, 800.0);
    let (left, top, width, height) = axes_frame(&scene);
    let expected_bbox = array(renderer_axes, "bbox_px");
    close_array(
        &format!("{preset}.renderer.axes.bbox_px"),
        &[left, top, width, height],
        &expected_bbox,
    );

    let root = &plane.wings[0].xsecs[0];
    let expected_root = [
        left + (root.xyz_le[1] - x_range.0) / (x_range.1 - x_range.0) * width,
        top + (1.0 - (-root.xyz_le[0] - y_range.0) / (y_range.1 - y_range.0)) * height,
    ];
    close_array(
        &format!("{preset}.renderer.baseline_main_wing_root_px"),
        &baseline_outline(&scene)[0],
        &expected_root,
    );

    let x_scale = width / (x_range.1 - x_range.0);
    let y_scale = height / (y_range.1 - y_range.0);
    close(
        &format!("{preset}.renderer.x_scale_px_per_m"),
        x_scale,
        number(renderer_axes, "x_scale_px_per_m"),
    );
    close(
        &format!("{preset}.renderer.y_scale_px_per_m"),
        y_scale,
        number(renderer_axes, "y_scale_px_per_m"),
    );
}

#[test]
fn w65_fixture_replays_planform_area_inputs_before_renderer_scale() {
    let fixture = load_json("report", "w65_planform_area");
    assert_eq!(fixture["schema"], "w65-planform-area-evidence/v1");
    for case in fixture["cases"].as_array().unwrap() {
        compare_case(case);
    }
}
