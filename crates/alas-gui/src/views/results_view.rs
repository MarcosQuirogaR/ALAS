// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Results page: summary stat tiles, then a tab per discipline, each a
//! grid of figures. A figure with no builder or no data for this run shows
//! "Not available for this run", the same graceful degradation the reference
//! desktop app's `ResultsScreen` shows.

use crate::state::AppState;
use crate::theme::selectable_button;
use crate::views::result_3d;
use crate::views::{tr, tr_fields};
use alas_report::scene::Scene;
use egui::{vec2, Align2, Area, Color32, Frame, Id, Key, Layout, Order, RichText, ScrollArea, Ui};

mod summary;
use summary::show_summary;
mod model_details;
mod solver;
pub use solver::SolverResultView;

/// One discipline tab in the Python desktop ResultsScreen.
struct Tab {
    id: &'static str,
    title: &'static str,
    category: &'static str,
}

const TABS: &[Tab] = &[
    Tab {
        id: "optimization",
        title: "Optimization",
        category: "Optimization",
    },
    Tab {
        id: "aero",
        title: "Aerodynamics",
        category: "Aerodynamics",
    },
    Tab {
        id: "wb",
        title: "Weight & Balance",
        category: "Weight & Balance",
    },
    Tab {
        id: "propulsion",
        title: "Propulsion",
        category: "Propulsion",
    },
    Tab {
        id: "structures",
        title: "Structures",
        category: "Structures",
    },
    Tab {
        id: "mission",
        title: "Mission & Route",
        category: "Mission",
    },
    Tab {
        id: "field",
        title: "Field Performance",
        category: "Field Performance",
    },
    Tab {
        id: "model",
        title: "Model Comparison",
        category: "Model Comparison",
    },
];

/// Minimum width of a result card before another responsive column is added.
pub(crate) const CARD_MIN_WIDTH: f32 = 320.0;
/// Horizontal space between adjacent result cards.
pub(crate) const CARD_GAP: f32 = 12.0;
/// Equal side margins around the result gallery content.
const RESULTS_SIDE_MARGIN: f32 = 20.0;
/// Result tabs are intentionally capped at five per row so their labels stay
/// legible and the Summary tab cannot push the last discipline off-screen.
const RESULT_TAB_MIN_WIDTH: f32 = 176.0;
const RESULT_TAB_MAX_COLUMNS: usize = 5;

/// Return a stable column count and card width for the current content pane.
///
/// The width is calculated from the whole available row, rather than from a
/// fixed card width, so a two-column row cannot leave a permanent right gutter.
pub(crate) fn responsive_card_layout(available_width: f32) -> (usize, f32) {
    let width = available_width.max(1.0);
    let columns = ((width + CARD_GAP) / (CARD_MIN_WIDTH + CARD_GAP))
        .floor()
        .clamp(1.0, 3.0) as usize;
    let card_width = (width - CARD_GAP * (columns - 1) as f32) / columns as f32;
    (columns, card_width.max(1.0))
}

fn result_tab_column_count(available_width: f32) -> usize {
    ((available_width / RESULT_TAB_MIN_WIDTH).floor() as usize).clamp(1, RESULT_TAB_MAX_COLUMNS)
}

fn show_result_tabs(state: &mut AppState, ui: &mut Ui) {
    let mut tabs = Vec::with_capacity(TABS.len() + 1);
    tabs.push(("summary", "Summary"));
    tabs.extend(TABS.iter().map(|tab| (tab.id, tab.title)));
    let columns = result_tab_column_count(ui.available_width()).min(tabs.len());
    for row in tabs.chunks(columns) {
        ui.columns(columns, |column_uis| {
            for (index, (id, title)) in row.iter().enumerate() {
                let selected = state.results_tab == *id;
                if column_uis[index]
                    .add_sized(
                        [column_uis[index].available_width(), 30.0],
                        selectable_button(tr(title), selected),
                    )
                    .clicked()
                {
                    state.results_tab = (*id).to_owned();
                }
            }
        });
        ui.add_space(4.0);
    }
}

