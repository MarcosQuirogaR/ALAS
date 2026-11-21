// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Live-preview and per-page side-preview scenes.

use alas_config::{design_variables::DesignVector, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_geom::wing_structure::WingStructureGeometry;
use alas_payload::{apply_cabin_preset, build::build_payload_layout};
use alas_pipeline::full_analysis::AnalysisReport;
use alas_report::families::aerodynamics::figure_status_message;
use alas_report::families::geometry::{
    figure_exterior_3d, figure_geometry, figure_threeview, figure_threeview_sized,
};
use alas_report::families::mass_balance::{
    figure_cg_envelope_sized, figure_landing_gear_planform, figure_landing_gear_planform_sized,
};
use alas_report::families::stability::{figure_control_surfaces, figure_control_surfaces_sized};
use alas_report::families::structures::figure_structures_designer_preview_sized;
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
    canvas: (f64, f64),
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
    Some(figure_structures_designer_preview_sized(
        &geometry,
        &sizing,
        Some(theme),
        canvas,
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
        // The exterior model is on its way from a worker: keep the dock's
        // scene rather than building a page figure in its place.
        if crate::sandbox::scene::live_model_pending() {
            return None;
        }
    }
    let id = match state.preview_tab {
        PreviewTab::Cabin => "cabin_3d",
        PreviewTab::Exterior => state.selected_preview_id.as_str(),
    };
    // The cabin is fitted to the dock's own pane shape, once per pane size.
    let fit = (state.preview_tab == PreviewTab::Cabin)
        .then_some(state.preview_fit)
        .flatten();
    let canvas = fit.map(|fit| fit.canvas);
    let reserved = fit.map_or((0.0, 0.0), |fit| fit.reserved);
    alas_report::capture_plot_data(|| {
        build_page_preview_inner(
            state,
            id,
            Some(state.active_preview_camera().into()),
            canvas,
            reserved,
        )
    })
}

/// A preview figure built live from the current configuration, by id.
///
/// Only the figures that need geometry alone are buildable before a run; the
/// report-fed ones (CG envelope, control surfaces, ...) return `None` here and
/// the page shows a "run to preview" note instead.
pub fn build_page_preview(state: &AppState, id: &str) -> Option<Scene> {
    build_page_preview_with_camera(state, id, None)
}

/// A page preview drawn on a canvas of `canvas` points. The charts and the
/// three-view lay themselves out for it, so their text keeps its point size
/// at any card width; the other figures ignore it.
pub fn build_page_preview_sized(
    state: &AppState,
    id: &str,
    canvas: Option<(f64, f64)>,
) -> Option<Scene> {
    alas_report::capture_plot_data(|| build_page_preview_inner(state, id, None, canvas, (0.0, 0.0)))
}

/// [`build_page_preview_sized`] from owned inputs, for a worker thread.
pub(crate) fn build_page_preview_detached(
    inputs: PreviewInputs,
    id: &str,
    canvas: Option<(f64, f64)>,
) -> Option<Scene> {
    alas_report::capture_plot_data(|| build_preview_from(inputs, id, None, canvas))
}

pub(super) fn cap_interactive_preview_mesh(config: &mut AlasConfig) {
    config.geometry.wing.n_subdivisions = config.geometry.wing.n_subdivisions.clamp(1, 2);
    config.geometry.empennage.n_subdivisions = config.geometry.empennage.n_subdivisions.clamp(1, 2);
}

/// The retained camera of the preview `id`.
fn preview_camera(state: &AppState, id: &str) -> Camera3D {
    let camera = state.preview_cameras.get(id).copied().unwrap_or_default();
    Camera3D {
        elev_deg: camera.pitch_deg,
        azim_deg: camera.yaw_deg,
        zoom: camera.zoom,
    }
}

/// Build one preview with an explicit camera, without changing the dock's
/// retained camera. Fullscreen views use this so their orbit gestures remain
/// local to the overlay instead of moving the preview behind it.
pub fn build_page_preview_with_camera(
    state: &AppState,
    id: &str,
    camera_override: Option<Camera3D>,
) -> Option<Scene> {
    alas_report::capture_plot_data(|| {
        build_page_preview_inner(state, id, camera_override, None, (0.0, 0.0))
    })
}

/// The airframe and payload layout behind the cabin preview.
struct CabinBuild {
    config: AlasConfig,
    airplane: alas_geom::aircraft::airplane::Airplane,
    layout: Option<alas_payload::PayloadLayout>,
}

thread_local! {
    // One entry: the dock and its detached window show one aircraft.
    static CABIN_BUILD: std::cell::RefCell<Option<(u64, std::rc::Rc<CabinBuild>)>> =
        const { std::cell::RefCell::new(None) };
}

/// The cabin preview's build, reused while the inputs it reads are unchanged.
/// A camera drag redraws the scene every frame; the airframe and the cabin
/// layout do not depend on the camera, so they are built once per
/// configuration, not once per frame.
fn cabin_build(state: &AppState) -> Option<std::rc::Rc<CabinBuild>> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    state.config_fingerprint_now().hash(&mut hasher);
    crate::state::state_memo::design_values_fingerprint(&state.design_values).hash(&mut hasher);
    alas_geom::airfoil_io::generation().hash(&mut hasher);
    let key = hasher.finish();
    if let Some(hit) = CABIN_BUILD.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|(cached, _)| *cached == key)
            .map(|(_, build)| std::rc::Rc::clone(build))
    }) {
        return Some(hit);
    }
    let mut config = state.typed_config()?;
    let design = preview_design(state);
    if config.requirements.cabin_preset != "Custom" {
        apply_cabin_preset(&mut config, Some(&design)).ok()?;
    }
    cap_interactive_preview_mesh(&mut config);
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .ok()?;
    let layout = build_payload_layout(&airplane, &config, 0.0, 0.0).ok();
    let build = std::rc::Rc::new(CabinBuild {
        config,
        airplane,
        layout,
    });
    CABIN_BUILD.with(|slot| *slot.borrow_mut() = Some((key, std::rc::Rc::clone(&build))));
    Some(build)
}

