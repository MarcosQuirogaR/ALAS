// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Vector PDF export for sectioned ALAS figure reports.
//!
//! [`Scene`] is the report-quality source shared by the interactive viewport
//! and SVG exporter. This backend writes the same vector geometry to PDF
//! without a rasterizer or a second figure implementation. External-solver
//! images remain path references, so the PDF identifies them while the SVG
//! archive preserves the source reference.

mod render;
mod win_ansi;

use crate::scene::{Scene, SceneElement};

/// One vector figure included in a [`PdfSection`].
#[derive(Debug, Clone, PartialEq)]
pub struct PdfFigure {
    /// Stable archive filename for the companion SVG source.
    pub file_name: String,
    /// User-facing figure title.
    pub title: String,
    /// Vector scene to place on its own report page.
    pub scene: Scene,
}

/// One named report section containing an ordered set of figures.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfSection {
    /// User-facing discipline name.
    pub title: String,
    /// Figures in the report and archive order.
    pub figures: Vec<PdfFigure>,
}

/// Errors raised before a document can be emitted.
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    /// A report without figures has no meaningful document body.
    #[error("cannot create a PDF report without exported figures")]
    EmptyDocument,
    /// PDF numeric operators cannot represent a non-finite scene coordinate.
    #[error("figure '{title}' contains a non-finite PDF coordinate")]
    NonFiniteScene {
        /// Figure title used to identify the invalid source.
        title: String,
    },
}

/// Render ordered report sections as a self-contained vector PDF document.
///
/// Every non-empty section starts with a separator page, followed by one page
/// per figure. The document therefore retains both the registry order and its
/// named engineering sections when opened independently of the archive.
pub fn render_sectioned_pdf(sections: &[PdfSection]) -> Result<Vec<u8>, PdfError> {
    let sections = sections
        .iter()
        .filter(|section| !section.figures.is_empty())
        .collect::<Vec<_>>();
    if sections.is_empty() {
        return Err(PdfError::EmptyDocument);
    }
    for section in &sections {
        for figure in &section.figures {
            if !scene_is_finite(&figure.scene) {
                return Err(PdfError::NonFiniteScene {
                    title: figure.title.clone(),
                });
            }
        }
    }
    Ok(render::render_sections(&sections))
}

fn scene_is_finite(scene: &Scene) -> bool {
    scene.width.is_finite()
        && scene.height.is_finite()
        && scene.width > 0.0
        && scene.height > 0.0
        && scene.elements.iter().all(element_is_finite)
}

fn element_is_finite(element: &SceneElement) -> bool {
    match element {
        SceneElement::Line { p1, p2, stroke } => {
            points_are_finite(&[*p1, *p2])
                && stroke_is_finite(stroke.width, stroke.dash_array.as_deref())
        }
        SceneElement::Polyline { points, stroke } => {
            points_are_finite(points)
                && stroke_is_finite(stroke.width, stroke.dash_array.as_deref())
        }
        SceneElement::Polygon { points, stroke, .. } => {
            points_are_finite(points) && optional_stroke_is_finite(stroke.as_ref())
        }
        SceneElement::Rect {
            x,
            y,
            width,
            height,
            rx,
            stroke,
            ..
        } => {
            [*x, *y, *width, *height, *rx]
                .iter()
                .all(|value| value.is_finite())
                && optional_stroke_is_finite(stroke.as_ref())
        }
        SceneElement::Circle {
            center,
            radius,
            stroke,
            ..
        } => {
            points_are_finite(&[*center])
                && radius.is_finite()
                && optional_stroke_is_finite(stroke.as_ref())
        }
        SceneElement::Image {
            x,
            y,
            width,
            height,
            ..
        } => [*x, *y, *width, *height]
            .iter()
            .all(|value| value.is_finite()),
        SceneElement::SphericalImage { center, radius, .. } => {
            points_are_finite(&[*center]) && radius.is_finite()
        }
        SceneElement::Text {
            pos,
            font_size,
            angle_deg,
            ..
        } => points_are_finite(&[*pos]) && font_size.is_finite() && angle_deg.is_finite(),
        SceneElement::TextBlock {
            pos,
            width,
            font_size,
            ..
        } => points_are_finite(&[*pos]) && width.is_finite() && font_size.is_finite(),
    }
}

fn points_are_finite(points: &[[f64; 2]]) -> bool {
    points.iter().flatten().all(|value| value.is_finite())
}

fn optional_stroke_is_finite(stroke: Option<&crate::scene::Stroke>) -> bool {
    stroke.is_none_or(|stroke| stroke_is_finite(stroke.width, stroke.dash_array.as_deref()))
}

fn stroke_is_finite(width: f64, dash: Option<&[f64]>) -> bool {
    width.is_finite() && dash.is_none_or(|dash| dash.iter().all(|value| value.is_finite()))
}

#[cfg(test)]
mod tests {
    use super::{render_sectioned_pdf, PdfError, PdfFigure, PdfSection};
    use crate::scene::{Color, Scene, SceneElement, Stroke};

    fn sample_scene() -> Scene {
        let mut scene = Scene::new(240.0, 160.0, Some(Color::rgb(255, 255, 255)));
        scene.add(SceneElement::Line {
            p1: [10.0, 10.0],
            p2: [220.0, 140.0],
            stroke: Stroke::new(Color::rgb(37, 99, 235), 2.0),
        });
        scene
    }

    #[test]
    fn pdf_preserves_section_and_figure_order_in_the_document_text() -> Result<(), PdfError> {
        let document = render_sectioned_pdf(&[
            PdfSection {
                title: "Aerodynamics".to_owned(),
                figures: vec![PdfFigure {
                    file_name: "01_aero_panel.svg".to_owned(),
                    title: "Lift and Drag".to_owned(),
                    scene: sample_scene(),
                }],
            },
            PdfSection {
                title: "Mission".to_owned(),
                figures: vec![PdfFigure {
                    file_name: "02_mission_profile.svg".to_owned(),
                    title: "Mission Profile".to_owned(),
                    scene: sample_scene(),
                }],
            },
        ])?;
        let text = String::from_utf8_lossy(&document);
        let positions = (
            text.find("Aerodynamics"),
            text.find("Lift and Drag"),
            text.find("Mission"),
        );

        assert!(document.starts_with(b"%PDF-1.4"));
        assert!(text.contains("/Count 4"));
        assert!(
            matches!(positions, (Some(section), Some(figure), Some(mission))
                if section < figure && figure < mission)
        );
        assert!(text.contains("01_aero_panel.svg"));
        assert!(text.contains("02_mission_profile.svg"));
        Ok(())
    }

    #[test]
    fn pdf_rejects_empty_or_non_finite_documents() {
        assert!(matches!(
            render_sectioned_pdf(&[]),
            Err(PdfError::EmptyDocument)
        ));

        let mut invalid = sample_scene();
        invalid.width = f64::NAN;
        let result = render_sectioned_pdf(&[PdfSection {
            title: "Invalid".to_owned(),
            figures: vec![PdfFigure {
                file_name: "invalid.svg".to_owned(),
                title: "Invalid Figure".to_owned(),
                scene: invalid,
            }],
        }]);
        assert!(matches!(result, Err(PdfError::NonFiniteScene { .. })));
    }
}
