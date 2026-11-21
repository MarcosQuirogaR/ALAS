// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Capture the guided shell page by page without running the pipeline, for the
//! v1.5 GUI rebuild review: every Design-flow and Workspace page in the Dark,
//! Light and Grey themes at the Basic and Advanced detail levels.
//!
//! Usage:
//!   cargo run --release -p alas-gui --example render_shell_shots -- \
//!       <output directory> [preset id] [page filter] [theme filter]
//!
//! `ALAS_TOUR_SIZE=WxH` sets the window's inner size in points; 1280x720 and
//! 1280x800 are 1920x1080 and 1920x1200 at 150 % display scaling. `build.rs`
//! enlarges the main-thread stack of the examples at link time.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

#[path = "render_shell_shots/fixtures.rs"]
mod fixtures;

use alas_gui::detail::Detail;
use alas_gui::{AlasApp, AppState, AppTheme};
use egui::Event;

/// A state to put the shell in before the capture, beyond the page.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Extra {
    None,
    SearchOpen,
    SearchJump,
    CleanSheet,
    FilesSaved,
    RunLog,
    /// A results figure opened in its own (here embedded) window.
    DetachedFigure,
    /// The 3D preview opened in its own (here embedded) window.
    DetachedPreview,
    Spanish,

    MissionPlanned,

    PayloadPlaced,
}

#[derive(Clone)]
struct Shot {
    name: String,
    theme: AppTheme,
    detail: Detail,
    page: &'static str,
    extra: Extra,
    /// The results tab to open, for the shots taken after a run.
    tab: Option<&'static str>,
}

const SETTLE_FRAMES: u32 = 20;

/// The pages captured, and whether the Advanced level differs from Basic.
const FLOW_PAGES: [(&str, bool); 8] = [
    ("inputs", true),
    ("cabin", false),
    ("mission", true),
    ("payload", false),
    ("setup_analyses", true),
    ("setup_tools", false),
    ("files", false),
    ("results", false),
];

