// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The page-level live preview widget and its sizing.

use egui::{vec2, RichText, Ui};

use crate::state::AppState;
use crate::views::tr;

/// The smallest and largest height a page preview is drawn at, in points.
pub(super) const PREVIEW_MIN_HEIGHT: f32 = 240.0;
pub(super) const PREVIEW_MAX_HEIGHT: f32 = 480.0;

/// The size a page preview widget is given, in points.
///
/// A widget as wide as the container with a capped height would draw the
/// three-view schematic height-limited inside a box several times wider than
/// the drawing, leaving most of the container empty and the axis labels tiny.
/// Sizing the widget to the *scene's own aspect* makes the drawing as large as
/// the height budget allows and removes the empty band, because the widget is
/// no longer wider than what it draws.
pub(super) fn preview_size(
    available_width: f32,
    screen_height: f32,
    scene: (f64, f64),
) -> egui::Vec2 {
    let available_width = available_width.max(220.0);
    let aspect = if scene.1 > 0.0 && scene.0 > 0.0 {
        (scene.0 / scene.1) as f32
    } else {
        1.6
    };
    let ceiling = (screen_height * 0.6).clamp(PREVIEW_MIN_HEIGHT, PREVIEW_MAX_HEIGHT);
    let floor = PREVIEW_MIN_HEIGHT.min(ceiling);
    let height = (available_width / aspect).clamp(floor, ceiling);
    let width = (height * aspect).min(available_width);
    vec2(width, height)
}

/// The canvas, in scene units, a figure that lays itself out for its size is
/// built on, or `None` for a figure that is scaled as a picture.
///
/// `available_width` and `screen_height` are the card width and the window
/// height in scene units: the points divided by the scale the figure is drawn
/// at (`crate::theme::figure`). The charts and the three-view take the card
/// width and a height from their aspect, capped by the window height; a very
/// wide card gets a wide chart up to an aspect of 2.6 and the three-view as
/// one row of four panels. The planforms (structures, landing gear) are
/// drawings at true scale: the canvas is the box they are fitted in.
pub(super) fn sized_canvas(
    id: &str,
    available_width: f32,
    screen_height: f32,
) -> Option<(f64, f64)> {
    let aspect = match id {
        // The load & trim sheet lays itself out for the card width; its
        // height follows from its content, so only the width is asked for.
        "mass_cg" => {
            let width = f64::from(available_width).clamp(380.0, 2400.0).floor();
            return Some((width, 600.0));
        }
        "drag" => 1.75,
        // The cabin preview takes the card width like the charts.
        "cabin_3d" => 1.6,
        "engine" => 1.65,
        "structures" => 1.6,
        "control_surfaces" => 1.55,
        "geometry" | "threeview" => 1.14,
        // A drawing at true scale, as large as the card and most of the
        // window height allow (see `figure_landing_gear_planform_sized`).
        "landing_gear" => {
            let width = f64::from(available_width).clamp(380.0, 2400.0).floor();
            let height = (f64::from(screen_height) * 0.85).clamp(360.0, 1600.0);
            return Some((width, (height / 10.0).floor() * 10.0));
        }
        _ => return None,
    };
    let available = f64::from(available_width).clamp(380.0, 2400.0).floor();
    let tall = f64::from(screen_height);
    if aspect < 1.2 {
        // The three-view: a 2 x 2 grid while the card is not much wider than
        // the grid at the window's height, else one row of four panels.
        let ceiling = (tall * 0.9).clamp(360.0, 1000.0);
        let grid_width = ceiling * aspect;
        if available > grid_width * 1.1 {
            let height = (available / 3.6).clamp(300.0, ceiling);
            return Some((available.min(height * 4.4), (height / 10.0).floor() * 10.0));
        }
        let height = (available / aspect).min(ceiling);
        return Some(((height * aspect).floor(), (height / 10.0).floor() * 10.0));
    }
    if id == "structures" {
        // The planform is drawn at its true aspect inside this limit (see
        // `figure_structures_designer_preview_sized`): the card width, and
        // the height the window allows.
        let height = (tall * 0.8).clamp(300.0, 760.0);
        return Some((available, (height / 10.0).floor() * 10.0));
    }
    let ceiling = (tall * 0.62).clamp(300.0, 760.0);
    let height = (available / aspect).clamp(300.0_f64.min(ceiling), ceiling);
    Some((available.min(height * 2.6), (height / 10.0).floor() * 10.0))
}

