// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A search jump reaches the controls the bespoke editors draw themselves.
//!
//! The generic form honours the jump by itself. A bespoke editor (one that
//! draws its own controls instead of calling the generic form) must call
//! `search::focus_anchor` for the control and open its card with
//! `search::focus_wants`. These tests keep that true:
//!
//! 1. every indexed field of the pages that host a bespoke editor belongs to a
//!    renderer in [`RENDERERS`], so no entry can land on a page that draws it
//!    nowhere;
//! 2. the source of every renderer still calls the hook the registry names;
//! 3. `focus_anchor` pulses the searched path and no other.

use super::{build_index, focus_anchor, publish_focus, Focus};
use crate::views::form_page::mesh_resolution::MESH_FIELDS;

/// (file, its source, the calls each indexed path of it relies on).
const RENDERERS: &[(&str, &str, &[&str])] = &[
    (
        "views/inputs_view.rs",
        include_str!("views/inputs_view.rs"),
        &[
            "anchor_span(ui, key",
            "\"requirements\"",
            "\"departure_airport\"",
        ],
    ),
    (
        "views/route_override.rs",
        include_str!("views/route_override.rs"),
        &["focus_anchor(ui, key"],
    ),
    (
        "views/inputs_mtow.rs",
        include_str!("views/inputs_mtow.rs"),
        &["\"requirements.mtow_kg\""],
    ),
    (
        "views/mission_stage_editor.rs",
        include_str!("views/mission_stage_editor.rs"),
        &["\"mission.profile\"", "mission_phase_focus::focused"],
    ),
    (
        "views/mission_stage_editor/list.rs",
        include_str!("views/mission_stage_editor/list.rs"),
        &["focus_anchor(ui, &focus.path"],
    ),
    (
        "views/mission_profile_inputs.rs",
        include_str!("views/mission_profile_inputs.rs"),
        &["\"::profile\""],
    ),
    (
        "views/form_page/cabin_editor.rs",
        include_str!("views/form_page/cabin_editor.rs"),
        &["focus_wants(", "focus_anchor(", "dynamic_form_at_help"],
    ),
    (
        "views/form_page/mesh_resolution.rs",
        include_str!("views/form_page/mesh_resolution.rs"),
        &["focus_wants(", "MESH_FIELDS"],
    ),
];

/// The renderer that draws the control of an indexed `path` on `page` (with
/// no Advanced section), when a bespoke editor does.
fn renderer(page: &str, path: &str) -> Option<&'static str> {
    match (page, path) {
        ("inputs", "departure_airport" | "arrival_airport") => Some("views/inputs_view.rs"),
        ("inputs", "toda_override_m" | "lda_override_m") => Some("views/route_override.rs"),
        ("inputs", "requirements.mtow_kg") => Some("views/inputs_mtow.rs"),
        ("inputs", p) if p.starts_with("requirements.") => Some("views/inputs_view.rs"),
        ("mission", p) if p.starts_with("mission.profile.") => {
            Some("views/mission_stage_editor.rs")
        }
        ("cabin", p) if p.starts_with("cabin.") => Some("views/form_page/cabin_editor.rs"),
        _ => None,
    }
}

#[test]
fn every_indexed_field_of_a_bespoke_page_has_a_renderer() {
    let state = crate::state::AppState::default();
    let mut orphans = Vec::new();
    for entry in build_index(&state.schema) {
        let (Some(path), None) = (&entry.field, entry.section) else {
            continue;
        };
        let bespoke_page = matches!(entry.page, "inputs" | "mission" | "cabin");
        if bespoke_page && renderer(entry.page, path).is_none() {
            orphans.push(format!("{} -> {path}", entry.page));
        }
    }
    assert!(
        orphans.is_empty(),
        "indexed on a page whose editors do not draw them: {orphans:?}"
    );
}

#[test]
fn the_mesh_resolution_fields_are_searchable_on_the_analysis_fidelity_page() {
    let state = crate::state::AppState::default();
    let index = build_index(&state.schema);
    for (_, prefix, name) in MESH_FIELDS {
        let path = format!("{prefix}.{name}");
        let entry = index
            .iter()
            .find(|entry| entry.field.as_deref() == Some(path.as_str()))
            .unwrap_or_else(|| panic!("{path} is not indexed"));
        assert_eq!(
            (entry.page, entry.section),
            ("setup_analyses", Some("Analysis Fidelity")),
            "{path}"
        );
    }
}

#[test]
fn the_mission_group_outside_the_phase_editors_lands_in_its_advanced_section() {
    let state = crate::state::AppState::default();
    let index = build_index(&state.schema);
    let landing = |path: &str| {
        index
            .iter()
            .find(|entry| entry.field.as_deref() == Some(path))
            .map(|entry| (entry.page, entry.section))
    };
    assert_eq!(
        landing("mission.profile.landing_air_speed_m_s"),
        Some(("mission", None))
    );
}

