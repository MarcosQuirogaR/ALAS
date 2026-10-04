// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Capture the real desktop shell, page by page, as PNG screenshots for visual
//! review: every sidebar page, every results tab and every Advanced Settings
//! tab, scrolled top to bottom, in English and Spanish and in the light and
//! dark themes.
//!
//! The shell runs one seeded optimization of the chosen preset through the
//! same Run action a user presses, then the tour drives navigation state and
//! scrolls with synthetic wheel events. Each screenshot is the painted
//! swapchain frame of a native window at the shell's default size.
//!
//! Usage:
//!   cargo run --release -p alas-gui --example render_gui_tour -- \
//!       <output directory> [preset] [screening budget] [refinement budget] [filter]
//!
//! `filter` keeps only shots whose name contains it. `advanced` as filter (or
//! any filter starting with `advanced`) opens the Advanced Settings mode: the
//! root window takes the detached window's default size and renders its
//! contents, because an immediate child viewport cannot be captured.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

use alas_gui::state::Language;
use alas_gui::{AlasApp, AppState, AppTheme};
use egui::{Event, Pos2, Vec2};

/// The shell's default size (`alas_gui::run`).
const MAIN_SIZE: [f32; 2] = [1280.0, 820.0];
/// The shell's automatic zoom at its default size on a 150 % display: the
/// physical client area exceeds 1240 x 760 by more than the 1.35 ceiling.
/// The zoom is context-wide, so the detached windows share it.
const ADVANCED_ZOOM: f32 = 1.35;
/// The Advanced Settings window opens at 0.6 of the main window's outer size
/// in zoomed points (about 576 x 384 here); the window size is given before
/// zoom.
const ADVANCED_SIZE: [f32; 2] = [576.0 * ADVANCED_ZOOM, 384.0 * ADVANCED_ZOOM];
/// The window's minimum size in zoomed points.
const ADVANCED_MIN_SIZE: [f32; 2] = [560.0 * ADVANCED_ZOOM, 380.0 * ADVANCED_ZOOM];

#[derive(Clone)]
struct Shot {
    name: String,
    language: Language,
    theme: AppTheme,
    page: &'static str,
    results_tab: Option<&'static str>,
    advanced_tab: Option<&'static str>,
    max_pages: usize,
    /// An MTOW mode to select before the shot, for the mode-dependent card.
    mtow_mode: Option<&'static str>,
}

fn shots(advanced: bool) -> Vec<Shot> {
    let mut list = Vec::new();
    for (theme, theme_name) in [(AppTheme::Light, "light"), (AppTheme::Dark, "dark")] {
        for (language, code) in [(Language::En, "en"), (Language::Es, "es")] {
            // Dark is reviewed at the top of each page only.
            let pages = if theme == AppTheme::Light { 30 } else { 1 };
            let shot = |name: String,
                        page: &'static str,
                        results_tab: Option<&'static str>,
                        advanced_tab: Option<&'static str>| Shot {
                name,
                language,
                theme,
                page,
                results_tab,
                advanced_tab,
                max_pages: pages,
                mtow_mode: None,
            };
            if advanced {
                for page in alas_gui::sandbox::advanced::pages() {
                    list.push(shot(
                        format!("advanced_{}_{theme_name}_{code}", page.id),
                        "inputs",
                        None,
                        Some(page.id),
                    ));
                }
                list.push(shot(
                    format!("advanced_run_options_{theme_name}_{code}"),
                    "inputs",
                    None,
                    Some("run_options"),
                ));
                continue;
            }
            for group in alas_gui::nav::NAV {
                for sub in group.subgroups {
                    for page in sub.pages {
                        if page.id == "results" {
                            continue;
                        }
                        list.push(shot(
                            format!("page_{}_{theme_name}_{code}", page.id),
                            page.id,
                            None,
                            None,
                        ));
                    }
                }
            }
            for tab in [
                "summary",
                "optimization",
                "geometry",
                "aero",
                "wb",
                "propulsion",
                "structures",
                "mission",
                "field",
                "model",
            ] {
                list.push(shot(
                    format!("results_{tab}_{theme_name}_{code}"),
                    "results",
                    Some(tab),
                    None,
                ));
            }
        }
    }
    // The MTOW card in its other modes and in the Mass settings tab. These
    // edit the form, so they come after every shot of the run's results.
    for (language, code) in [(Language::En, "en"), (Language::Es, "es")] {
        for mode in ["mtow_band", "payload_adjusted"] {
            let shot = Shot {
                name: format!(
                    "{}mtow_{mode}_light_{code}",
                    if advanced { "advanced_" } else { "" }
                ),
                language,
                theme: AppTheme::Light,
                page: "inputs",
                results_tab: None,
                advanced_tab: if advanced {
                    Some("mass_advanced")
                } else {
                    None
                },
                max_pages: 30,
                mtow_mode: Some(mode),
            };
            list.push(shot);
        }
    }
    list
}

