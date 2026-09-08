// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::common::assign_label_rows;
use super::fuel_volume::wing_fuel_volume_m3;
use super::fuel_volume::{figure_fuel_volume_check, figure_fuel_volume_check_for_loading};
use super::landing_gear::figure_landing_gear_planform;
use super::mass_breakdown::{figure_mass_breakdown, AC_CHORD_FRACTION, FUEL_NEG_COLOR};
use crate::scene::{Color, SceneElement};
use alas_aero::analysis::PolarSweep;
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec};
use alas_geom::aircraft::spacing::linspace;
use alas_geom::aircraft::wing::{Wing, WingXSec};
use alas_mass::breakdown::{
    FUEL, FURNISHINGS, FUSELAGE, GEAR, H_STAB, OEW_KEYS, PAYLOAD, PROPULSION, SYSTEMS, V_STAB, WING,
};
use alas_perf::landing_gear::size_landing_gear;
use alas_pipeline::feasibility::{
    FuelCapacityAssessment, FuelCapacityEvidence, FuelLoadingAssessment,
};
use alas_pipeline::full_analysis::{AnalysisReport, DesignPoint, PolarFit, PolarFitStatus};
use std::collections::HashMap;
fn test_wing(name: &str, symmetric: bool) -> Wing {
    let naca = Airfoil::from_name("naca0012").unwrap();
    Wing::new(
        name,
        vec![
            WingXSec::new([15.0, 0.0, 0.0], 6.0, 0.0, naca.clone()),
            WingXSec::new([17.0, 16.0, 0.0], 2.0, 0.0, naca),
        ],
        symmetric,
    )
}

fn test_fuselage() -> Fuselage {
    Fuselage::new(
        "Fuselage",
        vec![
            FuselageXSec {
                xyz_c: [0.0, 0.0, 0.0],
                width: 0.2,
                height: 0.2,
                shape: 2.0,
            },
            FuselageXSec {
                xyz_c: [20.0, 0.0, 0.0],
                width: 4.0,
                height: 4.0,
                shape: 2.0,
            },
            FuselageXSec {
                xyz_c: [38.0, 0.0, 0.0],
                width: 0.3,
                height: 0.3,
                shape: 2.0,
            },
        ],
    )
}

fn test_airplane() -> Airplane {
    Airplane {
        name: "Test".to_owned(),
        xyz_ref: [17.5, 0.0, 0.0],
        wings: vec![test_wing("Main Wing", true)],
        fuselages: vec![test_fuselage()],
        s_ref: 120.0,
        c_ref: 4.0,
        b_ref: 32.0,
    }
}

fn test_report(masses: HashMap<String, f64>) -> AnalysisReport {
    AnalysisReport {
        design: DesignVector::default(),
        airplane: test_airplane(),
        polar: PolarSweep {
            alpha_deg: Vec::new(),
            geometric_alpha_deg: Vec::new(),
            cl: Vec::new(),
            cd: Vec::new(),
            cd_induced: Vec::new(),
            cd_wave: Vec::new(),
            cd_parasite: Vec::new(),
            cm: Vec::new(),
            l_over_d: Vec::new(),
        },
        design_point: DesignPoint {
            alpha_deg: 2.0,
            cl: 0.5,
            cd: 0.03,
            l_over_d: 16.6,
        },
        polar_fit: PolarFit {
            cd0: 0.02,
            k: 0.04,
            oswald_e: 0.85,
            aspect_ratio: 8.5,
            status: PolarFitStatus::Fitted,
        },
        static_margin: 0.12,
        x_neutral_point: 18.0,
        trimmed_design_point: None,
        component_masses: masses,
        mass_coordinates: HashMap::new(),
        physical_cg: [17.0, 0.0, 0.0],
        geometry_summary: HashMap::new(),
        payload_layout: None,
        cg_envelope_ok: Some(true),
    }
}

