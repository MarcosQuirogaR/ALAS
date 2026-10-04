// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::metrics::{
    fuel_margin_rows, mass_metrics, mass_triplet_kg, mtow_band_row, payload_summary_metrics,
    static_margin_rows, takeoff_mass_margin, takeoff_mass_rows,
};
use super::propulsion::{
    localized_propulsion_label, propulsion_metric_card, propulsion_summary_entries,
};
use super::widgets::status_banner_title;
use crate::theme::{apply_theme, AppTheme};
use alas_payload::layout::{LayoutSummary, PassengerSummary, PayloadLayout};
use alas_pipeline::feasibility::MissionFuelStatus;

use egui::{Context, FontFamily, RawInput, Shape};

fn collect_text_families(shape: &Shape, families: &mut Vec<(String, FontFamily)>) {
    match shape {
        Shape::Text(text) => {
            for section in &text.galley.job.sections {
                families.push((
                    text.galley.job.text[section.byte_range.clone()].to_owned(),
                    section.format.font_id.family.clone(),
                ));
            }
        }
        Shape::Vec(shapes) => {
            for shape in shapes {
                collect_text_families(shape, families);
            }
        }
        _ => {}
    }
}

#[test]
fn propulsion_cards_follow_the_proportional_ui_font_family() {
    let context = Context::default();
    apply_theme(AppTheme::Dark, &context);
    let output = context.run(RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            propulsion_metric_card(ui, "Thermal efficiency (eta_t)", "\u{03b7}\u{209c} = 0.42");
        });
    });
    let mut families = Vec::new();
    for shape in &output.shapes {
        collect_text_families(&shape.shape, &mut families);
    }

    for label in ["Thermal efficiency (eta_t)", "\u{03b7}\u{209c} = 0.42"] {
        let family = families
            .iter()
            .find(|(text, _)| text == label)
            .unwrap_or_else(|| panic!("missing rendered summary text: {label}"))
            .1
            .clone();
        assert_eq!(family, FontFamily::Proportional, "{label}");
    }
}

fn passenger_layout() -> PayloadLayout {
    PayloadLayout {
        mode: alas_payload::layout::Mode::Passenger,
        items: Vec::new(),
        total_mass: 21_000.0,
        cg_x: 10.0,
        cg_y: 0.0,
        summary: LayoutSummary::Passenger(Box::new(PassengerSummary {
            total_pax: 204,
            seated_pax: 198,
            unseated_pax: 6,
            classes: vec![("Business", 18), ("Economy", 180)],
            lavatories: 4,
            galleys: 3,
            accessible_lavatories: 1,
            wheelchair_stowages: 1,
            exit_type: "A",
            exit_pairs: 4,
            exit_capacity: 220,
            max_certifiable_capacity: 220,
            geometric_capacity: 220,
            source_capacity_cap: None,
            source_exit_layout: None,
            capacity_binding: "geometry_exit_limit",
            payload_t: 21.0,
            seat_mass_t: 17.5,
            bag_mass_t: 2.8,
            belly_cargo_t: 0.7,
            hold_capacity_t: 8.0,
            hold_used_t: 3.5,
            hold_ulds: 5,
            forward_hold_baggage_fraction: 0.55,
            hold_compartment_masses_kg: vec![
                ("Forward hold".to_owned(), 1.9),
                ("Aft hold".to_owned(), 1.6),
            ],
            overload_kg: 0.0,
            aisle_width_m: 0.51,
            max_abreast: 6,
            n_aisles: 1,
            deck_utilization: vec![("main", 0.82)],
            cg_pct_mac: 25.4,
            double_deck: false,
        })),
    }
}

#[test]
fn passenger_summary_metrics_come_from_the_built_layout_one_value_per_row() {
    let metrics = payload_summary_metrics(&passenger_layout());
    for (label, value) in [
        ("Seated passengers", "198"),
        ("Requested passengers", "204"),
        ("Hold load", "3.5 t"),
        ("Hold capacity", "8.0 t"),
        ("Hold ULDs", "5"),
        ("Galleys", "3"),
        ("Lavatories", "4"),
        ("Exit pairs", "4"),
        ("Accessible lavatories", "1"),
        ("Seats abreast", "6"),
    ] {
        assert!(
            metrics
                .iter()
                .any(|(candidate, candidate_value)| *candidate == label
                    && candidate_value == value),
            "{label} = {value} missing from {metrics:?}"
        );
    }
    assert!(metrics
        .iter()
        .any(|(label, value)| *label == "Cabin class mix" && value.contains("Business 18")));
    assert!(
        metrics.iter().all(|(_, value)| !value.contains(" / ")),
        "slash-joined value remains: {metrics:?}"
    );
}

