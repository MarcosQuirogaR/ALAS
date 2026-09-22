// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless tests of the Wing Analysis window: opening, the revision gate,
//! a wing-only run with no mission or payload, the empennage option, and
//! cancellation.
//!
//! A fixture that cannot be built is a broken test rather than a library
//! failure, so these unwraps report where the fixture stopped being valid.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use egui::{Context, Pos2, RawInput, Rect};

use alas_aero::wing_analysis::SurfaceSet;

use super::wing_analysis::{reset_window, with_window, RunPhase, WingAnalysisTab};
use super::{open_wing_analysis, show_wing_analysis_window};
use crate::state::AppState;

/// The shell's declared minimum window, and a common large desktop window.
const NARROW: (f32, f32) = (880.0, 560.0);
const WIDE: (f32, f32) = (1548.0, 973.0);

/// Longest a headless test waits for one lattice solve.
const WORKER_TIMEOUT: Duration = Duration::from_secs(90);

/// Render one frame of the window on a screen of `size`.
fn frame(ctx: &Context, state: &mut AppState, size: (f32, f32)) -> egui::FullOutput {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_max(Pos2::ZERO, egui::pos2(size.0, size.1))),
        ..RawInput::default()
    };
    ctx.run(input, |ctx| show_wing_analysis_window(state, ctx))
}

