// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Focused characterization for the report performance figures.

// This test intentionally panics if its constructed fixture violates its precondition.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_aero::analysis::PolarSweep;
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::asb::airfoil::Airfoil;
use alas_geom::asb::airplane::Airplane;
use alas_geom::asb::wing::{Wing, WingXSec};
use alas_pipeline::full_analysis::{AnalysisReport, DesignPoint, PolarFit, PolarFitStatus};
use alas_report::families::performance;
use alas_report::svg::render_svg;
use std::collections::HashMap;

fn sample_report(payload_kg: f64) -> AnalysisReport {
    let airfoil = Airfoil::from_name("naca0012").expect("fixture airfoil");
    let wing = Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 3.0, 0.0, airfoil.clone()),
            WingXSec::new([0.0, 8.0, 0.0], 3.0, 0.0, airfoil),
        ],
        true,
    );
    let airplane = Airplane {
        name: "Figure fixture".to_owned(),
        xyz_ref: [0.0, 0.0, 0.0],
        s_ref: wing.reference_area(),
        c_ref: wing.mean_aerodynamic_chord(),
        b_ref: wing.reference_span(),
        wings: vec![wing],
        fuselages: Vec::new(),
    };
    let mut component_masses = HashMap::new();
    for (name, mass) in [
        ("Wing", 8000.0),
        ("H-Stab", 1000.0),
        ("V-Stab", 600.0),
        ("Fuselage", 12000.0),
        ("Gear", 2000.0),
        ("Propulsion", 5000.0),
        ("Systems", 4000.0),
        ("Furnishings", 3000.0),
    ] {
        component_masses.insert(name.to_owned(), mass);
    }
    component_masses.insert("Payload".to_owned(), payload_kg);
    AnalysisReport {
        design: DesignVector::default(),
        airplane,
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
            l_over_d: 16.0,
        },
        polar_fit: PolarFit {
            cd0: 0.02,
            k: 0.04,
            oswald_e: 0.85,
            aspect_ratio: 9.0,
            status: PolarFitStatus::Fitted,
        },
        static_margin: 0.1,
        x_neutral_point: 5.0,
        trimmed_design_point: None,
        component_masses,
        flops_mass_buildup: None,
        mass_coordinates: HashMap::new(),
        physical_cg: [4.0, 0.0, 0.0],
        geometry_summary: HashMap::new(),
        payload_layout: None,
        cg_envelope_ok: None,
    }
}

#[test]
fn performance_figures_render_from_the_report_and_configuration() {
    let config = AlasConfig::default();
    let report = sample_report(20_000.0);

    let payload = render_svg(&performance::figure_payload_range(&report, &config, None));
    assert!(payload.contains("20.0 t"));
    assert!(payload.contains("OEW:"));
    assert!(payload.contains("MTOW:"));
    assert!(!payload.contains("capacity evidence:"));
    assert!(!payload.contains("NOT AN AFM/WBM OPERATIONAL ENVELOPE"));

    let mut matching_config = config.clone();
    matching_config.performance.matching_chart_resolution = 7;
    let matching = render_svg(&performance::figure_matching_chart(
        &report,
        &matching_config,
        None,
    ));
    assert!(matching.contains("FEASIBLE"));
    // The default config has no condition-specific OEI thrust/drag evidence.
    // Its conceptual in-flight estimate is reported as a gap rather than
    // plotted on the installed SLS T/W axis.
    assert!(matching.matches("<polyline").count() >= 3);
    assert!(matching.contains("OEI SLS evidence gap"));

    let departure = render_svg(&performance::figure_lto_departure(&report, &config, None));
    let arrival = render_svg(&performance::figure_lto_arrival(&report, &config, None));
    assert!(departure.contains("London Heathrow"));
    assert!(arrival.contains("Dubai"));
    for svg in [departure, arrival] {
        for label in ["TODR", "BFL", "ASD", "LDR", "V1", "VR", "V2"] {
            assert!(svg.contains(label), "missing {label}");
        }
    }
}

#[test]
fn field_performance_uses_the_effective_dispatched_airport() {
    let config = AlasConfig::default();
    let report = sample_report(20_000.0);
    let dispatched_departure = alas_config::airports::get("OMDB").expect("Dubai airport");

    let svg = render_svg(&performance::figure_lto_for_airport(
        &report,
        &config,
        dispatched_departure,
        "Departure",
        None,
    ));

    assert!(svg.contains("Dubai"));
    assert!(!svg.contains("London Heathrow"));
}

#[test]
fn payload_range_changes_when_the_analyzed_payload_changes() {
    let config = AlasConfig::default();
    let low = render_svg(&performance::figure_payload_range(
        &sample_report(10_000.0),
        &config,
        None,
    ));
    let high = render_svg(&performance::figure_payload_range(
        &sample_report(30_000.0),
        &config,
        None,
    ));
    assert!(low.contains("10.0 t"));
    assert!(high.contains("30.0 t"));
    assert_ne!(low, high);
}

#[test]
fn payload_range_uses_a_registered_structural_payload_cap() {
    let mut config = AlasConfig::default();
    config.requirements.max_structural_payload_kg = 30_000.0;
    let svg = render_svg(&performance::figure_payload_range(
        &sample_report(20_000.0),
        &config,
        None,
    ));

    assert!(svg.contains("30.0 t"));
    assert!(!svg.contains("configured structural payload cap"));
}

#[test]
fn payload_range_status_names_the_missing_physical_input() {
    let config = AlasConfig::default();
    let mut report = sample_report(20_000.0);
    report.airplane.wings.clear();

    let svg = render_svg(&performance::figure_payload_range(&report, &config, None));
    assert!(svg.contains("no main wing"));
    assert!(!svg.contains("operational envelope"));
}

#[test]
fn performance_renderers_keep_the_w34_contract_details_visible() {
    let config = AlasConfig::default();
    let report = sample_report(20_000.0);

    let matching = performance::figure_matching_chart(&report, &config, Some("dark"));
    assert!(matching.title.as_deref() == Some("Matching Chart"));
    assert!(matching
        .elements
        .iter()
        .any(|element| matches!(element, alas_report::scene::SceneElement::Circle { .. })));
    assert!(render_svg(&matching).contains("Land London Heathrow"));

    let payload = performance::figure_payload_range(&report, &config, Some("dark"));
    let footer = payload.elements.iter().find_map(|element| match element {
        alas_report::scene::SceneElement::Text { text, pos, .. }
            if text.starts_with("OEW:") && text.contains("MTOW:") =>
        {
            Some(*pos)
        }
        _ => None,
    });
    assert!(footer.is_some_and(|pos| pos[1] < 470.0));

    let lto = performance::figure_lto_departure(&report, &config, Some("dark"));
    let lto_svg = render_svg(&lto);
    let numeric_labels = lto
        .elements
        .iter()
        .filter_map(|element| match element {
            alas_report::scene::SceneElement::Text { text, .. }
                if text.chars().all(|ch| ch.is_ascii_digit() || ch == ',') =>
            {
                Some(text)
            }
            _ => None,
        })
        .count();
    assert!(
        numeric_labels >= 5,
        "bottom panel has no complete numeric scale"
    );
    assert!(lto_svg.contains("Distance [m]"));
}
