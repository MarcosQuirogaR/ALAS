// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The aircraft thumbnail of the Files preview: the top view of the aircraft a
//! saved file holds, drawn by the same exterior renderer as the live preview.
//!
//! Three kinds of file carry an aircraft: a saved aircraft (`alas_my_aircraft`),
//! a library Aircraft object, and a library Results snapshot (the aircraft the
//! run ended on, at that run's design point). A plain configuration file is the
//! aircraft itself. The scene is built once per selection, not per frame, and
//! uses the interactive preview's panel cap so selecting a file stays quick.
//!
//! Frame: a plan view, +x aft along the page, +y to the right wing; the
//! renderer's elevation of 90 degrees looks straight down.

use alas_config::{design_variables::DesignVector, AlasConfig, WORKSPACE_ENVELOPE_KEY};
use alas_geom::builder::AircraftBuilder;
use alas_report::families::geometry::figure_exterior_3d;
use alas_report::scene::{Camera3D, Scene};
use egui::Ui;
use serde_json::Value;
use std::path::Path;

use crate::state::AppState;

/// Elevation of the plan view, degrees above the ground plane.
const TOP_VIEW_ELEVATION_DEG: f64 = 90.0;

/// The configuration document and, when the file states one, the design
/// vector of the aircraft in `file`. `None` when the file holds no aircraft.
pub(crate) fn aircraft_document(file: &Value) -> Option<(&Value, Option<&Value>)> {
    if let Some(workspace) = file.get("alas_my_aircraft").and(file.get("workspace")) {
        return Some((workspace, None));
    }
    if let Some(header) = file.get("alas_object") {
        let body = file.get("body")?;
        return match header.get("kind").and_then(Value::as_str) {
            Some("aircraft") => Some((body, None)),
            Some("results") => Some((body.get("workspace")?, body.get("design_values"))),
            _ => None,
        };
    }
    (file.get("geometry").is_some() && file.get("requirements").is_some()).then_some((file, None))
}

/// The design vector the aircraft is drawn at: the file's own, else the
/// registered preset's, else the default.
fn design_for(document: &Value, config: &AlasConfig, stated: Option<&Value>) -> DesignVector {
    let from_value = |value: &Value| serde_json::from_value::<DesignVector>(value.clone()).ok();
    stated
        .and_then(from_value)
        .or_else(|| {
            document
                .get(WORKSPACE_ENVELOPE_KEY)
                .and_then(|envelope| envelope.get("design_vector"))
                .filter(|vector| vector.as_object().is_some_and(|map| !map.is_empty()))
                .and_then(from_value)
        })
        .or_else(|| {
            alas_config::presets::get(&config.preset)
                .ok()
                .map(|preset| preset.design_vector)
        })
        .unwrap_or_default()
}

/// The plan-view scene of the aircraft in `file`, or `None` when the file holds
/// none or the geometry cannot be built. `theme` is a figure theme name.
pub(crate) fn aircraft_scene(file: &Value, theme: &str) -> Option<Scene> {
    let (document, stated) = aircraft_document(file)?;
    let config = AlasConfig::from_value(document).ok()?;
    let design = design_for(document, &config, stated);
    let mut geometry = config.geometry.clone();
    geometry.wing.n_subdivisions = geometry.wing.n_subdivisions.clamp(1, 2);
    geometry.empennage.n_subdivisions = geometry.empennage.n_subdivisions.clamp(1, 2);
    let airplane = AircraftBuilder::new(Some(geometry))
        .build(Some(&design), true)
        .ok()?;
    let camera = Camera3D {
        elev_deg: TOP_VIEW_ELEVATION_DEG,
        azim_deg: 0.0,
        zoom: 1.0,
    };
    Some(crate::scene::localize_scene_for_display(
        figure_exterior_3d(&airplane, Some(camera), Some(theme)),
    ))
}

/// Draw the top view of the file at `path` as large as fits `bounds` (points),
/// keeping the scene's own aspect ratio; returns the drawn size. `revision`
/// changes when the file is read again, so the cached picture is redrawn.
pub(crate) fn show_fitted(
    state: &mut AppState,
    ui: &mut Ui,
    path: &Path,
    scene: &Scene,
    bounds: egui::Vec2,
    revision: u64,
) {
    let aspect = (scene.height / scene.width.max(1.0)) as f32;
    let mut width = bounds.x;
    if width * aspect > bounds.y {
        width = bounds.y / aspect.max(0.01);
    }
    let key = format!("files_thumbnail::{}", path.display());
    let view = alas_viz::SceneView::new(scene, state.view_state_mut(key.clone()))
        .static_view()
        .show_toolbar(false)
        .cache_key(&key)
        .cache_revision(revision)
        .desired_size(egui::vec2(width, width * aspect));
    ui.add(view);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_files_that_hold_an_aircraft_get_a_thumbnail() {
        let config = serde_json::to_value(AlasConfig::default()).expect("config");
        assert!(aircraft_document(&config).is_some());
        let saved = json!({"alas_my_aircraft": {"name": "x"}, "workspace": config});
        assert!(aircraft_document(&saved).is_some());
        let engine = json!({"alas_object": {"kind": "engine"}, "body": {}});
        assert!(aircraft_document(&engine).is_none());
        let results = json!({
            "alas_object": {"kind": "results"},
            "body": {"workspace": config, "design_values": null}
        });
        assert!(aircraft_document(&results).is_some());
        assert!(aircraft_document(&json!([1, 2])).is_none());
    }

    #[test]
    fn a_saved_configuration_draws_a_top_view() {
        let config = serde_json::to_value(AlasConfig::default()).expect("config");
        let scene = aircraft_scene(&config, "dark").expect("the default aircraft builds");
        assert!(scene.width > 0.0 && scene.height > 0.0);
    }
}