struct Shots {
    app: AlasApp,
    shots: Vec<Shot>,
    index: usize,
    wait: u32,
    awaiting: bool,
    boot: u32,
    output: PathBuf,
    applied: bool,
    pending: Vec<Event>,
    /// Events to deliver after that many more frames.
    delayed: Vec<(u32, Vec<Event>)>,
    screen: egui::Rect,
    /// Whether the run the shots follow has been started / is still owed.
    run: RunPhase,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RunPhase {
    NotWanted,
    Owed,
    Started,
}

impl Shots {
    fn apply(&mut self, ctx: &egui::Context) {
        let shot = self.shots[self.index].clone();
        let state = self.app.state_mut();
        if state.theme != shot.theme {
            state.theme = shot.theme;
            alas_gui::apply_theme(shot.theme, ctx);
        }
        state.shell.detail = shot.detail;
        alas_gui::app::go_to_page(state, shot.page);
        state.shell.search.open = false;
        state.run_log_open = false;
        if let Some(tab) = shot.tab {
            state.results_tab = tab.to_owned();
        }
        state.language = alas_gui::state::Language::En;
        alas_i18n::set_language(Some("en"));
        state.shell.start_mode = alas_gui::views::aircraft_start::StartMode::Reference;
        match shot.extra {
            Extra::None => {}
            Extra::SearchOpen => {
                state.shell.search.open = true;
                state.shell.search.query = "airfoil".to_owned();
            }
            Extra::SearchJump => {
                let index = alas_gui::search::build_index(&state.schema);
                if let Some(entry) = index
                    .iter()
                    .find(|entry| entry.field.as_deref() == Some("geometry.wing.root_airfoil"))
                {
                    alas_gui::search::jump_to(state, entry);
                }
            }
            Extra::RunLog => state.run_log_open = true,
            Extra::DetachedFigure | Extra::DetachedPreview => {}
            Extra::Spanish => {
                state.language = alas_gui::state::Language::Es;
                alas_i18n::set_language(Some("es"));
            }
            Extra::MissionPlanned => fixtures::mission_plan(state),
            Extra::PayloadPlaced => fixtures::payload_items(state),
            Extra::CleanSheet => {
                state.shell.start_mode = alas_gui::views::aircraft_start::StartMode::CleanSheet;
            }
            Extra::FilesSaved => {
                for kind in [
                    alas_gui::file_library::ObjectKind::Aircraft,
                    alas_gui::file_library::ObjectKind::MissionPlan,
                    alas_gui::file_library::ObjectKind::PayloadLayout,
                    alas_gui::file_library::ObjectKind::Engine,
                ] {
                    let name = state.library_default_name(kind);
                    state.library_save(kind, &name, "");
                }
                state.shell.library.invalidate();
            }
        }
        // A native child window cannot be captured; embedded, it is drawn in the root.
        ctx.set_embed_viewports(matches!(
            shot.extra,
            Extra::DetachedFigure | Extra::DetachedPreview
        ));
        let double_click_at = match shot.extra {
            Extra::DetachedFigure => Some(egui::pos2(self.screen.width() * 0.45, 330.0)),
            Extra::DetachedPreview => Some(egui::pos2(self.screen.width() * 0.86, 380.0)),
            _ => None,
        };
        // The pointer arrives a few frames after the page settles, then the two
        // clicks in one frame (a double click), so the hit test uses laid-out widgets.
        self.wait = SETTLE_FRAMES;
        if let Some(pos) = double_click_at {
            let click = |pressed| Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.delayed.push((12, vec![Event::PointerMoved(pos)]));
            self.delayed.push((
                18,
                vec![click(true), click(false), click(true), click(false)],
            ));
            self.wait = 60;
        }
        state.update_preview_scene();
        self.applied = true;
        // The Advanced level adds sections below the routine cards: show them.
        if shot.extra != Extra::SearchJump {
            let down = shot.detail == Detail::Advanced && shot.extra == Extra::None;
            let pointer = egui::pos2(self.screen.width() * 0.4, self.screen.height() * 0.5);
            self.pending.push(Event::PointerMoved(pointer));
            self.pending.push(Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, if down { -4000.0 } else { 40_000.0 }),
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
}

impl eframe::App for Shots {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        let screenshot = raw_input.events.iter().find_map(|event| match event {
            Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        });
        raw_input.events.retain(|event| {
            !matches!(
                event,
                Event::PointerMoved(_) | Event::MouseWheel { .. } | Event::PointerGone
            )
        });
        raw_input.events.append(&mut self.pending);
        for (frames, events) in &mut self.delayed {
            if *frames == 0 {
                raw_input.events.append(events);
            } else {
                *frames -= 1;
            }
        }
        self.delayed.retain(|(_, events)| !events.is_empty());
        if let Some(image) = screenshot {
            if self.awaiting {
                self.awaiting = false;
                let shot = &self.shots[self.index];
                let pixels: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                let [width, height] = image.size;
                match alas_viz::raster::encode_png_rgba(width as u32, height as u32, &pixels) {
                    Ok(png) => {
                        let path = self.output.join(format!("{}.png", shot.name));
                        if let Err(error) = std::fs::write(&path, png) {
                            eprintln!("write {}: {error}", path.display());
                        }
                        println!("wrote {} ({width}x{height})", shot.name);
                    }
                    Err(error) => eprintln!("encode {}: {error}", shot.name),
                }
                self.index += 1;
                self.applied = false;
            }
        }
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ctx.request_repaint();
        self.screen = ctx.screen_rect();
        eframe::App::update(&mut self.app, ctx, frame);
        if self.boot > 0 {
            self.boot -= 1;
            return;
        }
        match self.run {
            RunPhase::NotWanted => {}
            RunPhase::Owed => {
                self.app.state_mut().start_pipeline(false);
                self.run = RunPhase::Started;
                return;
            }
            RunPhase::Started => {
                if self.app.state_mut().is_running {
                    return;
                }
                let state = self.app.state_mut();
                if !state.pipeline_result_complete {
                    eprintln!("pipeline did not complete: {}", state.status_message);
                }
                self.run = RunPhase::NotWanted;
            }
        }
        if self.awaiting {
            return;
        }
        if self.index >= self.shots.len() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if !self.applied {
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
}

fn size_override() -> Option<[f32; 2]> {
    let text = std::env::var("ALAS_TOUR_SIZE").ok()?;
    let (width, height) = text.split_once(['x', 'X'])?;
    let size = [
        width.trim().parse::<f32>().ok()?,
        height.trim().parse::<f32>().ok()?,
    ];
    size.iter()
        .all(|v| v.is_finite() && *v >= 320.0)
        .then_some(size)
}

// AppState owns private caches, so it is initialized through Default.
#[allow(clippy::field_reassign_with_default)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(args.next().unwrap_or_else(|| {
        std::env::temp_dir()
            .join("alas-shots/shell")
            .to_string_lossy()
            .into_owned()
    }));
    let preset = args.next().unwrap_or_else(|| "A220-300".to_owned());
    let page_filter = args.next().unwrap_or_default();
    let theme_filter = args.next().unwrap_or_default();
    std::fs::create_dir_all(&output)?;
    let size = size_override().unwrap_or([1280.0, 720.0]);

