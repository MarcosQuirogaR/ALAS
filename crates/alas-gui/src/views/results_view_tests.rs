// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{
    figure_gallery_layout, format_cg_pct_mac, fullscreen_camera_key, fullscreen_id,
    fullscreen_open, fullscreen_slot_key, fullscreen_view_key, open_fullscreen_result,
    responsive_card_layout, scene_has_external_images, set_fullscreen, unavailable_reason,
    CARD_GAP, CARD_MIN_WIDTH,
};
use crate::state::{AppState, PreviewCamera};
use alas_report::scene::{Color, Scene, SceneElement, TextAlign, TextBaseline};
use egui::{Context, RawInput};

#[test]
fn openvsp_center_overlay_takes_the_click_instead_of_the_geometry_canvas() {
    let directory = std::env::temp_dir().join(format!("alas-openvsp-hit-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let executable = directory.join(if cfg!(windows) { "vsp.exe" } else { "vsp" });
    // Deliberately invalid executable: verify dispatch and error feedback
    // without starting a real GUI or changing the user's desktop.
    std::fs::write(&executable, b"not an executable").unwrap();
    let model = directory.join("full.cad_preview.vsp3");
    std::fs::write(&model, b"test model").unwrap();
    let mut config = alas_config::AlasConfig::default();
    config.mission.enabled = false;
    config.mses.enabled = false;
    config.structures.enabled = false;
    let mut result = alas_pipeline::DesignPipeline::new(config)
        .run(
            &alas_pipeline::PipelineOptions {
                optimize: false,
                compare_baseline: false,
                quiet: true,
                output_dir: Some(directory.join("outputs")),
                ..Default::default()
            },
            &alas_exec::RunEnvironment::default(),
        )
        .unwrap();
    let export = result.openvsp_export.as_mut().unwrap();
    export.status = alas_pipeline::OpenVspExportStatus::Vsp3Materialized;
    export.runtime_executable = Some(directory.join("vspscript.exe"));
    export.cad_preview_vsp3_path = model;
    let mut state = AppState {
        pipeline_result: Some(result),
        ..Default::default()
    };
    let ctx = Context::default();
    let mut center = egui::Pos2::ZERO;
    let mut canvas_clicked = false;
    for frame in 0..4 {
        let events = match frame {
            2 | 3 => vec![
                egui::Event::PointerMoved(center),
                egui::Event::PointerButton {
                    pos: center,
                    button: egui::PointerButton::Primary,
                    pressed: frame == 2,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            _ => Vec::new(),
        };
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                time: Some(frame as f64 * 0.1),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (canvas, response) =
                        ui.allocate_exact_size(egui::vec2(640.0, 400.0), egui::Sense::click());
                    center = canvas.center();
                    canvas_clicked |= response.clicked();
                    super::openvsp::show_launch_button(&mut state, ui, canvas);
                });
            },
        );
    }
    assert!(
        !canvas_clicked,
        "the centered overlay must consume the canvas click"
    );
    assert!(
        state.status_message.contains("Could not open OpenVSP"),
        "{}",
        state.status_message
    );
    assert_eq!(
        alas_report::find_figure("openvsp_cad_preview")
            .unwrap()
            .category,
        "Geometry"
    );
}

#[test]
fn baseline_cg_fraction_is_displayed_once_as_percent_mac() {
    assert_eq!(format_cg_pct_mac(25.359), "25.4% MAC");
}

#[test]
fn a_missing_pipeline_run_has_an_actionable_figure_reason() {
    assert_eq!(
        unavailable_reason(&AppState::default(), "mission_profile"),
        "Not available: no completed pipeline run exists."
    );
}

#[test]
fn result_fullscreen_state_is_keyed_by_the_figure_cache_identity() {
    let ctx = Context::default();
    set_fullscreen(&ctx, "run=1;figure=mass_breakdown", true);

    assert!(fullscreen_open(&ctx, "run=1;figure=mass_breakdown"));
    assert!(!fullscreen_open(&ctx, "run=1;figure=mass_distribution"));
    assert_ne!(
        fullscreen_id("run=1;figure=mass_breakdown"),
        fullscreen_id("run=1;figure=mass_distribution")
    );
}

