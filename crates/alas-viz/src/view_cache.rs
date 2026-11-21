// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What a scene view keeps between frames: the rasterized texture of a scene,
//! and the vector overlay baked into meshes.

use alas_report::scene::Scene;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use egui::{Id, TextureHandle, Ui};

use crate::raster::{render_scene_rgba_scaled, render_scene_textures_rgba_scaled};
use crate::render::{join_mesh_runs, vector_overlay_shapes, Baker, ViewportTransform};

#[derive(Clone)]
struct CachedSceneTexture {
    scene_hash: u64,
    texture: TextureHandle,
}

/// One cached GPU texture per view, refreshed through `render` whenever the
/// scene revision changes. `render` returning `None` means the scene has
/// nothing to rasterize on this path, and no texture is created.
pub(crate) fn cached_scene_texture(
    scene: &Scene,
    ui: &Ui,
    raster_scale: f64,
    cache_id: Option<Id>,
    cache_revision: Option<u64>,
    render: fn(&Scene, f64) -> Option<egui::ColorImage>,
) -> Option<TextureHandle> {
    // The supersampled raster must fit the backend's largest texture: a scene
    // wider than half of it at the 2x scale would otherwise abort the frame
    // (egui panics on an oversized texture). Lower the scale, never the scene.
    let raster_scale = max_fitting_scale(
        raster_scale,
        scene.width.max(scene.height),
        ui.ctx().input(|input| input.max_texture_side),
    );
    let scene_hash = cache_revision.unwrap_or_else(|| {
        let mut hasher = DefaultHasher::new();
        format!("{scene:?}").hash(&mut hasher);
        hasher.finish()
    });
    let id = cache_id.unwrap_or_else(|| Id::new(("alas_scene_png", scene_hash)));
    if let Some(mut cached) = ui
        .ctx()
        .data(|data| data.get_temp::<CachedSceneTexture>(id))
    {
        if cached.scene_hash == scene_hash {
            return Some(cached.texture);
        }
        let image = render(scene, raster_scale)?;
        cached.texture.set(image, egui::TextureOptions::LINEAR);
        cached.scene_hash = scene_hash;
        let texture = cached.texture.clone();
        ui.ctx().data_mut(|data| data.insert_temp(id, cached));
        return Some(texture);
    }
    // Result cards are commonly displayed larger than the authored scene;
    // supersampling here keeps labels and one-pixel plot strokes legible.
    let image = render(scene, raster_scale)?;
    let texture = ui.ctx().load_texture(
        format!("alas-scene-png-{scene_hash}"),
        image,
        egui::TextureOptions::LINEAR,
    );
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            id,
            CachedSceneTexture {
                scene_hash,
                texture: texture.clone(),
            },
        )
    });
    Some(texture)
}

/// Rasterizations running on the background pool and not yet finished.
/// A pool of the preview's own. The pipeline keeps the global rayon pool busy
/// for minutes during a run, and a preview raster queued behind it never ran,
/// so the 3D preview did not repaint after an orbit or a tab switch until the
/// run ended. The nested parallel iterators of the renderer run on this pool.
static RASTER_POOL: std::sync::OnceLock<Option<rayon::ThreadPool>> = std::sync::OnceLock::new();

fn spawn_raster(job: impl FnOnce() + Send + 'static) {
    let pool = RASTER_POOL.get_or_init(|| {
        let threads = std::thread::available_parallelism()
            .map_or(2, |n| n.get())
            .clamp(2, 4);
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|index| format!("alas-raster-{index}"))
            .build()
            .ok()
    });
    match pool {
        Some(pool) => pool.spawn(job),
        None => rayon::spawn(job),
    }
}

static PENDING_BACKGROUND: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How many background rasterizations are still running. A view that asked for
/// one shows a placeholder until its texture is uploaded, so a caller that
/// waits for "nothing pending" (the performance tour) polls this.
pub fn pending_background_rasters() -> usize {
    PENDING_BACKGROUND.load(std::sync::atomic::Ordering::Relaxed)
}

/// Whether `ALAS_PERF_TRACE` asks for timing lines on stderr.
fn trace_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("ALAS_PERF_TRACE").is_some_and(|value| !value.is_empty()))
}

enum RasterSlot {
    Running,
    Done(Option<egui::ColorImage>),
    /// Uploaded (or failed): nothing left to collect for this revision.
    Taken,
}

#[derive(Clone)]
struct PendingRaster {
    scene_hash: u64,
    slot: Arc<Mutex<RasterSlot>>,
}

