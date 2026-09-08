// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native OpenVSP CAD-preview evidence.
//!
//! The image is deliberately kept as an external scene resource. OpenVSP
//! owns the rasterization and the report layer only records its path, status,
//! and provenance; no second geometry renderer is introduced here.

use alas_pipeline::{OpenVspExportResult, OpenVspExportStatus};

use crate::scene::{Color, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

const PREVIEW_WIDTH: f64 = 960.0;
const PREVIEW_HEIGHT: f64 = 540.0;

fn status_scene(title: &str, message: &str, ok: bool, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(760.0, 240.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some(title.to_owned());
    scene.add(SceneElement::Text {
        text: title.to_owned(),
        pos: [24.0, 34.0],
        font_size: 16.0,
        color: Color::from_hex(if ok { "#27ae60" } else { "#c0392b" }),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: true,
    });
    scene.add(SceneElement::Text {
        text: message.to_owned(),
        pos: [24.0, 82.0],
        font_size: 12.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}

/// Display OpenVSP's native CAD screenshot when the retained run produced it.
///
/// A screenshot is useful evidence of the actual exported VSP model, but it
/// is not the runtime acceptance criterion. Consequently a preview warning or
/// a rejected comparison is kept visible in the status line instead of being
/// rewritten as a successful solver result.
pub fn figure_openvsp_cad_preview(
    export: Option<&OpenVspExportResult>,
    theme: Option<&str>,
) -> Scene {
    let Some(export) = export else {
        return status_scene(
            "OpenVSP CAD preview unavailable",
            "No OpenVSP export was retained for this run.",
            false,
            theme,
        );
    };
    let preview = export.script_path.with_extension("preview.png");
    if !preview.is_file() {
        let detail = export
            .runtime_error
            .as_deref()
            .unwrap_or("OpenVSP did not materialize its native preview image.");
        return status_scene(
            "OpenVSP CAD preview unavailable",
            &format!(
                "status={}; expected artifact: {}; {detail}",
                export.status.as_str(),
                preview.display()
            ),
            false,
            theme,
        );
    }

    let pal = get_palette(theme);
    let accepted = export.status == OpenVspExportStatus::Vsp3Materialized;
    let mut scene = Scene::new(
        PREVIEW_WIDTH + 40.0,
        PREVIEW_HEIGHT + 92.0,
        Some(Color::from_hex(pal.bg)),
    );
    scene.title = Some("OpenVSP Native CAD Preview".to_owned());
    scene.add(SceneElement::Text {
        text: format!(
            "OpenVSP native preview - status: {}",
            export.status.as_str()
        ),
        pos: [20.0, 24.0],
        font_size: 14.0,
        color: Color::from_hex(if accepted { "#27ae60" } else { "#dc7d23" }),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: true,
    });
    scene.add(SceneElement::Text {
        text: preview.display().to_string(),
        pos: [20.0, 49.0],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Rect {
        x: 20.0,
        y: 76.0,
        width: PREVIEW_WIDTH,
        height: PREVIEW_HEIGHT,
        rx: 4.0,
        fill: None,
        stroke: Some(Stroke::new(Color::from_hex(pal.border), 1.0)),
    });
    scene.add(SceneElement::Image {
        source: preview.to_string_lossy().replace('\\', "/"),
        x: 20.0,
        y: 76.0,
        width: PREVIEW_WIDTH,
        height: PREVIEW_HEIGHT,
        source_rect: None,
    });
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn export(path: PathBuf, status: OpenVspExportStatus) -> OpenVspExportResult {
        OpenVspExportResult {
            script_path: path,
            vsp3_path: PathBuf::from("aircraft.vsp3"),
            vspaero_geometry_path: PathBuf::from("aircraft.vspgeom"),
            status,
            runtime_executable: None,
            runtime_error: None,
            runtime_stdout_path: None,
            runtime_stderr_path: None,
            component_count: 2,
            wheel_count: 0,
            approximations: Vec::new(),
            unsupported: Vec::new(),
        }
    }

    #[test]
    fn missing_native_preview_is_explained_without_inventing_a_figure() {
        let scene = figure_openvsp_cad_preview(
            Some(&export(
                PathBuf::from(r"C:\run\optimized_aircraft.openvsp.vspscript"),
                OpenVspExportStatus::RuntimeRejected,
            )),
            None,
        );
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("expected artifact"))
        }));
        assert!(!scene
            .elements
            .iter()
            .any(|element| matches!(element, SceneElement::Image { .. })));
    }
}
