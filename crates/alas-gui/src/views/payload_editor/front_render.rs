// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The aircraft behind the front view's cutaway: the lofted model drawn by the
//! same engine as the 3-D preview, seen from behind (`+y` to the right, `+z`
//! up, as the payload numbers read), at exactly the scale and place of the
//! overlay drawn over it.
//!
//! One texture is kept. It covers the visible window plus a margin, at one
//! texture pixel per physical pixel, and is drawn by mapping its metres through
//! the same [`Placement`] as the overlay, so zoom and pan move it like any
//! other part of the picture. It is re-rendered only when the aircraft, the
//! theme or the pixel density change, or when the framing has left the texture
//! (not covered, or magnified so much it would blur). The engine's scene takes
//! a few milliseconds; its raster takes hundreds, so it runs on a worker thread
//! and the stale texture stays on screen until the new one lands. An idle frame
//! costs one textured quad.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{pos2, Color32, Context, Id, Pos2, Rect, TextureHandle, TextureOptions, Ui};

use super::side_view::{Airframe, SideView};

/// Where metres sit on screen: `(y, z)` is drawn at `origin + (y, -z) * scale`
/// points. The overlay and the render both go through it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Placement {
    pub origin: Pos2,
    pub scale: f32,
}

impl Placement {
    pub(super) fn to_screen(self, y: f64, z: f64) -> Pos2 {
        pos2(
            self.origin.x + y as f32 * self.scale,
            self.origin.y - z as f32 * self.scale,
        )
    }

    fn to_metres(self, point: Pos2) -> (f64, f64) {
        (
            f64::from((point.x - self.origin.x) / self.scale),
            f64::from((self.origin.y - point.y) / self.scale),
        )
    }
}

/// Window beyond the visible one that the texture also covers, as a fraction
/// of the visible size per side.
const GROW: f64 = 0.25;
/// Largest texture side, pixels.
const MAX_SIDE_PX: f64 = 3600.0;
/// Largest texture area, pixels.
const MAX_AREA_PX: f64 = 7.0e6;
/// How often a frame waiting on a render in flight asks to be redrawn, s.
const MIN_INTERVAL_S: f64 = 0.15;
/// The texture may be magnified this much before it is re-rendered sharper.
const MAX_MAGNIFY: f64 = 1.15;
/// ... and minified this much before it is re-rendered coarser.
const MAX_MINIFY: f64 = 0.5;
/// Scene units the engine leaves around its viewport, per side.
const MARGIN_PX: f64 = crate::sandbox::scene::FRAMED_CANVAS_MARGIN;

/// What a texture shows: its centre in metres, its resolution and the size of
/// the area the picture fills (the scene canvas is [`MARGIN_PX`] larger on
/// every side).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Plan {
    pub centre: (f64, f64),
    /// Texture pixels per metre.
    pub ppm: f64,
    pub viewport_px: (f64, f64),
}

impl Plan {
    /// The plan for the screen rectangle `visible` placed by `placement`, at
    /// `ppp` pixels per point.
    pub(super) fn for_view(visible: Rect, placement: Placement, ppp: f32) -> Self {
        let ppp = f64::from(ppp.max(0.1));
        let (width, height) = (
            f64::from(visible.width()) * ppp * (1.0 + 2.0 * GROW),
            f64::from(visible.height()) * ppp * (1.0 + 2.0 * GROW),
        );
        let shrink = (MAX_SIDE_PX / width.max(height).max(1.0))
            .min((MAX_AREA_PX / (width * height).max(1.0)).sqrt())
            .min(1.0);
        Self {
            centre: placement.to_metres(visible.center()),
            ppm: f64::from(placement.scale) * ppp * shrink,
            viewport_px: (
                (width * shrink).round().max(8.0),
                (height * shrink).round().max(8.0),
            ),
        }
    }

    /// The whole scene canvas, texture pixels.
    pub(super) fn canvas_px(self) -> (f64, f64) {
        (
            self.viewport_px.0 + 2.0 * MARGIN_PX,
            self.viewport_px.1 + 2.0 * MARGIN_PX,
        )
    }

    /// The screen rectangle the texture of this plan occupies under
    /// `placement`: the same mapping the overlay uses.
    pub(super) fn screen_rect(self, placement: Placement) -> Rect {
        let (width, height) = self.canvas_px();
        let (half_y, half_z) = (width / 2.0 / self.ppm, height / 2.0 / self.ppm);
        Rect::from_two_pos(
            placement.to_screen(self.centre.0 - half_y, self.centre.1 + half_z),
            placement.to_screen(self.centre.0 + half_y, self.centre.1 - half_z),
        )
    }

