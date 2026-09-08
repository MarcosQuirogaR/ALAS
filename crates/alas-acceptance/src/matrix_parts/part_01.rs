// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

#[path = "../matrix/format.rs"]
mod format;
#[path = "../matrix/model_audit.rs"]
mod model_audit;

pub use format::{format_matrix_json, format_matrix_report};
pub use model_audit::PresetModelAudit;

use alas_config::presets;
use alas_config::{AlasConfig, DesignMissionEvidence};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_perf::performance::build_vn_diagram;
use alas_pipeline::full_analysis::FullAnalysis;
use alas_pipeline::{
    CruiseEquilibriumAssessment, DesignPipeline, FindingCode, FindingSeverity,
    FuelCapacityEvidence, PhysicalFinding, PipelineOptions, PipelineResult, PlanningCgStatus,
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

/// Evidence-aware result of validating a preset's design mission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetDesignMissionStatus {
    /// The registered preset has no complete, revision-locked mission source.
    Unverified,
    /// Source evidence exists, but the exact design mission was not evaluated.
    NotEvaluated,
    /// The source-backed design mission passed its physical checks.
    Passed,
    /// The source-backed design mission failed at least one physical check.
    Failed,
}

/// Metric summary from evaluating a single preset through the acceptance matrix.
#[derive(Debug, Clone)]
pub struct PresetAcceptanceResult {
    /// Aircraft preset identifier.
    pub name: String,
    /// Whether the geometry builder succeeded.
    pub geometry_valid: bool,
    /// Maximum takeoff weight in kilograms.
    pub mtow_kg: f64,
    /// Operating empty weight in kilograms.
    pub oew_kg: f64,
    /// MTOW mass-closure remainder assigned to fuel, in kilograms.
    pub mtow_closure_fuel_kg: f64,
    /// Established usable fuel capacity, in kilograms.
    pub usable_fuel_capacity_kg: Option<f64>,
    /// Fuel actually carried by the analyzed load case, in kilograms.
    pub analyzed_carried_fuel_kg: f64,
    /// Takeoff mass actually evaluated by the mission, in kilograms.
    pub analyzed_takeoff_mass_kg: f64,
    /// Amount by which the analyzed load case lies below MTOW, in kilograms.
    pub mtow_shortfall_kg: f64,
    /// Design payload in kilograms.
    pub payload_kg: f64,
    /// Lift-to-drag ratio at cruise design point.
    pub cruise_l_over_d: f64,
    /// Static margin as a fraction of mean aerodynamic chord.
    pub static_margin: f64,
    /// Neutral point longitudinal position in metres.
    pub neutral_point_x: f64,
    /// Design cruise Mach number.
    pub cruise_mach: f64,
    /// Sized wingbox structural mass in kilograms.
    pub wingbox_mass_kg: f64,
    /// Number of report figure scenes generated.
    pub figure_scenes_count: usize,
    /// Whether the public pipeline evaluated the selected preset design.
    pub public_design_matches_preset: bool,
    /// Whether every hard model-derived CG and gear-load constraint passed.
    ///
    /// This is a preliminary design-model result, not certification evidence.
    pub model_cg_envelope_ok: bool,
    /// Hard longitudinal-stability floor applied by the model.
    pub model_cg_static_margin_floor: f64,
    /// Static margin at the fuel-capped takeoff point assessed by the model.
    pub model_cg_analyzed_takeoff_static_margin: f64,
    /// Lowest static margin across OEW and the analyzed ZFW/TOW cases.
    pub model_cg_minimum_loading_static_margin: f64,
    /// Non-governing optimizer preference for takeoff static margin.
    pub model_cg_target_static_margin: f64,
    /// Whether analyzed-takeoff static margin reaches the preference.
    pub model_cg_target_preference_met: bool,
    /// Typed result of the manufacturer public planning-envelope comparison.
    ///
    /// `NotEvaluated` does not imply compliance; the aircraft WBM remains the
    /// operational authority for a real aircraft.
    pub public_planning_cg_status: PlanningCgStatus,
    /// Analyzed CG normalized by the built aerodynamic model's MAC frame.
    pub model_cg_pct_mac: f64,
    /// Analyzed CG normalized by the registered manufacturer planning frame.
    pub public_planning_cg_pct_mac: Option<f64>,
    /// Residual pitching-moment coefficient from a finite cruise trim result.
    pub trim_cm_residual: Option<f64>,
    /// Whether every segment of the selected interactive route converged.
    pub mission_converged: bool,
    /// Whether the selected interactive route fits the modeled available fuel.
    pub mission_fuel_within_available: bool,
    /// Solved cruise force-balance record from the selected interactive route.
    ///
    /// This is route telemetry, not design-mission evidence.
    pub cruise_equilibrium: Option<CruiseEquilibriumAssessment>,
    /// Provenance of the usable tank capacity applied to this load case.
    pub fuel_capacity_evidence: FuelCapacityEvidence,
    /// Evidence-aware design-mission verdict for this aircraft preset.
    pub design_mission_status: PresetDesignMissionStatus,
    /// Whether the built reference area respects the preset's configured cap.
    pub wing_area_within_limit: bool,
    /// End-to-end execution and finite-output verdict.
    pub execution_passed: bool,
    /// Physical-feasibility verdict from the checks currently implemented.
    pub physical_passed: bool,
    /// Typed physical findings published by the product pipeline.
    pub physical_findings: Vec<PhysicalFinding>,
    /// Detailed model outputs retained for public-data correlation work.
    pub model_audit: PresetModelAudit,
}

/// Aggregated acceptance matrix results across all aircraft presets.
#[derive(Debug, Clone)]
pub struct AcceptanceMatrixReport {
    /// Results for each evaluated aircraft preset.
    pub presets: Vec<PresetAcceptanceResult>,
    /// Whether every preset completed the execution checks.
    pub all_executed: bool,
    /// Whether execution and physical validity both passed for every preset.
    pub all_passed: bool,
    /// Whether every preset passed the implemented physical checks.
    pub all_physical_passed: bool,
    /// Whether every preset has a source-backed, evaluated design mission.
    pub all_design_missions_verified: bool,
}

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

        let s_ref = rep
            .geometry_summary
            .get("wing_s_ref_m2")
            .copied()
            .unwrap_or(122.0);
        let vn = build_vn_diagram(
            s_ref,
            &config.requirements,
            &config.performance,
            config.requirements.cruise_altitude_m,
        );
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
    // one field-by-field here. The hand-built version used to omit the cabin
    // seed, which is not a cosmetic difference: a `CabinConfig::default()` is
    // a widebody 15% business / 85% economy mix, and scoring a single-aisle
    // with a lie-flat business block spends enough pitch to lose thirteen
    // seats on an A320 and eighteen on an A220. Every "passenger payload
    // leaves N requested passengers without seats" finding in this matrix was
    // that mix, not the aeroplane -- selected properly, all eight presets seat
    // exactly what they ask for. This is the same class of defect as the
    // engine binding: a config assembled by hand skips a step the loader does.
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset.name }))
        .map_err(|e| format!("failed to select preset '{preset_name}': {e}"))?;
    // 1. Build geometry
    let builder = AircraftBuilder::new(Some(config.geometry.clone()));
    let airplane = builder
        .build(Some(&preset.design_vector), true)
        .map_err(|e| format!("failed to build aircraft for {preset_name}: {e}"))?;

    // 2. Full disciplinary baseline analysis (fast polar sweep for acceptance matrix)
    let full_analysis = FullAnalysis::new(config.clone());
    let full_report = full_analysis
        .run(&preset.design_vector, false)
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

fn extract_summary(
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