fn full_masses() -> HashMap<String, f64> {
    let mut m = HashMap::new();
    m.insert(WING.to_owned(), 8500.0);
    m.insert(H_STAB.to_owned(), 900.0);
    m.insert(V_STAB.to_owned(), 500.0);
    m.insert(FUSELAGE.to_owned(), 12000.0);
    m.insert(GEAR.to_owned(), 2200.0);
    m.insert(PROPULSION.to_owned(), 9000.0);
    m.insert(SYSTEMS.to_owned(), 4200.0);
    m.insert(FURNISHINGS.to_owned(), 3100.0);
    m.insert(PAYLOAD.to_owned(), 18000.0);
    m.insert(FUEL.to_owned(), 15000.0);
    m
}

#[test]
fn assign_label_rows_keeps_close_labels_off_the_same_row() {
    let rows = assign_label_rows(&[0.0, 0.5, 1.0, 3.0], 1.0);
    // 0.0 and 0.5 are closer than min_sep -> different rows; 1.0 is far
    // enough from row 0's 0.0 to reuse it (rows are tried in order, so
    // the earliest satisfied row wins); 3.0 is likewise far enough from
    // row 0's now-updated 1.0 to reuse it too.
    assert_eq!(rows, vec![0, 1, 0, 0]);
}

#[test]
fn assign_label_rows_reuses_a_row_once_separation_is_satisfied() {
    let rows = assign_label_rows(&[0.0, 10.0], 1.0);
    assert_eq!(rows, vec![0, 0]);
}

#[test]
fn mass_breakdown_draws_bars_and_the_mtow_reference_line() {
    let report = test_report(full_masses());
    let scene = figure_mass_breakdown(&report, Some("light"));
    let rects = scene
        .elements
        .iter()
        .filter(|e| matches!(e, SceneElement::Rect { .. }))
        .count();
    // The scene background is also a rectangle; the remaining twelve
    // rectangles are the stacked mass bars.
    assert!(rects >= 13);
    let has_mtow_line = scene.elements.iter().any(|e| {
            matches!(e, SceneElement::Line { stroke, .. } if stroke.dash_array.is_some() && stroke.color == Color::from_hex("#e74c3c"))
        });
    assert!(has_mtow_line);
}

#[test]
fn mass_breakdown_exposes_row_names_axis_labels_and_numeric_ticks() {
    let scene = figure_mass_breakdown(&test_report(full_masses()), Some("light"));
    let labels = scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(labels.contains(&"Components"));
    assert!(labels.contains(&"OEW -> MZFW"));
    assert!(labels.contains(&"Mass [t]"));
    assert!(
        labels
            .iter()
            .filter(|label| label.parse::<f64>().is_ok())
            .count()
            >= 4
    );
}

#[test]
fn mass_breakdown_draws_a_red_deficit_bar_when_fuel_is_negative() {
    let mut masses = full_masses();
    masses.insert(FUEL.to_owned(), -500.0);
    let report = test_report(masses);
    let scene = figure_mass_breakdown(&report, None);
    let has_deficit = scene.elements.iter().any(|e| {
            matches!(e, SceneElement::Rect { fill: Some(f), .. } if f.color == Color::from_hex(FUEL_NEG_COLOR))
        });
    assert!(has_deficit);
}

#[test]
fn mass_breakdown_with_no_masses_renders_a_placeholder_message() {
    let report = test_report(HashMap::new());
    let scene = figure_mass_breakdown(&report, None);
    assert!(scene
        .elements
        .iter()
        .any(|e| matches!(e, SceneElement::Text { text, .. } if text.contains("No mass data"))));
}