fn build_page_preview_inner(
    state: &AppState,
    id: &str,
    camera_override: Option<Camera3D>,
    canvas: Option<(f64, f64)>,
    reserved: (f64, f64),
) -> Option<Scene> {
    if id == "cabin_3d" {
        let build = cabin_build(state)?;
        let theme = state.theme.figure_theme_name().to_owned();
        let camera = camera_override.or_else(|| Some(preview_camera(state, id)));
        let scene = match &build.layout {
            Some(layout) => alas_report::families::geometry::figure_cabin_payload_3d_framed(
                layout,
                &build.airplane,
                &build.config,
                camera,
                Some(&theme),
                canvas.unwrap_or((800.0, 520.0)),
                reserved,
            ),
            None => figure_geometry(&build.airplane, Some(&theme)),
        };
        return Some(localize_scene_for_display(scene));
    }
    build_preview_from(preview_inputs(state, id)?, id, camera_override, canvas)
}

/// Everything a page preview (other than the cabin) reads, owned, so the
/// build can run on a worker thread ([`crate::preview_jobs`]).
pub(crate) struct PreviewInputs {
    config: AlasConfig,
    design: DesignVector,
    theme: String,
    camera: Camera3D,
    /// The completed run's report when the form still matches it (mass,
    /// landing-gear and control-surface previews).
    completed_report: Option<AnalysisReport>,
    /// The completed run's load and trim sheet (mass and CG preview).
    load_trim: Option<alas_report::families::mass_balance::load_trim::LoadTrimSheetData>,
}

/// The inputs of preview `id` as the form holds them now.
pub(crate) fn preview_inputs(state: &AppState, id: &str) -> Option<PreviewInputs> {
    let mass_bearing = matches!(id, "mass_cg" | "landing_gear" | "control_surfaces");
    let completed = completed_run_report_for_form(state);
    Some(PreviewInputs {
        config: state.typed_config()?,
        design: preview_design(state),
        theme: state.theme.figure_theme_name().to_owned(),
        camera: preview_camera(state, id),
        completed_report: completed.filter(|_| mass_bearing).cloned(),
        load_trim: completed
            .filter(|_| id == "mass_cg")
            .and(state.pipeline_result.as_ref())
            .and_then(
                alas_report::families::mass_balance::load_trim::data::load_trim_data_from_pipeline,
            ),
    })
}

