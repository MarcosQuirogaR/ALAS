// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! External image selection and texture loading for result scenes.

use crate::state::AppState;
use crate::views::tr_fields;
use alas_report::scene::Scene;
use egui::{vec2, Id, RichText, Ui};

pub(super) fn scene_has_external_images(scene: &Scene) -> bool {
    scene.elements.iter().any(|element| {
        matches!(
            element,
            alas_report::scene::SceneElement::Image { source, .. }
                if source != "embedded://nasa-blue-marble"
        )
    })
}

pub(super) fn show_external_images(
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
        "alas_external_render_selection",
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
                let label = labels.get(index).copied().unwrap_or("External render");
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
    let max_height = (available_height - if images.len() > 1 { 54.0 } else { 24.0 }).max(120.0);
    let panel_width = available_width.max(240.0);
    let panel_height = (panel_width * native_height as f32 / native_width as f32)
        .min(max_height)
        .max(120.0);
    let panel_width = (panel_height * native_width as f32 / native_height as f32).min(panel_width);
    match load_external_texture(state, ui.ctx(), source) {
        Some(texture) => {
            if ui
                .add(external_image(&texture, vec2(panel_width, panel_height)))
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
                        "External image unavailable:\n{path}",
                        &[("path", source.to_string())],
                    ))
                    .color(egui::Color32::from_rgb(192, 57, 43))
                    .strong(),
                ),
            );
        }
    }
    double_clicked
}

/// The maximizable render widget.  `egui::Image` senses hover only, so the
/// double-click that maximizes every other result figure never fired on a
/// Patran render until the widget asked for click input.
pub(super) fn external_image(
    texture: &egui::TextureHandle,
    size: egui::Vec2,
) -> egui::Image<'static> {
    egui::Image::from_texture(texture)
        .fit_to_exact_size(size)
        .sense(egui::Sense::click())
}

/// A load already attempted and refused: no `state.patran_textures` field
/// exists for failures, so `None` was retried by reading and decoding the
/// file again on every repaint. The memo lapses on a new run, and after
/// [`FAILED_LOAD_RETRY`] within one, since an external solver can write the
/// image after a snapshot has already named it.
#[derive(Clone, Copy)]
struct FailedExternalLoad {
    run_identity: u64,
    at: std::time::Instant,
}

const FAILED_LOAD_RETRY: std::time::Duration = std::time::Duration::from_secs(2);

fn record_load_failure(
    context: &egui::Context,
    id: Id,
    run_identity: u64,
) -> Option<egui::TextureHandle> {
    let failure = FailedExternalLoad {
        run_identity,
        at: std::time::Instant::now(),
    };
    context.data_mut(|data| data.insert_temp(id, failure));
    None
}

fn load_external_texture(
    state: &mut AppState,
    context: &egui::Context,
    source: &str,
) -> Option<egui::TextureHandle> {
    if let Some(texture) = state.patran_textures.get(source) {
        return Some(texture.clone());
    }
    let failure_id = Id::new(("patran_missing_texture", source));
    let run_identity = state.run_identity;
    if context
        .data(|data| data.get_temp::<FailedExternalLoad>(failure_id))
        .is_some_and(|failed| {
            failed.run_identity == run_identity && failed.at.elapsed() < FAILED_LOAD_RETRY
        })
    {
        return None;
    }
    let Ok(bytes) = std::fs::read(source) else {
        return record_load_failure(context, failure_id, run_identity);
    };
    let Ok(icon) = eframe::icon_data::from_png_bytes(&bytes) else {
        return record_load_failure(context, failure_id, run_identity);
    };
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [icon.width as usize, icon.height as usize],
        &icon.rgba,
    );
    let texture = context.load_texture(
        format!("external:{source}"),
        image,
        egui::TextureOptions::LINEAR,
    );
    state
        .patran_textures
        .insert(source.to_owned(), texture.clone());
    Some(texture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_source_is_not_reopened_within_the_same_run() {
        let ctx = egui::Context::default();
        let mut state = AppState::default();
        let missing = "does-not-exist.png";

        assert!(load_external_texture(&mut state, &ctx, missing).is_none());
        // The first call recorded the failure; a second call within the same
        // run must not touch the filesystem again to reach the same answer.
        assert!(load_external_texture(&mut state, &ctx, missing).is_none());
    }

    #[test]
    fn a_new_run_retries_a_source_that_previously_failed() {
        let ctx = egui::Context::default();
        let mut state = AppState::default();
        let missing = "still-does-not-exist.png";

        assert!(load_external_texture(&mut state, &ctx, missing).is_none());
        state.run_identity = state.run_identity.wrapping_add(1);
        assert!(load_external_texture(&mut state, &ctx, missing).is_none());

        // The memoized failure now carries the new run identity, which is
        // only possible if the second call re-attempted the load instead of
        // returning the cached answer from the previous run.
        let failure_id = Id::new(("patran_missing_texture", missing));
        let recorded = ctx.data(|data| data.get_temp::<FailedExternalLoad>(failure_id));
        assert_eq!(
            recorded.map(|failed| failed.run_identity),
            Some(state.run_identity)
        );
    }
}
