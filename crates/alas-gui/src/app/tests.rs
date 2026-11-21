// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::AlasApp;
use crate::state::{AppState, Language};
use crate::view_controls::{
    auto_zoom_factor, auto_zoom_for_physical_size, zoom_after_command, ZoomCommand,
};
use egui::vec2;

#[test]
fn shell_menu_and_log_strings_have_spanish_desktop_translations() {
    let catalog = alas_i18n::es::desktop_catalog();
    for key in [
        "Navigation",
        "Unpin",
        "Pin",
        "Collapse navigation to an 8 px hover rail",
        "Keep navigation open and reserve its column",
        "Open navigation",
        "Unknown page.",
        "File",
        "Path:",
        "Load configuration",
        "Save configuration",
        "Export figures (ZIP)",
        "Generate report (PDF)",
        "Manage storage...",
        "Load preset",
        "Exit",
        "View",
        "Dark",
        "Light",
        "Grey",
        "3D Live Preview",
        "Show Run Log",
        "Hide Run Log",
        "Close",
        "Reduced Animations",
        "Automatic zoom",
        "English",
        "Spanish",
        "Zoom in",
        "Zoom out",
        "Reset zoom (100%)",
        "Help",
        "Replay Walkthrough",
        "Documentation",
        "About ALAS",
        "Figure archive with {count} SVG sources written to {path}.",
        "Figure archive failed: {error}",
        "Sectioned PDF report with {count} SVG sources written to {path}.",
        "PDF report failed: {error}",
    ] {
        assert!(catalog.contains_key(key), "missing shell text: {key}");
    }
}

#[test]
fn explicit_state_constructor_activates_the_selected_walkthrough_language() {
    let mut state = AppState::default();
    state.finish_walkthrough();
    state.language = Language::Es;
    let _app = AlasApp::from_state(state);

    assert_eq!(alas_i18n::get_language(), "es");
    alas_i18n::set_language(Some("en"));
}

#[test]
fn zoom_commands_stay_inside_the_readable_interface_range() {
    assert_eq!(zoom_after_command(2.2, ZoomCommand::In), 2.2);
    assert_eq!(zoom_after_command(0.75, ZoomCommand::Out), 0.75);
    assert_eq!(zoom_after_command(1.4, ZoomCommand::Reset), 1.0);
}

#[test]
fn automatic_zoom_scales_from_the_client_area_with_recoverable_bounds() {
    assert_eq!(auto_zoom_for_physical_size(vec2(1_240.0, 760.0), 1.0), 1.0);
    assert_eq!(auto_zoom_for_physical_size(vec2(300.0, 190.0), 1.0), 0.60);
    assert_eq!(
        auto_zoom_for_physical_size(vec2(3_840.0, 2_160.0), 1.0),
        1.35
    );
    assert_eq!(auto_zoom_for_physical_size(vec2(1_400.0, 850.0), 1.0), 1.1);
}

#[test]
fn automatic_zoom_fits_the_minimum_layout_under_operating_system_scaling() {
    // 1920 x 1200 pixels at 150 % is 1280 x 800 points: the 1240 x 760 point
    // layout fits only up to a zoom of about 1.03, never at the old 1.35.
    let zoom = auto_zoom_for_physical_size(vec2(1_920.0, 1_200.0), 1.5);
    assert!(zoom <= 1.03, "zoom {zoom}");
    assert!(1_920.0 / (1.5 * zoom) >= 1_240.0 && 1_200.0 / (1.5 * zoom) >= 760.0);
    // A 200 % display of a 1366 x 768 panel leaves 683 x 384 points.
    let tight = auto_zoom_for_physical_size(vec2(1_366.0, 768.0), 2.0);
    assert_eq!(tight, 0.60);
}

#[test]
fn automatic_zoom_stays_stable_when_egui_applies_a_pending_zoom() {
    let context = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            vec2(1_400.0, 850.0),
        )),
        ..egui::RawInput::default()
    };

    let _ = context.run(input.clone(), |_| {});
    let automatic = auto_zoom_factor(&context);
    context.set_zoom_factor(automatic);
    let _ = context.run(input, |_| {});

    assert_eq!(auto_zoom_factor(&context), automatic);
}