/// Build preview `id` from owned inputs; the cabin preview is not built here.
pub(crate) fn build_preview_from(
    inputs: PreviewInputs,
    id: &str,
    camera_override: Option<Camera3D>,
    canvas: Option<(f64, f64)>,
) -> Option<Scene> {
    let PreviewInputs {
        mut config,
        design,
        theme,
        camera,
        completed_report,
        load_trim,
    } = inputs;
    // A named cabin preset is normally materialized by the pipeline. The live
    // preview has no pipeline pass, so apply it to this throwaway copy first
    // and keep Custom untouched for direct class-by-class editing.
    if config.requirements.cabin_preset != "Custom" {
        apply_cabin_preset(&mut config, Some(&design)).ok()?;
    }
    // Camera drags rebuild this scene repeatedly. The report and solvers keep
    // the configured discretization; the interactive preview only needs the
    // defining planform panels, so cap its throwaway mesh before building.
    // The Load & trim sheet is a result, not a throwaway view: its masses and
    // neutral point come from the configured discretization, so the sheet reads
    // what a run of the same inputs reads (the coarse mesh moved the ATR 72-600
    // aft limit from 34 to 26 %MAC and the AVE forward limit from 13 to 21).
    if id != "mass_cg" {
        cap_interactive_preview_mesh(&mut config);
    }
    let airplane =
        match AircraftBuilder::new(Some(config.geometry.clone())).build(Some(&design), true) {
            Ok(airplane) => airplane,
            // A failed build is a design-vector fault the user can act on: show
            // it as a status instead of an empty preview.
            Err(error) => {
                return Some(figure_status_message(
                    "Aircraft preview unavailable",
                    &format!("The aircraft builder rejected this design: {error}"),
                    false,
                    Some(&theme),
                ));
            }
        };
    let camera = camera_override.or(Some(camera));

    let scene = match id {
        // The `geometry` page preview is the four-panel three-view; the
        // `threeview` id is the same figure under its registry name.
        "geometry" | "threeview" => match canvas {
            Some(canvas) => figure_threeview_sized(&airplane, Some(&theme), canvas),
            None => figure_threeview(&airplane, Some(&theme)),
        },
        "exterior_3d" => figure_exterior_3d(&airplane, camera, Some(&theme)),
        "engine" => match canvas {
            Some(canvas) => {
                alas_report::families::propulsion::figure_engine_designer_preview_sized(
                    &config,
                    Some(&theme),
                    canvas,
                )
            }
            None => alas_report::families::propulsion::figure_engine_designer_preview(
                &config,
                Some(&theme),
            ),
        },
        "structures" => build_structures_preview(
            &airplane,
            &config,
            &design,
            &theme,
            canvas.unwrap_or(alas_report::families::structures::STRUCTURES_PREVIEW_CANVAS),
        )?,
        // These previews need a completed report. Returning None keeps the
        // form honest instead of silently showing an unrelated exterior view.
        "drag" => alas_report::families::aerodynamics::figure_drag_preview_sized(
            &airplane,
            &config,
            &design,
            Some(&theme),
            canvas.unwrap_or(alas_report::families::aerodynamics::DRAG_PREVIEW_CANVAS),
        ),
        "mass_cg" | "landing_gear" | "control_surfaces" => {
            let report = match preview_mass_report(completed_report, airplane, &config, design) {
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
                    use alas_report::families::mass_balance::load_trim::figure_load_trim_sheet;
                    load_trim.map_or_else(
                        || {
                            // Before a run: the sheet of the current inputs, with
                            // the VLM neutral point the CG gate's aft limit needs.
                            let mut draft = report.clone();
                            match alas_report::families::mass_balance::attach_preview_neutral_point(
                                &mut draft, &config,
                            ) {
                                Ok(()) => figure_cg_envelope_sized(
                                    &draft,
                                    &config,
                                    Some(&theme),
                                    canvas.map(|size| size.0),
                                ),
                                Err(error) => figure_status_message(
                                    &preview_unavailable_title(id),
                                    &format!("Neutral point unavailable: {error}"),
                                    false,
                                    Some(&theme),
                                ),
                            }
                        },
                        |data| {
                            let palette = alas_report::theme::get_palette(Some(&theme));
                            match canvas {
                                Some((width, _)) => {
                                    alas_report::families::mass_balance::load_trim::figure_load_trim_sheet_sized(
                                        &data, palette, width,
                                    )
                                }
                                None => figure_load_trim_sheet(&data, palette),
                            }
                        },
                    )
                }
                "landing_gear" => match canvas {
                    Some(bounds) => {
                        figure_landing_gear_planform_sized(&report, &config, Some(&theme), bounds)
                    }
                    None => figure_landing_gear_planform(&report, &config, Some(&theme)),
                },
                _ => match canvas {
                    Some(canvas) => {
                        figure_control_surfaces_sized(&report, &config, Some(&theme), canvas)
                    }
                    None => figure_control_surfaces(&report, &config, Some(&theme)),
                },
            }
        }
        _ => return None,
    };
    let mut scene = localize_scene_for_display(scene);
    if id == "landing_gear" {
        // The card heading already names the figure.
        scene.title = None;
        scene.suppress_derived_title();
    }
    if matches!(id, "geometry" | "threeview") {
        sentence_case_figure_title(&mut scene, "Three-View Drawing", "Three-view drawing");
    }
    Some(scene)
}

/// The report's figure titles are Title Case; the GUI shows sentence case.
/// Only an untranslated (English) title is rewritten.
fn sentence_case_figure_title(scene: &mut alas_report::scene::Scene, from: &str, to: &str) {
    if scene.title.as_deref() == Some(from) {
        scene.title = Some(to.to_owned());
    }
    for element in &mut scene.elements {
        if let alas_report::scene::SceneElement::Text { text, .. } = element {
            if text == from {
                *text = to.to_owned();
            }
        }
    }
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
    completed: Option<AnalysisReport>,
    airplane: alas_geom::aircraft::airplane::Airplane,
    config: &AlasConfig,
    design: DesignVector,
) -> Result<AnalysisReport, String> {
    match completed {
        Some(report) => Ok(report),
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