#[test]
fn result_fullscreen_state_keeps_one_slot_when_display_language_or_theme_changes() {
    let original = "run=1;solver=Vlm;theme=Dark;language=en;figure=mission_route_3d";
    let translated = "run=1;solver=Vlm;theme=Light;language=es;figure=mission_route_3d";
    let ctx = Context::default();
    set_fullscreen(&ctx, original, true);

    assert!(fullscreen_open(&ctx, translated));
    assert_eq!(
        fullscreen_slot_key(original),
        fullscreen_slot_key(translated)
    );
    assert_eq!(
        fullscreen_view_key(original),
        fullscreen_view_key(translated)
    );
}

#[test]
fn fullscreen_orbit_state_is_copied_without_mutating_the_card() {
    let ctx = Context::default();
    let mut state = AppState::default();
    let view_key = "run=1;figure=mission_route_3d";
    let camera_key = "result_camera::run=1;figure=mission_route_3d";
    state.view_state_mut(view_key).pan = egui::vec2(12.0, -8.0);
    *state.result_camera_mut(camera_key) = PreviewCamera::top();

    open_fullscreen_result(&mut state, &ctx, view_key, camera_key, true);
    state.view_state_mut(fullscreen_view_key(view_key)).pan = egui::vec2(33.0, 5.0);
    state
        .result_camera_mut(fullscreen_camera_key(camera_key))
        .zoom = 3.0;

    assert_eq!(state.view_state_mut(view_key).pan, egui::vec2(12.0, -8.0));
    assert_eq!(
        state.result_camera_mut(camera_key).zoom,
        PreviewCamera::top().zoom
    );
}

#[test]
fn result_cards_add_columns_only_when_the_minimum_width_fits() {
    assert_eq!(responsive_card_layout(640.0).0, 1);
    assert_eq!(responsive_card_layout(652.0).0, 2);
    assert_eq!(responsive_card_layout(984.0).0, 3);
}

#[test]
fn result_card_rows_use_all_available_width_without_a_gutter() {
    for available in [240.0, 320.0, 652.0, 760.0, 984.0, 1_280.0] {
        let (columns, card_width) = responsive_card_layout(available);
        let row_width = columns as f32 * card_width + (columns - 1) as f32 * CARD_GAP;
        assert_eq!(card_width >= CARD_MIN_WIDTH, available >= CARD_MIN_WIDTH);
        assert!((row_width - available).abs() < 0.01);
    }
}

#[test]
fn result_card_frame_padding_is_counted_inside_the_gallery_width() {
    for available in [320.0, 652.0, 984.0, 1_280.0] {
        let (columns, outer_width) = responsive_card_layout(available);
        let content_width = crate::theme::card_content_width(outer_width);
        let row_width = columns as f32 * (content_width + 2.0 * crate::theme::CARD_INNER_MARGIN_X)
            + (columns - 1) as f32 * CARD_GAP;
        assert!((row_width - available).abs() < 0.01);
    }
}

#[test]
fn model_comparison_uses_the_full_gallery_and_available_viewport_height() {
    let (columns, card_width, canvas_height) = figure_gallery_layout(1_200.0, 590.0, true);
    assert_eq!(columns, 1);
    assert_eq!(card_width, 1_200.0);
    assert_eq!(canvas_height, 538.0);

    let (_, _, short_canvas) = figure_gallery_layout(640.0, 250.0, true);
    assert_eq!(short_canvas, 320.0);
}

#[test]
fn patran_scene_images_take_the_gui_raster_display_path() {
    let mut scene = Scene::new(500.0, 500.0, None);
    scene.add(SceneElement::Image {
        source: "C:/renders/pull-up.png".to_owned(),
        x: 0.0,
        y: 0.0,
        width: 500.0,
        height: 470.0,
        source_rect: None,
    });
    assert!(scene_has_external_images(&scene));
}