#[test]
fn every_renderer_still_calls_the_hook_the_registry_names() {
    for (file, source, needles) in RENDERERS {
        for needle in *needles {
            assert!(source.contains(needle), "{file} lost `{needle}`");
        }
    }
    // The registry names every renderer `renderer` can return.
    for (page, path) in [
        ("inputs", "departure_airport"),
        ("inputs", "toda_override_m"),
        ("inputs", "requirements.mtow_kg"),
        ("inputs", "requirements.cruise_mach"),
        ("mission", "mission.profile.climb_mach"),
        ("cabin", "cabin.passenger.first.pitch_m"),
    ] {
        let file = renderer(page, path).expect("a renderer");
        assert!(RENDERERS.iter().any(|(name, _, _)| *name == file), "{file}");
    }
}

/// How many shapes one frame paints when `path` is anchored under `focus`.
fn shapes_painted(focus: Option<&Focus>, path: &str) -> usize {
    let ctx = egui::Context::default();
    publish_focus(&ctx, focus);
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(80.0, 20.0));
            focus_anchor(ui, path, rect);
        });
    });
    output.shapes.len()
}

#[test]
fn an_anchor_pulses_only_the_searched_path() {
    let focus = Focus::new(Some("cabin.passenger.first.pitch_m".to_owned()), None);
    let none = shapes_painted(None, "cabin.passenger.first.pitch_m");
    let other = shapes_painted(Some(&focus), "cabin.passenger.first.width_m");
    let hit = shapes_painted(Some(&focus), "cabin.passenger.first.pitch_m");
    assert_eq!(none, other, "another path draws nothing");
    assert!(hit > none, "the searched path draws the pulse");
}

// ---------------------------------------------------------------------------
// Every result lands on something drawn.
// ---------------------------------------------------------------------------

use super::{jump_to, search_in, SearchEntry};
use crate::detail::Detail;
use crate::nav::PageKind;

/// Make the controlling values of the entry's `visible_if` conditions true in
/// `config`, so a conditional field is drawn.
fn satisfy_conditions(config: &mut serde_json::Value, entry: &SearchEntry) {
    for (path, values, negate) in &entry.visible_if {
        let value = if *negate {
            "none-of-the-listed"
        } else {
            values[0]
        };
        let listed = path
            .split('.')
            .try_fold(&*config, |value, segment| value.get(segment))
            .and_then(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| value.as_bool().map(|on| on.to_string()))
            })
            .is_some_and(|current| values.contains(&current.as_str()));
        if listed != *negate {
            continue;
        }
        let mut node = &mut *config;
        let segments: Vec<&str> = path.split('.').collect();
        for (index, segment) in segments.iter().enumerate() {
            if !node.is_object() {
                *node = serde_json::json!({});
            }
            let map = node.as_object_mut().expect("an object");
            if index + 1 == segments.len() {
                // A switch condition ("true"/"false") sets the switch.
                let set = match value {
                    "true" => serde_json::json!(true),
                    "false" => serde_json::json!(false),
                    text => serde_json::json!(text),
                };
                map.insert((*segment).to_owned(), set);
                break;
            }
            node = map
                .entry((*segment).to_owned())
                .or_insert_with(|| serde_json::json!({}));
        }
    }
}

/// Render the page `jump_to` opens for `entry`, starting from `start`, for a
/// few frames the way the shell does, and report whether the highlight was
/// drawn on a control together with the level the jump selected.
fn lands(state: &mut crate::state::AppState, entry: &SearchEntry, start: Detail) -> (bool, Detail) {
    state.shell.detail = start;
    state.shell.search.focus = None;
    let saved = state.config_values.clone();
    satisfy_conditions(&mut state.config_values, entry);
    if state.config_values["geometry"]["engine"]["propulsion_technology"] == "turboprop" {
        // The engine designer reads the catalogue engine, not the label.
        if let Some(engine) = alas_config::engines::database()
            .iter()
            .find(|engine| engine.turboprop.is_some())
        {
            let name = engine.name.clone();
            state.set_engine(&name);
        }
    }
    state.on_config_modified();
    jump_to(state, entry);
    let detail = state.shell.detail;
    let page = crate::nav::page(&state.active_page)
        .expect("a page")
        .clone();
    let ctx = egui::Context::default();
    // The phase editors and other detached editors draw in-window here.
    ctx.set_embed_viewports(true);
    let mut result = (false, detail);
    for frame in 0..5 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1_400.0, 1_800.0),
            )),
            time: Some(f64::from(frame) * 0.05),
            max_texture_side: Some(8192),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            crate::detail::publish(ctx, state.shell.detail);
            crate::views::search_overlay::tick_focus(state, ctx);
            egui::CentralPanel::default().show(ctx, |ui| match page.kind {
                PageKind::Form => {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        crate::views::form_page::show_form_page(state, ui, &page);
                    });
                }
                PageKind::DesignSpace => {
                    crate::views::design_space_view::show_design_space_view(state, ui);
                }
                _ => crate::views::flow_pages::show_flow_page(state, ui, &page),
            });
            crate::views::mission_profile_inputs::show_mission_profile_window(state, ctx);
        });
        if super::focus_was_anchored(&ctx) {
            result = (true, detail);
            break;
        }
    }
    state.config_values = saved;
    state.on_config_modified();
    result
}