/// Render the Results page.
pub fn show_results_view(state: &mut AppState, ui: &mut Ui) {
    #[cfg(debug_assertions)]
    crate::layout_debug::record_ui(
        ui.ctx(),
        "results page",
        ui,
        crate::layout_debug::RegionKind::Content,
    );
    if state.pipeline_result.is_none() {
        ui.centered_and_justified(|ui| {
            ui.label(if state.is_running {
                tr("Running the pipeline...")
            } else {
                tr("Press Run to optimize and analyze, or Analyze baseline for a quick weight and balance pass.")
            });
        });
        return;
    }

    show_result_tabs(state, ui);
    ui.separator();

    let Some(result) = state.pipeline_result.as_ref() else {
        return;
    };

    if state.results_tab == "summary" {
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                #[cfg(debug_assertions)]
                crate::layout_debug::record_ui(
                    ui.ctx(),
                    "summary scroll",
                    ui,
                    crate::layout_debug::RegionKind::Scroll,
                );
                show_summary(state, ui, result);
            });
        return;
    }
    let Some(tab) = TABS.iter().find(|t| t.id == state.results_tab) else {
        return;
    };
    let result_config = result.config.clone();
    let category = tab.category;
    let model_details_item = state.results_tab == "model";
    let model_details_count = usize::from(model_details_item);
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            #[cfg(debug_assertions)]
            crate::layout_debug::record_ui(
                ui.ctx(),
                "results figure scroll",
                ui,
                crate::layout_debug::RegionKind::Scroll,
            );
            let descriptors = alas_report::RESULT_FIGURES
                .iter()
                .filter(|figure| figure.category == category)
                .collect::<Vec<_>>();
            let available_width =
                (ui.available_width() - 2.0 * RESULTS_SIDE_MARGIN).max(CARD_MIN_WIDTH);
            let (columns, tile_width) = responsive_card_layout(available_width);
            let item_count = descriptors.len() + model_details_count;
            for row_start in (0..item_count).step_by(columns) {
                let row_end = (row_start + columns).min(item_count);
                ui.horizontal(|ui| {
                    // Horizontal layout advances through explicit spaces. A
                    // zero-height allocation in the surrounding vertical
                    // layout only changes its min-rect; it does not create a
                    // visible left inset.
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(RESULTS_SIDE_MARGIN);
                    for item_index in row_start..row_end {
                        if model_details_item && item_index == 0 {
                            model_details::show_model_details(
                                ui,
                                state.run_identity,
                                state
                                    .pipeline_result
                                    .as_ref()
                                    .expect("pipeline result remains available while rendering"),
                                tile_width,
                            );
                        } else {
                            let descriptor = &descriptors[item_index - model_details_count];
                            figure_tile(
                                state,
                                ui,
                                &result_config,
                                descriptor.id,
                                descriptor.title,
                                descriptor.description,
                                tile_width,
                            );
                        }
                        if item_index + 1 < row_end {
                            ui.add_space(CARD_GAP);
                        }
                    }
                    ui.add_space(RESULTS_SIDE_MARGIN);
                });
                ui.add_space(16.0);
            }
        });
}

