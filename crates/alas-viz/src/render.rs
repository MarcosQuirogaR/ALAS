// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Conversion of [`Scene`] elements into `egui` painter shapes.

mod backend;
mod polygon;
mod transform;

pub use transform::{to_egui_color, to_egui_stroke, ViewportTransform};

use alas_report::scene::{visual_title, Scene, SceneElement};
use egui::Shape;
use rayon::prelude::*;

use backend::EguiBackend;

/// Elements per parallel chunk below which a scene is drawn on the calling
/// thread: a small chart gains nothing from the pool.
const PARALLEL_CHUNK: usize = 2048;

/// Tessellates shapes with the paint context's own settings, so a baked mesh
/// has the vertices egui itself would have made for the same shapes.
pub(crate) struct Baker {
    tessellator: egui::epaint::Tessellator,
}

impl Baker {
    /// A baker with `ctx`'s pixel density, tessellation options and font atlas.
    pub(crate) fn new(ctx: &egui::Context) -> Self {
        let (font_tex_size, prepared_discs) = ctx.fonts(|fonts| {
            (
                fonts.font_image_size(),
                fonts.texture_atlas().lock().prepared_discs(),
            )
        });
        let options = ctx.tessellation_options(|options| *options);
        Self {
            tessellator: egui::epaint::Tessellator::new(
                ctx.pixels_per_point(),
                options,
                font_tex_size,
                prepared_discs,
            ),
        }
    }

    /// Tessellate every run of non-text shapes into one mesh, keeping the
    /// painting order of the text between them.
    pub(crate) fn bake(&self, shapes: Vec<Shape>) -> Vec<Shape> {
        use egui::epaint::Mesh;
        let mut tessellator = self.tessellator.clone();
        let mut baked = Vec::new();
        let mut run = Mesh::default();
        let flush = |run: &mut Mesh, baked: &mut Vec<Shape>| {
            if !run.is_empty() {
                baked.push(Shape::Mesh(std::mem::take(run)));
            }
        };
        for shape in shapes {
            if matches!(shape, Shape::Text(_)) {
                flush(&mut run, &mut baked);
                baked.push(shape);
            } else {
                tessellator.tessellate_shape(shape, &mut run);
            }
        }
        flush(&mut run, &mut baked);
        baked
    }
}

/// Whether the vector-overlay path draws `element`: the textured elements
/// are the texture layer's.
fn is_vector(element: &SceneElement) -> bool {
    !matches!(
        element,
        SceneElement::Image { .. } | SceneElement::SphericalImage { .. }
    )
}

/// The shapes the vector-overlay path paints over the texture layer: the
/// scene without its textured elements and without its opaque background
/// paint (its `background` still decides the title's contrast), optionally
/// baked into meshes.
///
/// A dense scene is drawn in ordered chunks on the rayon pool, each chunk
/// with its own backend and tessellator, and the chunks are concatenated in
/// scene order. Every shape gets the vertices the sequential path gives it
/// and the paint order is unchanged, so the picture is the same; baked
/// meshes are split at chunk boundaries (see [`join_mesh_runs`]).
pub(crate) fn vector_overlay_shapes(
    scene: &Scene,
    transform: &ViewportTransform,
    context: &egui::Context,
    baker: Option<&Baker>,
) -> Vec<Shape> {
    let finish = |shapes: Vec<Shape>| match baker {
        Some(baker) => baker.bake(shapes),
        None => shapes,
    };
    let mut head = EguiBackend::with_capacity(transform, context.clone(), 1);
    if let Some(title) = visual_title(scene) {
        head.draw_scene_element(&title);
    }
    let elements: Vec<&SceneElement> = scene.elements.iter().filter(|e| is_vector(e)).collect();
    if elements.len() < 2 * PARALLEL_CHUNK {
        for element in elements {
            head.draw_scene_element(element);
        }
        return finish(head.shapes);
    }
    let chunk = PARALLEL_CHUNK.max(elements.len().div_ceil(4 * rayon::current_num_threads()));
    let parts: Vec<Vec<Shape>> = elements
        .par_chunks(chunk)
        .map(|part| {
            let mut backend = EguiBackend::with_capacity(transform, context.clone(), part.len());
            for element in part {
                backend.draw_scene_element(element);
            }
            finish(backend.shapes)
        })
        .collect();
    finish(head.shapes)
        .into_iter()
        .chain(parts.into_iter().flatten())
        .collect()
}