pub(super) fn render_preview(
    state: &mut AppState,
    ui: &mut Ui,
    preview: Option<&str>,
    preview_title: Option<&str>,
) {
    let preview_id = if let Some(preview) = preview {
        preview
    } else if state.active_page == "cabin" {
        // Cabin changes deserve immediate visual confirmation even though this
        // advanced page has no report-only figure.
        "cabin_3d"
    } else {
        return;
    };
    // The dock beside the page already shows the cabin interior: the card would repeat it.
    if preview_id == "cabin_3d"
        && ui
            .ctx()
            .data(|data| data.get_temp::<bool>(egui::Id::new("preview_dock_shown")))
            .unwrap_or(false)
    {
        return;
    }
    ui.add_space(8.0);
    let title = preview_title.unwrap_or(if preview_id == "cabin_3d" {
        "Seat and payload preview"
    } else {
        "Preview"
    });
    // A card with the same disclosure header as the page's other cards.
    let card = crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        let title = crate::theme::card_title(ui, alas_i18n::t(Some(title), None));
        let header = egui::CollapsingHeader::new(title)
            .id_salt(("page_preview", preview_id))
            .default_open(true)
            .open(crate::perf_tour::force_open());
        crate::theme::show_header(ui, header, |ui| {
            render_preview_figure(state, ui, preview_id)
        });
    });
    // A search for the sheet itself lands on its card.
    crate::search::scroll_to_section(
        ui,
        "Load & trim sheet",
        card.response.rect,
        state.shell.search.focus.as_ref(),
    );
}

/// The figure under a preview card's header, drawn through the card figure
/// system (`crate::theme::figure`): built on the card's width divided by the
/// scale that puts its smallest text at the figure-text token, then drawn at
/// that scale in the middle of the card.
fn render_preview_figure(state: &mut AppState, ui: &mut Ui, preview_id: &str) {
    use crate::theme::figure;
    let width = figure::settled_width(ui, ("page_preview", preview_id));
    let screen = ui.ctx().screen_rect().height();
    let canvas_at = |scale: f32| sized_canvas(preview_id, width / scale, screen / scale);
    // The memo ends the build-measure-rebuild loop for a figure whose text
    // scale depends on its canvas (see `figure::ScaleMemo`).
    let mut memo = figure::scale_memo(ui, ("page_preview", preview_id));
    let built_scale = memo.build_scale((width, screen), 1.2);
    // Built on a worker (`crate::preview_jobs`): until the scene for these
    // inputs arrives the card keeps its previous scene, or says it is building.
    let (mut built, mut pending) = preview_lookup(state, preview_id, canvas_at(built_scale));
    if let Some((scene, _)) = &built {
        let wanted = figure::text_scale(scene);
        if !pending && canvas_at(wanted).is_some() {
            if let Some(scale) = memo.resolve(built_scale, wanted) {
                (built, pending) = preview_lookup(state, preview_id, canvas_at(scale));
            }
        }
    }
    figure::store_scale_memo(ui, ("page_preview", preview_id), memo);
    let Some((scene, revision)) = built else {
        let note = if pending {
            "Building the preview\u{2026}"
        } else {
            "Preview needs a completed run."
        };
        ui.label(RichText::new(tr(note)).weak());
        return;
    };
    let size = match canvas_at(1.0) {
        // Built for the card: drawn at the scale its text asks for, never
        // wider than the card. A sheet that grows with its content is taller
        // than the canvas it was asked for; its own size is drawn.
        Some(_) => figure::figure_size(
            &scene,
            memo.held().unwrap_or_else(|| figure::text_scale(&scene)),
            ui.available_width(),
        ),
        None => preview_size(ui.available_width(), screen, (scene.width, scene.height)),
    };
    let view_key = format!("page_preview::{preview_id}");
    // The raster of a new preview is drawn on the raster pool while the
    // preview workers are on; the card keeps its previous image meanwhile.
    let background = state
        .preview_jobs
        .borrow()
        .enabled()
        .then(|| tr("Building the preview\u{2026}"));
    let view = alas_viz::SceneView::new(&scene, state.view_state_mut(view_key.clone()))
        .static_view()
        .show_toolbar(false)
        .cache_key(&view_key)
        .cache_revision(revision)
        .background_raster(background);
    let response = figure::show(ui, size, view);
    super::super::plot_data_menu::attach_scene(&response, &scene, preview_id, None);
}