#[test]
fn static_margins_are_shown_together_in_percent_mac_with_an_explicit_missing_case() {
    let rows = static_margin_rows(Some(0.052), Some(0.081));
    assert_eq!(
        rows[0],
        ("Static margin (baseline)", "5.2 % MAC".to_owned())
    );
    assert_eq!(
        rows[1],
        ("Static margin (optimized)", "8.1 % MAC".to_owned())
    );
    let rows = static_margin_rows(Some(0.052), None);
    assert_eq!(rows[1].0, "Static margin (optimized)");
    assert_eq!(rows[1].1, "No optimized design in this run");
}

#[test]
fn fuel_margin_rows_split_mass_and_share_and_name_the_stop_case() {
    let rows = fuel_margin_rows(MissionFuelStatus::Completed, 20_000.0, Some(15_000.0));
    assert_eq!(
        rows[0],
        ("Fuel margin at destination", "+5.00 t".to_owned())
    );
    assert_eq!(
        rows[1],
        ("Fuel margin, share of carried fuel", "+25.0 %".to_owned())
    );
    let rows = fuel_margin_rows(MissionFuelStatus::Exhausted, 20_000.0, Some(20_000.0));
    assert_eq!(rows[0].0, "Fuel margin at stop");
    let rows = fuel_margin_rows(MissionFuelStatus::NotRequested, f64::NAN, None);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, "Not evaluated");
}

#[test]
fn takeoff_mass_margin_reads_as_one_value() {
    assert_eq!(takeoff_mass_margin(0.2), "At MTOW");
    assert_eq!(takeoff_mass_margin(1_250.0), "1.25 t below MTOW");
    assert_eq!(takeoff_mass_margin(f64::NAN), "Not established");
}

#[test]
fn aircraft_mass_summary_excludes_payload_and_fuel_from_oew() {
    let masses = std::collections::HashMap::from([
        ("Wing".to_owned(), 12_000.0),
        ("Fuselage".to_owned(), 8_000.0),
        ("Payload".to_owned(), 6_000.0),
        ("Fuel".to_owned(), 4_000.0),
    ]);
    assert_eq!(
        mass_triplet_kg(&masses, 29_000.0, 32_000.0),
        Some((20_000.0, 29_000.0, 32_000.0))
    );
}

#[test]
fn headline_mass_rows_label_the_sized_result_apart_from_the_mtow_input() {
    let rows = takeoff_mass_rows(20_000.0, 29_000.0, 32_000.0);
    assert_eq!(rows[0], ("Operating empty mass", "20.0 t".to_owned()));
    assert_eq!(
        rows[1],
        ("Takeoff mass (sized result)", "29.0 t".to_owned())
    );
    assert_eq!(rows[2], ("MTOW limit (input)", "32.0 t".to_owned()));
}