    /// Whether this texture can stand in for one planned as `wanted` to fill
    /// `visible` under `placement`: it covers `visible` and is neither too
    /// blurry nor too coarse.
    fn serves(self, wanted: Self, visible: Rect, placement: Placement) -> bool {
        let ratio = wanted.ppm / self.ppm;
        let (width, height) = self.canvas_px();
        let (half_y, half_z) = (width / 2.0 / self.ppm, height / 2.0 / self.ppm);
        let (left, top) = placement.to_metres(visible.left_top());
        let (right, bottom) = placement.to_metres(visible.right_bottom());
        (MAX_MINIFY..=MAX_MAGNIFY).contains(&ratio)
            && left >= self.centre.0 - half_y
            && right <= self.centre.0 + half_y
            && bottom >= self.centre.1 - half_z
            && top <= self.centre.1 + half_z
    }
}

/// Which way the aircraft is seen: from behind (`Placement` axes are `y`, `z`)
/// or from the side (`x`, `z`). Each keeps its own texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Projection {
    Front,
    Side,
}

/// What a texture shows: the airframe (an opaque, subdued background) or the
/// cabin's seats (transparent, drawn over the cutaway body).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Layer {
    Airframe,
    Seats,
}

/// What a texture was made for; two renders of the same scene share it.
#[derive(Clone, PartialEq)]
struct SceneKey {
    projection: Projection,
    layer: Layer,
    /// Fingerprint of the seat rows (the seats layer; 0 for the airframe).
    seats: u64,
    airframe: Arc<Airframe>,
    theme: &'static str,
    ppp_bits: u32,
}

impl SceneKey {
    /// Same aircraft, theme and pixel density (a pointer match, else equality).
    fn is(&self, other: &Self) -> bool {
        self.projection == other.projection
            && self.layer == other.layer
            && self.seats == other.seats
            && self.theme == other.theme
            && self.ppp_bits == other.ppp_bits
            && (Arc::ptr_eq(&self.airframe, &other.airframe) || *self.airframe == *other.airframe)
    }
}

#[derive(Clone)]
struct Cached {
    key: SceneKey,
    plan: Plan,
    texture: TextureHandle,
}

/// The raster of a render in flight on a worker thread.
type Slot = Arc<Mutex<Option<egui::ColorImage>>>;

#[derive(Clone)]
struct Pending {
    key: SceneKey,
    plan: Plan,
    slot: Slot,
}

fn layer_cache_id(projection: Projection, layer: Layer) -> Id {
    Id::new((
        "payload_front_render",
        projection == Projection::Side,
        layer == Layer::Seats,
    ))
}

fn layer_pending_id(projection: Projection, layer: Layer) -> Id {
    Id::new((
        "payload_front_render_job",
        projection == Projection::Side,
        layer == Layer::Seats,
    ))
}

#[cfg(test)]
fn cache_id(projection: Projection) -> Id {
    layer_cache_id(projection, Layer::Airframe)
}

/// The engine's scene of the aircraft for `plan`.
fn build_scene(
    projection: Projection,
    airframe: &Airframe,
    theme: &str,
    plan: Plan,
) -> Option<alas_report::scene::Scene> {
    let build = match projection {
        Projection::Front => crate::sandbox::scene::build_framed_background_view,
        Projection::Side => crate::sandbox::scene::build_framed_background_side_view,
    };
    build(
        airframe.geometry.clone(),
        airframe.design,
        theme,
        plan.viewport_px,
        plan.centre,
        plan.ppm,
    )
    .map(|(scene, _)| scene)
}

/// The scene of `layer` for `plan`: the airframe, or the seat rows seen from
/// the side (the front view draws its few seats directly).
pub(super) fn build_layer_scene(
    layer: Layer,
    projection: Projection,
    view: &SideView,
    airframe: &Airframe,
    theme: &str,
    plan: Plan,
) -> Option<alas_report::scene::Scene> {
    match layer {
        Layer::Airframe => build_scene(projection, airframe, theme, plan),
        Layer::Seats if projection == Projection::Side => {
            Some(crate::sandbox::scene::build_framed_seat_layer(
                &super::seat_layer::profiles(view),
                plan.viewport_px,
                plan.centre,
                plan.ppm,
            ))
        }
        Layer::Seats => None,
    }
}