    alas_i18n::es::install();
    let mut state = AppState::default();
    state.boot_frames_remaining = 0;
    state.tool_locator = alas_exec::ToolLocator::new(output.join("app"), output.join("data"));
    state.load_preset(&preset);
    // `ALAS_SHOTS_RUN=<screening>,<refinement>` runs one seeded search first and
    // captures the populated Results tabs and the run log instead of the pages.
    let run_budget: Option<(i64, i64)> = std::env::var("ALAS_SHOTS_RUN").ok().and_then(|text| {
        let (screening, refinement) = text.split_once(',')?;
        Some((
            screening.trim().parse().ok()?,
            refinement.trim().parse().ok()?,
        ))
    });
    if let Some((screening, refinement)) = run_budget {
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
    }

    let mut shots = Vec::new();
    let mut late = Vec::new();
    for (theme, theme_name) in [
        (AppTheme::Dark, "dark"),
        (AppTheme::Light, "light"),
        (AppTheme::Grey, "grey"),
    ] {
        if !theme_filter.is_empty() && !theme_name.contains(&theme_filter) {
            continue;
        }
        if run_budget.is_some() {
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
                shots.push(Shot {
                    name: format!("results_{tab}_{theme_name}"),
                    theme,
                    detail: Detail::Basic,
                    page: "results",
                    extra: Extra::None,
                    tab: Some(tab),
                });
            }
            // Last: the embedded windows stay open for whatever follows.
            for (name, tab, extra) in [
                ("detached_figure", "aero", Extra::DetachedFigure),
                ("detached_preview", "summary", Extra::DetachedPreview),
            ] {
                if theme_name != "dark" {
                    continue;
                }
                late.push(Shot {
                    name: format!("{name}_{theme_name}"),
                    theme,
                    detail: Detail::Basic,
                    page: "results",
                    extra,
                    tab: Some(tab),
                });
            }
            shots.push(Shot {
                name: format!("runlog_after_run_{theme_name}"),
                theme,
                detail: Detail::Basic,
                page: "results",
                extra: Extra::RunLog,
                tab: Some("summary"),
            });
            continue;
        }
        for (detail, detail_name) in [(Detail::Basic, "basic"), (Detail::Advanced, "advanced")] {
            for (page, advanced_too) in FLOW_PAGES {
                if detail == Detail::Advanced && !advanced_too {
                    continue;
                }
                if !page.contains(&page_filter) {
                    continue;
                }
                shots.push(Shot {
                    name: format!("{page}_{theme_name}_{detail_name}"),
                    theme,
                    detail,
                    page,
                    extra: Extra::None,
                    tab: None,
                });
            }
        }
        if page_filter.is_empty() {
            for (suffix, page, extra, detail) in [
                ("search", "inputs", Extra::SearchOpen, Detail::Basic),
                ("jump", "inputs", Extra::SearchJump, Detail::Advanced),
                ("cleansheet", "inputs", Extra::CleanSheet, Detail::Basic),
                ("filessaved", "files", Extra::FilesSaved, Detail::Basic),
                ("runlog", "inputs", Extra::RunLog, Detail::Basic),
                (
                    "mission_plan",
                    "mission",
                    Extra::MissionPlanned,
                    Detail::Basic,
                ),
                (
                    "payload_items",
                    "payload",
                    Extra::PayloadPlaced,
                    Detail::Basic,
                ),
                ("spanish_aircraft", "inputs", Extra::Spanish, Detail::Basic),
                (
                    "spanish_analyses",
                    "setup_analyses",
                    Extra::Spanish,
                    Detail::Advanced,
                ),
            ] {
                shots.push(Shot {
                    name: format!("{suffix}_{theme_name}"),
                    theme,
                    detail,
                    page,
                    extra,
                    tab: None,
                });
            }
        }
    }

    shots.append(&mut late);

    let options = alas_gui::native_options("ALAS shell shots", size, size);
    eframe::run_native(
        "ALAS shell shots",
        options,
        Box::new(move |cc| {
            alas_gui::apply_theme(AppTheme::Dark, &cc.egui_ctx);
            Ok(Box::new(Shots {
                app: AlasApp::from_state(state),
                shots,
                index: 0,
                wait: 0,
                awaiting: false,
                boot: 10,
                output,
                applied: false,
                pending: Vec::new(),
                delayed: Vec::new(),
                screen: egui::Rect::from_min_size(egui::Pos2::ZERO, size.into()),
                run: if run_budget.is_some() {
                    RunPhase::Owed
                } else {
                    RunPhase::NotWanted
                },
            }))
        }),
    )?;
    Ok(())
}
