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
use alas_payload::build::build_payload_layout;
use alas_payload::oew::oew_and_cg;
use alas_pipeline::full_analysis::{AnalysisReport, DesignPoint, PolarFit, PolarFitStatus};

pub use super::mass_balance_layout::{figure_landing_gear_planform, figure_mass_breakdown};
pub use cg_envelope::figure_cg_envelope;
pub use mass_distribution::figure_mass_distribution;

/// Construct the geometry-and-mass report used by live CG, gear, and control
/// surface previews. The preview deliberately does not run aerodynamic VLM;
/// those figures only read the airplane, mass coordinates, and static-margin
/// fallback from the report shell.
///
/// # The preview resolves the same cabin the analysis does
///
/// `alas_mass::analysis::complete_mass_analysis` prices the payload on
/// `requirements.num_passengers` when it is handed no layout, and on the
/// capacity the cabin engine resolves from the candidate's own geometry when it
/// is. `DesignRequirements` declares the second authoritative — "Passenger
/// capacity is always recomputed for each candidate shell", and
/// `resolves_payload_from_candidate_geometry` is unconditionally true — so
/// `num_passengers` is a seed, not a loading.
///
/// This preview used to pass `None` while every residual, baseline and export
/// path passes a layout, so the centre-of-gravity envelope, the landing-gear
/// planform and the control-surface figure were drawn for a **different
/// loading** than the feasibility verdict shown beside them: measured on the
/// registered presets, up to 3 000 kg of payload and 21.7 points of %MAC apart
/// (`alas-payload/examples/payload_placement_divergence.rs`). It now runs the
/// same two passes `alas_pipeline::baseline` does — a lumped pass for the
/// operating empty mass and its station, then the resolved cabin — so the
/// previewed aircraft is the evaluated aircraft.
///
/// # What this deliberately does not do
///
/// No mission, optimizer or aerodynamic stage is pulled in: the second pass is
/// the same `run_mass_analysis_with_model_checked_product_with_gear` call the
/// preview already made, plus one cabin layout.
///
/// The FLOPS operating items are repriced on the seated count through
/// `alas_pipeline::full_analysis::cabin_sync`, which is now shared rather than
/// mirrored — duplicating cabin logic across crates is what produced this
/// divergence in the first place, and there is still exactly one
/// implementation. (It could not move to `alas-mass` instead:
/// `cabin_synchronized` reads an `alas_payload::layout::PayloadLayout` and
/// `alas-payload` already depends on `alas-mass`.) With both halves of the
/// rule applied, this preview's operating empty mass matches the full
/// analysis's to the milligram on every probed preset, against +1 226 kg on
/// the A320-200 and +12 012 kg on the A380-800 before — see
/// `examples/preview_cabin_parity.rs`.
pub fn quick_preview_report(
    airplane: Airplane,
    config: &AlasConfig,
    design: DesignVector,
) -> Result<AnalysisReport, String> {
    let geometry = config.geometry.clone();
    // The same one-cabin-per-case rule the full analysis applies, taken from
    // its own module rather than mirrored here: a cabin declared by count is
    // the first pass's cabin, and once a layout exists the FLOPS operating
    // items are repriced on the seats it placed. Without this the preview's
    // operating empty mass prices `requirements.num_passengers` while the
    // payload beside it prices the layout - on the A320-200 a 30-seat
    // difference in the furnishings, passenger-service, cabin-crew and
    // air-conditioning terms.
    let (declared_requirements, analysis_mass_model) =
        alas_pipeline::full_analysis::cabin_sync::declared_cabin(
            &config.requirements,
            &config.analysis_mass_model(config.requirements.mtow_kg),
            &config.cabin.passenger,
        );
    let run = |requirements: &alas_config::DesignRequirements,
               mass_model: &alas_config::MassModelConfig,
               layout: Option<&alas_mass::breakdown::PayloadLayoutSummary>| {
        run_mass_analysis_with_model_checked_product_with_gear(
            &airplane,
            requirements,
            &geometry,
            &config.cabin,
            &config.control_surfaces,
            Some(mass_model),
            layout,
            MassCoordinateModel::StructuralWingbox(&config.structures),
            &config.landing_gear,
        )
        .map_err(|error| error.to_string())
    };

    // First pass, lumped: its only product is the operating empty mass and the
    // station the cabin engine balances the payload against.
    let (masses_init, coords_init, cg_init) =
        run(&declared_requirements, &analysis_mass_model, None)?;
    let (oew, x_oew) = oew_and_cg(&masses_init, &coords_init);

    // Second pass, on the resolved cabin. A cabin that will not resolve is a
    // real state for a candidate the user is still editing, and it must leave a
    // preview the interface can draw rather than an error: the lumped pass
    // stands, and `payload_layout` stays `None` so a reader can tell which of
    // the two this report is.
    let (payload_layout, masses, coordinates, physical_cg) =
        match build_payload_layout(&airplane, config, oew, x_oew) {
            Ok(layout) => {
                let summary = alas_mass::breakdown::PayloadLayoutSummary {
                    total_mass: layout.total_mass,
                    cg_x: layout.cg_x,
                    cg_y: layout.cg_y,
                };
                let (cabin_requirements, cabin_mass_model) =
                    alas_pipeline::full_analysis::cabin_sync::cabin_synchronized(
                        &declared_requirements,
                        &analysis_mass_model,
                        &layout,
                    );
                match run(&cabin_requirements, &cabin_mass_model, Some(&summary)) {
                    Ok((masses, coordinates, cg)) => (Some(layout), masses, coordinates, cg),
                    Err(_) => (None, masses_init, coords_init, cg_init),
                }
            }
            Err(_) => (None, masses_init, coords_init, cg_init),
        };

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
        payload_layout,
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
