// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py: figure_structures_patran
// (L6180-6229).

//! Display externally rendered Patran deformation plots.
//!
//! Patran owns the raster output, so this family places the ordered PNG paths
//! into [`crate::scene::SceneElement::Image`] nodes. SVG export keeps those
//! files external, matching the reference's `imshow` of the runner outputs.

use alas_pipeline::structural::StructuralAnalysisResult;

use super::status::{resolve_structural_result, status_message_scene, tool_not_configured_scene};
use crate::scene::{Color, Scene, SceneElement, TextAlign, TextBaseline};
use crate::theme::get_palette;

const TITLE: &str = "Structural Analysis: Patran Renders";

/// Display one externally rendered deformation image per available load case.
pub fn figure_structures_patran(
    result: Option<&StructuralAnalysisResult>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let result = match resolve_structural_result(result, TITLE, pal) {
        Ok(result) => result,
        Err(scene) => return scene,
    };
    let Some(patran) = result.patran.as_ref() else {
        return status_message_scene(
            TITLE,
            "Not run for this design (Analyses > Advanced > Structures > Render Patran deformation plots). Requires a working Patran install and a successful NASTRAN SOL 101 solve.",
            false,
            pal,
        );
    };
    if patran.png_paths.is_empty() && patran.status == "not_configured" {
        return tool_not_configured_scene(
            TITLE,
            "Patran",
            &format!(
                "Set its executable under Workspace > External Tools to render deformation plots. Reported status: {}",
                patran.error.as_deref().unwrap_or("executable not configured")
            ),
            pal,
        );
    }
    if patran.status == "not_run" {
        return status_message_scene(
            TITLE,
            "Not run for this design (Analyses > Advanced > Structures > Render Patran deformation plots). Requires a working Patran install and a successful NASTRAN SOL 101 solve.",
            false,
            pal,
        );
    }
    if patran.png_paths.is_empty() {
        return status_message_scene(
            TITLE,
            &format!(
                "Patran export failed: {}",
                patran.error.as_deref().unwrap_or("unknown error")
            ),
            false,
            pal,
        );
    }

    let panel_width = 500.0;
    let panel_height = 500.0;
    let mut scene = Scene::new(
        panel_width * patran.png_paths.len() as f64,
        panel_height,
        Some(Color::from_hex(pal.bg)),
    );
    scene.title = Some("Patran deformation renders".to_owned());
    for (index, (name, path)) in patran.png_paths.iter().enumerate() {
        let x = index as f64 * panel_width;
        scene.add(SceneElement::Image {
            source: path.to_string_lossy().replace('\\', "/"),
            x,
            y: 30.0,
            width: panel_width,
            height: panel_height - 30.0,
            source_rect: None,
        });
        scene.add(SceneElement::Text {
            text: name.clone(),
            pos: [x + panel_width / 2.0, 18.0],
            font_size: 12.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Center,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
    }
    if let Some(error) = patran.error.as_deref() {
        scene.add(SceneElement::Text {
            text: format!("Some load cases failed to render: {error}"),
            pos: [scene.width / 2.0, scene.height - 8.0],
            font_size: 9.0,
            color: Color::from_hex("#c0392b"),
            align: TextAlign::Center,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: false,
        });
    }
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn missing_patran_export_reports_the_reference_action() {
        let result = StructuralAnalysisResult {
            status: "ok".to_owned(),
            ..StructuralAnalysisResult::default()
        };
        let scene = figure_structures_patran(Some(&result), None);
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } | SceneElement::TextBlock { text, .. } if text.contains("working Patran"))
        }));
    }

    #[test]
    fn successful_patran_paths_become_ordered_image_nodes() {
        let result = StructuralAnalysisResult {
            status: "ok".to_owned(),
            patran: Some(alas_pipeline::structural::PatranExportResult {
                status: "ok".to_owned(),
                error: Some("level failed".to_owned()),
                png_paths: vec![
                    ("pull-up".to_owned(), PathBuf::from(r"C:\renders\pull.png")),
                    (
                        "push-down".to_owned(),
                        PathBuf::from(r"C:\renders\push.png"),
                    ),
                ],
            }),
            ..StructuralAnalysisResult::default()
        };
        let scene = figure_structures_patran(Some(&result), None);
        let images: Vec<&str> = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Image { source, .. } => Some(source.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(images, ["C:/renders/pull.png", "C:/renders/push.png"]);
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("Some load cases failed"))
        }));
    }
    fn patran_result(status: &str, error: &str) -> StructuralAnalysisResult {
        StructuralAnalysisResult {
            status: "ok".to_owned(),
            patran: Some(alas_pipeline::structural::PatranExportResult {
                status: status.to_owned(),
                error: Some(error.to_owned()),
                png_paths: Vec::new(),
            }),
            ..StructuralAnalysisResult::default()
        }
    }

    fn title_color(scene: &Scene) -> Option<Color> {
        scene.elements.iter().find_map(|e| match e {
            SceneElement::Text { text, color, .. } if text == TITLE => Some(*color),
            _ => None,
        })
    }

    #[test]
    fn a_real_patran_error_naming_an_absent_file_keeps_the_red_card() {
        let error = "Patran produced no image: result file absent after launch";
        let scene = figure_structures_patran(Some(&patran_result("error", error)), None);
        let pal = get_palette(None);
        assert_ne!(title_color(&scene), Some(Color::from_hex(pal.title)));
        assert!(scene.elements.iter().any(|e| {
            matches!(e, SceneElement::TextBlock { text, .. } if text.contains("Patran export failed") && text.contains(error))
        }));
    }

    #[test]
    fn a_missing_patran_tool_shows_the_neutral_card_with_the_original_text() {
        let error = "Patran executable absent: configure it under Workspace > External Tools";
        let scene = figure_structures_patran(Some(&patran_result("not_configured", error)), None);
        let pal = get_palette(None);
        assert_eq!(title_color(&scene), Some(Color::from_hex(pal.title)));
        assert!(scene.elements.iter().any(|e| {
            matches!(e, SceneElement::TextBlock { text, .. } if text.contains("not configured") && text.contains(error))
        }));
    }
}