/// Pages that are a screen of their own: a result there is the page.
fn is_field_page(entry: &SearchEntry) -> bool {
    entry.field.is_some()
        && crate::nav::page(entry.page).is_some_and(|page| {
            matches!(
                page.kind,
                PageKind::Inputs
                    | PageKind::Mission
                    | PageKind::Form
                    | PageKind::Analyses
                    | PageKind::DesignSpace
            )
        })
}

#[test]
fn every_result_lands_on_a_drawn_control_at_the_level_the_jump_selects() {
    let mut state = crate::state::AppState::default();
    let index = build_index(&state.schema);
    let mut misses = Vec::new();
    for entry in index.iter().filter(|entry| is_field_page(entry)) {
        let (anchored, detail) = lands(&mut state, entry, Detail::Basic);
        assert_eq!(
            detail == Detail::Advanced,
            entry.advanced,
            "{:?} jumps to the wrong level",
            entry.field
        );
        if !anchored {
            misses.push(format!(
                "{} -> {} / {:?} / {:?} (advanced {})",
                entry.title,
                entry.page,
                entry.section,
                entry.field.as_deref().unwrap_or(""),
                entry.advanced
            ));
        }
    }
    assert!(
        misses.is_empty(),
        "{} results land on nothing drawn:\n{}",
        misses.len(),
        misses.join("\n")
    );
}

#[test]
fn a_result_tagged_advanced_is_not_drawn_at_basic() {
    let mut state = crate::state::AppState::default();
    let index = build_index(&state.schema);
    let mut drawn = Vec::new();
    for entry in index
        .iter()
        .filter(|entry| entry.advanced && is_field_page(entry))
    {
        // Jump without the level switch, as a Basic user would see the page.
        let mut at_basic = entry.clone();
        at_basic.advanced = false;
        if lands(&mut state, &at_basic, Detail::Basic).0 {
            drawn.push(format!("{} -> {:?}", entry.title, entry.field));
        }
    }
    assert!(
        drawn.is_empty(),
        "{} results are tagged Advanced but drawn at Basic:\n{}",
        drawn.len(),
        drawn.join("\n")
    );
}

#[test]
fn wing_ranks_the_wing_group_header_first_and_a_group_is_a_target_of_its_own() {
    let state = crate::state::AppState::default();
    let index = build_index(&state.schema);
    let hits = search_in(&index, "wing", 12, Some(&state.config_values));
    let first = hits.first().expect("hits");
    assert_eq!(first.field.as_deref(), Some("geometry.wing"));
    assert_eq!(first.title, "Wing");
    // The concepts that describe a group land on its header, not on a leaf.
    for (title, path) in [
        ("Wing Geometry", "geometry.wing"),
        ("Tail Sizing", "geometry.empennage"),
    ] {
        let concept = index
            .iter()
            .find(|entry| entry.title == title)
            .unwrap_or_else(|| panic!("{title} is a concept"));
        assert_eq!(concept.field.as_deref(), Some(path), "{title}");
    }
}

#[test]
fn a_field_the_configuration_hides_is_not_a_result() {
    let state = crate::state::AppState::default();
    let index = build_index(&state.schema);
    let conditional: Vec<&SearchEntry> = index
        .iter()
        .filter(|entry| {
            entry.field.is_some()
                && !entry.visible_if.is_empty()
                && entry.visible_if.iter().all(|(_, _, negate)| !negate)
        })
        .collect();
    assert!(!conditional.is_empty(), "the schema has conditional fields");
    let entry = conditional[0];
    let title = entry.title.to_lowercase();
    let (path, any_of, _) = entry.visible_if[0];
    let with = |value: &str| {
        let mut config = state.config_values.clone();
        let mut node = &mut config;
        let segments: Vec<&str> = path.split('.').collect();
        for segment in &segments[..segments.len() - 1] {
            node = node
                .as_object_mut()
                .expect("object")
                .entry((*segment).to_owned())
                .or_insert_with(|| serde_json::json!({}));
        }
        node.as_object_mut().expect("object").insert(
            segments[segments.len() - 1].to_owned(),
            serde_json::json!(value),
        );
        config
    };
    let shown = with(any_of[0]);
    let hidden = with("a-value-no-condition-lists");
    let found = |config: &serde_json::Value| {
        search_in(&index, &title, 50, Some(config))
            .iter()
            .any(|hit| hit.field == entry.field)
    };
    assert!(
        found(&shown),
        "{title} is searchable when its condition holds"
    );
    assert!(!found(&hidden), "{title} is not offered when it does not");
}