#[test]
fn patran_case_selector_labels_are_rendered_once_without_a_duplicate_caption() {
    let first_image = concat!(env!("CARGO_MANIFEST_DIR"), "/../../app_branding.png");
    let second_image = concat!(env!("CARGO_MANIFEST_DIR"), "/../../app_logo.png");
    let mut scene = Scene::new(1_000.0, 500.0, None);
    for (index, (name, source)) in [("pull-up", first_image), ("push-down", second_image)]
        .into_iter()
        .enumerate()
    {
        let x = index as f64 * 500.0;
        scene.add(SceneElement::Image {
            source: source.to_owned(),
            x,
            y: 30.0,
            width: 500.0,
            height: 470.0,
            source_rect: None,
        });
        scene.add(SceneElement::Text {
            text: name.to_owned(),
            pos: [x + 250.0, 18.0],
            font_size: 12.0,
            color: Color::from_hex("#ffffff"),
            align: TextAlign::Center,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
    }

    let context = Context::default();
    let mut state = AppState::default();
    let card_viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
    let first_output = context.run(
        RawInput {
            screen_rect: Some(card_viewport),
            time: Some(0.0),
            ..RawInput::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!super::images::show_external_images(
                    &mut state, ui, &scene, 560.0, 360.0, false,
                ));
            });
        },
    );
    let second_case_pos = first_output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.job.text == "push-down" => {
                Some(text.pos + 0.5 * text.galley.size())
            }
            _ => None,
        })
        .expect("Patran case selector renders the second case");
    let second_output = context.run(
        RawInput {
            screen_rect: Some(card_viewport),
            time: Some(0.1),
            events: vec![
                egui::Event::PointerMoved(second_case_pos),
                egui::Event::PointerButton {
                    pos: second_case_pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: second_case_pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..RawInput::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!super::images::show_external_images(
                    &mut state, ui, &scene, 560.0, 360.0, false,
                ));
            });
        },
    );
    let selection_id = egui::Id::new((
        "alas_external_render_selection",
        vec![first_image, second_image],
    ));
    assert_eq!(
        context.data(|data| data.get_temp::<usize>(selection_id)),
        Some(1),
        "clicking the selector switches to the second case"
    );

    // The maximized result calls the same image helper with a larger viewport;
    // its selection must continue to point at the case chosen in the card.
    let fullscreen_output = context.run(
        RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            time: Some(0.2),
            ..RawInput::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!super::images::show_external_images(
                    &mut state, ui, &scene, 1_100.0, 680.0, true,
                ));
            });
        },
    );
    assert_eq!(
        context.data(|data| data.get_temp::<usize>(selection_id)),
        Some(1),
        "the maximized view keeps the selected case"
    );

    let rendered_count = |output: &egui::FullOutput, label: &str| {
        output
            .shapes
            .iter()
            .filter(|shape| {
                matches!(
                    &shape.shape,
                    egui::Shape::Text(text) if text.galley.job.text == label
                )
            })
            .count()
    };
    for output in [&first_output, &second_output, &fullscreen_output] {
        assert_eq!(rendered_count(output, "pull-up"), 1);
        assert_eq!(rendered_count(output, "push-down"), 1);
    }
}

#[test]
fn embedded_blue_marble_stays_on_the_scene_display_path() {
    let mut scene = Scene::new(500.0, 500.0, None);
    scene.add(SceneElement::Image {
        source: "embedded://nasa-blue-marble".to_owned(),
        x: 0.0,
        y: 0.0,
        width: 500.0,
        height: 250.0,
        source_rect: None,
    });
    assert!(!scene_has_external_images(&scene));
    scene.add(SceneElement::SphericalImage {
        source: "embedded://nasa-blue-marble".to_owned(),
        center: [250.0, 250.0],
        radius: 200.0,
        camera: alas_report::scene::Camera3D::front(),
        mirror_longitude: false,
        clip: None,
    });
    assert!(!scene_has_external_images(&scene));
}

mod maximized_overlay {
    //! The maximized Results figure is an overlay inside the main window.
    //! Native viewports are reserved for menus and tool windows; a figure
    //! double-clicked in the gallery must never spawn a detached OS window.

    use super::super::fullscreen_result::{fullscreen_area_id, show_fullscreen_result};
    use super::super::images::external_image;
    use super::super::{
        fullscreen_open, fullscreen_view_key, open_fullscreen_result, FullscreenFigure,
    };
    use crate::state::AppState;
    use alas_report::scene::Scene;
    use egui::{Context, Event, Modifiers, PointerButton, RawInput, ViewportId};

    const VIEW_KEY: &str = "run=1;solver=Vlm;theme=Dark;language=en;figure=mass_breakdown";
    const CAMERA_KEY: &str = "result_camera::run=1;figure=mass_breakdown";
    const WINDOW: egui::Vec2 = egui::vec2(1280.0, 800.0);