/// The preview scene with its revision, and whether a newer one is building.
fn preview_lookup(
    state: &AppState,
    preview_id: &str,
    canvas: Option<(f64, f64)>,
) -> (
    Option<(std::sync::Arc<alas_report::scene::Scene>, u64)>,
    bool,
) {
    use crate::preview_jobs::PreviewLookup;
    match state.page_preview_background(preview_id, canvas) {
        PreviewLookup::Ready(scene) => (
            scene.map(|scene| (scene, state.page_preview_key(preview_id, canvas))),
            false,
        ),
        PreviewLookup::Pending(stale) => (stale, true),
    }
}

#[cfg(test)]
mod tests {
    use super::sized_canvas;

    /// The charts take the card width at a sane aspect, never wider than it,
    /// and the three-view switches to one row when the card is much wider
    /// than the grid it would be drawn as.
    #[test]
    fn sized_figures_fill_the_card_at_a_sane_aspect() {
        for id in ["drag", "engine", "structures"] {
            for (card, screen) in [(830.0, 770.0), (1400.0, 1120.0), (2400.0, 1120.0)] {
                let (width, height) = sized_canvas(id, card, screen).expect("sized figure");
                assert!(
                    width <= f64::from(card) && width >= 380.0,
                    "{id} {card}: {width}"
                );
                assert!(
                    width / height <= 2.7 || id == "structures",
                    "{id} {card}: aspect {}",
                    width / height
                );
                assert!((240.0..=760.0).contains(&height), "{id} {card}: {height}");
            }
            // A card narrower than the chart's natural aspect is filled in width.
            let (width, _) = sized_canvas(id, 830.0, 770.0).expect("sized figure");
            assert!(width >= 800.0, "{id}: {width}");
        }
        let (grid_w, grid_h) = sized_canvas("threeview", 830.0, 770.0).expect("three-view");
        assert!(grid_w / grid_h < 1.3, "a 2 x 2 grid on a modest card");
        let (row_w, row_h) = sized_canvas("threeview", 2400.0, 1120.0).expect("three-view");
        assert!(
            row_w / row_h >= 2.0,
            "one row of four panels on a very wide card"
        );
        assert!(row_w >= 1700.0, "and it takes the width: {row_w}");
        let (cabin_w, _) = sized_canvas("cabin_3d", 830.0, 770.0).expect("cabin preview");
        assert!(
            cabin_w >= 600.0,
            "the cabin preview fills the card: {cabin_w}"
        );
        let (sheet_w, _) = sized_canvas("mass_cg", 830.0, 770.0).expect("load & trim sheet");
        assert!(
            (600.0..=830.0).contains(&sheet_w),
            "the sheet takes the card width: {sheet_w}"
        );
    }

    /// The load & trim sheet is drawn on the card's width with legible text,
    /// whatever the width, and every text stays on the canvas.
    #[test]
    fn the_load_trim_sheet_is_legible_at_every_card_width() {
        let mut state = crate::state::AppState::default();
        // `ALAS_PREVIEW_PRESET` picks another preset, for a look at its sheet.
        let preset = std::env::var("ALAS_PREVIEW_PRESET").unwrap_or_else(|_| "A340-300".to_owned());
        state.load_preset(&preset);
        for card in [520.0_f32, 830.0, 1400.0, 2400.0] {
            let canvas = sized_canvas("mass_cg", card, 770.0).expect("sized");
            let scene = crate::scene::build_page_preview_sized(&state, "mass_cg", Some(canvas))
                .expect("sheet builds");
            assert!(
                (scene.width - canvas.0).abs() < 1e-9,
                "{card}: {}",
                scene.width
            );
            for element in &scene.elements {
                if let alas_report::scene::SceneElement::Text {
                    text,
                    pos,
                    font_size,
                    angle_deg,
                    ..
                } = element
                {
                    assert!(*font_size >= 9.5, "{card}: {text} at {font_size} pt");
                    if *angle_deg == 0.0 {
                        assert!(
                            pos[0] >= 0.0 && pos[0] <= scene.width && pos[1] <= scene.height,
                            "{card}: {text} at {pos:?} outside {}x{}",
                            scene.width,
                            scene.height
                        );
                    }
                }
            }
            if let Some(dir) = std::env::var_os("ALAS_PREVIEW_PNG_DIR") {
                let png = alas_viz::raster::render_scene_png(&scene).expect("raster");
                let path = std::path::Path::new(&dir).join(format!("mass_cg_{preset}_{card}.png"));
                std::fs::write(path, png).expect("write png");
            }
        }
    }