/// Small seeded optimized runs of the A220-300 preset. Sized by the mission,
/// the summary's takeoff mass is the pipeline's analysed mass, not the
/// preset MTOW, and the MTOW row is the configured limit under its own label.
/// Under Hard MTOW the analysed, sized and summary takeoff masses are one
/// closure: MTOW, or ZFW plus the usable capacity when the tanks limit the
/// load, and the summary names which limit governed.
#[test]
fn summary_masses_of_an_optimized_run_are_the_pipeline_result() {
    let result = seeded_a220(alas_config::MtowSizing::SizedByMission);
    assert_sized_by_mission_summary(&result);

    let result = seeded_a220(alas_config::MtowSizing::FixedRequirement);
    let declared_mtow_kg = result.config.requirements.mtow_kg;
    let fuel = &result.feasibility.fuel_loading;
    let loading = fuel
        .design_takeoff_loading
        .expect("a Hard-MTOW run reports its takeoff fuel loading");
    let sized_kg = result
        .optimized_report
        .as_ref()
        .and_then(|report| report.sized_takeoff_mass_kg())
        .expect("the optimized report is bound to the sized takeoff mass");
    let analyzed_kg = fuel.analyzed_takeoff_mass_kg;
    let expected_kg = match loading.status.as_str() {
        "volume_limited" => {
            loading.zero_fuel_mass_kg + loading.usable_capacity_kg.expect("established capacity")
        }
        _ => declared_mtow_kg,
    };
    for (name, kg) in [("sized", sized_kg), ("analysed", analyzed_kg)] {
        assert!(
            (kg - expected_kg).abs() <= 1e-6 * expected_kg,
            "{name} {kg} kg vs {expected_kg} kg ({})",
            loading.status.as_str()
        );
    }
    let metrics = mass_metrics(&result);
    let value = |label: &str| {
        metrics
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| panic!("missing summary row {label}"))
    };
    assert_eq!(
        value("Takeoff mass (sized result)"),
        format!("{:.1} t", expected_kg / 1_000.0)
    );
    assert_eq!(
        value("Takeoff fuel load"),
        super::metrics::takeoff_fuel_load(loading.status.as_str())
    );

    // The objective tile states a number in the unit its label names.
    let optimization = result.optimization_result.as_ref().expect("optimized");
    let kind = alas_pipeline::optimizer_summary::objective::history_objective_kind(
        &optimization.history,
        result.config.optimizer.objective.kind,
    );
    let text = super::objective_value_text(&result, kind);
    assert!(
        text.split_whitespace()
            .next()
            .is_some_and(|value| value.parse::<f64>().is_ok_and(f64::is_finite)),
        "objective tile value: {text}"
    );
}

fn seeded_a220(mode: alas_config::MtowSizing) -> alas_pipeline::PipelineResult {
    use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};

    let mut config =
        alas_config::AlasConfig::from_value(&serde_json::json!({ "preset": "A220-300" }))
            .expect("A220-300 preset");
    config.optimizer.objective.mtow_sizing = mode;
    config.optimizer.solver.method = alas_config::optimizer::PRODUCT_DE_METHOD.to_owned();
    config.optimizer.solver.refinement.max_evaluations = 128;
    config.optimizer.solver.screening.max_evaluations = 8;
    config.optimizer.solver.workers = std::thread::available_parallelism()
        .map_or(1, |n| n.get().min(8))
        .try_into()
        .unwrap_or(1);
    config.structures.enabled = false;
    // The checks are about numbers flowing consistently between stages, not
    // about any aerodynamic value, so the run uses the draft analysis
    // resolution (only its five resolution fields, leaving tuned assumptions
    // alone) and a worker pool; the seeded search is independent of the
    // worker count.
    let draft = &alas_config::fidelity_presets::get("draft")
        .expect("draft fidelity preset")
        .analysis;
    let analysis = &mut config.analysis;
    analysis.sweep_n_points = draft.sweep_n_points;
    analysis.spanwise_resolution = draft.spanwise_resolution;
    analysis.chordwise_resolution = draft.chordwise_resolution;
    analysis.fine_spanwise_resolution = draft.fine_spanwise_resolution;
    analysis.fine_chordwise_resolution = draft.fine_chordwise_resolution;
    let design = alas_config::presets::get("A220-300")
        .expect("A220-300 preset")
        .design_vector;
    let envelope = config.optimizer.design_space.envelope(&design);
    let bounds: Vec<(f64, f64)> = alas_config::DESIGN_VARIABLE_SPECS
        .iter()
        .map(|spec| {
            let variable = envelope
                .iter()
                .find(|v| v.name == spec.name)
                .expect("every design variable has an envelope");
            (variable.lower, variable.upper)
        })
        .collect();
    let options = PipelineOptions {
        optimize: true,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    DesignPipeline::new(config)
        .run_with_design_space(&options, &RunEnvironment::default(), &design, &bounds)
        .expect("the seeded A220-300 finalist is delivered")
}