/// Where the page's scroll area ended with the jump target: the rect the
/// search scrolled to and the visible clip rect of the area it sits in.
fn target_after_jump_frames(
    state: &mut crate::state::AppState,
    entry: &SearchEntry,
    screen: egui::Vec2,
) -> Option<(egui::Rect, egui::Rect)> {
    jump_to(state, entry);
    let page = crate::nav::page(&state.active_page)
        .expect("a page")
        .clone();
    let ctx = egui::Context::default();
    ctx.set_embed_viewports(true);
    for frame in 0..30 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
            time: Some(f64::from(frame) * 0.05),
            max_texture_side: Some(8192),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            crate::detail::publish(ctx, state.shell.detail);
            crate::views::search_overlay::tick_focus(state, ctx);
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    crate::views::flow_pages::show_flow_page(state, ui, &page);
                });
            });
        });
    }
    super::last_scroll_target(&ctx)
}

#[test]
fn a_jump_ends_with_the_target_inside_the_visible_scroll_area() {
    let mut state = crate::state::AppState::default();
    state.shell.detail = Detail::Basic;
    let index = build_index(&state.schema);
    let entry = search_in(&index, "wing", 12, Some(&state.config_values))
        .first()
        .map(|entry| (*entry).clone())
        .expect("a Wing hit");
    assert_eq!(entry.field.as_deref(), Some("geometry.wing"));
    let (target, clip) = target_after_jump_frames(&mut state, &entry, egui::vec2(1_270.0, 760.0))
        .expect("the jump drew its target");
    assert!(
        clip.contains_rect(target),
        "target {target:?} is not inside the visible area {clip:?}"
    );
}

#[test]
fn choosing_an_advanced_only_result_in_basic_switches_to_advanced_and_targets_it() {
    let mut state = crate::state::AppState::default();
    let index = build_index(&state.schema);
    let hits = search_in(&index, "propeller", 12, Some(&state.config_values));
    let entry = hits
        .into_iter()
        .find(|entry| entry.advanced)
        .expect("an Advanced-only propeller result")
        .clone();
    state.shell.detail = Detail::Basic;
    state.shell.search.open = true;
    jump_to(&mut state, &entry);
    assert_eq!(
        state.shell.detail,
        Detail::Advanced,
        "the target is drawn only there"
    );
    assert_eq!(state.active_page, entry.page);
    let focus = state
        .shell
        .search
        .focus
        .as_ref()
        .expect("a highlight starts");
    assert_eq!(focus.field, entry.field);
    assert_eq!(focus.section, entry.section, "its section opens");
    assert!(!state.shell.search.open);
}

#[test]
fn a_group_header_target_is_marked_through_the_whole_highlight() {
    let target = "geometry.wing";
    let focus_at = |elapsed: f32| {
        let mut focus = Focus::new(Some(target.to_owned()), None);
        focus.elapsed = elapsed;
        focus
    };
    let shapes = |focus: Option<&Focus>| {
        let ctx = egui::Context::default();
        publish_focus(&ctx, focus);
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let rect =
                    egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(300.0, 120.0));
                super::focus_anchor_tail(ui, "::wing", rect, rect);
            });
        });
        output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(rect) => Some(rect.stroke.color.a()),
                _ => None,
            })
            .max()
            .unwrap_or(0)
    };
    assert_eq!(shapes(None), 0);
    for elapsed in [0.0, 0.5, super::FOCUS_SECONDS * 0.8] {
        assert!(
            shapes(Some(&focus_at(elapsed))) > 40,
            "the header outline is visible {elapsed} s after the jump"
        );
    }
}

#[test]
fn weak_matches_outside_the_title_are_dropped_when_a_title_matches() {
    let mut state = crate::state::AppState::default();
    // The propeller inputs exist for a turboprop only.
    state.config_values["geometry"]["engine"]["propulsion_technology"] =
        serde_json::json!("turboprop");
    let index = build_index(&state.schema);
    let hits = search_in(&index, "propeller", 12, Some(&state.config_values));
    assert!(!hits.is_empty());
    for entry in hits {
        let title = entry.title.to_lowercase();
        let translated = alas_i18n::t(Some(entry.title.as_str()), None).to_lowercase();
        let concept = crate::search_concepts::CONCEPTS
            .iter()
            .any(|concept| concept.title == entry.title);
        assert!(
            concept || title.contains("propeller") || translated.contains("propeller"),
            "{:?} matches only outside its title",
            entry.title
        );
    }
}