    /// The cycle preview draws its inset of the packed stations only where the
    /// plot is free, and the inset's tick text is smaller than the labels that
    /// replace it. On cards about 960 to 1130 points wide the larger canvas has
    /// room for the inset and the smaller has not, so each build asked for the
    /// other's scale: the card rebuilt the figure on every frame and showed the
    /// two layouts in turn. With the scale memo every sized preview reaches a
    /// frame that builds once, and stays there.
    #[test]
    fn every_sized_preview_settles_at_every_card_width() {
        use crate::theme::figure;
        let mut state = crate::state::AppState::default();
        state.load_preset("A320-200");
        let screen = 1275.0_f32;
        let mut cards: Vec<(&str, f32)> = (0..=20)
            .map(|step| ("engine", 940.0 + 10.0 * step as f32))
            .collect();
        for id in ["engine", "drag", "structures", "threeview"] {
            cards.extend([520.0_f32, 830.0, 1400.0, 2400.0].map(|card| (id, card)));
        }
        for (id, card) in cards {
            let canvas_at = |scale: f32| sized_canvas(id, card / scale, screen / scale);
            let mut memo = figure::ScaleMemo::default();
            let mut builds = Vec::new();
            for _ in 0..8 {
                let built = memo.build_scale((card, screen), 1.2);
                let scene = crate::scene::build_page_preview_sized(&state, id, canvas_at(built))
                    .unwrap_or_else(|| panic!("{id} builds at {card}"));
                let rebuilt = memo.resolve(built, figure::text_scale(&scene));
                builds.push(1 + usize::from(rebuilt.is_some()));
            }
            let settled = builds.iter().position(|count| *count == 1);
            assert!(
                settled.is_some_and(
                    |frame| frame <= 3 && builds[frame..].iter().all(|count| *count == 1)
                ),
                "{id} at a {card}-point card never settles: builds per frame {builds:?}"
            );
        }
    }

    /// With `ALAS_PREVIEW_PNG_DIR` set, write each sized figure at a modest and a
    /// very wide card, for a visual check; without it the figures are only built.
    #[test]
    fn the_sized_figures_build_at_every_card_width() {
        let mut state = crate::state::AppState::default();
        state.load_preset("A340-300");
        for (card, screen) in [(830.0_f32, 770.0_f32), (1400.0, 1120.0), (2400.0, 1120.0)] {
            for id in ["drag", "engine", "structures", "threeview"] {
                let canvas = sized_canvas(id, card, screen).expect("sized");
                let scene = crate::scene::build_page_preview_sized(&state, id, Some(canvas))
                    .unwrap_or_else(|| panic!("{id} builds at {canvas:?}"));
                if id == "structures" {
                    // Drawn at the planform's true aspect inside the limit.
                    assert!(scene.width <= canvas.0 && scene.height <= canvas.1, "{id}");
                    assert!(
                        scene.width > 0.5 * canvas.0,
                        "{id}: {} of {}",
                        scene.width,
                        canvas.0
                    );
                } else {
                    assert_eq!((scene.width, scene.height), canvas, "{id}");
                }
                if let Some(dir) = std::env::var_os("ALAS_PREVIEW_PNG_DIR") {
                    let png = alas_viz::raster::render_scene_png(&scene).expect("raster");
                    let path = std::path::Path::new(&dir).join(format!("{id}_{card}.png"));
                    std::fs::write(path, png).expect("write png");
                }
            }
        }
    }
}
