// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Plotters drawing backend that records into the report scene graph.
//!
//! Ordinary scientific charts should use Plotters for coordinate ranges,
//! numeric ticks, gridlines, labels, and legends.  Recording those drawing
//! calls as scene primitives keeps the chart independent of the eventual
//! output: the SVG exporter and the egui viewer consume the same result.

use crate::scene::{Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use plotters::coord::Shift;
use plotters::drawing::{DrawingArea, DrawingAreaErrorKind};
use plotters_backend::{
    text_anchor::{HPos, VPos},
    BackendColor, BackendCoord, BackendStyle, BackendTextStyle, DrawingBackend, DrawingErrorKind,
    FontStyle, FontTransform,
};
use std::cell::RefCell;
use std::io;
use std::rc::Rc;

/// Plotters backend which records vector operations in a [`Scene`].
///
/// The backend intentionally does not rasterize.  A chart built with
/// `ChartBuilder` therefore has one source of truth for SVG and GUI output.
pub struct SceneBackend {
    scene: Scene,
}

/// Coverage ledger for ordinary 2-D chart migration.
///
/// Geometry-heavy and solver-specific figures remain scene-native until their
/// data contracts are migrated; keeping that boundary explicit prevents a
/// second ad-hoc chart implementation from quietly returning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartCoverage {
    /// Stable figure or family identifier used by the report registry.
    pub id: &'static str,
    /// Whether the figure currently uses `ChartBuilder` through this backend.
    pub migrated: bool,
}

/// Current Plotters migration ledger and bounded follow-up list.
pub const PLOTTERS_CHART_COVERAGE: &[ChartCoverage] = &[
    ChartCoverage {
        id: "performance.vn_diagram",
        migrated: false,
    },
    ChartCoverage {
        id: "mass",
        migrated: false,
    },
    ChartCoverage {
        id: "aerodynamics",
        migrated: false,
    },
    ChartCoverage {
        id: "mission",
        migrated: false,
    },
    ChartCoverage {
        id: "propulsion",
        migrated: false,
    },
    ChartCoverage {
        id: "structural",
        migrated: false,
    },
    ChartCoverage {
        id: "mses",
        migrated: false,
    },
];

impl SceneBackend {
    /// Construct a scene target with the requested canvas size and background.
    pub fn new(width: u32, height: u32, background: Option<Color>) -> Self {
        Self {
            scene: Scene::new(width as f64, height as f64, background),
        }
    }

    /// Return the recorded scene after Plotters has finished drawing.
    pub fn into_scene(self) -> Scene {
        self.scene
    }

    /// Render one Plotters chart into a backend-neutral scene.
    pub fn draw_chart<F>(
        width: u32,
        height: u32,
        background: Option<Color>,
        draw: F,
    ) -> Result<Scene, DrawingAreaErrorKind<io::Error>>
    where
        F: FnOnce(&DrawingArea<SceneBackend, Shift>) -> Result<(), DrawingAreaErrorKind<io::Error>>,
    {
        let shared = Rc::new(RefCell::new(Self::new(width, height, background)));
        let root: DrawingArea<SceneBackend, Shift> = (&shared).into();
        let result = draw(&root);
        drop(root);

        let backend = Rc::try_unwrap(shared).map_err(|_| {
            DrawingAreaErrorKind::BackendError(DrawingErrorKind::DrawingError(io::Error::other(
                "chart drawing area was retained after rendering",
            )))
        })?;
        result.map(|()| backend.into_inner().into_scene())
    }

    fn backend_color(color: BackendColor) -> Color {
        let alpha = (color.alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color::rgba(color.rgb.0, color.rgb.1, color.rgb.2, alpha)
    }

    fn style<S: BackendStyle>(style: &S) -> Stroke {
        let color = Self::backend_color(style.color());
        Stroke::new(color, style.stroke_width().max(1) as f64)
    }

    fn element_text<S: BackendTextStyle>(text: &str, style: &S, pos: BackendCoord) -> SceneElement {
        SceneElement::Text {
            text: text.to_owned(),
            pos: [pos.0 as f64, pos.1 as f64],
            font_size: style.size(),
            color: Self::backend_color(style.color()),
            align: match style.anchor().h_pos {
                HPos::Left => TextAlign::Left,
                HPos::Center => TextAlign::Center,
                HPos::Right => TextAlign::Right,
            },
            baseline: match style.anchor().v_pos {
                VPos::Top => TextBaseline::Top,
                VPos::Center => TextBaseline::Middle,
                VPos::Bottom => TextBaseline::Bottom,
            },
            angle_deg: match style.transform() {
                FontTransform::None => 0.0,
                FontTransform::Rotate90 => 90.0,
                FontTransform::Rotate180 => 180.0,
                FontTransform::Rotate270 => 270.0,
            },
            bold: matches!(style.style(), FontStyle::Bold),
        }
    }
}

impl DrawingBackend for SceneBackend {
    type ErrorType = io::Error;

    fn get_size(&self) -> (u32, u32) {
        (self.scene.width as u32, self.scene.height as u32)
    }