/// The root prompts are answered in the Sandbox too: closing the window with
/// unsaved edits there must raise the prompt, not swallow the request.
#[test]
fn closing_the_window_in_the_sandbox_with_unsaved_edits_raises_the_prompt() {
    let root = std::env::temp_dir().join(format!("alas-sandbox-close-{}", std::process::id()));
    let mut state = AppState {
        tool_locator: alas_exec::ToolLocator::new(root.join("app"), root.join("data")),
        ..AppState::default()
    };
    state.finish_walkthrough();
    state.boot_frames_remaining = 0;
    state.load_preset("A320-200");
    state.reset_undo_baseline();
    state.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(805.0);
    state.on_config_modified();
    assert!(state.enter_sandbox(true));
    assert!(state.sandbox.active());
    assert!(state.has_unsaved_edits());
    let mut app = AlasApp::from_state(state);
    let ctx = egui::Context::default();
    let frame = |app: &mut AlasApp, close: bool| {
        let mut info = egui::ViewportInfo::default();
        if close {
            info.events.push(egui::ViewportEvent::Close);
        }
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(1280.0, 800.0),
                )),
                viewports: std::iter::once((egui::ViewportId::ROOT, info)).collect(),
                ..egui::RawInput::default()
            },
            |ctx| app.show_frame(ctx),
        )
    };
    frame(&mut app, false);
    let output = frame(&mut app, true);
    let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::CancelClose)),
        "the close is held back"
    );
    assert!(matches!(
        app.state.shell.pending,
        Some(crate::shell_confirm::Pending::Exit)
    ));
    // The prompt is on screen: the modal flag is up and the root takes focus.
    assert!(crate::native_viewport::modal_pending(&ctx));
    assert!(commands
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::Focus)));
    // Answering it (Cancel) takes the flag down again.
    app.state.shell.pending = None;
    frame(&mut app, false);
    assert!(!crate::native_viewport::modal_pending(&ctx));
    let _ = std::fs::remove_dir_all(root);
}

/// Edits made only in the Sandbox are unsaved edits: the guided case is clean,
/// yet closing the window asks.
#[test]
fn closing_the_window_with_only_sandbox_edits_raises_the_prompt() {
    let root = std::env::temp_dir().join(format!("alas-sandbox-only-{}", std::process::id()));
    let mut state = AppState {
        tool_locator: alas_exec::ToolLocator::new(root.join("app"), root.join("data")),
        ..AppState::default()
    };
    state.finish_walkthrough();
    state.boot_frames_remaining = 0;
    state.load_preset("A320-200");
    state.reset_undo_baseline();
    state.shell.undo.mark_saved();
    assert!(!state.has_unsaved_edits());
    assert!(state.enter_sandbox(true));
    assert!(!state.has_unsaved_edits(), "entering edits nothing");
    state.on_sandbox_model_changed();
    assert!(state.sandbox.dirty());
    let mut app = AlasApp::from_state(state);
    let ctx = egui::Context::default();
    let mut info = egui::ViewportInfo::default();
    info.events.push(egui::ViewportEvent::Close);
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1280.0, 800.0),
            )),
            viewports: std::iter::once((egui::ViewportId::ROOT, info)).collect(),
            ..egui::RawInput::default()
        },
        |ctx| app.show_frame(ctx),
    );
    let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
    assert!(commands
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::CancelClose)));
    assert!(matches!(
        app.state.shell.pending,
        Some(crate::shell_confirm::Pending::Exit)
    ));
    let _ = std::fs::remove_dir_all(root);
}

/// Drive `pages` through the performance tour in a plain egui context and
/// return the measured rows.
fn headless_tour(
    preset: &str,
    pages: &[&'static str],
    sampling: (usize, usize),
) -> Vec<crate::perf_tour::Row> {
    let tour = crate::perf_tour::PerfTour::for_pages(preset, pages, sampling);
    run_headless_tour(tour, 2 * pages.len() * (sampling.0 + sampling.1 + 2) + 10)
}

/// Drive `tour` in a plain egui context for at most `budget` frames and
/// return the measured rows.
fn run_headless_tour(
    tour: crate::perf_tour::PerfTour,
    budget: usize,
) -> Vec<crate::perf_tour::Row> {
    let state = AppState {
        boot_frames_remaining: 0,
        ..AppState::default()
    };
    let mut app = AlasApp::from_state(state);
    app.tour = Some(tour);
    let ctx = egui::Context::default();
    // `ALAS_PERF_ASYNC=1`: page previews on worker threads, as in the app
    // (unit tests otherwise build them in the frame).
    let background = std::env::var_os("ALAS_PERF_ASYNC").is_some_and(|value| !value.is_empty());
    if background {
        app.state.preview_jobs.get_mut().enable_for_test(&ctx);
        crate::sandbox::scene::build_live_models_in_background(&ctx);
    }
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1280.0, 800.0));
    for _ in 0..budget {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                // A GPU backend reports at least 8192; the default 2048 would
                // reject the page previews' 2x rasters.
                max_texture_side: Some(8192),
                ..egui::RawInput::default()
            },
            |ctx| app.timed_frame(ctx),
        );
        if app.tour.as_ref().is_some_and(|tour| tour.finished) {
            break;
        }
        // The app waits for a worker's repaint request; this loop would spend
        // its frame budget spinning meanwhile (a front view raster takes
        // about 0.6 s), so it waits a little while one is pending.
        if background && alas_viz::pending_background_rasters() > 0 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    let tour = app.tour.take().expect("tour is installed");
    assert!(tour.finished, "the tour finished within its frame budget");
    tour.rows
}

#[test]
fn the_perf_tour_measures_each_step_once() {
    let rows = headless_tour("A320-200", &["inputs", "geometry"], (1, 3));
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|row| row.frames == 3 && row.cpu_ms.2 > 0.0));
    assert_eq!(rows[0].detail, "basic");
    assert_eq!(rows[3].page, "geometry");
    assert_eq!(rows[3].detail, "advanced");
}

