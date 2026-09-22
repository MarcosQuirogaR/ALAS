// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Render every GUI/report registry slot and retain typed unavailable evidence.

use std::error::Error;
use std::path::{Path, PathBuf};

use alas_config::DesignVector;
use alas_gui::scene::{build_page_preview, build_result_figure, build_screening_figure};
use alas_gui::state::{AppState, Language};
use alas_gui::AppTheme;
use alas_opt::history::OptimizationHistory;
use alas_opt::OptimizationResult;
use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};
use alas_report::{FigureDescriptor, Scene};
use alas_screen::{AirfoilCandidateResult, AirfoilScreeningResult};
use serde::Serialize;

#[derive(Serialize)]
struct AuditEntry {
    registry: &'static str,
    id: &'static str,
    title: &'static str,
    category: &'static str,
    status: &'static str,
    evidence: String,
    png: Option<String>,
    svg: Option<String>,
    width: Option<f64>,
    height: Option<f64>,
    element_count: Option<usize>,
}

#[derive(Serialize)]
struct AuditIndex {
    schema: &'static str,
    language: &'static str,
    theme: &'static str,
    entries: Vec<AuditEntry>,
}

fn write_scene(
    directory: &Path,
    registry: &'static str,
    descriptor: &'static FigureDescriptor,
    scene: &Scene,
    status: &'static str,
    evidence: impl Into<String>,
) -> Result<AuditEntry, Box<dyn Error>> {
    let stem = format!("{registry}__{}__es_dark", descriptor.id);
    let png_name = format!("{stem}.png");
    let svg_name = format!("{stem}.svg");
    let png = alas_viz::raster::render_scene_png(scene).map_err(std::io::Error::other)?;
    std::fs::write(directory.join(&png_name), png)?;
    std::fs::write(directory.join(&svg_name), alas_report::render_svg(scene))?;
    Ok(AuditEntry {
        registry,
        id: descriptor.id,
        title: descriptor.title,
        category: descriptor.category,
        status,
        evidence: evidence.into(),
        png: Some(png_name),
        svg: Some(svg_name),
        width: Some(scene.width),
        height: Some(scene.height),
        element_count: Some(scene.elements.len()),
    })
}

fn result_evidence(id: &str) -> (&'static str, &'static str) {
    match id {
        "mses_pressure" | "mses_mach_contours" => (
            "rendered_status",
            "the local pipeline retained an explicit not-configured MSES status; no physical field is claimed",
        ),
        "structures_patran" | "structures_vibration" => (
            "rendered_status",
            "the local pipeline retained an explicit unavailable external-solver status; no physical result is claimed",
        ),
        "structures_modes" => (
            "rendered_partial",
            "the local analytical Rayleigh modes rendered; an external SOL103 comparison was unavailable",
        ),
        "model_comparison" => (
            "rendered_partial",
            "local ALAS, VLM, Fourier lifting-line, and Helmbold content rendered; optional VSPAERO and MSES overlays were unavailable",
        ),
        "optimization_history" | "design_evolution" => (
            "rendered",
            "Spanish GUI dispatch using a clearly representative synthetic optimization history and the accessible dark theme",
        ),
        _ => (
            "rendered",
            "Spanish GUI dispatch using an actual local pipeline result and the accessible dark figure theme",
        ),
    }
}

fn unavailable(
    registry: &'static str,
    descriptor: &'static FigureDescriptor,
    evidence: impl Into<String>,
) -> AuditEntry {
    AuditEntry {
        registry,
        id: descriptor.id,
        title: descriptor.title,
        category: descriptor.category,
        status: "unavailable",
        evidence: evidence.into(),
        png: None,
        svg: None,
        width: None,
        height: None,
        element_count: None,
    }
}

fn representative_history() -> OptimizationHistory {
    let mut history = OptimizationHistory::new();
    let base = DesignVector::default();
    for index in 0..12 {
        let mut design = base;
        let progress = index as f64 / 11.0;
        design.span_m *= 0.92 + 0.12 * progress;
        design.root_chord_m *= 1.04 - 0.06 * progress;
        history.record(
            design,
            index != 2 && index != 7,
            1.4 - 0.65 * progress,
            13.2 + 5.1 * progress,
            design.span_m,
            3.1 - 0.7 * progress,
            116.0 + 8.0 * progress,
            -0.8 + 0.5 * progress,
            if index == 2 || index == 7 {
                "static_margin"
            } else {
                ""
            },
        );
    }
    history
}