    fn ensure_prepared(&mut self) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        Ok(())
    }

    fn present(&mut self) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        Ok(())
    }

    fn draw_pixel(
        &mut self,
        point: BackendCoord,
        color: BackendColor,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.scene.add(SceneElement::Circle {
            center: [point.0 as f64, point.1 as f64],
            radius: 0.5,
            fill: Some(Fill::new(Self::backend_color(color))),
            stroke: None,
        });
        Ok(())
    }

    fn draw_line<S: BackendStyle>(
        &mut self,
        from: BackendCoord,
        to: BackendCoord,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.scene.add(SceneElement::Line {
            p1: [from.0 as f64, from.1 as f64],
            p2: [to.0 as f64, to.1 as f64],
            stroke: Self::style(style),
        });
        Ok(())
    }

    fn draw_path<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        path: I,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        let points = path
            .into_iter()
            .map(|(x, y)| [x as f64, y as f64])
            .collect::<Vec<_>>();
        if points.len() >= 2 {
            self.scene.add(SceneElement::Polyline {
                points,
                stroke: Self::style(style),
            });
        }
        Ok(())
    }

    fn draw_rect<S: BackendStyle>(
        &mut self,
        upper_left: BackendCoord,
        bottom_right: BackendCoord,
        style: &S,
        fill: bool,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        let stroke = Self::style(style);
        let x = upper_left.0.min(bottom_right.0) as f64;
        let y = upper_left.1.min(bottom_right.1) as f64;
        let width = upper_left.0.abs_diff(bottom_right.0) as f64;
        let height = upper_left.1.abs_diff(bottom_right.1) as f64;
        self.scene.add(SceneElement::Rect {
            x,
            y,
            width,
            height,
            rx: 0.0,
            fill: fill.then_some(Fill::new(stroke.color)),
            stroke: (!fill).then_some(stroke),
        });
        Ok(())
    }

    fn draw_circle<S: BackendStyle>(
        &mut self,
        center: BackendCoord,
        radius: u32,
        style: &S,
        fill: bool,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        let stroke = Self::style(style);
        self.scene.add(SceneElement::Circle {
            center: [center.0 as f64, center.1 as f64],
            radius: radius as f64,
            fill: fill.then_some(Fill::new(stroke.color)),
            stroke: (!fill).then_some(stroke),
        });
        Ok(())
    }

    fn fill_polygon<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        vertices: I,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.scene.add(SceneElement::Polygon {
            points: vertices
                .into_iter()
                .map(|(x, y)| [x as f64, y as f64])
                .collect(),
            fill: Some(Fill::new(Self::backend_color(style.color()))),
            stroke: None,
        });
        Ok(())
    }

    fn draw_text<TStyle: BackendTextStyle>(
        &mut self,
        text: &str,
        style: &TStyle,
        pos: BackendCoord,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.scene.add(Self::element_text(text, style, pos));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotters::prelude::*;

    #[test]
    fn chart_builder_records_numeric_axes_and_series_in_one_scene() {
        let scene = SceneBackend::draw_chart(320, 220, None, |root| {
            let mut chart = ChartBuilder::on(root)
                .margin(10)
                .x_label_area_size(30)
                .y_label_area_size(35)
                .build_cartesian_2d(0.0..2.0, -1.0..1.0)
                .map_err(|error| {
                    DrawingAreaErrorKind::BackendError(DrawingErrorKind::DrawingError(
                        io::Error::other(error.to_string()),
                    ))
                })?;
            chart
                .configure_mesh()
                .x_desc("Distance [m]")
                .y_desc("Load [kN]")
                .draw()
                .map_err(|error| {
                    DrawingAreaErrorKind::BackendError(DrawingErrorKind::DrawingError(
                        io::Error::other(error.to_string()),
                    ))
                })?;
            chart
                .draw_series(LineSeries::new([(0.0, 0.0), (1.0, 0.5), (2.0, 0.0)], &RED))
                .map_err(|error| {
                    DrawingAreaErrorKind::BackendError(DrawingErrorKind::DrawingError(
                        io::Error::other(error.to_string()),
                    ))
                })?;
            Ok(())
        })
        .unwrap_or_else(|error| panic!("chart backend should record a valid chart: {error}"));

        assert!(scene
            .elements
            .iter()
            .any(|element| matches!(element, SceneElement::Polyline { .. })));
        let labels = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(labels.contains(&"Distance [m]"));
        assert!(labels.contains(&"Load [kN]"));
        assert!(labels.iter().any(|text| text.parse::<f64>().is_ok()));
        let svg = crate::svg::render_svg(&scene);
        assert!(svg.contains("polyline"));
        assert!(svg.contains("Distance [m]"));
    }

    #[test]
    fn migration_ledger_keeps_unmigrated_figure_families_explicit() {
        assert_eq!(PLOTTERS_CHART_COVERAGE[0].id, "performance.vn_diagram");
        assert!(!PLOTTERS_CHART_COVERAGE[0].migrated);
        assert!(PLOTTERS_CHART_COVERAGE
            .iter()
            .any(|entry| entry.id == "mses"));
    }
}
