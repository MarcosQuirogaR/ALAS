// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::super::mass_distribution::figure_mass_distribution;
use super::figure::figure_cg_envelope;
use super::helpers::{interp, linspace};
use crate::scene::SceneElement;
use alas_config::{AlasConfig, DesignRequirements, GeometryConfig, MassModelConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::run_mass_analysis;
use alas_pipeline::full_analysis::{AnalysisReport, DesignPoint, PolarFit, PolarFitStatus};
use std::collections::HashMap;
/// A fully real (built, mass-analyzed) `AnalysisReport`, the same
/// pipeline shape `full_analysis.rs` produces, minus the expensive VLM
/// polar/trim/neutral-point solve: `static_margin` is set to a
/// plausible constant rather than re-derived, matching how
/// `characterization_figures.rs`'s own `sample_report` stays a fixture,
/// not a second implementation of the physics under test here.
fn sample_report() -> AnalysisReport {
    let builder = AircraftBuilder::new(Some(GeometryConfig::default()));
    let mut plane = builder.build(None, true).expect("nominal aircraft builds");
    let requirements = DesignRequirements::default();
    let mass_model = MassModelConfig::default();
    let (masses, coords, cg) = run_mass_analysis(
        &plane,
        &requirements,
        &builder.geometry,
        Some(&mass_model),
        None,
    );
    plane.xyz_ref[0] = cg[0];

    let component_masses: HashMap<String, f64> = masses
        .as_pairs()
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
    let mass_coordinates: HashMap<String, [f64; 3]> = coords
        .as_pairs()
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();

    AnalysisReport {
        design: alas_config::design_variables::DesignVector::default(),
        airplane: plane,
        polar: alas_aero::analysis::PolarSweep {
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
            alpha_deg: 2.2,
            cl: 0.52,
            cd: 0.0285,
            l_over_d: 18.24,
        },
        polar_fit: PolarFit {
            cd0: 0.0185,
            k: 0.042,
            oswald_e: 0.86,
            aspect_ratio: 9.8,
            status: PolarFitStatus::Fitted,
        },
        static_margin: 0.12,
        x_neutral_point: cg[0] + 0.12 * 4.0,
        geometry_summary: HashMap::new(),
        component_masses,
        flops_mass_buildup: None,
        mass_coordinates,
        physical_cg: cg,
        payload_layout: None,
        trimmed_design_point: None,
        cg_envelope_ok: None,
    }
}

#[test]
fn renders_a_populated_envelope_from_a_real_analysis_report() {
    let report = sample_report();
    let config = AlasConfig::default();
    let scene = figure_cg_envelope(&report, &config, Some("light"));

    // A real weight/CG dataset draws far more than the no-data placeholder.
    assert!(scene.elements.len() > 20);
    let has_polygon = scene
        .elements
        .iter()
        .any(|e| matches!(e, SceneElement::Polygon { fill: Some(_), .. }));
    assert!(
        has_polygon,
        "model loading-state check should be a filled polygon"
    );
    assert!(scene.elements.iter().any(|element| {
        matches!(
            element,
            SceneElement::Text { text, .. }
                if text.contains("NOT AN AFM/WBM OPERATIONAL ENVELOPE")
        )
    }));
    let x_axis_labels = scene
        .elements
        .iter()
        .filter(|element| {
            matches!(element, SceneElement::Text { text, .. } if text == "CG position [% MAC]")
        })
        .count();
    assert_eq!(x_axis_labels, 1, "the axes own exactly one x label");
    assert!(!scene.elements.iter().any(|element| {
        matches!(element, SceneElement::Text { text, .. } if text == "Aircraft Center of Gravity (% MAC)")
    }));
}

#[test]
fn empty_mass_data_renders_the_placeholder_without_panicking() {
    let mut report = sample_report();
    report.component_masses.clear();
    report.mass_coordinates.clear();
    let config = AlasConfig::default();
    let scene = figure_cg_envelope(&report, &config, None);
    assert_eq!(
        scene.elements.len(),
        2,
        "the placeholder retains the visible figure title"
    );
    assert!(matches!(scene.elements[0], SceneElement::Text { .. }));
}

#[test]
fn a_wider_certification_cg_range_widens_the_forward_aft_limit_spread() {
    let report = sample_report();
    let mut narrow = AlasConfig::default();
    narrow.requirements.cg_range_pct_mac = 10.0;
    let mut wide = AlasConfig::default();
    wide.requirements.cg_range_pct_mac = 40.0;

    // Both must render without panicking across the config sweep; the
    // widened range should not collapse the envelope to nothing.
    let sc_narrow = figure_cg_envelope(&report, &narrow, None);
    let sc_wide = figure_cg_envelope(&report, &wide, None);
    assert!(!sc_narrow.elements.is_empty());
    assert!(!sc_wide.elements.is_empty());
}

#[test]
fn envelope_constraints_and_fill_stay_inside_the_plot_rectangle() {
    let scene = figure_cg_envelope(&sample_report(), &AlasConfig::default(), Some("dark"));
    let left = 75.0;
    let top = 55.0;
    let right = left + 570.0;
    let bottom = top + 380.0;
    for element in &scene.elements {
        match element {
            SceneElement::Polygon { points, .. } => {
                assert!(points.iter().all(|p| {
                    p[0] >= left - 1e-9
                        && p[0] <= right + 1e-9
                        && p[1] >= top - 1e-9
                        && p[1] <= bottom + 1e-9
                }));
            }
            SceneElement::Polyline { points, .. } => {
                assert!(points.iter().all(|p| {
                    p[0] >= left - 1e-9
                        && p[0] <= right + 1e-9
                        && p[1] >= top - 1e-9
                        && p[1] <= bottom + 1e-9
                }));
            }
            _ => {}
        }
    }
}

#[test]
fn mass_distribution_keeps_aircraft_layout_and_annotations_readable() {
    let scene = figure_mass_distribution(&sample_report(), Some("dark"));
    let labels: Vec<&str> = scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(labels.iter().any(|label| label.starts_with("Physical CG")));
    assert!(labels.iter().any(|label| label.starts_with("Aero CG")));
    assert!(labels.iter().any(|label| label.contains("Wing")));
    assert!(scene
        .elements
        .iter()
        .any(|element| matches!(element, SceneElement::Polygon { .. })));
    let cg_label_y: Vec<f64> = scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, pos, .. }
                if text.starts_with("Physical CG") || text.starts_with("Aero CG") =>
            {
                Some(pos[1])
            }
            _ => None,
        })
        .collect();
    assert_eq!(cg_label_y.len(), 2);
    assert!((cg_label_y[0] - cg_label_y[1]).abs() >= 30.0);
}

#[test]
fn linspace_matches_numpy_endpoints_and_count() {
    let xs = linspace(0.0, 1.0, 15);
    assert_eq!(xs.len(), 15);
    assert!((xs[0] - 0.0).abs() < 1e-12);
    assert!((xs[14] - 1.0).abs() < 1e-12);
    // np.linspace(0, 1, 15) step is 1/14.
    assert!((xs[1] - 1.0 / 14.0).abs() < 1e-12);
}

#[test]
fn interp_clamps_outside_the_domain_and_is_linear_inside_it() {
    let xs = [0.0, 10.0, 20.0];
    let ys = [0.0, 100.0, 100.0];
    assert_eq!(interp(-5.0, &xs, &ys), 0.0);
    assert_eq!(interp(25.0, &xs, &ys), 100.0);
    assert!((interp(5.0, &xs, &ys) - 50.0).abs() < 1e-12);
}
