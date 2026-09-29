// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! SVG figure export for headless runs.

use std::fs;
use std::path::{Path, PathBuf};

use alas_pipeline::PipelineResult;

/// One registry entry recorded by the headless plot exporter.
///
/// Keeping unavailable entries in the manifest makes an incomplete export
/// explicit instead of making a missing SVG indistinguishable from a writer
/// failure or an unrequested figure.
#[derive(Debug, serde::Serialize)]
struct PlotExportEntry {
    id: &'static str,
    title: &'static str,
    required_stage: String,
    status: &'static str,
    path: Option<String>,
    reason: Option<String>,
}

pub(super) fn plots_dir(output_dir: Option<&Path>) -> PathBuf {
    output_dir
        .map(|dir| dir.join("plots"))
        .unwrap_or_else(|| PathBuf::from("plots"))
}

/// Build and save every result scene registered by [`alas_report`].
///
/// The CLI already depends on `alas-gui`, whose public scene factory is the
/// single dispatch point for report figures. Keeping this call here avoids a
/// second id-to-family mapping while allowing headless runs to use a caller's
/// output directory. Every registered ID is attempted, and unavailable scenes
/// are retained in `plot_manifest.json` with the stage metadata that explains
/// why no SVG was emitted. Scene serialization is used only as the bridge to
/// this crate's SVG writer; it preserves the report scene primitives and does
/// not invent plot data.
pub(super) fn save_result_plots(
    result: &PipelineResult,
    output_dir: Option<&Path>,
) -> Result<usize, String> {
    let dir = plots_dir(output_dir);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create plot directory {}: {e}", dir.display()))?;

    // Construct through the public default state so the CLI does not need to
    // name the GUI's private window bookkeeping.  Use the shared completion
    // transition so GUI figure/export helpers do not treat this valid CLI
    // result as an in-flight snapshot.
    let mut state = alas_gui::AppState::default();
    state.set_completed_pipeline_result(result.clone());

    let mut written = 0;
    let mut manifest = Vec::with_capacity(alas_report::RESULT_FIGURES.len());
    for descriptor in alas_report::RESULT_FIGURES {
        let (status, path, reason) = match alas_gui::scene::build_result_figure(
            &state,
            descriptor.id,
            &result.config,
            "dark",
        ) {
            Some(Some(scene)) => {
                let svg = alas_pipeline::render_scene_svg(&scene)?;
                let path = dir.join(format!("{}.svg", descriptor.id));
                fs::write(&path, svg)
                    .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
                written += 1;
                ("written", Some(format!("{}.svg", descriptor.id)), None)
            }
            Some(None) => (
                "unavailable",
                None,
                Some("the registered figure has no usable data for this run".to_owned()),
            ),
            None => (
                "unavailable",
                None,
                Some("the pipeline result does not expose the required report stage".to_owned()),
            ),
        };
        manifest.push(PlotExportEntry {
            id: descriptor.id,
            title: descriptor.title,
            required_stage: format!("{:?}", descriptor.required_stage),
            status,
            path,
            reason,
        });
    }

    let manifest_path = dir.join("plot_manifest.json");
    let manifest_json = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("serialize plot manifest: {error}"))?;
    fs::write(&manifest_path, manifest_json)
        .map_err(|error| format!("failed to write {}: {error}", manifest_path.display()))?;

    Ok(written)
}