fn candidate(name: &str, rank: usize) -> AirfoilCandidateResult {
    let offset = rank as f64;
    AirfoilCandidateResult {
        name: name.to_owned(),
        status: "ok".to_owned(),
        l_over_d: Some(18.0 - 0.7 * offset),
        cl: Some(0.54),
        cd: Some(0.030 + 0.001 * offset),
        alpha_deg: Some(2.4 + 0.1 * offset),
        max_thickness_frac: Some(0.11 + 0.006 * offset),
        tank_volume_m3: Some(24.0 + 0.8 * offset),
        tank_capacity_kg: Some(19_000.0 + 650.0 * offset),
        score: Some(0.95 - 0.05 * offset),
        robustness: Some(0.97 - 0.01 * offset),
        refined: true,
        l_over_d_3d: Some(16.5 - 0.6 * offset),
        cd_3d: Some(0.033 + 0.001 * offset),
        alpha_3d_deg: Some(2.8 + 0.1 * offset),
        cm_residual_3d: Some(0.0002 * offset),
        static_margin_3d: Some(0.11 + 0.005 * offset),
        score_3d: Some(0.93 - 0.05 * offset),
        mses_verified: true,
        l_over_d_mses: Some(15.9 - 0.55 * offset),
        cd_mses: Some(0.034 + 0.001 * offset),
        cdw_mses: Some(0.0012 + 0.0001 * offset),
        mses_status: Some("ok".to_owned()),
        is_reference: rank == 0,
        ..Default::default()
    }
}

fn representative_screening() -> AirfoilScreeningResult {
    let candidates = ["naca0012", "naca2412", "rae2822", "sc20412", "sc20714"]
        .iter()
        .enumerate()
        .map(|(index, name)| candidate(name, index))
        .collect::<Vec<_>>();
    AirfoilScreeningResult {
        baseline_airfoil: "naca0012".to_owned(),
        cruise_mach: 0.78,
        cruise_reynolds: 18_000_000.0,
        cruise_altitude_m: 11_000.0,
        cl_target: 0.54,
        transonic_caveat: true,
        refined_3d: true,
        n_total: candidates.len(),
        n_ok: candidates.len(),
        n_refined: candidates.len(),
        n_mses_verified: candidates.len(),
        candidates,
        ..Default::default()
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let directory = std::env::args_os().nth(1).map_or_else(
        || PathBuf::from("outputs/figure_registry_audit"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&directory)?;

    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    let mut state = AppState::default();
    state.theme = AppTheme::Dark;
    state.language = Language::Es;
    let config = state
        .typed_config()
        .ok_or_else(|| std::io::Error::other("default GUI configuration is invalid"))?;
    let mut entries = Vec::new();

    for descriptor in alas_report::PREVIEW_FIGURES {
        match build_page_preview(&state, descriptor.id) {
            Some(scene) => entries.push(write_scene(
                &directory,
                "preview",
                descriptor,
                &scene,
                "rendered",
                "Spanish GUI preview dispatch using the default valid configuration and accessible dark figure theme",
            )?),
            None => entries.push(unavailable(
                "preview",
                descriptor,
                "the default valid configuration produced no preview scene",
            )),
        }
    }

    let mut result = DesignPipeline::new(config.clone())
        .run(
            &PipelineOptions {
                optimize: false,
                compare_baseline: true,
                parallel: false,
                aerodynamic_solver: Default::default(),
                optimization_solver: Default::default(),
                output_dir: None,
                save_plots: false,
                seed: Some(42),
                quiet: true,
            },
            &RunEnvironment::default(),
        )
        .map_err(std::io::Error::other)?;
    let history = representative_history();
    result.optimization_result = Some(OptimizationResult {
        best_design: result.optimized_design.unwrap_or_default(),
        best_cost: history.cost.last().copied().unwrap_or_default(),
        best_valid: history.valid.last().copied().unwrap_or(false),
        history,
        wall_time_s: 1.0,
        method: "differential_evolution".to_owned(),
        strategy: "best1bin".to_owned(),
        termination: "example".to_owned(),
        pareto_front: Vec::new(),
        // This audit renders figures from a hand-built result, so it carries
        // no staged-search telemetry; `None` is the honest value rather than
        // a fabricated diagnostic the figures would then display.
        search_diagnostics: None,
        delivered_acceptance: None,
    });
    state.pipeline_result = Some(result);

    for descriptor in alas_report::RESULT_FIGURES {
        match build_result_figure(&state, descriptor.id, &config, "dark-accessible") {
            Some(Some(scene)) => {
                let (status, evidence) = result_evidence(descriptor.id);
                entries.push(write_scene(
                    &directory, "result", descriptor, &scene, status, evidence,
                )?)
            }
            Some(None) | None => entries.push(unavailable(
                "result",
                descriptor,
                format!(
                    "the representative local pipeline run produced no {:?} data",
                    descriptor.required_stage
                ),
            )),
        }
    }

    state.screening.result = Some(representative_screening());
    for descriptor in alas_report::SCREENING_FIGURES {
        match build_screening_figure(&state, descriptor.id, "dark-accessible") {
            Some(scene) => entries.push(write_scene(
                &directory,
                "screening",
                descriptor,
                &scene,
                "rendered",
                "Spanish GUI dispatch using a clearly representative synthetic completed three-stage screening result; this is renderer evidence, not an installed-MSES solve",
            )?),
            None => entries.push(unavailable(
                "screening",
                descriptor,
                "the representative completed three-stage screening produced no scene",
            )),
        }
    }

    let index = AuditIndex {
        schema: "alas-figure-registry-audit/v1",
        language: "es",
        theme: "dark-accessible",
        entries,
    };
    std::fs::write(
        directory.join("index.json"),
        serde_json::to_string_pretty(&index)?,
    )?;
    Ok(())
}