fn figure_tile(
    state: &mut AppState,
    ui: &mut Ui,
    config: &alas_config::AlasConfig,
    id: &str,
    title: &str,
    description: &str,
    tile_width: f32,
) {
    let theme = state.theme.figure_theme_name().to_owned();
    let language = alas_i18n::get_language();
    let view_key = format!(
        "run={};solver={:?};theme={theme};language={language};figure={id}",
        state.run_identity, state.selected_solver_view
    );
    let orbitable = result_3d::is_orbitable_result(id);
    let camera_key = result_3d::result_camera_key(state.run_identity, id);
    let scene = if orbitable {
        let camera = result_3d::result_camera(state, &camera_key);
        state.cached_result_figure_with_camera(&view_key, id, config, &theme, Some(camera))
    } else {
        state.cached_result_figure(&view_key, id, config, &theme)
    };
    let card = crate::theme::card_frame(ui).show(ui, |ui| {
        let content_width = crate::theme::card_content_width(tile_width);
        ui.set_min_width(content_width);
        ui.set_max_width(content_width);
        ui.vertical(|ui| {
            ui.label(RichText::new(tr(title)).strong())
                .on_hover_text(tr(description));
            if state.help_verbose {
                ui.label(RichText::new(tr(description)).weak().small());
            }
            match scene.as_ref() {
                Some(scene) => {
                    let canvas_width = ui.available_width().max(1.0);
                    if scene_has_external_images(scene) {
                        if show_external_images(state, ui, scene, canvas_width, 320.0, false) {
                            open_fullscreen_result(
                                state,
                                ui.ctx(),
                                &view_key,
                                &camera_key,
                                orbitable,
                            );
                        }
                    } else if orbitable {
                        let interaction = result_3d::show_orbit_view(
                            state,
                            ui,
                            scene,
                            &camera_key,
                            &view_key,
                            vec2(canvas_width, 320.0),
                        );
                        if interaction.double_clicked {
                            open_fullscreen_result(
                                state,
                                ui.ctx(),
                                &view_key,
                                &camera_key,
                                orbitable,
                            );
                        }
                        if interaction.camera_changed {
                            result_3d::rebuild_scene(state, &view_key, &camera_key, config, &theme);
                            ui.ctx().request_repaint();
                        }
                    } else {
                        let response = ui.add(
                            alas_viz::SceneView::new(scene, state.view_state_mut(view_key.clone()))
                                .static_view()
                                .show_toolbar(false)
                                .desired_size(vec2(canvas_width, 320.0)),
                        );
                        if response.double_clicked() {
                            open_fullscreen_result(
                                state,
                                ui.ctx(),
                                &view_key,
                                &camera_key,
                                orbitable,
                            );
                        }
                    }
                }
                None => {
                    let canvas_width = ui.available_width().max(1.0);
                    ui.add_sized(
                        [canvas_width, 132.0],
                        egui::Label::new(RichText::new(unavailable_reason(state, id)).weak()),
                    );
                }
            }
        });
    });
    #[cfg(debug_assertions)]
    crate::layout_debug::record(
        ui.ctx(),
        format!("figure card: {id}"),
        card.response.rect,
        crate::layout_debug::RegionKind::Figure,
    );
    #[cfg(not(debug_assertions))]
    let _ = card;
    if fullscreen_open(ui.ctx(), &view_key) {
        if let Some(scene) = scene.as_ref() {
            show_fullscreen_result(
                state,
                ui.ctx(),
                FullscreenFigure {
                    scene,
                    config,
                    theme: &theme,
                    camera_key: &camera_key,
                    view_key: &view_key,
                    title,
                    description,
                    orbitable,
                },
            );
        }
    }
}

fn fullscreen_id(view_key: &str) -> Id {
    Id::new(("alas_result_fullscreen", view_key))
}

fn fullscreen_open(ctx: &egui::Context, view_key: &str) -> bool {
    ctx.data(|data| data.get_temp::<bool>(fullscreen_id(view_key)))
        .unwrap_or(false)
}

fn set_fullscreen(ctx: &egui::Context, view_key: &str, open: bool) {
    ctx.data_mut(|data| data.insert_temp(fullscreen_id(view_key), open));
}

fn fullscreen_view_key(view_key: &str) -> String {
    format!("fullscreen_view::{view_key}")
}

fn fullscreen_camera_key(camera_key: &str) -> String {
    format!("fullscreen_camera::{camera_key}")
}

fn fullscreen_cache_key(view_key: &str) -> String {
    format!("fullscreen_scene::{view_key}")
}

/// Open an isolated copy of a gallery figure.  The card remains its own view
/// while the overlay receives independent pan, zoom, and orbit state.
fn open_fullscreen_result(
    state: &mut AppState,
    ctx: &egui::Context,
    view_key: &str,
    camera_key: &str,
    orbitable: bool,
) {
    let fullscreen_view = fullscreen_view_key(view_key);
    let view_state = state.view_states.get(view_key).cloned().unwrap_or_default();
    state.view_states.insert(fullscreen_view, view_state);
    if orbitable {
        let camera = *state.result_camera_mut(camera_key);
        state
            .result_cameras
            .insert(fullscreen_camera_key(camera_key), camera);
    }
    set_fullscreen(ctx, view_key, true);
}

struct FullscreenFigure<'a> {
    scene: &'a Scene,
    config: &'a alas_config::AlasConfig,
    theme: &'a str,
    camera_key: &'a str,
    view_key: &'a str,
    title: &'a str,
    description: &'a str,
    orbitable: bool,
}