#[test]
fn landing_gear_planform_draws_one_polygon_per_wheel_plus_wing_and_fuselage() {
    let report = test_report(full_masses());
    let config = AlasConfig::default();
    let scene = figure_landing_gear_planform(&report, &config, Some("dark"));

    // Recompute the gear layout independently to cross-check the wheel count.
    let mac = report.airplane.c_ref;
    let wing = &report.airplane.wings[0];
    let x_wing_ac = wing.aerodynamic_center(AC_CHORD_FRACTION)[0];
    let x_mac_le = x_wing_ac - 0.25 * mac;
    let fus = &report.airplane.fuselages[0];
    let x_nlg = fus.xsecs[0].xyz_c[0]
        + (fus.xsecs[2].xyz_c[0] - fus.xsecs[0].xyz_c[0]) * config.mass_model.nlg_x_fraction;
    let x_mlg = x_mac_le + config.mass_model.mlg_x_fraction_mac * mac;
    let x_np = report.airplane.xyz_ref[0] + report.static_margin * mac;
    let np_pct = (x_np - x_mac_le) / mac * 100.0;
    let aft = np_pct - config.requirements.target_static_margin * 100.0;
    let fwd = aft - config.requirements.cg_range_pct_mac;
    let fwd_x = x_mac_le + fwd / 100.0 * mac;
    let aft_x = x_mac_le + aft / 100.0 * mac;
    let oew: f64 = OEW_KEYS
        .iter()
        .map(|&k| report.component_masses.get(k).copied().unwrap_or(0.0))
        .sum();
    let mtow = oew
        + report.component_masses.get(PAYLOAD).copied().unwrap_or(0.0)
        + report.component_masses.get(FUEL).copied().unwrap_or(0.0);
    let fus_diam = config.geometry.fuselage.diameter_m;
    let gear = size_landing_gear(
        mtow,
        x_nlg,
        x_mlg,
        fwd_x,
        aft_x,
        fus_diam,
        fus_diam * 1.1,
        &config.landing_gear,
    );

    let polygons = scene
        .elements
        .iter()
        .filter(|e| matches!(e, SceneElement::Polygon { .. }))
        .count();
    // wing (symmetric -> 2) + fuselage (1) + wheels.
    assert_eq!(polygons, 2 + 1 + gear.wheels.len());
}

#[test]
fn landing_gear_planform_legend_has_one_entry_per_distinct_strut_label() {
    let report = test_report(full_masses());
    let config = AlasConfig::default();
    let scene = figure_landing_gear_planform(&report, &config, None);
    let texts: Vec<&String> = scene
        .elements
        .iter()
        .filter_map(|e| match e {
            SceneElement::Text { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|t| t.as_str() == "NLG"));
    assert!(texts.iter().any(|t| t.as_str() == "MLG-L"));
}

#[test]
fn landing_gear_legend_places_distinct_struts_on_a_horizontal_row() {
    let scene =
        figure_landing_gear_planform(&test_report(full_masses()), &AlasConfig::default(), None);
    let positions: Vec<[f64; 2]> = scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, pos, .. } if text == "NLG" || text == "MLG-L" => Some(*pos),
            _ => None,
        })
        .collect();
    assert_eq!(positions.len(), 2);
    assert!((positions[0][1] - positions[1][1]).abs() < 1e-9);
    assert!((positions[0][0] - positions[1][0]).abs() > 20.0);
}

#[test]
fn landing_gear_planform_exposes_both_coordinate_axis_labels_and_ticks() {
    let scene =
        figure_landing_gear_planform(&test_report(full_masses()), &AlasConfig::default(), None);
    let labels = scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(labels.contains(&"Y [m]"));
    assert!(labels.contains(&"X [m] (fuselage station)"));
    assert!(
        labels
            .iter()
            .filter(|label| label.parse::<f64>().is_ok())
            .count()
            >= 4
    );
}