/// Rasterize `scene` at one texture pixel per scene unit, premultiplied.
pub(super) fn raster(scene: &alas_report::scene::Scene) -> Option<egui::ColorImage> {
    let (width, height, rgba) = alas_viz::raster::render_scene_rgba_scaled(scene, 1.0).ok()?;
    Some(egui::ColorImage::from_rgba_premultiplied(
        [width as usize, height as usize],
        &rgba,
    ))
}

/// The figure theme's background, so the render's canvas matches the other
/// figures.
fn background(theme: &str) -> Color32 {
    let colour =
        alas_report::scene::Color::from_hex(alas_report::theme::get_palette(Some(theme)).bg);
    alas_viz::to_egui_color(&colour)
}

/// Start the raster of `scene` on a worker thread: the engine's scene takes a
/// few milliseconds, the raster hundreds, and only the second is moved off the
/// frame. The context is woken when the image is ready.
fn start(ctx: &Context, key: SceneKey, plan: Plan, scene: alas_report::scene::Scene) -> Pending {
    let slot: Slot = Arc::new(Mutex::new(None));
    let (worker_slot, wake) = (Arc::clone(&slot), ctx.clone());
    std::thread::spawn(move || {
        if let Some(image) = raster(&scene) {
            if let Ok(mut slot) = worker_slot.lock() {
                *slot = Some(image);
            }
        }
        wake.request_repaint();
    });
    Pending { key, plan, slot }
}

/// Paint the aircraft behind the overlay into `rect`.
pub(super) fn paint(
    ui: &Ui,
    painter: &egui::Painter,
    rect: Rect,
    placement: Placement,
    view: &SideView,
    theme: &'static str,
    projection: Projection,
) {
    painter.rect_filled(rect, super::camera::CANVAS_RADIUS, background(theme));
    paint_layer(
        ui,
        painter,
        rect,
        placement,
        view,
        theme,
        projection,
        Layer::Airframe,
    );
}

/// Paint the cabin's seat rows (side view) over the cutaway body.
pub(super) fn paint_seats(
    ui: &Ui,
    painter: &egui::Painter,
    rect: Rect,
    placement: Placement,
    view: &SideView,
    theme: &'static str,
) {
    paint_layer(
        ui,
        painter,
        rect,
        placement,
        view,
        theme,
        Projection::Side,
        Layer::Seats,
    );
}

