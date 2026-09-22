// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Final reconciliation of the gallery against completed result data.

use crate::state::{AppState, LogKind};
use crate::views::result_3d;
use alas_report::scene::SceneElement;
use std::collections::BTreeSet;

impl AppState {
    /// Install a pipeline result that has completed all requested stages.
    ///
    /// Headless callers use the same readiness transition as the desktop run
    /// completion path so exports do not mistake a valid final result for an
    /// in-flight snapshot.
    pub fn set_completed_pipeline_result(&mut self, result: alas_pipeline::PipelineResult) {
        self.pipeline_result = Some(result);
        self.pipeline_result_complete = true;
    }

    /// Run once at successful completion. Build the same registry and cache
    /// keys used by the gallery, and decode external PNGs before claiming that
    /// their produced artifacts are ready. Diagnostics stay in the Run Log.
    pub(crate) fn verify_final_result_figures(&mut self) {
        if !self.pipeline_result_complete {
            return;
        }
        let Some(result) = &self.pipeline_result else {
            return;
        };
        let config = result.config.clone();
        let expected_patran = result
            .structural_result
            .as_ref()
            .and_then(|structure| structure.patran.as_ref())
            .map(|patran| {
                patran
                    .png_paths
                    .iter()
                    .map(|(_, path)| path.to_string_lossy().replace('\\', "/"))
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        let theme = self.theme.figure_theme_name().to_owned();
        let language = alas_i18n::get_language();
        let mut ready = 0;
        let mut unavailable = 0;
        let mut images = BTreeSet::new();
        let mut issues = Vec::new();
        for figure in alas_report::RESULT_FIGURES {
            let key = format!(
                "run={};solver={:?};theme={theme};language={language};figure={}",
                self.run_identity, self.selected_solver_view, figure.id
            );
            let camera = if result_3d::is_orbitable_result(figure.id) {
                let camera_key = result_3d::result_camera_key(self.run_identity, figure.id);
                Some(result_3d::result_camera(self, &camera_key))
            } else {
                None
            };
            let Some(scene) =
                self.cached_result_figure_with_camera(&key, figure.id, &config, &theme, camera)
            else {
                unavailable += 1;
                continue;
            };
            ready += 1;
            for element in &scene.elements {
                if let SceneElement::Image { source, .. } = element {
                    if !source.starts_with("embedded://") {
                        images.insert(source.clone());
                    }
                }
            }
        }
        for path in expected_patran.difference(&images) {
            issues.push(format!(
                "Patran produced an image missing from the gallery: {path}"
            ));
        }
        for path in images {
            if std::fs::read(&path)
                .ok()
                .and_then(|bytes| eframe::icon_data::from_png_bytes(&bytes).ok())
                .is_none()
            {
                issues.push(format!(
                    "Final figure image is missing or cannot be decoded: {path}"
                ));
            }
        }
        self.log(format!("Final figure check: {ready} scenes rebuilt, {unavailable} without required data, {} artifact errors.", issues.len()),
            if issues.is_empty() { LogKind::Info } else { LogKind::Warn });
        for issue in issues {
            self.log(issue, LogKind::Warn);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_report::Scene;
    use std::sync::Arc;

    #[test]
    fn new_result_data_evicts_unavailable_and_old_scenes_without_resetting_views() {
        let mut state = AppState::default();
        state
            .result_figure_cache
            .insert("run=1;figure=structures_patran".into(), None);
        state.result_figure_cache.insert(
            "run=1;figure=openvsp_cad_preview".into(),
            Some(Arc::new(Scene::new(10.0, 10.0, None))),
        );
        state
            .view_states
            .insert("keep-camera".into(), Default::default());
        state.update_result_scene();
        assert!(state.result_figure_cache.is_empty());
        assert!(state.view_states.contains_key("keep-camera"));
    }

    #[test]
    fn completed_result_setter_marks_export_readiness() {
        let mut state = AppState::default();
        state.set_completed_pipeline_result(completed_result());

        assert!(state.pipeline_result.is_some());
        assert!(state.pipeline_result_complete);
    }

    fn completed_result() -> alas_pipeline::PipelineResult {
        let mut config = alas_config::AlasConfig::default();
        config.mission.enabled = false;
        config.mses.enabled = false;
        config.structures.enabled = false;
        alas_pipeline::DesignPipeline::new(config)
            .run(
                &alas_pipeline::PipelineOptions {
                    optimize: false,
                    compare_baseline: false,
                    quiet: true,
                    ..Default::default()
                },
                &alas_exec::RunEnvironment::default(),
            )
            .expect("native fixed-design result")
    }

    #[test]
    fn final_check_replaces_early_patran_status_with_completed_image_scene() {
        let mut state = AppState {
            pipeline_result: Some(completed_result()),
            ..Default::default()
        };
        let config = state.pipeline_result.as_ref().unwrap().config.clone();
        let theme = state.theme.figure_theme_name().to_owned();
        let key = format!(
            "run=0;solver={:?};theme={theme};language={};figure=structures_patran",
            state.selected_solver_view,
            alas_i18n::get_language()
        );
        let early = state
            .cached_result_figure(&key, "structures_patran", &config, &theme)
            .unwrap();
        assert!(!early
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Image { .. })));
        let path =
            std::env::temp_dir().join(format!("alas-patran-refresh-{}.png", std::process::id()));
        let pixels = alas_viz::raster::render_scene_png(&Scene::new(
            4.0,
            4.0,
            Some(alas_report::scene::Color::rgb(0, 100, 200)),
        ))
        .unwrap();
        std::fs::write(&path, pixels).unwrap();
        state.pipeline_result.as_mut().unwrap().structural_result =
            Some(alas_pipeline::structural::StructuralAnalysisResult {
                status: "ok".into(),
                patran: Some(alas_pipeline::structural::PatranExportResult {
                    status: "ok".into(),
                    png_paths: vec![("Level".into(), path.clone())],
                    ..Default::default()
                }),
                ..Default::default()
            });
        state.pipeline_result_complete = true;
        state.update_result_scene();
        state.verify_final_result_figures();
        let final_scene = state
            .result_figure_cache
            .get(&key)
            .unwrap()
            .as_ref()
            .unwrap();
        assert!(final_scene.elements.iter().any(|element| matches!(element,
            SceneElement::Image { source, .. } if source == &path.to_string_lossy().replace('\\', "/"))));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    #[ignore = "requires ALAS_CAD_MESH from a retained native OpenVSP run"]
    fn retained_openvsp_mesh_replaces_early_status_and_renders_without_graphics_build() {
        let path =
            std::path::PathBuf::from(std::env::var_os("ALAS_CAD_MESH").expect("retained mesh"));
        let mut state = AppState {
            pipeline_result: Some(completed_result()),
            ..Default::default()
        };
        let result = state.pipeline_result.as_mut().unwrap();
        let export = result.openvsp_export.as_mut().unwrap();
        export.status = alas_pipeline::OpenVspExportStatus::Vsp3Materialized;
        export.cad_preview_geometry_available = true;
        export.cad_preview_geometry_path = path;
        export.preview_available = false;
        export.preview_error = Some("runtime has no graphics-capable GUI build".into());
        let scene =
            alas_report::families::geometry::figure_openvsp_cad_preview(Some(export), Some("dark"));
        assert!(scene
            .elements
            .iter()
            .any(|element| matches!(element, SceneElement::Polygon { .. })));
        let png = alas_viz::raster::render_scene_png(&scene).expect("native mesh rasterizes");
        let output = std::path::PathBuf::from(
            std::env::var_os("ALAS_CAD_PREVIEW_OUTPUT").expect("preview destination"),
        );
        std::fs::write(output, png).unwrap();
    }
}
