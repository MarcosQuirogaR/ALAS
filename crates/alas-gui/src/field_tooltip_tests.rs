// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Field help tooltips on the live Aircraft page, driven the way the running
//! application is: frames come only when egui asks for them (the repaint
//! delay of the previous frame), the page sits in a scroll area, and the
//! pointer is placed once and then left alone.

use crate::state::AppState;

fn find_text(output: &egui::FullOutput, wanted: &str) -> Option<egui::Rect> {
    fn visit(shape: &egui::Shape, wanted: &str) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| visit(s, wanted)),
            egui::Shape::Text(text) if text.galley.text().contains(wanted) => {
                Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
            }
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|clipped| visit(&clipped.shape, wanted))
}

struct Live {
    ctx: egui::Context,
    app: crate::app::AlasApp,
    time: f64,
    next_due: f64,
}

impl Live {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("alas-tooltip-{}", std::process::id()));
        let mut state = AppState {
            tool_locator: alas_exec::ToolLocator::new(root.join("app"), root.join("data")),
            ..AppState::default()
        };
        state.finish_walkthrough();
        state.boot_frames_remaining = 0;
        state.load_preset("A320-200");
        state.reset_undo_baseline();
        let ctx = egui::Context::default();
        Self {
            ctx,
            app: crate::app::AlasApp::from_state(state),
            time: 1.0,
            next_due: 0.0,
        }
    }

    /// One frame at the current time with `events`.
    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let Self { ctx, app, time, .. } = self;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 900.0),
                )),
                time: Some(*time),
                events,
                ..Default::default()
            },
            |ctx| app.show_frame(ctx),
        );
        let delay = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        self.next_due = self.time + delay.as_secs_f64().min(3600.0);
        output
    }

    /// Let `seconds` pass with no input, running a frame only when egui asked
    /// for one (reactive mode, as the application runs).
    fn idle(&mut self, seconds: f64) -> egui::FullOutput {
        let end = self.time + seconds;
        let mut last = self.frame(vec![]);
        while self.next_due <= end {
            self.time = self.next_due.max(self.time + 1e-3);
            last = self.frame(vec![]);
        }
        self.time = end;
        last
    }
}

fn hover_shows_docs(target: impl Fn(egui::Rect) -> egui::Pos2) -> bool {
    let mut live = Live::new();
    let mut output = live.idle(0.3);
    let label = find_text(&output, "Cruise Mach number").expect("the label is painted");
    let at = target(label);
    live.time += 0.1;
    live.frame(vec![egui::Event::PointerMoved(at)]);
    output = live.idle(3.0);
    find_text(&output, "Docs").is_some()
}

#[test]
fn resting_on_the_value_box_of_a_numeric_field_shows_its_tooltip_in_reactive_mode() {
    assert!(hover_shows_docs(
        |label| label.left_bottom() + egui::vec2(30.0, 18.0)
    ));
}

#[test]
fn resting_on_the_label_of_a_numeric_field_shows_its_tooltip_in_reactive_mode() {
    assert!(hover_shows_docs(|label| label.center()));
}

/// egui hovers only what is at or above the topmost interactive widget under
/// the pointer. A click-and-drag widget registered after the form over the
/// whole cell (a drag-to-scroll background, a focus card) took the hover from
/// the cell's help overlay, and the tooltip never opened. The help follows the
/// pointer, not that order.
#[test]
fn a_later_interactive_widget_over_the_cell_does_not_take_the_tooltip_away() {
    let state = AppState::default();
    let fields = match &state
        .schema
        .field("requirements")
        .expect("requirements")
        .entry
    {
        alas_config::Entry::Node(node) => node.fields.clone(),
        alas_config::Entry::Leaf(_) => panic!("requirements is a group"),
    };
    let mut values = serde_json::to_value(alas_config::AlasConfig::default())
        .expect("config")
        .get("requirements")
        .cloned()
        .expect("requirements values");
    let ctx = egui::Context::default();
    crate::theme::apply_theme(crate::theme::AppTheme::Dark, &ctx);
    let mut time = 1.0;
    let mut run = |events: Vec<egui::Event>, time: f64| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(780.0, 900.0),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    crate::views::form::dynamic_form(
                        ui,
                        &fields,
                        &mut values,
                        &Default::default(),
                        Some("en"),
                        false,
                    );
                    // Registered after every field, over all of them.
                    let all = ui.min_rect();
                    ui.interact(all, egui::Id::new("thief"), egui::Sense::click_and_drag());
                });
            },
        )
    };
    let mut output = run(vec![], time);
    for _ in 0..2 {
        time += 0.05;
        output = run(vec![], time);
    }
    let label = find_text(&output, "Cruise Mach number").expect("label");
    time += 0.05;
    run(vec![egui::Event::PointerMoved(label.center())], time);
    for _ in 0..12 {
        time += 0.05;
        output = run(vec![], time);
    }
    assert!(find_text(&output, "Docs").is_some(), "the tooltip opens");
}

/// The orbit hint of the 3-D preview dock belongs to a response that spans the
/// whole dock, down to the bottom edge of the window. egui places a widget
/// tooltip under (or above, or beside) the widget rectangle, so the hint was
/// drawn in the bottom-right corner of the window wherever the pointer rested
/// in the dock. It must open at the pointer.
#[test]
fn the_preview_dock_hint_opens_at_the_pointer_not_in_the_window_corner() {
    let mut live = Live::new();
    live.idle(0.5);
    for at in [
        egui::pos2(1100.0, 400.0),
        egui::pos2(1200.0, 850.0),
        egui::pos2(1100.0, 120.0),
    ] {
        live.time += 0.1;
        live.frame(vec![egui::Event::PointerMoved(at)]);
        let output = live.idle(2.0);
        let hint = find_text(&output, "Left drag: orbit").expect("the hint opens over the dock");
        assert!(
            hint.min.y <= at.y + 40.0 && hint.max.y >= at.y - 60.0,
            "pointer {at:?}: hint {hint:?} is not at the pointer"
        );
        assert!(
            hint.min.x <= at.x + 40.0 && hint.max.x >= at.x - 40.0,
            "pointer {at:?}: hint {hint:?} is not at the pointer"
        );
    }
}