enum Phase {
    Boot(u32),
    Running,
    Shots,
    Done,
}

struct Tour {
    app: AlasApp,
    advanced: bool,
    advanced_state: Option<AppState>,
    shots: Vec<Shot>,
    index: usize,
    page: usize,
    wait: u32,
    awaiting: bool,
    pending: Vec<Event>,
    previous: Option<Vec<u8>>,
    output: PathBuf,
    phase: Phase,
    needs_apply: bool,
    screen: egui::Rect,
    index_lines: Vec<String>,
}

const SETTLE_FRAMES: u32 = 24;

impl Tour {
    fn state(&mut self) -> &mut AppState {
        match self.advanced_state.as_mut() {
            Some(state) => state,
            None => self.app.state_mut(),
        }
    }

    fn apply(&mut self, ctx: &egui::Context) {
        let shot = self.shots[self.index].clone();
        let state = self.state();
        state.language = shot.language;
        alas_i18n::set_language(Some(shot.language.code()));
        if state.theme != shot.theme {
            state.theme = shot.theme;
            alas_gui::apply_theme(shot.theme, ctx);
        }
        state.active_page = shot.page.to_owned();
        // The run opens the log; a reviewer closes it to read the pages.
        state.run_log_open = false;
        if let Some(tab) = shot.results_tab {
            state.results_tab = tab.to_owned();
        }
        if let Some(tab) = shot.advanced_tab {
            state.sandbox.advanced_tab = tab.to_owned();
        }
        if let Some(mode) = shot.mtow_mode {
            state.config_values["optimizer"]["objective"]["mtow_sizing"] = serde_json::json!(mode);
        }
        state.update_preview_scene();
        state.update_result_scene();
        // A new page starts at the top: scroll far up first.
        self.scroll(-1.0e5);
        self.page = 0;
        self.previous = None;
        self.wait = SETTLE_FRAMES;
    }

    fn scroll(&mut self, delta: f32) {
        // Over the content pane, clear of the navigation rail, the preview
        // dock and the control bar, in the shell's current points.
        let screen = self.screen;
        let pointer = if self.advanced {
            Pos2::new(screen.center().x, screen.top() + 0.7 * screen.height())
        } else {
            let dock = alas_gui::layout::PREVIEW_DOCK_DEFAULT_WIDTH;
            Pos2::new(
                screen.left() + 0.5 * (screen.width() - dock),
                screen.top() + 0.5 * screen.height(),
            )
        };
        self.pending.push(Event::PointerMoved(pointer));
        self.pending.push(Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, -delta),
            modifiers: egui::Modifiers::NONE,
        });
    }

    fn save(&mut self, image: &egui::ColorImage) {
        let pixels: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
        let shot = &self.shots[self.index];
        if self.previous.as_ref() == Some(&pixels) || self.page >= shot.max_pages {
            self.index += 1;
            self.previous = None;
            self.needs_apply = true;
            return;
        }
        let name = format!("{}_p{}.png", shot.name, self.page);
        let [width, height] = image.size;
        match alas_viz::raster::encode_png_rgba(width as u32, height as u32, &pixels) {
            Ok(png) => {
                let path = self.output.join(&name);
                if let Err(error) = std::fs::write(&path, png) {
                    eprintln!("write {}: {error}", path.display());
                }
                self.index_lines.push(name.clone());
                println!("wrote {name}");
            }
            Err(error) => eprintln!("encode {name}: {error}"),
        }
        self.previous = Some(pixels);
        self.page += 1;
        let page_height = self.screen.height() * if self.advanced { 0.7 } else { 0.5 };
        self.scroll(page_height);
        self.wait = SETTLE_FRAMES;
    }
}