fn assert_sized_by_mission_summary(result: &alas_pipeline::PipelineResult) {
    let declared_mtow_kg = result.config.requirements.mtow_kg;
    let sized_kg = result
        .optimized_report
        .as_ref()
        .and_then(|report| report.sized_takeoff_mass_kg())
        .expect("the optimized report is bound to the sized takeoff mass");
    let analyzed_kg = result.feasibility.fuel_loading.analyzed_takeoff_mass_kg;
    assert!((declared_mtow_kg - analyzed_kg).abs() > 1.0);

    let metrics = mass_metrics(result);
    let value = |label: &str| {
        metrics
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| panic!("missing summary row {label}"))
    };
    assert_eq!(
        value("Takeoff mass (sized result)"),
        format!("{:.1} t", analyzed_kg / 1_000.0)
    );
    assert_eq!(
        value("MTOW limit (input)"),
        format!("{:.1} t", declared_mtow_kg / 1_000.0)
    );
    assert_ne!(
        value("Takeoff mass (sized result)"),
        value("MTOW limit (input)")
    );
    // The analysed mass and the report's sized mass are one closure.
    assert!(
        (analyzed_kg - sized_kg).abs() < 0.005 * sized_kg,
        "analysed {analyzed_kg} kg vs sized {sized_kg} kg"
    );
}

#[test]
fn mtow_band_row_needs_the_band_mode_and_a_sized_report() {
    let mut plan = alas_config::AlasConfig::default().mtow_plan();
    plan.mode = alas_config::MtowSizing::MtowBand;
    plan.lower_bound_kg = Some(60_000.0);
    plan.upper_bound_kg = Some(66_000.0);
    let (label, value) = mtow_band_row(&plan, true).expect("sized band report shows the row");
    assert_eq!(label, "MTOW band (lower to upper edge)");
    assert_eq!(value, "60.0 to 66.0 t");
    assert!(mtow_band_row(&plan, false).is_none());
    plan.mode = alas_config::MtowSizing::FixedRequirement;
    assert!(mtow_band_row(&plan, true).is_none());
}

#[test]
fn propulsion_summary_splits_compact_cycle_metrics_into_independent_cards() {
    let lines = vec![
        "Engine: Demo (high-bypass turbofan)".to_owned(),
        "BPR = 8.0    OPR = 32.0    FPR = 1.6    TIT = 1450 K".to_owned(),
        "Thermal efficiency (eta_t) = 0.42".to_owned(),
    ];
    let entries = propulsion_summary_entries(&lines);
    assert_eq!(entries.len(), 6);
    assert!(entries
        .iter()
        .any(|(label, value)| { label == "BPR" && value == "8.0" }));
    assert!(entries
        .iter()
        .any(|(label, value)| { label == "TIT" && value == "1450 K" }));
    assert!(entries
        .iter()
        .any(|(label, value)| { label == "Thermal efficiency (eta_t)" && value == "0.42" }));
}

#[test]
fn propulsion_summary_localizes_all_catalogued_engine_labels_without_gluing_suffixes() {
    assert_eq!(
        localized_propulsion_label("TSFC (computed)"),
        "TSFC (computed)"
    );
    assert_eq!(
        localized_propulsion_label("Per-engine thrust, this cruise pt"),
        "Per-engine thrust, this cruise pt"
    );
    assert_eq!(
        localized_propulsion_label("Total installed thrust (x4)"),
        "Total installed thrust (x4)"
    );
}

#[test]
fn propulsion_summary_uses_existing_spanish_cycle_labels() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));

    assert_eq!(
        localized_propulsion_label("TSFC (computed)"),
        "TSFC (calculado)"
    );
    assert_eq!(
        localized_propulsion_label("Per-engine thrust, this cruise pt"),
        "Empuje por motor, en este punto de crucero"
    );
    assert_eq!(
        localized_propulsion_label("Total installed thrust (x4)"),
        "Empuje total instalado (x4)"
    );

    alas_i18n::set_language(Some("en"));
}

#[test]
fn incomplete_snapshots_never_receive_a_feasibility_verdict() {
    assert_eq!(status_banner_title(false, 0, 0), "Assessment incomplete");
    assert_ne!(
        status_banner_title(false, 0, 0),
        "Feasible under implemented checks"
    );
    assert_eq!(
        status_banner_title(true, 0, 0),
        "Feasible under implemented checks"
    );
}