#[test]
fn wing_fuel_volume_matches_the_torenbeek_closed_form() {
    let wing = test_wing("Main Wing", true);
    let volume = wing_fuel_volume_m3(&wing, 0.85);

    let s = wing.reference_area();
    let b = wing.reference_span();
    let taper = wing.taper_ratio();
    let sample = linspace(0.0, 1.0, 101);
    let t_over_c = wing.xsecs[0].airfoil.max_thickness(&sample);
    let expected = 0.54 * (s * s / b) * t_over_c * (1.0 + taper + taper * taper)
        / (1.0 + taper).powi(2)
        * 0.85;
    assert!((volume - expected).abs() < 1e-9);
    assert!(volume > 0.0);
}

#[test]
fn fuel_volume_check_is_green_when_capacity_covers_the_required_fuel() {
    let mut masses = full_masses();
    masses.insert(FUEL.to_owned(), 1.0); // trivially small vs. a wing this size
    let report = test_report(masses);
    let config = AlasConfig::default();
    let scene = figure_fuel_volume_check(&report, &config, None);
    let sufficient_fill = Color::from_hex("#27ae60");
    assert!(scene.elements.iter().any(|e| {
            matches!(e, SceneElement::Rect { fill: Some(f), .. } if f.color.r == sufficient_fill.r && f.color.g == sufficient_fill.g && f.color.b == sufficient_fill.b)
        }));
}

#[test]
fn fuel_volume_check_exposes_named_rows_without_redundant_numeric_y_ticks() {
    let mut masses = full_masses();
    masses.insert(FUEL.to_owned(), 1.0);
    let scene = figure_fuel_volume_check(&test_report(masses), &AlasConfig::default(), None);
    let labels = scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(labels.contains(&"Tank capacity"));
    assert!(labels.contains(&"Required fuel"));
    assert!(labels.contains(&"Fuel storage"));
    assert!(labels.contains(&"Fuel mass [t]"));
    assert_eq!(
        labels
            .iter()
            .filter(|label| label.parse::<f64>().is_ok())
            .count(),
        0
    );
}

#[test]
fn fuel_volume_check_is_red_when_required_fuel_exceeds_capacity() {
    let mut masses = full_masses();
    masses.insert(FUEL.to_owned(), 1.0e9); // absurdly large, guaranteed to exceed tank capacity
    let report = test_report(masses);
    let config = AlasConfig::default();
    let scene = figure_fuel_volume_check(&report, &config, None);
    let insufficient = Color::from_hex("#e74c3c");
    assert!(scene.elements.iter().any(|e| {
            matches!(e, SceneElement::Rect { fill: Some(f), stroke: None, .. } if f.color.r == insufficient.r && f.color.g == insufficient.g && f.color.b == insufficient.b)
        }));
}

#[test]
fn product_fuel_volume_check_uses_typed_carried_fuel_and_capacity() {
    let loading = FuelLoadingAssessment {
        usable_capacity: FuelCapacityAssessment {
            capacity_kg: Some(20_000.0),
            evidence: FuelCapacityEvidence::PublishedPreset,
        },
        analyzed_carried_fuel_kg: 18_000.0,
        ..FuelLoadingAssessment::default()
    };
    let scene = figure_fuel_volume_check_for_loading(&loading, None);
    let labels = scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(labels.contains(&"Published usable capacity"));
    assert!(labels.contains(&"Analyzed carried fuel"));
    assert!(!labels.contains(&"Required fuel"));
}

#[test]
fn w35_reference_contract_covers_every_mass_balance_figure_in_both_themes() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../golden/report/reference_render_corpus.json"
    ))
    .expect("reference render corpus is valid JSON");
    for id in [
        "cg_envelope",
        "fuel_volume_check",
        "landing_gear_planform",
        "mass_breakdown",
        "mass_distribution",
    ] {
        for theme in ["light", "dark"] {
            let key = format!("{id}:{theme}");
            let figure = &corpus["figures"][&key];
            assert_eq!(figure["available"], true, "{key} is unavailable");
            assert_eq!(figure["theme"], theme);
            assert!(figure["image"].as_str().is_some(), "{key} has no image");
        }
    }
}