/// Like [`cached_scene_texture`], but a texture that is not cached yet is
/// rasterized on the rayon pool instead of inside the frame. While it renders
/// this returns the previous texture of the same view (a scene that changed)
/// or `None` (a view that has never shown one), and asks for a repaint when
/// the image is ready. A scene whose rasterization fails is not retried for
/// the same revision.
pub(crate) fn background_scene_texture(
    scene: &Scene,
    ui: &Ui,
    raster_scale: f64,
    cache_id: Option<Id>,
    cache_revision: Option<u64>,
    render: fn(&Scene, f64) -> Option<egui::ColorImage>,
) -> Option<TextureHandle> {
    let ctx = ui.ctx();
    let raster_scale = max_fitting_scale(
        raster_scale,
        scene.width.max(scene.height),
        ctx.input(|input| input.max_texture_side),
    );
    let scene_hash = cache_revision.unwrap_or_else(|| {
        let mut hasher = DefaultHasher::new();
        format!("{scene:?}").hash(&mut hasher);
        hasher.finish()
    });
    let id = cache_id.unwrap_or_else(|| Id::new(("alas_scene_png", scene_hash)));
    let pending_id = id.with("background_raster");
    let cached = ctx.data(|data| data.get_temp::<CachedSceneTexture>(id));
    if let Some(cached) = &cached {
        if cached.scene_hash == scene_hash {
            return Some(cached.texture.clone());
        }
    }
    let pending = ctx.data(|data| data.get_temp::<PendingRaster>(pending_id));
    if let Some(pending) = pending.filter(|pending| pending.scene_hash == scene_hash) {
        let finished = match pending.slot.lock() {
            Ok(mut slot) => match std::mem::replace(&mut *slot, RasterSlot::Taken) {
                RasterSlot::Running => {
                    *slot = RasterSlot::Running;
                    None
                }
                RasterSlot::Done(image) => Some(image),
                RasterSlot::Taken => Some(None),
            },
            Err(_) => Some(None),
        };
        let Some(image) = finished else {
            return cached.map(|cached| cached.texture);
        };
        let Some(image) = image else {
            return cached.map(|cached| cached.texture);
        };
        let texture = match cached {
            Some(mut cached) => {
                cached.texture.set(image, egui::TextureOptions::LINEAR);
                cached.scene_hash = scene_hash;
                let texture = cached.texture.clone();
                ctx.data_mut(|data| data.insert_temp(id, cached));
                texture
            }
            None => {
                let texture = ctx.load_texture(
                    format!("alas-scene-png-{scene_hash}"),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                ctx.data_mut(|data| {
                    data.insert_temp(
                        id,
                        CachedSceneTexture {
                            scene_hash,
                            texture: texture.clone(),
                        },
                    )
                });
                texture
            }
        };
        return Some(texture);
    }
    let slot = Arc::new(Mutex::new(RasterSlot::Running));
    ctx.data_mut(|data| {
        data.insert_temp(
            pending_id,
            PendingRaster {
                scene_hash,
                slot: Arc::clone(&slot),
            },
        )
    });
    PENDING_BACKGROUND.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let scene = scene.clone();
    let repaint = ctx.clone();
    spawn_raster(move || {
        let started = std::time::Instant::now();
        let image = render(&scene, raster_scale);
        if trace_enabled() {
            trace_raster(&scene, started.elapsed());
        }
        if let Ok(mut slot) = slot.lock() {
            *slot = RasterSlot::Done(image);
        }
        PENDING_BACKGROUND.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        repaint.request_repaint();
    });
    cached.map(|cached| cached.texture)
}

// Diagnostic output on request; stderr is the tour's only channel.
#[allow(clippy::print_stderr)]
fn trace_raster(scene: &Scene, elapsed: std::time::Duration) {
    eprintln!(
        "raster {:.0}x{:.0} pt, {} elements: {:.1} ms",
        scene.width,
        scene.height,
        scene.elements.len(),
        elapsed.as_secs_f64() * 1e3
    );
}

pub(crate) fn scene_color_image(scene: &Scene, raster_scale: f64) -> Option<egui::ColorImage> {
    let (width, height, rgba) = render_scene_rgba_scaled(scene, raster_scale).ok()?;
    Some(egui::ColorImage::from_rgba_premultiplied(
        [width as usize, height as usize],
        &rgba,
    ))
}

/// The textured elements alone, on a transparent canvas; `None` when the
/// scene has none.
pub(crate) fn textures_color_image(scene: &Scene, raster_scale: f64) -> Option<egui::ColorImage> {
    let (width, height, rgba) = render_scene_textures_rgba_scaled(scene, raster_scale)?.ok()?;
    Some(egui::ColorImage::from_rgba_premultiplied(
        [width as usize, height as usize],
        &rgba,
    ))
}

/// The overlay shapes of one frame, tessellated once per scene revision.
///
/// egui tessellates every shape on every frame, outside `App::update`. A dense
/// aircraft is about 70 000 shapes, which cost 12-13 ms per frame (measured)
/// even when nothing moved. With a caller-owned cache key and revision the
/// non-text shapes are baked into shared meshes keyed by the revision, the
/// viewport transform and the pixel density, so an idle frame only copies
/// vertices. Text stays a shape: its glyph uvs belong to the live font atlas.
///
/// The meshes are kept only once a key is seen on a second frame. A camera
/// drag makes a new key every frame, and keeping each frame's meshes would
/// copy every vertex once more per frame (the A380-800 preview bakes about
/// 440 000 vertices, 1-2 ms to copy) for meshes no later frame reads; the
/// view that settles is baked once more and kept from then on.
pub(crate) fn overlay_shapes(
    scene: &Scene,
    transform: &ViewportTransform,
    ctx: &egui::Context,
    cache_id: Option<Id>,
    cache_revision: Option<u64>,
) -> Vec<egui::Shape> {
    let (Some(cache_id), Some(revision)) = (cache_id, cache_revision) else {
        return vector_overlay_shapes(scene, transform, ctx, None);
    };
    let id = cache_id.with("baked_overlay");
    let key = BakedKey {
        revision,
        transform: [
            transform.scale.to_bits(),
            transform.offset_x.to_bits(),
            transform.offset_y.to_bits(),
            transform.target_rect.width().to_bits(),
            transform.target_rect.height().to_bits(),
        ],
        pixels_per_point: ctx.pixels_per_point().to_bits(),
    };
    let seen = ctx.data(|data| data.get_temp::<Arc<BakedOverlay>>(id));
    let settled = match seen.as_deref() {
        Some(BakedOverlay {
            key: seen_key,
            shapes: Some(shapes),
        }) if *seen_key == key => return shapes.clone(),
        Some(BakedOverlay { key: seen_key, .. }) => *seen_key == key,
        None => false,
    };
    let shapes = vector_overlay_shapes(scene, transform, ctx, Some(&Baker::new(ctx)));
    // Kept shapes are repainted every idle frame: join their chunk meshes once.
    let shapes = if settled {
        join_mesh_runs(shapes)
    } else {
        shapes
    };
    let kept = settled.then(|| shapes.clone());
    ctx.data_mut(|data| data.insert_temp(id, Arc::new(BakedOverlay { key, shapes: kept })));
    shapes
}

#[derive(Clone, PartialEq)]
struct BakedKey {
    revision: u64,
    transform: [u32; 5],
    pixels_per_point: u32,
}

#[derive(Clone)]
struct BakedOverlay {
    key: BakedKey,
    /// The baked shapes, once the key has been seen on a second frame.
    shapes: Option<Vec<egui::Shape>>,
}

/// The scene without its textured elements, and with its opaque background
/// paint suppressed: the reference the vector-overlay path draws the same
/// shapes as without cloning the scene. `background` itself stays set so
/// `visual_title`'s contrast decision still sees the true theme color.
#[cfg(test)]
pub(crate) fn vector_overlay_scene(scene: &Scene) -> Scene {
    let mut overlay = scene.clone();
    overlay.hide_background_paint();
    overlay.elements.retain(|element| {
        !matches!(
            element,
            alas_report::scene::SceneElement::Image { .. }
                | alas_report::scene::SceneElement::SphericalImage { .. }
        )
    });
    overlay
}

/// `requested`, lowered when needed so a scene whose longest side is
/// `longest_pt` rasterizes to at most `max_texture_side` pixels (the raster
/// rounds the side, so the bound is kept half a pixel inside).
fn max_fitting_scale(requested: f64, longest_pt: f64, max_texture_side: usize) -> f64 {
    let longest = longest_pt.max(1.0);
    let fitting = (max_texture_side as f64 - 0.5) / longest;
    requested.min(fitting)
}

#[cfg(test)]
mod raster_scale_tests {
    use super::max_fitting_scale;

    #[test]
    fn a_wide_scene_is_supersampled_only_up_to_the_largest_texture() {
        // A 1035 pt scene at 2x is 2070 px, over the 2048 px egui default.
        let scale = max_fitting_scale(2.0, 1035.0, 2048);
        assert!(scale < 2.0);
        assert!((1035.0 * scale).round() <= 2048.0);
        // A scene that fits keeps the requested supersampling.
        assert_eq!(max_fitting_scale(2.0, 800.0, 2048), 2.0);
    }
}
