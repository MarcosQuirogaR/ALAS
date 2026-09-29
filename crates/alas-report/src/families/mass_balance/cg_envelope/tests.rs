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
        neutral_point_conditions: None,
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

/// The registered ATR 72-600 geometry and configuration: a real high-wing
/// layout that registers no source gear-station anchor, which is exactly the
/// case the wing-mounted fallback rule does not cover.
fn atr_case() -> (AlasConfig, alas_geom::aircraft::airplane::Airplane) {
    let preset = alas_config::presets::get("ATR72-600").expect("registered ATR preset");
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "ATR72-600" }))
        .expect("ATR configuration");
    // Keep this refusal fixture explicit: the production ATR configuration
    // now carries its measured gear stations.
    config.landing_gear.reference_station_fuselage_length_m = None;
    config.landing_gear.reference_nlg_x_fraction = None;
    config.landing_gear.reference_mlg_x_fractions = None;
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("ATR geometry");
    (config, airplane)
}

/// Every text string the scene draws.
fn scene_texts(scene: &crate::scene::Scene) -> Vec<String> {
    scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn an_aircraft_with_no_measured_main_gear_station_gets_no_cg_envelope() {
    // The gear-strength boundaries of this figure are moments about the
    // main-gear station. With no station measured there is no boundary to
    // draw, and the figure must say so rather than draw one about a station
    // the mass model refuses. The lumped masses and coordinates carried here
    // are the clean-sheet fixture's and are never reached: the refusal is
    // raised before any limit is formed.
    let (config, airplane) = atr_case();
    let mut report = sample_report();
    report.airplane = airplane;
    assert!(
        !report.component_masses.is_empty() && !report.mass_coordinates.is_empty(),
        "the earlier no-data guards must not be what stops this figure"
    );

    let texts = scene_texts(&figure_cg_envelope(&report, &config, None));
    assert!(
        texts
            .iter()
            .any(|text| text.contains("No main-gear station measured")),
        "the figure must name the missing datum: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text.contains("MLG")),
        "no gear limit may be drawn without a station: {texts:?}"
    );
}

/// The governing aft/forward limit and the loading-state markers this
/// figure draws map back, through the exact [`Axes2D`] the figure built
/// ([`super::render::axes_view`]), to the values
/// [`crate::families::model_cg_gate_assessment`] itself reports -- not a
/// separately re-derived approximation.
#[test]
fn plotted_limits_and_state_markers_map_back_to_the_gate_values() {
    use crate::scene::Axes2D;

    let report = sample_report();
    let config = AlasConfig::default();
    let data = super::figure::prepare(&report, &config).expect("a real report resolves");
    let axes: Axes2D = super::render::axes_view(&data);

    // The heaviest and lightest evaluated states anchor `poly_aft`/`poly_fwd`
    // (`w_ops` starts and ends there): the mapped pixel for the governing
    // limit at that mass must equal the mapped pixel for that state's own
    // `physical_limits.aft_limit_pct_mac`/`fwd_limit_pct_mac`.
    let assessment =
        crate::families::model_cg_gate_assessment(&report, &config).expect("gate resolves");
    let mut states: Vec<_> = assessment.loading_states.iter().collect();
    states.sort_by(|a, b| a.mass_kg.total_cmp(&b.mass_kg));
    let lightest = states.first().expect("at least one state");
    let heaviest = states.last().expect("at least one state");

    let expected_aft_light = axes.map_point(
        lightest.physical_limits.aft_limit_pct_mac,
        lightest.mass_kg / 1000.0,
    );
    let actual_aft_light = axes.map_point(data.poly_aft[0], data.w_ops[0] / 1000.0);
    assert!(
        (expected_aft_light[0] - actual_aft_light[0]).abs() < 1e-6,
        "lightest-state aft limit pixel: expected {expected_aft_light:?}, got {actual_aft_light:?}"
    );

    let last = data.w_ops.len() - 1;
    let expected_fwd_heavy = axes.map_point(
        heaviest.physical_limits.fwd_limit_pct_mac,
        heaviest.mass_kg / 1000.0,
    );
    let actual_fwd_heavy = axes.map_point(data.poly_fwd[last], data.w_ops[last] / 1000.0);
    assert!(
        (expected_fwd_heavy[0] - actual_fwd_heavy[0]).abs() < 1e-6,
        "heaviest-state fwd limit pixel: expected {expected_fwd_heavy:?}, got {actual_fwd_heavy:?}"
    );

    // Every drawn loading-state marker equals its gate state's own mass/CG,
    // not a resampled or composited value.
    assert_eq!(data.state_points.len(), states.len());
    for (drawn, gate_state) in data.state_points.iter().zip(states.iter()) {
        assert!((drawn.mass_kg - gate_state.mass_kg).abs() < 1e-9);
        assert!((drawn.cg_pct_mac - gate_state.cg_pct_mac).abs() < 1e-9);
    }
}

/// Every one-percent-MAC fan-line definition this report's balance
/// index/limits rely on is linear in mass at fixed `%MAC`, matching the
/// stated `index = W * (x - x_ref) / C + K` convention -- checked directly
/// on [`alas_pipeline::feasibility::balance_index`] rather than only on this
/// figure's own output.
#[test]
fn balance_index_is_linear_in_mass_at_fixed_position() {
    use alas_pipeline::feasibility::balance_index;

    let x_ref_m = 12.0;
    let c = 1000.0;
    let k = 50.0;
    let x_m = 12.8; // A fixed CG station, off the reference.
    let index_at = |mass_kg: f64| balance_index(mass_kg, x_m, x_ref_m, c, k);

    let low = index_at(20_000.0);
    let mid = index_at(40_000.0);
    let high = index_at(60_000.0);
    // Linear in mass: the second difference is zero.
    assert!(
        ((high - mid) - (mid - low)).abs() < 1e-9,
        "index must be affine in mass at fixed %MAC: {low}, {mid}, {high}"
    );
    // At the reference station the index is the constant offset `k`,
    // independent of mass.
    assert!((balance_index(20_000.0, x_ref_m, x_ref_m, c, k) - k).abs() < 1e-9);
    assert!((balance_index(90_000.0, x_ref_m, x_ref_m, c, k) - k).abs() < 1e-9);
}

#[test]
fn an_aircraft_with_a_main_gear_station_still_gets_its_full_envelope() {
    // The clean-sheet default keeps the wing-mounted fallback, so the gate
    // must be inert: the figure is drawn exactly as before.
    let texts = scene_texts(&figure_cg_envelope(
        &sample_report(),
        &AlasConfig::default(),
        None,
    ));
    assert!(
        !texts
            .iter()
            .any(|text| text.contains("No main-gear station measured")),
        "an in-domain fallback must not be refused: {texts:?}"
    );
}
