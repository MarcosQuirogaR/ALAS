// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Evaluating one preset through the full pipeline and summarizing it.

use super::*;

use alas_config::presets;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_pipeline::full_analysis::FullAnalysis;
use alas_pipeline::{
    DesignPipeline, FindingCode, FindingSeverity, PipelineOptions, PipelineResult, PlanningCgStatus,
};
use alas_report::families::aerodynamics::{
    figure_drag_breakdown, figure_polar_comparison, figure_span_loading,
};
use alas_report::families::geometry::{figure_exterior_3d, figure_geometry, figure_threeview};
use alas_report::families::mass_balance::figure_mass_breakdown;
use alas_report::families::performance::{
    figure_matching_chart, figure_payload_range, figure_vn_diagram,
};
use alas_report::families::stability::figure_dynamic_modes;
use alas_report::families::structures::figure_structures_sizing;
use alas_report::scene::{Camera3D, Scene};

/// Generate all disciplinary figure scenes for an evaluation outcome.
pub fn generate_scenes_for_preset(
    res: &PipelineResult,
    config: &AlasConfig,
    airplane: &Airplane,
) -> Vec<Scene> {
    let mut scenes = Vec::new();
    let theme = Some("dark");

    scenes.push(figure_geometry(airplane, theme));
    scenes.push(figure_threeview(airplane, theme));
    let camera = Camera3D::default();
    scenes.push(figure_exterior_3d(airplane, Some(camera), theme));

    let report_opt = res
        .baseline_analysis
        .as_ref()
        .or(res.optimized_report.as_ref());

    if let Some(rep) = report_opt {
        let baseline = res.baseline_analysis.as_ref().unwrap_or(rep);
        let optimized = res.optimized_report.as_ref().unwrap_or(rep);
        scenes.push(figure_polar_comparison(
            baseline,
            optimized,
            ("Baseline", "Optimized"),
            None,
            theme,
        ));
        scenes.push(figure_drag_breakdown(rep, theme));
        scenes.push(figure_span_loading(rep, theme));
        scenes.push(figure_mass_breakdown(rep, theme));
        scenes.push(figure_payload_range(rep, config, theme));
        scenes.push(figure_matching_chart(rep, config, theme));
        scenes.push(figure_dynamic_modes(rep, config, theme));

        let vn = alas_pipeline::design_vn_diagram(config, rep);
        scenes.push(figure_vn_diagram(&vn, theme));
    }

    if res.structural_result.is_some() {
        scenes.push(figure_structures_sizing(
            res.structural_result.as_ref(),
            theme,
        ));
    }

    scenes
}

/// Evaluate an aircraft preset through the end-to-end acceptance suite.
pub fn evaluate_preset(preset_name: &str) -> Result<PresetAcceptanceResult, String> {
    let preset =
        presets::get(preset_name).map_err(|e| format!("unknown preset '{preset_name}': {e}"))?;

    // Select the preset the way the program does, rather than reassembling
    // one field-by-field here. A hand-built configuration omits the cabin
    // seed, which is not cosmetic: a `CabinConfig::default()` is a widebody
    // 15% business / 85% economy mix, and scoring a single-aisle with a
    // lie-flat business block spends enough pitch to lose thirteen seats on
    // an A320 and eighteen on an A220. A config assembled by hand skips steps
    // the loader performs (cabin seed, engine binding).
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset.name }))
        .map_err(|e| format!("failed to select preset '{preset_name}': {e}"))?;
    // 1. Build geometry
    let builder = AircraftBuilder::new(Some(config.geometry.clone()));
    let airplane = builder
        .build(Some(&preset.design_vector), true)
        .map_err(|e| format!("failed to build aircraft for {preset_name}: {e}"))?;

    // 2. Full disciplinary baseline analysis (fast polar sweep for acceptance matrix)
    //
    // `include_engines = true` so the propulsion mass sits at its
    // real nacelle station rather than the no-nacelle wing-centroid fallback
    // (`alas_mass::stations::ComponentStations::propulsion_station_fallback`).
    // With no nacelles the propulsion centroid equals the wing centroid to
    // ~15 significant digits, which silently moved wing-mounted engines
    // several metres aft of their real station and put the published CG at
    // odds with the gate's own verdict for the same design.
    let full_analysis = FullAnalysis::new(config.clone());
    let full_report = full_analysis
        .run(&preset.design_vector, true)
        .map_err(|e| format!("full analysis failed for {preset_name}: {e}"))?;

    // 3. Full design pipeline execution
    let pipeline = DesignPipeline::new(config.clone());
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: true,
        parallel: true,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };

    let pipeline_res = pipeline
        .run(&options, &alas_pipeline::RunEnvironment::default())
        .map_err(|e| format!("pipeline run failed for {preset_name}: {e}"))?;
    let public_design_matches_preset = pipeline_res.optimized_design == Some(preset.design_vector);
    if !public_design_matches_preset {
        return Err(format!(
            "public pipeline evaluated a different design than preset {preset_name}"
        ));
    }

    let summary = extract_summary(
        preset_name,
        &pipeline_res.config,
        &full_report,
        &pipeline_res,
        &airplane,
        public_design_matches_preset,
    );
    Ok(summary)
}

