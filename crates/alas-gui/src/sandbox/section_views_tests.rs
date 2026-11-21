// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The section editors draw the built geometry: AVE, A320-200 and A380-800.

use std::collections::HashSet;

use alas_report::families::geometry::SceneComponent;
use egui::{pos2, vec2, Rect};

use super::super::built_sections::{body_stations, lifting_sections};
use super::super::drag::{handles, HandleKind};
use super::super::fields::Discipline;
use super::super::scene::build_sandbox_airplane;
use super::super::session::SandboxDesign;
use super::{Frame, PlanView};
use crate::state::AppState;

const PRESETS: [&str; 3] = ["AVE", "A320-200", "A380-800"];

/// A sandbox holding the named preset's aircraft.
fn sandbox_with(name: &str) -> AppState {
    let mut state = AppState::default();
    if name != "AVE" {
        state.load_preset(name);
        state.sandbox.last_design = Some(SandboxDesign {
            config_values: state.config_values.clone(),
            design_values: state.design_values.clone(),
        });
        state.sandbox.resume_exact = true;
    }
    assert!(
        state.enter_sandbox(name == "AVE"),
        "{name}: the sandbox opens"
    );
    assert!(
        state.sandbox.airplane.is_some(),
        "{name}: the aircraft builds"
    );
    state
}

/// Quantised vertex key, so a point is found in a face list by value.
fn key(p: [f64; 3]) -> [i64; 3] {
    p.map(|c| (c * 1e6).round() as i64)
}

/// Every section the editor draws is a section of the aircraft the builder
/// lofts and the 3-D model draws, on every preset.
#[test]
fn the_editor_draws_the_sections_of_the_built_aircraft_on_every_preset() {
    for name in PRESETS {
        let state = sandbox_with(name);
        let plane = state.sandbox.airplane.as_ref().expect("plane");
        let model = state.sandbox.model.as_ref().expect("model");
        // An independent build of the committed configuration.
        let (rebuilt, _) = build_sandbox_airplane(&state).expect("rebuild");

        for component in [
            SceneComponent::Wing,
            SceneComponent::HorizontalTail,
            SceneComponent::VerticalTail,
        ] {
            let editor = lifting_sections(plane, component);
            let wing = rebuilt
                .wings
                .iter()
                .find(|w| SceneComponent::of_wing(w) == component)
                .unwrap_or_else(|| panic!("{name}: no {component:?} built"));
            assert_eq!(editor.len(), wing.xsecs.len(), "{name} {component:?}");
            let vertices: HashSet<[i64; 3]> = model
                .faces()
                .iter()
                .filter(|face| face.component == component)
                .flat_map(|face| face.points.iter().copied().map(key))
                .collect();
            for (section, xsec) in editor.iter().zip(&wing.xsecs) {
                assert_eq!(section.le, xsec.xyz_le, "{name} {component:?}");
                assert_eq!(section.chord, xsec.chord);
                assert_eq!(section.twist_deg, xsec.twist);
                assert!(
                    section.outline.iter().all(|p| vertices.contains(&key(*p))),
                    "{name} {component:?}: an editor outline point is not a vertex of the 3-D model"
                );
            }
        }

        for component in [SceneComponent::Fuselage, SceneComponent::Nacelles] {
            let editor = body_stations(plane, component);
            let built: Vec<_> = rebuilt
                .fuselages
                .iter()
                .enumerate()
                .filter(|(i, b)| SceneComponent::of_fuselage(*i, b) == component)
                .map(|(_, b)| b)
                .collect();
            assert!(!built.is_empty(), "{name}: no {component:?} built");
            assert_eq!(editor.len(), built.len(), "{name} {component:?}");
            for (stations, body) in editor.iter().zip(built) {
                assert_eq!(stations.len(), body.xsecs.len(), "{name} {component:?}");
                for (station, xsec) in stations.iter().zip(&body.xsecs) {
                    assert_eq!(station.centre, xsec.xyz_c);
                    assert_eq!(station.width, xsec.width);
                    assert_eq!(station.height, xsec.height);
                }
            }
        }
    }
}

/// The A380-800 fuselage is lofted from real sections, not a parametric
/// body; its editor stations differ from what the parametric generator would
/// give, so a generic drawing could not stand in for it.
#[test]
fn a_lofted_fuselage_shows_its_loft_stations() {
    let state = sandbox_with("A380-800");
    let config = state.typed_config().expect("config");
    assert!(
        config.geometry.fuselage.loft.is_some(),
        "the A380-800 fuselage is lofted"
    );
    let plane = state.sandbox.airplane.as_ref().expect("plane");
    let stations = &body_stations(plane, SceneComponent::Fuselage)[0];
    let widths: Vec<f64> = stations.iter().map(|s| s.width).collect();
    let max = widths.iter().copied().fold(f64::MIN, f64::max);
    let min = widths.iter().copied().fold(f64::MAX, f64::min);
    assert!(max > 6.0 && min < 1.0, "nose to cabin widths {min}..{max}");
    // Double-deck: the section centres are not on one horizontal line.
    let (zmin, zmax) = stations.iter().fold((f64::MAX, f64::MIN), |(lo, hi), s| {
        (lo.min(s.centre[2]), hi.max(s.centre[2]))
    });
    assert!(
        zmax - zmin > 0.1,
        "the loft has a nose droop and tail upsweep: {zmin}..{zmax}"
    );
}