/// Drive frames until the worker reports, or the timeout expires.
fn run_to_completion(ctx: &Context, state: &mut AppState) {
    let started = Instant::now();
    while started.elapsed() < WORKER_TIMEOUT {
        frame(ctx, state, WIDE);
        if !with_window(|window| window.running()) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the wing analysis worker did not report within the timeout");
}

/// A window opened on the default configuration, with its geometry snapshot
/// taken by one rendered frame.
fn opened_window(ctx: &Context) -> AppState {
    reset_window();
    let mut state = AppState::default();
    open_wing_analysis(ctx);
    frame(ctx, &mut state, WIDE);
    state
}

#[test]
fn the_menu_action_opens_the_window_on_its_setup_tab() {
    let ctx = Context::default();
    reset_window();
    assert!(!with_window(|window| window.window_open));

    open_wing_analysis(&ctx);

    assert!(with_window(|window| window.window_open));
    assert_eq!(with_window(|window| window.tab), WingAnalysisTab::Setup);
}

#[test]
fn opening_the_window_snapshots_the_wing_and_previews_it() {
    let ctx = Context::default();
    let _state = opened_window(&ctx);

    with_window(|window| {
        assert!(window.model.is_some(), "{:?}", window.geometry_error);
        assert_eq!(window.surface_names(), vec!["Main Wing".to_owned()]);
        assert!(window.preview.is_some(), "the live preview must be built");
        let reference = window.reference().unwrap();
        assert!(reference.area_m2 > 0.0 && reference.span_m > 0.0);
        assert!(window.inputs.moment_reference_m[0].is_finite());
    });
}

#[test]
fn a_wing_only_run_needs_no_mission_or_payload_and_reports_a_lift_distribution() {
    let ctx = Context::default();
    let mut state = opened_window(&ctx);
    with_window(|window| {
        window.inputs.sweep.points = 3;
        window.start().expect("a wing-only run starts");
    });

    run_to_completion(&ctx, &mut state);

    with_window(|window| {
        assert_eq!(window.phase, RunPhase::Finished);
        assert!(window.result_is_current());
        let outcome = window.result.as_ref().unwrap();
        assert!(outcome.cl > 0.0, "a positive angle must lift");
        assert!(outcome.cd_induced > 0.0);
        assert!(
            outcome.span_load.len() > 8,
            "a span distribution is reported"
        );
        assert_eq!(outcome.sweep.len(), 3);
        assert!(outcome.stability.is_none(), "no empennage, no stability");
        assert_eq!(outcome.modelled_surfaces.len(), 1);
    });
}

#[test]
fn the_empennage_option_changes_the_preview_the_surfaces_and_the_outputs() {
    let ctx = Context::default();
    let mut state = opened_window(&ctx);
    with_window(|window| {
        window.inputs.sweep.points = 2;
        window.start().expect("a wing-only run starts");
    });
    run_to_completion(&ctx, &mut state);
    let (wing_only_revision, wing_only_preview) = with_window(|window| {
        (
            window.geometry_revision,
            window.preview.as_ref().unwrap().elements.len(),
        )
    });

    with_window(|window| window.set_surfaces(SurfaceSet::WingAndEmpennage));
    frame(&ctx, &mut state, WIDE);

    with_window(|window| {
        assert!(window.geometry_revision > wing_only_revision);
        assert!(window.result.is_none(), "the earlier result is invalidated");
        assert_eq!(
            window.surface_names(),
            vec![
                "Main Wing".to_owned(),
                "Horizontal Stabilizer".to_owned(),
                "Vertical Stabilizer".to_owned()
            ]
        );
        let preview = window.preview.as_ref().unwrap().elements.len();
        assert!(
            preview > wing_only_preview,
            "the preview must draw the added surfaces: {preview} against {wing_only_preview}"
        );
        window.start().expect("a wing-empennage run starts");
    });
    run_to_completion(&ctx, &mut state);

    with_window(|window| {
        let outcome = window.result.as_ref().unwrap();
        assert_eq!(outcome.modelled_surfaces.len(), 3);
        let stability = outcome
            .stability
            .expect("the empennage enables static stability");
        assert!(stability.cl_alpha_per_rad > 0.0);
        assert!(stability.static_margin.is_finite());
    });
}

#[test]
fn a_result_for_older_inputs_is_discarded_instead_of_installed() {
    let ctx = Context::default();
    let mut state = opened_window(&ctx);
    with_window(|window| {
        window.inputs.sweep.points = 2;
        window.start().expect("a run starts");
        // The edit lands while the worker is solving: its revision is now
        // older than the window's, which is exactly the race the gate covers.
        window.inputs.condition.altitude_m = 6_000.0;
        window.invalidate_inputs();
    });

    run_to_completion(&ctx, &mut state);

    with_window(|window| {
        assert!(
            window.result.is_none(),
            "a late result must not be installed"
        );
        assert!(!window.result_is_current());
        assert_eq!(window.phase, RunPhase::Idle);
        assert!(window.status.contains("discarded"), "{}", window.status);
    });
}

#[test]
fn an_input_edit_drops_an_installed_result_rather_than_relabelling_it() {
    let ctx = Context::default();
    let mut state = opened_window(&ctx);
    with_window(|window| {
        window.inputs.sweep.points = 2;
        window.start().expect("a run starts");
    });
    run_to_completion(&ctx, &mut state);
    assert!(with_window(|window| window.result_is_current()));

    with_window(|window| {
        window.inputs.condition.altitude_m = 4_000.0;
        window.invalidate_inputs();
        assert!(window.result.is_none());
        assert!(!window.result_is_current());
    });
}

#[test]
fn cancellation_stops_the_run_without_installing_a_result() {
    let ctx = Context::default();
    let mut state = opened_window(&ctx);
    with_window(|window| {
        window.inputs.sweep.points = 41;
        window.inputs.chordwise_resolution = 12;
        window.start().expect("a run starts");
        window.cancel();
        assert_eq!(window.phase, RunPhase::Cancelling);
    });

    run_to_completion(&ctx, &mut state);

    with_window(|window| {
        assert!(window.result.is_none());
        assert_eq!(window.phase, RunPhase::Idle);
        assert!(window.error.is_none(), "a cancellation is not a failure");
        assert!(window.status.contains("cancelled"), "{}", window.status);
    });
}

#[test]
fn invalid_inputs_are_refused_before_a_worker_is_started() {
    let ctx = Context::default();
    let _state = opened_window(&ctx);

    with_window(|window| {
        window.inputs.sweep.min_deg = 10.0;
        window.inputs.sweep.max_deg = -10.0;
        let error = window.start().expect_err("an invalid sweep cannot run");
        assert!(error.contains("alpha sweep"), "{error}");
        assert_eq!(window.phase, RunPhase::Failed);
        assert!(window.result.is_none());
    });
}

/// Every painted string whose glyphs fall outside the clip rectangle they
/// were painted under: what a hard-clipped label looks like in shapes.
fn clipped_text(output: &egui::FullOutput) -> Vec<String> {
    fn walk(shape: &egui::Shape, clip: Rect, found: &mut Vec<String>) {
        match shape {
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, clip, found);
                }
            }
            egui::Shape::Text(text) => {
                let rect = text.galley.rect.translate(text.pos.to_vec2());
                if rect.right() > clip.right() + 0.5 || rect.left() < clip.left() - 0.5 {
                    found.push(text.galley.text().trim().to_owned());
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for primitive in &output.shapes {
        walk(&primitive.shape, primitive.clip_rect, &mut found);
    }
    found
}

#[test]
fn both_tabs_stay_inside_their_clip_rectangles_at_the_supported_window_sizes() {
    let ctx = Context::default();
    let mut state = opened_window(&ctx);
    with_window(|window| {
        window.inputs.sweep.points = 2;
        window.start().expect("a run starts");
    });
    run_to_completion(&ctx, &mut state);

    for size in [NARROW, WIDE] {
        for tab in [WingAnalysisTab::Setup, WingAnalysisTab::Results] {
            with_window(|window| window.tab = tab);
            // The first frame lays the viewport out; the second paints with
            // the settled geometry, which is what a user sees.
            frame(&ctx, &mut state, size);
            let output = frame(&ctx, &mut state, size);
            let clipped = clipped_text(&output);
            assert!(
                clipped.is_empty(),
                "{tab:?} at {size:?} clipped: {clipped:?}"
            );
        }
    }
}

#[test]
fn the_window_renders_under_every_theme_without_losing_its_content() {
    let ctx = Context::default();
    let mut state = opened_window(&ctx);

    for theme in [
        crate::theme::AppTheme::Dark,
        crate::theme::AppTheme::Light,
        crate::theme::AppTheme::Grey,
    ] {
        state.theme = theme;
        crate::theme::apply_theme(theme, &ctx);
        frame(&ctx, &mut state, NARROW);
        let output = frame(&ctx, &mut state, NARROW);
        assert!(
            !output.shapes.is_empty(),
            "{theme:?} produced an empty window"
        );
        let expected = alas_report::theme::get_palette(Some(theme.figure_theme_name()));
        with_window(|window| {
            let preview = window.preview.as_ref().expect("a preview scene");
            assert_eq!(
                preview.background,
                Some(alas_report::scene::Color::from_hex(expected.bg)),
                "{theme:?} preview keeps its own canvas"
            );
        });
    }
}

/// The files this window is built from, relative to the crate source root.
const WINDOW_SOURCES: [&str; 7] = [
    "wing_analysis.rs",
    "wing_analysis_worker.rs",
    "views/wing_analysis_view.rs",
    "views/wing_analysis_view_setup.rs",
    "views/wing_analysis_view_results.rs",
    "views/wing_analysis_view_draw.rs",
    "views/wing_analysis_view_tests.rs",
];

/// Every string literal this file sends through `tr` or `tr_fields`.
///
/// The crate-wide contract lives in `tests/i18n_gui.rs`; this one is scoped to
/// the Wing Analysis window so its catalogue coverage stays checkable while
/// other surfaces are edited.
fn translated_literals(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    for function in ["tr", "tr_fields"] {
        let marker = format!("{function}(");
        let mut cursor = 0;
        while let Some(relative) = source[cursor..].find(&marker) {
            let start = cursor + relative;
            let preceded = start > 0 && {
                let byte = source.as_bytes()[start - 1];
                byte.is_ascii_alphanumeric() || byte == b'_'
            };
            let mut index = start + marker.len();
            while source
                .as_bytes()
                .get(index)
                .is_some_and(u8::is_ascii_whitespace)
            {
                index += 1;
            }
            if preceded || source.as_bytes().get(index) != Some(&b'"') {
                cursor = index.max(start + marker.len());
                continue;
            }
            index += 1;
            let mut value = String::new();
            while let Some(&byte) = source.as_bytes().get(index) {
                if byte == b'"' {
                    found.push(value);
                    index += 1;
                    break;
                }
                if byte != b'\x5c' {
                    value.push(byte as char);
                    index += 1;
                    continue;
                }
                index += 1;
                match source.as_bytes().get(index).copied() {
                    Some(b'n') => value.push('\n'),
                    Some(b'"') => value.push('"'),
                    Some(b'\x5c') => value.push('\x5c'),
                    // A backslash before the line break continues the literal
                    // on the next line; the break is LF or CRLF.
                    Some(b'\n') | Some(b'\r') => {
                        while source
                            .as_bytes()
                            .get(index)
                            .is_some_and(u8::is_ascii_whitespace)
                        {
                            index += 1;
                        }
                        continue;
                    }
                    Some(other) => value.push(other as char),
                    None => break,
                }
                index += 1;
            }
            cursor = index;
        }
    }
    found
}

#[test]
fn every_wing_analysis_string_has_spanish_catalog_provenance() {
    let base = alas_i18n::es::catalog();
    let desktop = alas_i18n::es::desktop_catalog();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut checked = 0;
    for name in WINDOW_SOURCES {
        let path = root.join(name);
        let source = std::fs::read_to_string(&path).expect("window source");
        for key in translated_literals(&source) {
            assert!(
                base.contains_key(&key) || desktop.contains_key(&key),
                "{name} sends an uncatalogued literal: {key:?}"
            );
            checked += 1;
        }
    }
    assert!(checked > 60, "only {checked} literals were checked");
}
