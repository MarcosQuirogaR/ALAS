// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Figure scene generation, SVG export, palettes, route geometry, and design reports.
//!
//! [`theme`] ports `alas/reporting/theme.py`: color palettes (Light, Grey, Dark)
//! and standard plot trace styles.
//!
//! [`scene`] provides backend-neutral 2D and 3D vector scene structures,
//! coordinate transforms, axes, and wireframe projection.
//!
//! [`svg`] provides standalone XML SVG rendering for any [`scene::Scene`].
//!
//! [`document`] ports `alas/reporting/design_report.py`: JSON database exports,
//! Selig `.dat` airfoil file writes, and formatted design summaries.
//!
//! [`route_geometry`] ports `alas/reporting/route_globe.py`: geographic coordinate
//! transformations and mission mass/altitude profile projection onto waypoint routes.
//!
//! [`families`] contains figure generator factories grouped by engineering discipline.
//!
//! [`registry`] ports `alas/sidecar/figures.py`: figure metadata catalog and registries.
//!
//! [`colormap`] provides named colormaps for colorbar-driven figures.
//!
//! [`chart_kit`] provides colorbar and legend chrome built from `scene` primitives.

pub mod chart_kit;
pub mod colormap;
pub mod document;
pub mod families;
pub mod pdf;
pub mod plotters_backend;
pub mod registry;
mod registry_export;
pub mod route_geometry;
pub mod scene;
pub mod svg;
pub mod theme;

pub use chart_kit::{draw_axes, draw_colorbar, draw_legend, LegendMarker};
pub use colormap::Colormap;
pub use document::{export_airfoil_dat, export_json, format_summary, DesignDatabase};
pub use pdf::{render_sectioned_pdf, PdfError, PdfFigure, PdfSection};
pub use plotters_backend::{ChartCoverage, SceneBackend, PLOTTERS_CHART_COVERAGE};
pub use registry::{
    find_figure, FigureDescriptor, RequiredStage, PREVIEW_FIGURES, RESULT_FIGURES,
    SCREENING_FIGURES,
};
pub use registry_export::export_file_stem;
pub use route_geometry::{route_to_xyz, sync_mass_to_route, sync_mass_to_route_series};
pub use scene::{Axes2D, Camera3D, Color, Fill, Scale, Scene, SceneElement, Stroke};
pub use svg::render_svg;
pub use theme::{get_palette, Palette, PALETTE_DARK, PALETTE_GREY, PALETTE_LIGHT};