// reason: the painter, the framing and the layer are all needed by every caller.
#[allow(clippy::too_many_arguments)]
fn paint_layer(
    ui: &Ui,
    painter: &egui::Painter,
    rect: Rect,
    placement: Placement,
    view: &SideView,
    theme: &'static str,
    projection: Projection,
    layer: Layer,
) {
    let Some(airframe) = view.airframe.as_ref() else {
        return;
    };
    let ctx = ui.ctx();
    let (cache_id, pending_id) = (
        layer_cache_id(projection, layer),
        layer_pending_id(projection, layer),
    );
    let ppp = ctx.pixels_per_point();
    let key = SceneKey {
        projection,
        layer,
        seats: if layer == Layer::Seats {
            super::seat_layer::fingerprint(view)
        } else {
            0
        },
        airframe: Arc::clone(airframe),
        theme,
        ppp_bits: ppp.to_bits(),
    };
    let wanted = Plan::for_view(rect, placement, ppp);
    let mut cached: Option<Cached> = ctx.data(|data| data.get_temp(cache_id));
    let mut pending: Option<Pending> = ctx.data(|data| data.get_temp(pending_id));

    // A finished raster becomes the texture, if it is still of this scene.
    let finished = pending.as_ref().and_then(|job| {
        job.slot
            .lock()
            .ok()
            .and_then(|mut slot| slot.take())
            .map(|image| (job.key.clone(), job.plan, image))
    });
    if let Some((job_key, plan, image)) = finished {
        pending = None;
        if job_key.is(&key) {
            let texture = match cached.as_ref() {
                Some(entry) => {
                    let mut texture = entry.texture.clone();
                    texture.set(image, TextureOptions::LINEAR);
                    texture
                }
                None => ctx.load_texture("payload-front-render", image, TextureOptions::LINEAR),
            };
            cached = Some(Cached {
                key: job_key,
                plan,
                texture,
            });
        }
    }
    // What is on hand is of another scene (geometry, theme or density changed):
    // it must not be drawn.
    if cached.as_ref().is_some_and(|entry| !entry.key.is(&key)) {
        cached = None;
    }
    let current = cached
        .as_ref()
        .is_some_and(|entry| entry.plan.serves(wanted, rect, placement));
    let in_flight = pending.as_ref().is_some_and(|job| job.key.is(&key));
    let job_serves = pending
        .as_ref()
        .is_some_and(|job| job.key.is(&key) && job.plan.serves(wanted, rect, placement));
    if !current && !job_serves {
        // Wait for a job of another framing to land first; its result is
        // replaced by the next one the moment it does.
        if in_flight {
            ctx.request_repaint_after(Duration::from_secs_f64(MIN_INTERVAL_S));
        } else if let Some(scene) =
            build_layer_scene(layer, projection, view, airframe, theme, wanted)
        {
            pending = Some(start(ctx, key, wanted, scene));
        }
    }
    ctx.data_mut(|data| match &pending {
        Some(job) => data.insert_temp(pending_id, job.clone()),
        None => data.remove::<Pending>(pending_id),
    });
    let Some(entry) = cached else {
        return;
    };
    painter.image(
        entry.texture.id(),
        entry.plan.screen_rect(placement),
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    ctx.data_mut(|data| data.insert_temp(cache_id, entry));
}

/// Whether a render texture has been delivered (tests drive the page).
#[cfg(test)]
pub(super) fn has_texture(ctx: &Context, projection: Projection) -> bool {
    ctx.data(|data| data.get_temp::<Cached>(cache_id(projection)))
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use egui::vec2;

    fn preset_view() -> SideView {
        let mut state = AppState::default();
        state.load_preset("A340-300");
        let config = state.typed_config_arc().expect("typed config");
        super::super::side_view::build(&state, &config).expect("side view")
    }

    /// The overlay and the render share one mapping: a known fuselage
    /// half-width (metres) lands on the same screen point whether it is read
    /// from the overlay's placement or from the pixels of the engine's scene.
    #[test]
    fn the_render_registers_with_the_overlay_at_every_framing() {
        let view = preset_view();
        let airframe = view.airframe.as_ref().expect("airframe");
        let section_x = f64::midpoint(view.cabin_x[0], view.cabin_x[1]);
        let half = view.at(&view.widths, section_x) / 2.0;
        let (low, high) = (
            view.at(&view.bottom, section_x),
            view.at(&view.top, section_x),
        );
        let centre_z = f64::midpoint(low, high);
        assert!(half > 1.0, "a cabin half-width in metres: {half}");
        let rect = Rect::from_min_size(pos2(40.0, 70.0), vec2(900.0, 460.0));
        for (ppp, scale, origin) in [
            (1.0_f32, 14.0_f32, pos2(490.0, 300.0)),
            (1.25, 33.5, pos2(300.0, 410.0)),
            (2.0, 90.0, pos2(700.0, 200.0)),
        ] {
            let placement = Placement { origin, scale };
            let plan = Plan::for_view(rect, placement, ppp);
            let (scene, framing) = crate::sandbox::scene::build_framed_view(
                airframe.geometry.clone(),
                airframe.design,
                "dark-accessible",
                plan.viewport_px,
                plan.centre,
                plan.ppm,
            )
            .expect("renders");
            assert!((scene.width - plan.canvas_px().0).abs() < 1e-9);
            let texture = plan.screen_rect(placement);
            for (y, z) in [(half, centre_z), (-half, centre_z), (0.0, high), (0.0, low)] {
                let overlay = placement.to_screen(y, z);
                let px = framing.project([0.0, y, z]);
                let from_render = pos2(
                    texture.min.x + px[0] as f32 / ppp,
                    texture.min.y + px[1] as f32 / ppp,
                );
                assert!(
                    (overlay - from_render).length() < 0.01,
                    "ppp {ppp} ({y}, {z}): overlay {overlay:?} vs render {from_render:?}"
                );
            }
            // One scale both ways: a metre across is a metre up.
            let across = framing.project([0.0, 1.0, 0.0])[0] - framing.project([0.0, 0.0, 0.0])[0];
            let up = framing.project([0.0, 0.0, 0.0])[1] - framing.project([0.0, 0.0, 1.0])[1];
            assert!((across - up).abs() < 1e-6 && (across - plan.ppm).abs() < 1e-6);
        }
    }

    /// A rendered texture serves nearby framings and is replaced when the
    /// view leaves it or magnifies it too far.
    #[test]
    fn a_texture_serves_small_moves_and_not_large_ones() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 400.0));
        let base = Placement {
            origin: pos2(400.0, 200.0),
            scale: 10.0,
        };
        let plan = Plan::for_view(rect, base, 1.0);
        assert!(plan.serves(Plan::for_view(rect, base, 1.0), rect, base));
        let nudged = Placement {
            origin: pos2(430.0, 190.0),
            ..base
        };
        assert!(plan.serves(Plan::for_view(rect, nudged, 1.0), rect, nudged));
        let far = Placement {
            origin: pos2(-600.0, 200.0),
            ..base
        };
        assert!(!plan.serves(Plan::for_view(rect, far, 1.0), rect, far));
        let closer = Placement {
            scale: 20.0,
            ..base
        };
        assert!(!plan.serves(Plan::for_view(rect, closer, 1.0), rect, closer));
        let farther = Placement { scale: 4.0, ..base };
        assert!(!plan.serves(Plan::for_view(rect, farther, 1.0), rect, farther));
    }

    /// Draw frames of the render until its worker has delivered a texture of
    /// the wanted resolution; returns that texture's plan.
    fn settle(ctx: &Context, view: &SideView, rect: Rect, placement: Placement) -> Plan {
        let wanted = Plan::for_view(rect, placement, ctx.pixels_per_point());
        for _ in 0..400 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let painter = ui.painter_at(rect);
                    paint(
                        ui,
                        &painter,
                        rect,
                        placement,
                        view,
                        "dark-accessible",
                        Projection::Front,
                    );
                });
            });
            if let Some(entry) =
                ctx.data(|data| data.get_temp::<Cached>(cache_id(Projection::Front)))
            {
                if entry.plan.serves(wanted, rect, placement) {
                    return entry.plan;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no texture of the wanted framing arrived");
    }

    /// The worker delivers a texture; zooming in re-renders it sharper and
    /// a nearby pan does not; the idle frame between them only draws.
    #[test]
    fn the_texture_arrives_and_is_re_rendered_only_when_the_framing_needs_it() {
        let view = preset_view();
        let ctx = Context::default();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(700.0, 420.0));
        let base = Placement {
            origin: pos2(350.0, 250.0),
            scale: 9.0,
        };
        let first = settle(&ctx, &view, rect, base);
        assert!((first.ppm - 9.0).abs() < 1e-9, "{first:?}");
        let before = ctx
            .data(|data| data.get_temp::<Cached>(cache_id(Projection::Front)))
            .expect("cached");
        let nudged = Placement {
            origin: pos2(360.0, 245.0),
            ..base
        };
        let same = settle(&ctx, &view, rect, nudged);
        assert_eq!(same, first, "a small pan reuses the texture");
        let after = ctx
            .data(|data| data.get_temp::<Cached>(cache_id(Projection::Front)))
            .expect("cached");
        assert_eq!(before.texture.id(), after.texture.id());
        let closer = Placement {
            scale: 36.0,
            ..base
        };
        let sharp = settle(&ctx, &view, rect, closer);
        assert!((sharp.ppm - 36.0).abs() < 1e-9, "{sharp:?}");
    }

    /// Cost of one re-render (the only expensive step; an idle frame draws a
    /// cached texture). Printed with `--nocapture`.
    #[test]
    // reason: the timing is the output, read with --nocapture.
    #[allow(clippy::print_stdout)]
    fn a_rerender_at_the_page_size_is_timed() {
        let view = preset_view();
        let airframe = view.airframe.as_ref().expect("airframe");
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 560.0));
        let placement = Placement {
            origin: pos2(500.0, 330.0),
            scale: 13.0,
        };
        let plan = Plan::for_view(rect, placement, 1.25);
        let mut times = Vec::new();
        let mut engine = Vec::new();
        for _ in 0..3 {
            let start = std::time::Instant::now();
            let scene =
                build_scene(Projection::Front, airframe, "dark-accessible", plan).expect("scene");
            let image = raster(&scene).expect("renders");
            times.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(image.size[0] > 1000);
            let start = std::time::Instant::now();
            let _ = crate::sandbox::scene::build_framed_view(
                airframe.geometry.clone(),
                airframe.design,
                "dark-accessible",
                plan.viewport_px,
                plan.centre,
                plan.ppm,
            )
            .expect("renders");
            engine.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        println!(
            "front render ms (engine scene + raster) {times:?}; engine scene alone {engine:?}"
        );
        if let Some(path) = std::env::var_os("ALAS_FRONT_RENDER_PNG") {
            let (scene, _) = crate::sandbox::scene::build_framed_view(
                airframe.geometry.clone(),
                airframe.design,
                "dark-accessible",
                plan.viewport_px,
                plan.centre,
                plan.ppm,
            )
            .expect("renders");
            let png = alas_viz::raster::render_scene_png(&scene).expect("raster");
            std::fs::write(path, png).expect("write png");
        }
    }
}