/// `cargo test -p alas-gui --lib --release -- --ignored perf_tour --nocapture`
/// prints the per-page frame cost without a GPU.
#[test]
#[ignore = "timing report; run on demand"]
// The on-demand timing report is this test's output: it prints to stdout.
#[allow(clippy::print_stdout)]
fn the_perf_tour_report_for_every_page() {
    let preset = std::env::var("ALAS_PERF_PRESET")
        .unwrap_or_else(|_| crate::perf_tour::DEFAULT_PRESET.to_owned());
    let mut seen = std::collections::HashSet::new();
    let only = std::env::var("ALAS_PERF_PAGES").unwrap_or_default();
    let pages: Vec<&'static str> = crate::nav::all_pages()
        .filter(|page| page.kind != crate::nav::PageKind::Sandbox)
        .filter(|page| only.is_empty() || only.split(',').any(|id| id == page.id))
        .filter(|page| seen.insert(page.id))
        .map(|page| page.id)
        .collect();
    let mut rows = headless_tour(&preset, &pages, (10, 40));
    rows.sort_by(|a, b| b.cpu_ms.1.total_cmp(&a.cpu_ms.1));
    println!("{}", crate::perf_tour::CSV_HEADER);
    for row in &rows {
        println!("{}", row.csv());
    }
}

/// `cargo test -p alas-gui --lib --release -- --ignored perf_orbit --nocapture`
/// prints the drag-orbit frame cost of the dock and the sandbox without a GPU.
#[test]
#[ignore = "timing report; run on demand"]
// The on-demand timing report is this test's output: it prints to stdout.
#[allow(clippy::print_stdout)]
fn the_perf_orbit_report() {
    let preset = std::env::var("ALAS_PERF_PRESET")
        .unwrap_or_else(|_| crate::perf_tour::DEFAULT_PRESET.to_owned());
    let views = ["dock", "sandbox"];
    let tour = crate::perf_tour::PerfTour::for_orbits(&preset, &views, (10, 60));
    let rows = run_headless_tour(tour, views.len() * 80 + 10);
    println!("{}", crate::perf_tour::CSV_HEADER);
    for row in &rows {
        println!("{}", row.csv());
    }
}

/// `cargo test -p alas-gui --lib --release -- --ignored dock_frame_split --nocapture`
/// splits one dock-page frame into update, tessellation and shape count: the
/// perf tour times only the update, but tessellation runs on every frame too.
#[test]
#[ignore = "timing report; run on demand"]
// The on-demand timing report is this test's output: it prints to stdout.
#[allow(clippy::print_stdout)]
fn dock_frame_split() {
    let mut state = AppState {
        boot_frames_remaining: 0,
        ..AppState::default()
    };
    state.load_preset("A380-800");
    crate::app::go_to_page(&mut state, "geometry");
    let mut app = AlasApp::from_state(state);
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1584.0, 961.0));
    let (mut run_ms, mut tess_ms, mut shapes) = (Vec::new(), Vec::new(), 0usize);
    for frame in 0..80 {
        let started = std::time::Instant::now();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                max_texture_side: Some(8192),
                ..egui::RawInput::default()
            },
            |ctx| app.timed_frame(ctx),
        );
        let ran = started.elapsed();
        shapes = output.shapes.len();
        let started = std::time::Instant::now();
        let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
        let tess = started.elapsed();
        if frame >= 40 {
            run_ms.push(ran.as_secs_f64() * 1e3);
            tess_ms.push(tess.as_secs_f64() * 1e3);
        }
        let _ = meshes;
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    println!(
        "update {:.2} ms, tessellate {:.2} ms, top-level shapes {shapes}",
        mean(&run_ms),
        mean(&tess_ms)
    );
}

/// Start-up (`AlasApp::default`, the first-start state with its preset load,
/// and the first frames) must fit in 768 KiB of stack, leaving the rest of the
/// 1 MiB Windows default main-thread reserve to the runtime, eframe and winit.
/// The GUI runs on the main thread, where a build without the larger linker
/// reserve overflowed the stack at launch. A stack overflow aborts the test
/// process instead of failing one assertion; that is the failure mode here.
#[test]
fn gui_start_up_fits_the_default_main_thread_stack() {
    let worker = std::thread::Builder::new()
        .name("start-up-stack".to_owned())
        .stack_size(768 * 1024)
        .spawn(|| {
            let mut app = AlasApp::default();
            let ctx = egui::Context::default();
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1280.0, 800.0));
            for _ in 0..3 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        max_texture_side: Some(8192),
                        ..egui::RawInput::default()
                    },
                    |ctx| app.timed_frame(ctx),
                );
            }
            app.state.boot_frames_remaining
        })
        .expect("the start-up thread starts");
    let remaining = worker.join().expect("start-up did not panic");
    assert!(remaining > 0, "the boot splash is still counting down");
}