/// `shapes` with each run of consecutive meshes on one texture joined into
/// a single mesh; the paint order is unchanged.
///
/// egui paints one large mesh much faster than the chunk meshes of
/// [`vector_overlay_shapes`] (A380-800 dock, idle frame: 8.1 ms of
/// tessellation for about 64 chunk meshes, 3.7 ms joined, measured headless),
/// but joining copies every vertex once more (about 2 ms in the same frame).
/// A view repainted many times from the same baked shapes joins them once;
/// shapes drawn for a single frame are cheaper left as they are.
pub(crate) fn join_mesh_runs(shapes: Vec<Shape>) -> Vec<Shape> {
    let mut joined = Vec::new();
    let mut run: Vec<egui::epaint::Mesh> = Vec::new();
    for shape in shapes {
        match shape {
            Shape::Mesh(mesh)
                if run
                    .last()
                    .is_none_or(|last| last.texture_id == mesh.texture_id) =>
            {
                run.push(mesh);
            }
            shape => {
                if !run.is_empty() {
                    joined.push(Shape::Mesh(join_meshes(std::mem::take(&mut run))));
                }
                match shape {
                    Shape::Mesh(mesh) => run.push(mesh),
                    shape => joined.push(shape),
                }
            }
        }
    }
    if !run.is_empty() {
        joined.push(Shape::Mesh(join_meshes(run)));
    }
    joined
}

/// One mesh holding `meshes` (all on one texture) in order: the same
/// vertices, and each mesh's indices offset by the vertices before it.
/// The buffers are allocated once and filled in parallel, so joining costs
/// one copy spread over the pool rather than a growing serial append.
fn join_meshes(mut meshes: Vec<egui::epaint::Mesh>) -> egui::epaint::Mesh {
    use egui::epaint::{Mesh, Vertex};
    if meshes.len() == 1 {
        return meshes.pop().unwrap_or_default();
    }
    let total_vertices = meshes.iter().map(|mesh| mesh.vertices.len()).sum();
    let total_indices = meshes.iter().map(|mesh| mesh.indices.len()).sum();
    let mut vertices = vec![Vertex::default(); total_vertices];
    let mut indices = vec![0_u32; total_indices];
    let mut jobs = Vec::with_capacity(meshes.len());
    let (mut vertex_rest, mut index_rest) = (vertices.as_mut_slice(), indices.as_mut_slice());
    let mut offset = 0_u32;
    for mesh in &meshes {
        let (vertex_part, vertex_tail) = vertex_rest.split_at_mut(mesh.vertices.len());
        let (index_part, index_tail) = index_rest.split_at_mut(mesh.indices.len());
        jobs.push((mesh, vertex_part, index_part, offset));
        offset += mesh.vertices.len() as u32;
        (vertex_rest, index_rest) = (vertex_tail, index_tail);
    }
    jobs.into_par_iter()
        .for_each(|(mesh, vertex_part, index_part, offset)| {
            vertex_part.copy_from_slice(&mesh.vertices);
            for (out, index) in index_part.iter_mut().zip(&mesh.indices) {
                *out = index + offset;
            }
        });
    Mesh {
        indices,
        vertices,
        texture_id: meshes
            .first()
            .map_or_else(Default::default, |mesh| mesh.texture_id),
    }
}

/// Render all elements of a [`Scene`] to a list of [`Shape`] primitives ready for egui painting.
pub fn render_scene_to_shapes(scene: &Scene, transform: &ViewportTransform) -> Vec<Shape> {
    let context = egui::Context::default();
    let _ = context.run(egui::RawInput::default(), |_| {});
    render_scene_to_shapes_with_context(scene, transform, &context)
}

/// Render a scene using the font atlas of the context that will paint it.
///
/// A [`TextShape`] contains a galley whose texture identifiers belong to the
/// context that laid it out. The desktop path must therefore use the active UI
/// context rather than a private one, or axes and annotations become invisible
/// even though their shapes exist.
pub fn render_scene_to_shapes_with_context(
    scene: &Scene,
    transform: &ViewportTransform,
    context: &egui::Context,
) -> Vec<Shape> {
    let mut backend = EguiBackend::new(scene, transform, context.clone());

    if let (true, Some(bg)) = (scene.paint_background, scene.background) {
        backend.fill_rect(
            backend.snapped([0.0, 0.0]),
            backend.snapped([scene.width, scene.height]),
            bg,
        );
    }

    if let Some(title) = visual_title(scene) {
        backend.draw_scene_element(&title);
    }
    for elem in &scene.elements {
        backend.draw_scene_element(elem);
    }

    backend.shapes
}
