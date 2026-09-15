// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py
// Reference: alas @ rust-port-baseline.

//! Mass breakdown, longitudinal weight distribution, model CG check, and landing gear layout figures.
//!
//! `figure_cg_envelope` and `figure_mass_distribution` alone translate close
//! to 980 lines of `visualization.py`, past what one file under this crate's
//! 500-line limit can hold, so this became a directory module: the same
//! split `alas-geom::aircraft::airfoil` already uses. [`cg_envelope`] and
//! [`mass_distribution`] hold those two.
//!
//! The sibling layout module owns the mass-breakdown and landing-gear figures;
//! re-exporting them keeps the mass-balance API organized by discipline.

mod cg_envelope;
mod mass_distribution;

use std::collections::HashMap;

use alas_aero::analysis::PolarSweep;
use alas_config::{design_variables::DesignVector, AlasConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::breakdown::{
    run_mass_analysis_with_model_checked_product_with_gear, MassCoordinateModel,
};
use alas_pipeline::full_analysis::{AnalysisReport, DesignPoint, PolarFit, PolarFitStatus};

pub use super::mass_balance_layout::{figure_landing_gear_planform, figure_mass_breakdown};
pub use cg_envelope::figure_cg_envelope;
pub use mass_distribution::figure_mass_distribution;

/// Construct the geometry-and-mass report used by live CG, gear, and control
/// surface previews. The preview deliberately does not run aerodynamic VLM;
/// those figures only read the airplane, mass coordinates, and static-margin
/// fallback from the report shell.
pub fn quick_preview_report(
    airplane: Airplane,
    config: &AlasConfig,
    design: DesignVector,
) -> Result<AnalysisReport, String> {
    let geometry = config.geometry.clone();
    let analysis_mass_model = config.analysis_mass_model(config.requirements.mtow_kg);
    let (masses, coordinates, physical_cg) =
        run_mass_analysis_with_model_checked_product_with_gear(
            &airplane,
            &config.requirements,
            &geometry,
            &config.cabin,
            &config.control_surfaces,
            Some(&analysis_mass_model),
            None,
            MassCoordinateModel::StructuralWingbox(&config.structures),
            &config.landing_gear,
        )
        .map_err(|error| error.to_string())?;
    let component_masses = masses
        .as_pairs()
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect::<HashMap<_, _>>();
    let mass_coordinates = coordinates
        .as_pairs()
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect::<HashMap<_, _>>();
    Ok(AnalysisReport {
        design,
        airplane,
        polar: PolarSweep {
            alpha_deg: Vec::new(),
            geometric_alpha_deg: Vec::new(),
            cl: Vec::new(),
            cd: Vec::new(),
            cd_induced: Vec::new(),
            cd_wave: Vec::new(),
            cd_parasite: Vec::new(),
            cm: Vec::new(),
            l_over_d: Vec::new(),
        },
        design_point: DesignPoint {
            alpha_deg: 0.0,
            cl: 0.0,
            cd: 0.0,
            l_over_d: 0.0,
        },
        polar_fit: PolarFit {
            cd0: 0.0,
            k: 0.0,
            oswald_e: 0.0,
            aspect_ratio: 0.0,
            // The preview intentionally skips aerodynamic analysis, so an
            // empty polar cannot be reported as a successful fit.
            status: PolarFitStatus::FallbackInsufficientPoints,
        },
        static_margin: f64::NAN,
        x_neutral_point: f64::NAN,
        geometry_summary: HashMap::new(),
        component_masses,
        flops_mass_buildup: None,
        mass_coordinates,
        physical_cg,
        payload_layout: None,
        trimmed_design_point: None,
        cg_envelope_ok: None,
    })
}

use crate::scene::{Color, Scene, SceneElement, TextAlign, TextBaseline};
use crate::theme::Palette;

/// Fade a color's alpha channel to `alpha` (`0.0`-`1.0`): shared by the CG
/// envelope's dotted/translucent reference lines and the mass distribution's
/// translucent wing/fuselage/nacelle fills, matching matplotlib's per-artist
/// `alpha=` kwarg both Python figures set throughout.
// Private rather than `pub(super)`: descendant modules ([`cg_envelope`],
// [`mass_distribution`]) already see module-private items of an ancestor.
fn with_alpha(color: Color, alpha: f64) -> Color {
    Color::rgba(
        color.r,
        color.g,
        color.b,
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

/// Centered placeholder text for a figure with no mass/coordinate data yet:
/// the Python side's early-return `ax.text(0.5, 0.5, ..., ha="center", va="center")`.
fn no_data_scene(mut scene: Scene, pal: &Palette, message: &str) -> Scene {
    scene.add(SceneElement::Text {
        text: message.to_owned(),
        pos: [scene.width * 0.5, scene.height * 0.5],
        font_size: 12.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}