pub(super) fn extract_summary(
    name: &str,
    config: &AlasConfig,
    full: &alas_pipeline::full_analysis::AnalysisReport,
    res: &PipelineResult,
    airplane: &Airplane,
    public_design_matches_preset: bool,
) -> PresetAcceptanceResult {
    let mtow = config.requirements.mtow_kg;
    let payload = full.component_masses.get("Payload").copied().unwrap_or(0.0);
    let mtow_closure_fuel_kg = full.component_masses.get("Fuel").copied().unwrap_or(0.0);
    let oew = (mtow - payload - mtow_closure_fuel_kg).max(0.0);

    let wingbox_mass = res
        .structural_result
        .as_ref()
        .and_then(|s| s.sizing.as_ref())
        .map(|sz| sz.total_mass_kg)
        .unwrap_or(0.0);
    let scenes = generate_scenes_for_preset(res, config, airplane);
    let n_scenes = scenes.len();
    let model_cg_envelope_ok = res
        .feasibility
        .model_cg
        .as_ref()
        .is_some_and(alas_opt::ModelCgEnvelopeAssessment::hard_constraints_pass);
    let model_cg_static_margin_floor = res
        .feasibility
        .model_cg
        .as_ref()
        .map_or(f64::NAN, |assessment| {
            assessment.minimum_physical_static_margin
        });
    let model_cg_analyzed_takeoff_static_margin = res
        .feasibility
        .model_cg
        .as_ref()
        .map_or(f64::NAN, |assessment| {
            assessment.target_static_margin.actual
        });
    let model_cg_minimum_loading_static_margin = res
        .feasibility
        .model_cg
        .as_ref()
        .and_then(|assessment| {
            assessment
                .loading_states
                .iter()
                .map(|state| state.static_margin)
                .reduce(f64::min)
        })
        .unwrap_or(f64::NAN);
    let model_cg_target_static_margin = res
        .feasibility
        .model_cg
        .as_ref()
        .map_or(f64::NAN, |assessment| {
            assessment.target_static_margin.target
        });
    let model_cg_target_preference_met = res
        .feasibility
        .model_cg
        .as_ref()
        .is_some_and(|assessment| assessment.target_static_margin.met_or_exceeded());
    let public_planning_cg_status = res.feasibility.cg_envelope.planning_status;
    let model_cg_pct_mac = res
        .feasibility
        .model_cg
        .as_ref()
        .and_then(|assessment| {
            assessment
                .loading_states
                .iter()
                .find(|state| state.state == alas_opt::ModelCgLoadingState::AnalyzedTakeoff)
        })
        .map_or(f64::NAN, |state| state.cg_pct_mac);
    let public_planning_cg_pct_mac = res.feasibility.cg_envelope.cg_pct_mac;
    let public_planning_cg_violation = matches!(
        public_planning_cg_status,
        PlanningCgStatus::ForwardLimitViolation | PlanningCgStatus::AftLimitViolation
    ) || res
        .feasibility
        .contains(FindingCode::PublicPlanningCgEnvelopeViolation);
    let trim_cm_residual = full.trimmed_design_point.as_ref().and_then(|trim| {
        (trim.alpha_deg.is_finite()
            && trim.trim_ih_deg.is_finite()
            && trim.cl.is_finite()
            && trim.cd.is_finite()
            && trim.cm_residual.is_finite())
        .then_some(trim.cm_residual)
    });
    let mission_converged = !res.feasibility.contains(FindingCode::MissionUnavailable)
        && !res.feasibility.contains(FindingCode::MissionNotConverged)
        && !res
            .feasibility
            .contains(FindingCode::InvalidMissionFuelBurn);
    let mission_fuel_within_available =
        mission_converged && !res.feasibility.contains(FindingCode::MissionFuelShortfall);
    let design_mission_evidence = presets::get(name)
        .map(|preset| preset.reference.design_mission_evidence.clone())
        .unwrap_or_default();
    let design_mission_status = assess_design_mission_status(&design_mission_evidence);
    let fuel_loading = res.feasibility.fuel_loading;
    let wing_area_within_limit = !res.feasibility.contains(FindingCode::WingAreaLimit);

    let execution_passed = mtow.is_finite()
        && oew.is_finite()
        && full.design_point.l_over_d.is_finite()
        && full.x_neutral_point.is_finite()
        && n_scenes >= 8
        && public_design_matches_preset;
    let physical_passed = execution_passed
        && mtow > 0.0
        && oew > 0.0
        && full.design_point.l_over_d > 8.0
        && full.x_neutral_point > 0.0
        && res.feasibility.findings.iter().all(|finding| {
            finding.severity != FindingSeverity::Error || !finding_governs_preset(finding)
        })
        && !public_planning_cg_violation;

    let model_audit = model_audit::extract(config, full, res);
    PresetAcceptanceResult {
        name: name.to_owned(),
        geometry_valid: true,
        mtow_kg: mtow,
        oew_kg: oew,
        mtow_closure_fuel_kg: fuel_loading.mtow_closure_fuel_kg,
        usable_fuel_capacity_kg: fuel_loading.usable_capacity.capacity_kg,
        analyzed_carried_fuel_kg: fuel_loading.analyzed_carried_fuel_kg,
        analyzed_takeoff_mass_kg: fuel_loading.analyzed_takeoff_mass_kg,
        mtow_shortfall_kg: fuel_loading.mtow_shortfall_kg,
        payload_kg: payload,
        cruise_l_over_d: full.design_point.l_over_d,
        static_margin: full.static_margin,
        neutral_point_x: full.x_neutral_point,
        cruise_mach: config.requirements.cruise_mach,
        wingbox_mass_kg: wingbox_mass,
        figure_scenes_count: n_scenes,
        public_design_matches_preset,
        model_cg_envelope_ok,
        model_cg_static_margin_floor,
        model_cg_analyzed_takeoff_static_margin,
        model_cg_minimum_loading_static_margin,
        model_cg_target_static_margin,
        model_cg_target_preference_met,
        public_planning_cg_status,
        model_cg_pct_mac,
        public_planning_cg_pct_mac,
        trim_cm_residual,
        mission_converged,
        mission_fuel_within_available,
        cruise_equilibrium: res.feasibility.cruise_equilibrium.clone(),
        fuel_capacity_evidence: fuel_loading.usable_capacity.evidence,
        design_mission_status,
        wing_area_within_limit,
        execution_passed,
        physical_passed,
        physical_findings: res.feasibility.findings.clone(),
        model_audit,
    }
}