/// A view is a uniform-scale orthographic map: a metre is the same number
/// of points along both axes, and the screen point inverts exactly.
#[test]
fn every_view_keeps_true_aspect_and_inverts() {
    let state = sandbox_with("A320-200");
    let model = state.sandbox.model.as_ref().expect("model");
    let points: Vec<[f64; 3]> = model
        .faces()
        .iter()
        .flat_map(|face| face.points.iter().copied())
        .collect();
    for view in [PlanView::Top, PlanView::Side, PlanView::Front] {
        let rect = Rect::from_min_size(pos2(30.0, 40.0), vec2(420.0, 180.0));
        let frame = Frame::fit(rect, view, &points).expect("frame");
        let back = frame.model_at(frame.to_screen([12.0, 0.0, 0.0]));
        let projected = frame.to_screen(back);
        assert!((projected - frame.to_screen([12.0, 0.0, 0.0])).length() < 1e-3);
        // Everything fits the rectangle.
        for p in &points {
            assert!(
                rect.expand(0.5).contains(frame.to_screen(*p)),
                "{view:?}: {p:?} outside"
            );
        }
        // Uniform scale: a 1 m step along each visible axis is `scale` points.
        let origin = frame.to_screen([0.0, 0.0, 0.0]);
        for axis in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
            let step = (frame.to_screen(axis) - origin).length();
            assert!(
                step < 1e-3 || (f64::from(step) - frame.scale).abs() < 1e-3,
                "{view:?}: axis {axis:?} stretched ({step} vs {})",
                frame.scale
            );
        }
    }
}

/// Dragging a handle in the plan view edits the same field as in 3-D: the
/// span grows when the tip handle moves outboard, and the edit is one undo
/// transaction.
#[test]
fn a_plan_view_handle_edits_the_span_through_the_undo_path() {
    let mut state = sandbox_with("AVE");
    let model = state.sandbox.model.as_ref().expect("model");
    let points: Vec<[f64; 3]> = model
        .faces()
        .iter()
        .filter(|face| face.component == SceneComponent::Wing)
        .flat_map(|face| face.points.iter().copied())
        .collect();
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 240.0));
    let frame = Frame::fit(rect, PlanView::Top, &points).expect("frame");
    let config = state.typed_config().expect("config");
    let design = state.current_design().expect("design");
    let span = handles(&config, &design, Some(Discipline::Wing))
        .into_iter()
        .find(|h| h.kind == HandleKind::Span)
        .expect("span handle");
    let before = state.design_values["span_m"];
    assert!(state.begin_handle_drag(&span, &frame.framing, 1.0));
    // Starboard (+y) is up in the plan view: a drag upward is outboard.
    state.update_handle_drag(vec2(0.0, -20.0));
    state.end_handle_drag();
    let after = state.design_values["span_m"];
    assert!(after > before, "span {before} -> {after}");
    assert!(state.sandbox_undo());
    assert!((state.design_values["span_m"] - before).abs() < 1e-9);
}

/// The review pictures: every discipline's editor on the three presets, at
/// the Discipline Window's default width. Written when the folder exists.
#[test]
fn render_the_section_editors_for_review() {
    for name in PRESETS {
        for discipline in Discipline::ALL {
            let mut state = sandbox_with(name);
            let shot = crate::test_raster::render(400.0, 1100.0, 3, 0, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    crate::theme::card_frame(ui).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        super::show_section_editor(&mut state, ui, discipline);
                    });
                });
            });
            assert!(shot.width > 0);
            crate::test_raster::write_in(
                &crate::test_raster::render_dir(),
                &format!(
                    "editor-{}-{}.png",
                    name.to_ascii_lowercase(),
                    discipline.id()
                ),
                &shot,
            );
        }
    }
}

/// Every built section shows its thickness in the editor, the fin included:
/// the strip projects onto the surface's own normal, not always onto z.
#[test]
fn every_airfoil_strip_shows_thickness_on_every_surface() {
    for name in PRESETS {
        let state = sandbox_with(name);
        let plane = state.sandbox.airplane.as_ref().expect("plane");
        for component in [
            SceneComponent::Wing,
            SceneComponent::HorizontalTail,
            SceneComponent::VerticalTail,
        ] {
            let sections = lifting_sections(plane, component);
            for (index, section) in sections.iter().enumerate() {
                let axis = super::thickness_axis(&sections, index);
                let (mut lo, mut hi) = (f64::MAX, f64::MIN);
                for p in &section.outline {
                    let v = (p[1] - section.le[1]) * axis[1] + (p[2] - section.le[2]) * axis[2];
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
                assert!(
                    (hi - lo) / section.chord > 0.04,
                    "{name} {component:?} section {index}: thickness {} of chord {}",
                    hi - lo,
                    section.chord
                );
            }
        }
    }
}

/// At the window size of the size-E review (676 x 495 points) the fin's
/// planform and its airfoil cells are both on screen, side by side: no text of
/// the editor lies below the window, and the cells' captions are drawn.
#[test]
fn the_fin_editor_shows_planform_and_sections_together_in_a_small_window() {
    let mut state = sandbox_with("AVE");
    let (w, h) = (676.0, 495.0);
    let shot = crate::test_raster::render(w, h, 3, 0, |ui| {
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            super::show_section_editor(&mut state, ui, Discipline::VerticalTail);
        });
    });
    let captions: Vec<_> = shot
        .texts
        .iter()
        .filter(|(t, _)| t.starts_with("y ") || t.starts_with("z "))
        .collect();
    assert!(
        shot.texts.iter().all(|(_, r)| r.right() <= w),
        "text clipped at the right edge: {:?}",
        shot.texts
    );
    assert!(captions.len() >= 2, "airfoil cells drawn: {:?}", shot.texts);
    for (text, rect) in &shot.texts {
        assert!(rect.bottom() <= h, "{text:?} below the window: {rect:?}");
    }
    // The planform view (a drawing) and the cells share the window width.
    assert!(
        captions.iter().all(|(_, r)| r.left() > w * 0.4),
        "cells sit in the right column"
    );
}
