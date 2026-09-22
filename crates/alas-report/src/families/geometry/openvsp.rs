// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native OpenVSP CAD-preview evidence.
//!
//! OpenVSP owns the rasterization when its GUI build is available. Headless
//! runs still expose a deterministic vector projection of the native full
//! outer-mold-line `.cad_preview.vspgeom` mesh (fuselage, landing gear, and
//! wings), so the report never substitutes a generic aircraft thumbnail for
//! missing runtime evidence. That mesh is a second, preview-only export: it
//! is distinct from `vspaero_geometry_path`, the thin wing-only mesh an
//! installed VSPAERO run actually solves, so this preview can show the whole
//! aircraft without changing what the aerodynamic solver sees.

use alas_pipeline::{OpenVspExportResult, OpenVspExportStatus};
use std::fs;

use crate::scene::{Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

const PREVIEW_WIDTH: f64 = 960.0;
const PREVIEW_HEIGHT: f64 = 540.0;
const MAX_MESH_BYTES: u64 = 64 * 1024 * 1024;

fn status_scene(title: &str, message: &str, ok: bool, theme: Option<&str>) -> Scene {
    crate::status_figure::figure_status_message(title, message, ok, theme)
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
    if export.preview_available && export.preview_path.is_file() {
        return native_preview_scene(export, theme);
    }

    if export.status == OpenVspExportStatus::Vsp3Materialized {
        if let Some(scene) = mesh_projection_scene(export, theme) {
            return scene;
        }
    }

    // Prefer explaining why the full-aircraft mesh projection specifically
    // failed: that is the artifact this fallback is trying to substitute
    // for, and reporting only the (usually unrelated) screenshot failure
    // would leave the actual cause of a missing preview unexplained.
    let detail = export
        .cad_preview_geometry_error
        .as_deref()
        .or(export.preview_error.as_deref())
        .or(export.runtime_error.as_deref())
        .unwrap_or("OpenVSP did not materialize a native preview image or mesh projection.");
    status_scene(
        "OpenVSP CAD preview unavailable",
        &format!(
            "status={}; expected artifact (native screenshot): {}; {detail}",
            export.status.as_str(),
            export.preview_path.display()
        ),
        false,
        theme,
    )
}

fn native_preview_scene(export: &OpenVspExportResult, theme: Option<&str>) -> Scene {
    let preview = &export.preview_path;
    let pal = get_palette(theme);
    let accepted = export.status == OpenVspExportStatus::Vsp3Materialized;
    let mut scene = Scene::new(
        PREVIEW_WIDTH + 40.0,
        PREVIEW_HEIGHT + 92.0,
        Some(Color::from_hex(pal.bg)),
    );
    scene.title = Some("OpenVSP Native CAD Preview".to_owned());
    scene.suppress_derived_title();
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
        text: "Native OpenVSP shaded view: open the model to rotate, zoom, and inspect components."
            .to_owned(),
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
    let [image_width, image_height] = native_image_size(preview);
    scene.add(SceneElement::Image {
        source: preview.to_string_lossy().replace('\\', "/"),
        x: 20.0 + (PREVIEW_WIDTH - image_width) * 0.5,
        y: 76.0 + (PREVIEW_HEIGHT - image_height) * 0.5,
        width: image_width,
        height: image_height,
        source_rect: None,
    });
    scene
}

fn native_image_size(path: &std::path::Path) -> [f64; 2] {
    use std::io::Read;
    let mut header = [0_u8; 24];
    let dimensions = fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .ok()
        .filter(|_| &header[..8] == b"\x89PNG\r\n\x1a\n" && &header[12..16] == b"IHDR")
        .map(|_| {
            (
                u32::from_be_bytes(header[16..20].try_into().unwrap()),
                u32::from_be_bytes(header[20..24].try_into().unwrap()),
            )
        });
    if let Some((w, h)) = dimensions.filter(|(w, h)| *w > 0 && *h > 0) {
        let scale = (PREVIEW_WIDTH / f64::from(w)).min(PREVIEW_HEIGHT / f64::from(h));
        [f64::from(w) * scale, f64::from(h) * scale]
    } else {
        [PREVIEW_WIDTH, PREVIEW_HEIGHT]
    }
}

fn mesh_projection_scene(export: &OpenVspExportResult, theme: Option<&str>) -> Option<Scene> {
    // The full outer-mold-line mesh is a distinct, preview-only artifact
    // (fuselage and landing gear included) from the thin wing-only mesh an
    // installed VSPAERO run actually solves. Gate on the freshness-checked
    // availability flag, not just file presence, so a leftover file from an
    // unrelated run can never be drawn as if it were current evidence.
    if !export.cad_preview_geometry_available {
        return None;
    }
    let (points, faces) = read_vspgeom_mesh(&export.cad_preview_geometry_path)?;
    let projected = project_mesh(&points, &faces);
    if projected.is_empty() {
        return None;
    }

    let pal = get_palette(theme);
    let mut scene = Scene::new(
        PREVIEW_WIDTH + 40.0,
        PREVIEW_HEIGHT + 112.0,
        Some(Color::from_hex(pal.bg)),
    );
    scene.title = Some("OpenVSP Native Mesh Projection".to_owned());
    scene.suppress_derived_title();
    scene.add(SceneElement::Text {
        text: "OpenVSP native mesh projection - ScreenGrab unavailable".to_owned(),
        pos: [20.0, 24.0],
        font_size: 14.0,
        color: Color::from_hex("#dc7d23"),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: true,
    });
    scene.add(SceneElement::Text {
        text: format!(
            "Projected from native full-aircraft VSPGEOM mesh: {} ({} faces)",
            export
                .cad_preview_geometry_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy(),
            projected.len()
        ),
        pos: [20.0, 49.0],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: "Native OpenVSP fuselage, landing gear, and wings. Preview geometry is separate from the aerodynamic solver."
            .to_owned(),
        pos: [20.0, 66.0],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });

    for (shade, points) in projected {
        let shade = shade.clamp(0.0, 1.0);
        let green = 105_u8.saturating_add((shade * 70.0).round() as u8);
        let blue = 180_u8.saturating_add((shade * 60.0).round() as u8);
        scene.add(SceneElement::Polygon {
            points,
            fill: Some(Fill::new(Color::rgba(35, green, blue, 255))),
            // Same-color subpixel coverage prevents antialiasing cracks
            // between adjacent opaque faces in vector renderers.
            stroke: Some(Stroke::new(Color::rgba(35, green, blue, 255), 0.35)),
        });
    }
    scene.add(SceneElement::Rect {
        x: 20.0,
        y: 84.0,
        width: PREVIEW_WIDTH,
        height: PREVIEW_HEIGHT,
        rx: 4.0,
        fill: None,
        stroke: Some(Stroke::new(Color::from_hex(pal.border), 1.0)),
    });
    Some(scene)
}

/// Read the first native OpenVSP VSPGEOM surface.
///
/// OpenVSP's `vspgeom v3` format begins with a version line, a surface count,
/// then each surface's point/face counts followed by its point and face rows.
/// A parsed `.vspgeom` surface: the point coordinates and each face's point
/// indices.
type VspGeomMesh = (Vec<[f64; 3]>, Vec<Vec<usize>>);

/// The export currently requests one thin geometry surface. Invalid or
/// unexpectedly large files are rejected so a report cannot present guessed
/// geometry as evidence.
fn read_vspgeom_mesh(path: &std::path::Path) -> Option<VspGeomMesh> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.len() == 0 || metadata.len() > MAX_MESH_BYTES {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next()?.trim() != "# vspgeom v3" {
        return None;
    }
    let surface_count = lines.next()?.trim().parse::<usize>().ok()?;
    if surface_count == 0 {
        return None;
    }

    // Only the first surface is needed for the current thin-geometry export.
    // Its rows contain the full native mesh used by the report projection.
    let header = lines.by_ref().find(|line| !line.trim().is_empty())?;
    let mut header_fields = header.split_whitespace();
    let point_count = header_fields.next()?.parse::<usize>().ok()?;
    let _declared_face_count = header_fields.next()?.parse::<usize>().ok()?;
    let _dimension = header_fields.next()?.parse::<usize>().ok()?;
    if !(3..=2_000_000).contains(&point_count) {
        return None;
    }

    let mut points = Vec::with_capacity(point_count);
    for _ in 0..point_count {
        let row = lines.next()?.split_whitespace().collect::<Vec<_>>();
        if row.len() < 3 {
            return None;
        }
        let x = row[0].parse::<f64>().ok()?;
        let y = row[1].parse::<f64>().ok()?;
        let z = row[2].parse::<f64>().ok()?;
        if !(x.is_finite() && y.is_finite() && z.is_finite()) {
            return None;
        }
        points.push([x, y, z]);
    }

    let face_count_line = lines.by_ref().find(|line| !line.trim().is_empty())?;
    let face_count = face_count_line.trim().parse::<usize>().ok()?;
    if face_count == 0 || face_count > 2_000_000 {
        return None;
    }
    let mut faces = Vec::with_capacity(face_count);
    for _ in 0..face_count {
        let row = lines.next()?.split_whitespace().collect::<Vec<_>>();
        let vertex_count = row.first()?.parse::<usize>().ok()?;
        // Native OpenVSP surfaces are not limited to triangles and quads.
        // The retained full-aircraft export contains tessellated polygons
        // with more boundary vertices, so reject pathological rows while
        // preserving the face topology OpenVSP actually wrote.
        if !(3..=64).contains(&vertex_count) || row.len() < vertex_count + 1 {
            return None;
        }
        let mut face = Vec::with_capacity(vertex_count);
        for token in row.iter().skip(1).take(vertex_count) {
            let index = token.parse::<usize>().ok()?.checked_sub(1)?;
            if index >= points.len() {
                return None;
            }
            face.push(index);
        }
        faces.push(face);
    }
    Some((points, faces))
}

fn project_mesh(points: &[[f64; 3]], faces: &[Vec<usize>]) -> Vec<(f64, Vec<[f64; 2]>)> {
    let projected_points = points
        .iter()
        .map(|point| {
            // Orthonormal camera basis; depth increases toward the viewer.
            let horizontal = 0.8 * point[0] + 0.6 * point[1];
            let vertical = -0.3 * point[0] + 0.4 * point[1] + (3.0_f64.sqrt() / 2.0) * point[2];
            let depth = (3.0_f64.sqrt() / 2.0) * (0.6 * point[0] - 0.8 * point[1]) + 0.5 * point[2];
            [horizontal, vertical, depth]
        })
        .collect::<Vec<_>>();
    let mut min_horizontal = f64::INFINITY;
    let mut max_horizontal = f64::NEG_INFINITY;
    let mut min_vertical = f64::INFINITY;
    let mut max_vertical = f64::NEG_INFINITY;
    for point in &projected_points {
        min_horizontal = min_horizontal.min(point[0]);
        max_horizontal = max_horizontal.max(point[0]);
        min_vertical = min_vertical.min(point[1]);
        max_vertical = max_vertical.max(point[1]);
    }
    let horizontal_span = (max_horizontal - min_horizontal).max(1.0e-9);
    let vertical_span = (max_vertical - min_vertical).max(1.0e-9);
    let plot_left = 20.0;
    let plot_top = 84.0;
    let plot_width = PREVIEW_WIDTH;
    let plot_height = PREVIEW_HEIGHT;
    let scale = ((plot_width - 56.0) / horizontal_span).min((plot_height - 56.0) / vertical_span);
    let to_canvas = |point: &[f64; 3]| {
        [
            plot_left
                + plot_width * 0.5
                + (point[0] - (min_horizontal + max_horizontal) * 0.5) * scale,
            plot_top + plot_height * 0.5 - (point[1] - (min_vertical + max_vertical) * 0.5) * scale,
        ]
    };

    let mut projected_faces = faces
        .iter()
        .filter_map(|face| {
            let polygon = face
                .iter()
                .map(|index| to_canvas(&projected_points[*index]))
                .collect::<Vec<_>>();
            if polygon.len() < 3 || polygon_area(&polygon).abs() < 1.0e-6 {
                return None;
            }
            let depth = face
                .iter()
                .map(|index| projected_points[*index][2])
                .sum::<f64>()
                / face.len() as f64;
            // Newell normal supports native polygons with collinear vertices.
            let mut normal = [0.0_f64; 3];
            for i in 0..face.len() {
                let a = projected_points[face[i]];
                let b = projected_points[face[(i + 1) % face.len()]];
                normal[0] += (a[1] - b[1]) * (a[2] + b[2]);
                normal[1] += (a[2] - b[2]) * (a[0] + b[0]);
                normal[2] += (a[0] - b[0]) * (a[1] + b[1]);
            }
            // Two-sided lighting also supports thin lifting surfaces.
            if normal[2] < 0.0 {
                normal = normal.map(|v| -v);
            }
            let length = normal.iter().map(|v| v * v).sum::<f64>().sqrt();
            let diffuse = if length > 1.0e-12 {
                ((-0.3 * normal[0] + 0.4 * normal[1] + (3.0_f64.sqrt() / 2.0) * normal[2]) / length)
                    .max(0.0)
            } else {
                0.0
            };
            Some((depth, polygon, 0.25 + 0.75 * diffuse))
        })
        .collect::<Vec<_>>();
    projected_faces.sort_by(|left, right| left.0.total_cmp(&right.0));
    projected_faces
        .into_iter()
        .map(|(_, polygon, shade)| (shade, polygon))
        .collect()
}

fn polygon_area(points: &[[f64; 2]]) -> f64 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(left, right)| left[0] * right[1] - right[0] * left[1])
        .sum::<f64>()
        * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn export(path: PathBuf, status: OpenVspExportStatus) -> OpenVspExportResult {
        OpenVspExportResult {
            script_path: path,
            vsp3_path: PathBuf::from("aircraft.vsp3"),
            vspaero_geometry_path: PathBuf::from("aircraft.vspgeom"),
            cad_preview_vsp3_path: PathBuf::from("aircraft.cad_preview.vsp3"),
            cad_preview_geometry_path: PathBuf::from("aircraft.cad_preview.vspgeom"),
            cad_preview_geometry_available: false,
            cad_preview_geometry_error: None,
            preview_path: PathBuf::from("aircraft.preview.png"),
            preview_available: false,
            preview_error: None,
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

    /// Minimal valid `vspgeom v3` cube fixture, reused by both the positive
    /// (CAD preview available) and negative-control (solver-only mesh must
    /// not be substituted) tests below.
    fn cube_vspgeom() -> &'static str {
        "# vspgeom v3\n1\n8 6 3\n-1 -1 -1\n1 -1 -1\n1 1 -1\n-1 1 -1\n-1 -1 1\n1 -1 1\n1 1 1\n-1 1 1\n6\n4 1 2 3 4\n4 5 8 7 6\n4 1 5 6 2\n4 2 6 7 3\n4 3 7 8 4\n4 5 1 4 8\n"
    }

    fn concave_pentagon_vspgeom() -> &'static str {
        "# vspgeom v3\n1\n5 1 3\n0 0 0\n2 0 0\n2 2 0\n1 1 0\n0 2 0\n1\n5 1 2 3 4 5\n"
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
            matches!(element, SceneElement::Text { text, .. } | SceneElement::TextBlock { text, .. } if text.contains("expected artifact"))
        }));
        assert!(!scene
            .elements
            .iter()
            .any(|element| matches!(element, SceneElement::Image { .. })));
    }

    #[test]
    fn cropped_native_screenshot_keeps_its_aspect_ratio_and_replaces_mesh() {
        let path =
            std::env::temp_dir().join(format!("alas-native-aspect-{}.png", std::process::id()));
        let mut header = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        header.extend_from_slice(&800_u32.to_be_bytes());
        header.extend_from_slice(&800_u32.to_be_bytes());
        fs::write(&path, header).unwrap();
        let mut result = export(
            path.with_extension("vspscript"),
            OpenVspExportStatus::Vsp3Materialized,
        );
        result.preview_available = true;
        result.preview_path = path.clone();
        result.cad_preview_geometry_available = true;
        let scene = figure_openvsp_cad_preview(Some(&result), Some("dark"));
        assert!(scene.elements.iter().any(|e| matches!(e,
            SceneElement::Image { width, height, x, .. }
            if (*width - 540.0).abs() < 1.0e-9 && (*height - 540.0).abs() < 1.0e-9 && (*x - 230.0).abs() < 1.0e-9)));
        assert!(!scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Polygon { .. })));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn unavailable_preview_has_one_red_visible_title_and_keeps_metadata() {
        let scene = figure_openvsp_cad_preview(None, Some("dark"));
        assert_eq!(
            scene.title.as_deref(),
            Some("OpenVSP CAD preview unavailable")
        );
        assert!(!scene.render_title);
        let title_elements = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text {
                    text, color, bold, ..
                } if text == "OpenVSP CAD preview unavailable" => Some((*color, *bold)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(title_elements, vec![(Color::from_hex("#c0392b"), true)]);
    }

    #[test]
    fn unavailable_diagnostics_wrap_inside_the_content_box() {
        use crate::scene::conservative_char_budget;
        use crate::status_figure::{STATUS_BODY_FONT_SIZE, STATUS_MARGIN, STATUS_WIDTH};
        let long_path = format!(r"C:\runs\{}\aircraft.preview.png", "diagnostic".repeat(40));
        let message = format!("status=runtime_rejected; expected artifact: {long_path}");
        let scene = status_scene(
            "OpenVSP CAD preview unavailable",
            &message,
            false,
            Some("grey"),
        );
        let body = scene
            .elements
            .iter()
            .find_map(|element| match element {
                SceneElement::TextBlock {
                    text, pos, width, ..
                } => Some((text.as_str(), *pos, *width)),
                _ => None,
            })
            .expect("unavailable diagnostic body");
        assert_eq!(body.0, message, "the diagnostic is retained verbatim");
        assert_eq!(body.1[0], STATUS_MARGIN);
        assert_eq!(body.1[0] + body.2, STATUS_WIDTH - STATUS_MARGIN);

        let svg = crate::svg::render_svg(&scene);
        assert!(svg.contains("viewBox=\"0 0 760.0"));
        let budget = conservative_char_budget(STATUS_BODY_FONT_SIZE, body.2);
        let rows = svg
            .split("<tspan")
            .skip(1)
            .filter_map(|row| {
                let start = row.find('>')? + 1;
                let end = row.find("</tspan>")?;
                Some(&row[start..end])
            })
            .collect::<Vec<_>>();
        assert!(rows.len() > 3, "long path should use multiple rows");
        assert!(rows.iter().all(|row| row.chars().count() <= budget));
        assert_eq!(svg.matches("x=\"32.00\"").count(), rows.len());
    }

    #[test]
    fn materialized_native_mesh_gets_a_truthful_vector_projection() {
        let root = std::env::temp_dir().join(format!("alas-openvsp-report-{}", std::process::id()));
        fs::create_dir_all(&root).expect("temporary report fixture directory");
        let script_path = root.join("aircraft.vspscript");
        let mesh_path = root.join("aircraft.cad_preview.vspgeom");
        fs::write(&mesh_path, cube_vspgeom()).expect("write VSPGEOM fixture");
        let mut result = export(script_path, OpenVspExportStatus::Vsp3Materialized);
        result.cad_preview_geometry_path = mesh_path.clone();
        result.cad_preview_geometry_available = true;
        result.preview_path = root.join("aircraft.preview.png");
        result.preview_error = Some("headless GUI build".to_owned());

        let scene = figure_openvsp_cad_preview(Some(&result), None);
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("native mesh projection"))
        }));
        assert!(scene
            .elements
            .iter()
            .any(|element| matches!(element, SceneElement::Polygon { .. })));
        assert!(!scene
            .elements
            .iter()
            .any(|element| matches!(element, SceneElement::Image { .. })));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("VSPGEOM"))
        }));
        fs::remove_dir_all(root).expect("remove temporary report fixture directory");
    }

    #[test]
    fn native_mesh_keeps_openvsp_faces_with_more_than_four_vertices() {
        let root = std::env::temp_dir().join(format!(
            "alas-openvsp-report-polygon-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("temporary report fixture directory");
        let mesh_path = root.join("aircraft.cad_preview.vspgeom");
        fs::write(&mesh_path, concave_pentagon_vspgeom()).expect("write polygon VSPGEOM fixture");
        let mut result = export(
            root.join("aircraft.vspscript"),
            OpenVspExportStatus::Vsp3Materialized,
        );
        result.cad_preview_geometry_path = mesh_path;
        result.cad_preview_geometry_available = true;

        let scene = figure_openvsp_cad_preview(Some(&result), None);
        assert!(scene.elements.iter().any(
            |element| matches!(element, SceneElement::Polygon { points, .. } if points.len() == 5)
        ));
        fs::remove_dir_all(root).expect("remove temporary report fixture directory");
    }

    #[test]
    fn projection_keeps_every_face_above_the_old_limit() {
        let face_count = 10_230;
        let mut points = Vec::with_capacity(face_count * 3);
        let mut faces = Vec::with_capacity(face_count);
        for index in 0..face_count {
            let x = index as f64;
            let base = points.len();
            points.extend_from_slice(&[[x, 0.0, 0.0], [x + 0.4, 0.0, 0.0], [x, 0.4, 0.0]]);
            faces.push(vec![base, base + 1, base + 2]);
        }

        let projected = project_mesh(&points, &faces);
        assert_eq!(projected.len(), face_count);
        assert!(projected.iter().all(|(_, polygon)| polygon.len() == 3));
    }

    #[test]
    fn projection_preserves_camera_plane_lengths_and_margins() {
        // Unit square in the camera plane: both edges must have equal screen length.
        let up = [-0.3, 0.4, 3.0_f64.sqrt() / 2.0];
        let points = [
            [0.0, 0.0, 0.0],
            [0.8, 0.6, 0.0],
            [0.8 + up[0], 0.6 + up[1], up[2]],
            up,
        ];
        let projected = project_mesh(&points, &[vec![0, 1, 2, 3]]);
        let polygon = &projected[0].1;
        let distance = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
        assert!(
            (distance(polygon[0], polygon[1]) - distance(polygon[1], polygon[2])).abs() < 1.0e-9
        );
        for p in polygon {
            assert!(p[0] >= 48.0 - 1.0e-9 && p[0] <= 20.0 + PREVIEW_WIDTH - 28.0 + 1.0e-9);
            assert!(p[1] >= 112.0 - 1.0e-9 && p[1] <= 84.0 + PREVIEW_HEIGHT - 28.0 + 1.0e-9);
        }
    }

    /// Optional visual audit against a real native export, without invoking OpenVSP.
    #[test]
    #[ignore = "set ALAS_PREVIEW_MESH and ALAS_PREVIEW_SVG for a local visual audit"]
    fn render_native_preview_for_visual_audit() {
        let mesh =
            std::path::PathBuf::from(std::env::var_os("ALAS_PREVIEW_MESH").expect("mesh path"));
        let output = std::env::var_os("ALAS_PREVIEW_SVG").expect("SVG output path");
        let mut result = export(
            mesh.with_extension("vspscript"),
            OpenVspExportStatus::Vsp3Materialized,
        );
        result.cad_preview_geometry_path = mesh;
        result.cad_preview_geometry_available = true;
        let scene = mesh_projection_scene(&result, Some("dark")).expect("valid native mesh");
        fs::write(output, crate::svg::render_svg(&scene)).expect("write visual audit");
    }

    /// Negative control for the reported bug. Even when the solver-only,
    /// wing-only mesh (`vspaero_geometry_path`) is present and structurally
    /// valid, it must never be substituted for a missing full-aircraft CAD
    /// preview: that would silently resurrect a milder form of "preview has
    /// no fuselage" instead of honestly reporting the preview as unavailable.
    #[test]
    fn a_present_solver_only_mesh_is_not_substituted_for_a_missing_cad_preview() {
        let root = std::env::temp_dir().join(format!(
            "alas-openvsp-report-negctrl-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("temporary report fixture directory");
        let script_path = root.join("aircraft.vspscript");
        let solver_mesh_path = root.join("aircraft.vspgeom");
        fs::write(&solver_mesh_path, cube_vspgeom()).expect("write solver VSPGEOM fixture");
        let mut result = export(script_path, OpenVspExportStatus::Vsp3Materialized);
        result.vspaero_geometry_path = solver_mesh_path;
        result.cad_preview_geometry_path = root.join("aircraft.cad_preview.vspgeom");
        result.cad_preview_geometry_available = false;
        result.cad_preview_geometry_error =
            Some("OpenVSP completed the solver-facing export, but no fresh full outer-mold-line preview mesh was written".to_owned());
        result.preview_path = root.join("aircraft.preview.png");
        result.preview_error = Some("headless GUI build".to_owned());

        let scene = figure_openvsp_cad_preview(Some(&result), None);
        assert!(!scene
            .elements
            .iter()
            .any(|element| matches!(element, SceneElement::Polygon { .. })));
        assert!(!scene
            .elements
            .iter()
            .any(|element| matches!(element, SceneElement::Image { .. })));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } | SceneElement::TextBlock { text, .. } if text.contains("expected artifact"))
        }));
        fs::remove_dir_all(root).expect("remove temporary report fixture directory");
    }
}