    fn raw_input(time: f64, events: Vec<Event>) -> RawInput {
        RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW)),
            time: Some(time),
            events,
            ..RawInput::default()
        }
    }

    fn click_at(pos: egui::Pos2) -> Vec<Event> {
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ]
    }

    struct Harness {
        ctx: Context,
        state: AppState,
        scene: Scene,
        config: alas_config::AlasConfig,
    }

    impl Harness {
        fn maximized() -> Self {
            let ctx = Context::default();
            let mut state = AppState::default();
            open_fullscreen_result(&mut state, &ctx, VIEW_KEY, CAMERA_KEY, false);
            Self {
                ctx,
                state,
                scene: Scene::new(640.0, 400.0, None),
                config: alas_config::AlasConfig::default(),
            }
        }

        fn frame(&mut self, input: RawInput) -> egui::FullOutput {
            let Self {
                ctx,
                state,
                scene,
                config,
            } = self;
            ctx.run(input, |ctx| {
                show_fullscreen_result(
                    state,
                    ctx,
                    FullscreenFigure {
                        scene,
                        config,
                        theme: "Dark",
                        camera_key: CAMERA_KEY,
                        view_key: VIEW_KEY,
                        title: "Mass breakdown",
                        description: "Test figure",
                        orbitable: false,
                    },
                );
            })
        }

        fn is_open(&self) -> bool {
            fullscreen_open(&self.ctx, VIEW_KEY)
        }
    }

    #[test]
    fn maximized_result_figure_is_an_overlay_inside_the_main_window() {
        let mut harness = Harness::maximized();
        let output = harness.frame(raw_input(0.0, Vec::new()));

        // Only the root viewport rendered: no detached OS window was
        // requested for the figure.
        assert_eq!(output.viewport_output.len(), 1);
        assert!(output.viewport_output.contains_key(&ViewportId::ROOT));
        let area = harness
            .ctx
            .memory(|memory| memory.area_rect(fullscreen_area_id(VIEW_KEY)))
            .expect("the maximized figure registers its overlay area");
        assert_eq!(area.min, egui::Pos2::ZERO);
        assert!(area.width() >= WINDOW.x && area.height() >= WINDOW.y);
        assert!(harness.is_open());
    }

    #[test]
    fn escape_restores_the_maximized_result_figure() {
        let mut harness = Harness::maximized();
        harness.frame(raw_input(0.0, Vec::new()));
        harness.frame(raw_input(
            0.1,
            vec![Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        ));

        assert!(!harness.is_open());
        assert!(!harness
            .state
            .view_states
            .contains_key(&fullscreen_view_key(VIEW_KEY)));
    }

    #[test]
    fn double_clicking_the_maximized_figure_restores_it_but_a_single_click_keeps_it() {
        let mut harness = Harness::maximized();
        // Lay the overlay out once so egui can hit-test the figure widget.
        harness.frame(raw_input(0.0, Vec::new()));
        let centre = egui::pos2(WINDOW.x / 2.0, WINDOW.y / 2.0);

        harness.frame(raw_input(0.5, click_at(centre)));
        assert!(
            harness.is_open(),
            "a single click keeps the figure maximized"
        );

        harness.frame(raw_input(0.6, click_at(centre)));
        assert!(
            !harness.is_open(),
            "the second click of a double-click restores it"
        );
    }

    #[test]
    fn external_render_widget_accepts_the_maximize_double_click() {
        let ctx = Context::default();
        let mut sense = egui::Sense::hover();
        let _ = ctx.run(raw_input(0.0, Vec::new()), |ctx| {
            let texture = ctx.load_texture(
                "test-render",
                egui::ColorImage::example(),
                egui::TextureOptions::LINEAR,
            );
            egui::CentralPanel::default().show(ctx, |ui| {
                sense = ui
                    .add(external_image(&texture, egui::vec2(200.0, 150.0)))
                    .sense;
            });
        });
        assert!(sense.click, "egui::Image senses hover only unless asked");
    }
}

#[test]
fn a_result_figure_card_shows_its_explanation_only_as_hover_text() {
    let descriptor = alas_report::RESULT_FIGURES
        .iter()
        .find(|descriptor| !descriptor.description.is_empty())
        .expect("a registered result figure with an explanation");
    let context = Context::default();
    let mut state = AppState {
        help_verbose: true,
        ..Default::default()
    };
    let config = state
        .typed_config()
        .expect("the default state has a typed configuration");
    let output = context.run(
        RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            )),
            ..RawInput::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                super::figure_tile(&mut state, ui, &config, descriptor, 420.0, 260.0);
            });
        },
    );
    let painted: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        painted.iter().any(|text| text == descriptor.title),
        "the card paints the figure title: {painted:?}"
    );
    assert!(
        !painted.iter().any(|text| text == descriptor.description),
        "Learn-more help must not repeat the hover explanation as a subtitle: {painted:?}"
    );
}