fn show_fullscreen_result(state: &mut AppState, ctx: &egui::Context, figure: FullscreenFigure<'_>) {
    let FullscreenFigure {
        scene,
        config,
        theme,
        camera_key,
        view_key,
        title,
        description,
        orbitable,
    } = figure;
    if ctx.input(|input| input.key_pressed(Key::Escape)) {
        close_fullscreen_result(state, ctx, view_key, camera_key);
        return;
    }

    let fullscreen_view = fullscreen_view_key(view_key);
    let fullscreen_camera = fullscreen_camera_key(camera_key);
    let fullscreen_cache = fullscreen_cache_key(view_key);
    let fullscreen_scene = if orbitable {
        let camera = (*state.result_camera_mut(&fullscreen_camera)).into();
        state.cached_result_figure_with_camera(
            &fullscreen_cache,
            result_3d::MISSION_ROUTE_3D,
            config,
            theme,
            Some(camera),
        )
    } else {
        None
    };
    let active_scene = fullscreen_scene.as_deref().unwrap_or(scene);
    let screen = ctx.screen_rect();
    let screen_size = screen.size();
    Area::new(Id::new(("alas_result_fullscreen_area", view_key)))
        .order(Order::Foreground)
        .default_size(screen.size())
        .constrain_to(screen)
        .pivot(Align2::LEFT_TOP)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            // Area defaults to the size its content requests. A figure was
            // therefore able to create a short overlay on a tall monitor.
            // Establish the full screen before the frame measures itself.
            ui.set_min_size(screen_size);
            Frame::default()
                .fill(Color32::from_black_alpha(220))
                .inner_margin(egui::Margin::same(18.0))
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    ui.horizontal(|ui| {
                        ui.heading(tr(title)).on_hover_text(tr(description));
                        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                            if crate::theme::close_icon_button(ui, tr("Close")).clicked() {
                                close_fullscreen_result(state, ctx, view_key, camera_key);
                            }
                        });
                    });
                    ui.add_space(6.0);
                    // Keep the scene above the footer and inside the inset
                    // frame, including on the smallest supported window.
                    let available = ui.available_size();
                    let available = vec2(available.x.max(320.0), (available.y - 28.0).max(180.0));
                    if scene_has_external_images(active_scene) {
                        let _ = show_external_images(
                            state,
                            ui,
                            active_scene,
                            available.x,
                            available.y,
                            true,
                        );
                    } else if orbitable {
                        let interaction = result_3d::show_orbit_view(
                            state,
                            ui,
                            active_scene,
                            &fullscreen_camera,
                            &fullscreen_view,
                            vec2(available.x, available.y.max(180.0)),
                        );
                        if interaction.camera_changed {
                            result_3d::rebuild_scene(
                                state,
                                &fullscreen_cache,
                                &fullscreen_camera,
                                config,
                                theme,
                            );
                            ctx.request_repaint();
                        }
                    } else {
                        ui.add(
                            alas_viz::SceneView::new(
                                active_scene,
                                state.view_state_mut(fullscreen_view.clone()),
                            )
                            .wheel_zoom(true)
                            .show_toolbar(false)
                            .desired_size(available),
                        );
                    }
                });
        });
}

fn close_fullscreen_result(
    state: &mut AppState,
    ctx: &egui::Context,
    view_key: &str,
    camera_key: &str,
) {
    state.view_states.remove(&fullscreen_view_key(view_key));
    state
        .result_cameras
        .remove(&fullscreen_camera_key(camera_key));
    state
        .result_figure_cache
        .remove(&fullscreen_cache_key(view_key));
    set_fullscreen(ctx, view_key, false);
}

fn scene_has_external_images(scene: &Scene) -> bool {
    scene.elements.iter().any(|element| {
        matches!(
            element,
            alas_report::scene::SceneElement::Image { source, .. }
                if source != "embedded://nasa-blue-marble"
        )
    })
}