impl eframe::App for Tour {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        let screenshot = raw_input.events.iter().find_map(|event| match event {
            Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        });
        // Real pointer motion over the window would disturb the tour.
        raw_input.events.retain(|event| {
            !matches!(
                event,
                Event::PointerMoved(_) | Event::MouseWheel { .. } | Event::PointerGone
            )
        });
        raw_input.events.append(&mut self.pending);
        if let Some(image) = screenshot {
            if self.awaiting {
                self.awaiting = false;
                self.save(&image);
            }
        }
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ctx.request_repaint();
        self.screen = ctx.screen_rect();
        if let Some(state) = self.advanced_state.as_mut() {
            egui::CentralPanel::default().show(ctx, |ui| {
                alas_gui::sandbox::advanced::show_advanced_settings_contents(state, ui);
            });
        } else {
            eframe::App::update(&mut self.app, ctx, frame);
        }
        match self.phase {
            Phase::Boot(ref mut frames) => {
                if *frames > 0 {
                    *frames -= 1;
                    return;
                }
                if self.advanced {
                    self.phase = Phase::Shots;
                    self.apply(ctx);
                } else {
                    self.app.state_mut().start_pipeline(false);
                    self.phase = Phase::Running;
                }
            }
            Phase::Running => {
                let state = self.app.state_mut();
                if !state.is_running {
                    if !state.pipeline_result_complete {
                        eprintln!("pipeline did not complete: {}", state.status_message);
                        for line in state.logs.iter().rev().take(12).rev() {
                            eprintln!("  {}", line.text);
                        }
                    }
                    self.phase = Phase::Shots;
                    self.apply(ctx);
                }
            }
            Phase::Shots => {
                if self.awaiting {
                    return;
                }
                if self.index >= self.shots.len() {
                    self.phase = Phase::Done;
                    let index = self.output.join("tour_index.txt");
                    let _ = std::fs::write(index, self.index_lines.join("\n"));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                if self.needs_apply {
                    self.needs_apply = false;
                    self.apply(ctx);
                    return;
                }
                if self.wait > 0 {
                    self.wait -= 1;
                    return;
                }
                self.awaiting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            }
            Phase::Done => {}
        }
    }
}

// AppState owns private caches, so it is initialized through Default.
#[allow(clippy::field_reassign_with_default)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(args.next().unwrap_or_else(|| {
        std::env::temp_dir()
            .join("alas-shots/qa")
            .to_string_lossy()
            .into_owned()
    }));
    let preset = args.next().unwrap_or_else(|| "A320-200".to_owned());
    let screening: i64 = args.next().map_or(Ok(240), |value| value.parse())?;
    let refinement: i64 = args.next().map_or(Ok(160), |value| value.parse())?;
    let filter = args.next().unwrap_or_default();
    let advanced = filter.starts_with("advanced");
    std::fs::create_dir_all(&output)?;
    let size = if filter.contains("narrow") {
        ADVANCED_MIN_SIZE
    } else if advanced {
        ADVANCED_SIZE
    } else {
        MAIN_SIZE
    };

    alas_i18n::es::install();
    let mut state = AppState::default();
    state.boot_frames_remaining = 0;
    state.load_preset(&preset);
    state.run_options.optimize = true;
    state.run_options.compare_baseline = true;
    state.run_options.write_outputs = false;
    let solver = &mut state.config_values["optimizer"]["solver"];
    solver["screening"]["max_evaluations"] = serde_json::json!(screening);
    solver["refinement"]["max_evaluations"] = serde_json::json!(refinement);
    solver["stop_on_evaluations_only"] = serde_json::json!(true);
    state.config_values["structures"]["run_nastran"] = serde_json::json!(false);
    state.config_values["structures"]["run_patran_export"] = serde_json::json!(false);
    state.config_values["mses"]["enabled"] = serde_json::json!(false);

    let mut list = shots(advanced);
    if !filter.is_empty() && !advanced && filter != "narrow" {
        list.retain(|shot| shot.name.contains(&filter));
    }
    if advanced && filter.len() > "advanced".len() {
        let rest = filter
            .trim_start_matches("advanced")
            .trim_start_matches('_');
        let rest = rest.trim_start_matches("narrow").trim_start_matches('_');
        if !rest.is_empty() {
            list.retain(|shot| shot.name.contains(rest));
        }
    }
    if filter.contains("narrow") {
        for shot in &mut list {
            shot.name = shot.name.replacen("advanced_", "advanced_narrow_", 1);
        }
    }
    let (app, advanced_state) = if advanced {
        let mut advanced_state = AppState::default();
        advanced_state.load_preset(&preset);
        advanced_state.boot_frames_remaining = 0;
        (
            AlasApp::from_state(AppState::default()),
            Some(advanced_state),
        )
    } else {
        (AlasApp::from_state(state), None)
    };
    let tour = Tour {
        app,
        advanced,
        advanced_state,
        shots: list,
        index: 0,
        page: 0,
        wait: SETTLE_FRAMES,
        awaiting: false,
        pending: Vec::new(),
        previous: None,
        output,
        phase: Phase::Boot(10),
        needs_apply: false,
        screen: egui::Rect::from_min_size(Pos2::ZERO, size.into()),
        index_lines: Vec::new(),
    };
    let options = alas_gui::native_options("ALAS visual tour", size, size);
    eframe::run_native(
        "ALAS visual tour",
        options,
        Box::new(|cc| {
            alas_gui::apply_theme(AppTheme::Light, &cc.egui_ctx);
            if advanced {
                cc.egui_ctx.set_zoom_factor(ADVANCED_ZOOM);
            }
            Ok(Box::new(tour))
        }),
    )?;
    Ok(())
}
