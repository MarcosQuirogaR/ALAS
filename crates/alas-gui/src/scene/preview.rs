// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Live-preview and per-page side-preview scenes.

use alas_config::{design_variables::DesignVector, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_geom::wing_structure::WingStructureGeometry;
use alas_payload::{apply_cabin_preset, build::build_payload_layout};
use alas_pipeline::full_analysis::AnalysisReport;
use alas_report::families::aerodynamics::figure_status_message;
use alas_report::families::geometry::{figure_exterior_3d, figure_geometry, figure_threeview};
use alas_report::families::mass_balance::{figure_cg_envelope, figure_landing_gear_planform};
use alas_report::families::stability::figure_control_surfaces;
use alas_report::families::structures::figure_structures_designer_preview;
use alas_report::scene::{Camera3D, Scene};
use alas_struct::sizing::size_wingbox;

use super::localize_scene_for_display;
use crate::state::{AppState, PreviewTab};

/// The design vector the previews are built at.
fn preview_design(state: &AppState) -> DesignVector {
    state.current_design().unwrap_or_default()
}

/// Build the structural live preview using the same already-portable sizing
/// factory as the report figure. This is dispatch glue; the renderer remains
/// owned by `alas-report::families::structures`.
fn build_structures_preview(
    airplane: &alas_geom::aircraft::airplane::Airplane,
    config: &AlasConfig,
    design: &DesignVector,
    theme: &str,
) -> Option<Scene> {
    let wing = airplane.wings.first()?;
    let root_airfoil = wing.xsecs.first()?.airfoil.clone();
    let tip_airfoil = wing.xsecs.last()?.airfoil.clone();
    let (spar_fracs, spar_full_span) = config.structures.resolved_spars();
    let geometry = WingStructureGeometry::new(
        design,
        &config.geometry.wing,
        &root_airfoil,
        &tip_airfoil,
        &spar_fracs,
        Some(&spar_full_span),
    )
    .ok()?;
    let sizing = size_wingbox(
        &geometry,
        &config.structures,
        &config.requirements,
        alas_config::materials::get(&config.structures.skin_material).ok()?,
        alas_config::materials::get(&config.structures.spar_web_material).ok()?,
        alas_config::materials::get(&config.structures.spar_cap_material).ok()?,
        alas_config::materials::get(&config.structures.rib_material).ok()?,
    );
    Some(figure_structures_designer_preview(
        &geometry,
        &sizing,
        Some(theme),
    ))
}

/// The live-preview scene for the unified aircraft viewer's current mode.
pub fn build_preview_scene(state: &AppState) -> Option<Scene> {
    // Exterior live preview and Sandbox must share one renderer and one
    // section sampling policy. This keeps arbitrary sandbox stations, wing
    // twists and fuselage lofts visible at the same fidelity in both places.
    if state.preview_tab == PreviewTab::Exterior {
        if let Some(scene) = crate::sandbox::scene::build_live_preview_scene(
            state,
            state.active_preview_camera().into(),
        ) {
            return Some(scene);
        }
    }
    let id = match state.preview_tab {
        PreviewTab::Cabin => "cabin_3d",
        PreviewTab::Exterior => state.selected_preview_id.as_str(),
    };
    build_page_preview_with_camera(state, id, Some(state.active_preview_camera().into()))
}

/// A preview figure built live from the current configuration, by id.
///
/// Only the figures that need geometry alone are buildable before a run; the
/// report-fed ones (CG envelope, control surfaces, ...) return `None` here and
/// the page shows a "run to preview" note instead.
pub fn build_page_preview(state: &AppState, id: &str) -> Option<Scene> {
    build_page_preview_with_camera(state, id, None)
}

pub(super) fn cap_interactive_preview_mesh(config: &mut AlasConfig) {
    config.geometry.wing.n_subdivisions = config.geometry.wing.n_subdivisions.clamp(1, 2);
    config.geometry.empennage.n_subdivisions = config.geometry.empennage.n_subdivisions.clamp(1, 2);
}

/// Build one preview with an explicit camera, without changing the dock's
/// retained camera. Fullscreen views use this so their orbit gestures remain
/// local to the overlay instead of moving the preview behind it.
pub fn build_page_preview_with_camera(
    state: &AppState,
    id: &str,
    camera_override: Option<Camera3D>,
) -> Option<Scene> {
    let mut config = state.typed_config()?;
    let design = preview_design(state);
    // A named cabin preset is normally materialized by the pipeline. The live
    // preview has no pipeline pass, so apply it to this throwaway copy first
    // and keep Custom untouched for direct class-by-class editing.
    if config.requirements.cabin_preset != "Custom" {
        apply_cabin_preset(&mut config, Some(&design)).ok()?;
    }
    // Camera drags rebuild this scene repeatedly. The report and solvers keep
    // the configured discretization; the interactive preview only needs the
    // defining planform panels, so cap its throwaway mesh before building.
    cap_interactive_preview_mesh(&mut config);
    let theme = state.theme.figure_theme_name().to_owned();
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .ok()?;
    let camera = camera_override.or_else(|| {
        let preview_camera = state.preview_cameras.get(id).copied().unwrap_or_default();
        Some(Camera3D {
            elev_deg: preview_camera.pitch_deg,
            azim_deg: preview_camera.yaw_deg,
            zoom: preview_camera.zoom,
        })
    });

    let scene = match id {
        "cabin_3d" => match build_payload_layout(&airplane, &config, 0.0, 0.0) {
            Ok(layout) => alas_report::families::geometry::figure_cabin_payload_3d(
                &layout,
                &airplane,
                &config,
                camera,
                Some(&theme),
            ),
            Err(_) => figure_geometry(&airplane, Some(&theme)),
        },
        // The `geometry` page preview is the four-panel three-view; the
        // `threeview` id is the same figure under its registry name.
        "geometry" | "threeview" => figure_threeview(&airplane, Some(&theme)),
        "exterior_3d" => figure_exterior_3d(&airplane, camera, Some(&theme)),
        "engine" => {
            alas_report::families::propulsion::figure_engine_designer_preview(&config, Some(&theme))
        }
        "structures" => build_structures_preview(&airplane, &config, &design, &theme)?,
        // These previews need a completed report. Returning None keeps the
        // form honest instead of silently showing an unrelated exterior view.
        "drag" => alas_report::families::aerodynamics::figure_drag_preview(
            &airplane,
            &config,
            &design,
            Some(&theme),
        ),
        "mass_cg" | "landing_gear" | "control_surfaces" => {
            let report = match preview_mass_report(state, airplane, &config, design) {
                Ok(report) => report,
                Err(error) => {
                    return Some(localize_scene_for_display(figure_status_message(
                        &preview_unavailable_title(id),
                        &format!("Invalid structural mass-coordinate model: {error}"),
                        false,
                        Some(&theme),
                    )));
                }
            };
            match id {
                "mass_cg" => {
                    use alas_report::families::mass_balance::load_trim::{
                        data::load_trim_data_from_pipeline, figure_load_trim_sheet,
                    };
                    let data = completed_run_report_for_form(state)
                        .and(state.pipeline_result.as_ref())
                        .and_then(load_trim_data_from_pipeline);
                    data.map_or_else(
                        || figure_cg_envelope(&report, &config, Some(&theme)),
                        |data| {
                            figure_load_trim_sheet(
                                &data,
                                alas_report::theme::get_palette(Some(&theme)),
                            )
                        },
                    )
                }
                "landing_gear" => figure_landing_gear_planform(&report, &config, Some(&theme)),
                _ => figure_control_surfaces(&report, &config, Some(&theme)),
            }
        }
        _ => return None,
    };
    Some(localize_scene_for_display(scene))
}

/// The optimized report of a completed run whose configuration is exactly the
/// one now on the form: the run's report is bound to the pipeline's sized
/// takeoff mass, so a preview of an unchanged configuration reads it rather
/// than re-pricing the mass analysis at the declared MTOW.
pub(crate) fn completed_run_report_for_form(state: &AppState) -> Option<&AnalysisReport> {
    if !state.pipeline_result_complete {
        return None;
    }
    let result = state.pipeline_result.as_ref()?;
    let report = result.optimized_report.as_ref()?;
    // Design edits do not change the typed configuration, so the design
    // values the run started from are compared as well.
    let design_unchanged =
        state.pipeline_result_design_values.as_ref() == Some(&state.design_values);
    // The run may adjust its own copy of the configuration (the preset
    // dispatch anchors the optimizer bounds), so the form is compared with
    // what it held when the run started, and with the run's configuration
    // for results installed without a recorded form.
    let form = state.typed_config();
    let config_unchanged = form.is_some()
        && (form.as_ref() == Some(&result.config) || form == state.pipeline_result_form_config);
    (design_unchanged && config_unchanged).then_some(report)
}

/// The mass-bearing report behind the CG, landing-gear and control-surface
/// previews: the completed run's optimized report when the form still matches
/// it, else the pre-run draft (`quick_preview_report`, priced at the declared
/// MTOW because no sized takeoff mass exists yet).
fn preview_mass_report(
    state: &AppState,
    airplane: alas_geom::aircraft::airplane::Airplane,
    config: &AlasConfig,
    design: DesignVector,
) -> Result<AnalysisReport, String> {
    match completed_run_report_for_form(state) {
        Some(report) => Ok(report.clone()),
        None => alas_report::families::mass_balance::quick_preview_report(airplane, config, design),
    }
}

/// The title a status figure carries when it stands in for preview `id`.
///
/// One `Err` arm serves the CG envelope, the landing-gear planform and the
/// control-surface layout, and a hard-coded
/// "Mass and balance preview unavailable" would mis-title two of them. The
/// panel title is already declared once, in [`crate::nav`], so the status figure takes it
/// from there instead of restating it.
pub(crate) fn preview_unavailable_title(id: &str) -> String {
    let figure = crate::nav::all_pages()
        .find(|page| page.preview == Some(id))
        .and_then(|page| page.preview_title)
        .unwrap_or("Preview");
    crate::views::tr_fields(
        "{figure} unavailable",
        &[("figure", crate::views::tr(figure))],
    )
}