fn show_external_images(
    state: &mut AppState,
    ui: &mut Ui,
    scene: &Scene,
    available_width: f32,
    available_height: f32,
    _allow_scroll: bool,
) -> bool {
    let images = scene
        .elements
        .iter()
        .filter_map(|element| match element {
            alas_report::scene::SceneElement::Image {
                source,
                width,
                height,
                ..
            } => Some((source.as_str(), *width, *height)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if images.is_empty() {
        return false;
    }
    let labels = scene
        .elements
        .iter()
        .filter_map(|element| match element {
            alas_report::scene::SceneElement::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    // Patran renders are independent views, not tiles of one panorama.  A
    // horizontal row divided a narrow results card by the image count but did
    // not wrap, leaving the second render outside its frame.  Keep one
    // labelled render visible at a useful size and let the user switch views.
    let selection_id = Id::new((
        "alas_patran_render_selection",
        images
            .iter()
            .map(|(source, _, _)| *source)
            .collect::<Vec<_>>(),
    ));
    let mut selected = ui
        .ctx()
        .data(|data| data.get_temp::<usize>(selection_id))
        .unwrap_or(0)
        .min(images.len() - 1);

    let mut double_clicked = false;
    if images.len() > 1 {
        ui.horizontal_wrapped(|ui| {
            for index in 0..images.len() {
                let label = labels.get(index).copied().unwrap_or("Patran render");
                if ui.selectable_label(selected == index, label).clicked() {
                    selected = index;
                }
            }
        });
        ui.add_space(4.0);
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(selection_id, selected));

    let (source, native_width, native_height) = images[selected];
    if let Some(label) = labels.get(selected) {
        ui.label(RichText::new(*label).strong());
    }
    let max_height = (available_height - if images.len() > 1 { 54.0 } else { 24.0 }).max(120.0);
    let panel_width = available_width.max(240.0);
    let panel_height = (panel_width * native_height as f32 / native_width as f32)
        .min(max_height)
        .max(120.0);
    let panel_width = (panel_height * native_width as f32 / native_height as f32).min(panel_width);
    match load_patran_texture(state, ui.ctx(), source) {
        Some(texture) => {
            if ui
                .add(
                    egui::Image::from_texture(&texture)
                        .fit_to_exact_size(vec2(panel_width, panel_height)),
                )
                .double_clicked()
            {
                double_clicked = true;
            }
        }
        None => {
            ui.add_sized(
                [panel_width, panel_height],
                egui::Label::new(
                    RichText::new(tr_fields(
                        "Patran image unavailable:\n{path}",
                        &[("path", source.to_string())],
                    ))
                    .weak(),
                ),
            );
        }
    }
    double_clicked
}

fn load_patran_texture(
    state: &mut AppState,
    context: &egui::Context,
    source: &str,
) -> Option<egui::TextureHandle> {
    if let Some(texture) = state.patran_textures.get(source) {
        return Some(texture.clone());
    }
    let bytes = std::fs::read(source).ok()?;
    let icon = eframe::icon_data::from_png_bytes(&bytes).ok()?;
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [icon.width as usize, icon.height as usize],
        &icon.rgba,
    );
    let texture = context.load_texture(
        format!("patran:{source}"),
        image,
        egui::TextureOptions::LINEAR,
    );
    state
        .patran_textures
        .insert(source.to_owned(), texture.clone());
    Some(texture)
}

fn unavailable_reason(state: &AppState, id: &str) -> String {
    let Some(result) = state.pipeline_result.as_ref() else {
        return tr("Not available: no completed pipeline run exists.");
    };
    let required_stage = alas_report::find_figure(id).map(|figure| figure.required_stage);
    if result.optimized_report.is_none() {
        tr("Not available for this run: baseline-only analysis has no optimized report.")
    } else if matches!(id, "mission_route_2d" | "mission_route_3d") && result.route.is_none() {
        tr("Not available: route planning did not produce a route.")
    } else if required_stage == Some(alas_report::RequiredStage::Mission)
        && result.mission_result.is_none()
    {
        tr("Not available: mission was disabled or did not produce telemetry.")
    } else if required_stage == Some(alas_report::RequiredStage::Mses) {
        let detail = result
            .mses_pressure
            .as_ref()
            .and_then(|stage| stage.error.as_deref())
            .or_else(|| {
                result
                    .mses_result
                    .as_ref()
                    .and_then(|stage| stage.error.as_deref())
            })
            .unwrap_or("MSES was disabled or did not produce the requested export.");
        tr_fields("Not available: {detail}", &[("detail", tr(detail))])
    } else if required_stage == Some(alas_report::RequiredStage::Structures) {
        let detail = result
            .structural_result
            .as_ref()
            .and_then(|stage| stage.error.as_deref())
            .unwrap_or("structural analysis or its required solver output was not available.");
        tr_fields("Not available: {detail}", &[("detail", tr(detail))])
    } else if id == "optimization_history" && result.optimization_result.is_none() {
        tr("Not available: optimization was disabled for this run.")
    } else {
        tr("Not available: the required analysis stage produced no data.")
    }
}

fn format_cg_pct_mac(cg_fraction: f64) -> String {
    format!("{cg_fraction:.1}% MAC")
}

#[cfg(test)]
#[path = "results_view_tests.rs"]
mod tests;
