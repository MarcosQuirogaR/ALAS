// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Solid, depth-sorted geometry for the interactive cabin preview.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_payload::geometry::CabinGeometry;
use alas_payload::layout::{DeckItem, ItemKind, ItemMeta, PayloadLayout, SeatMeta};

use crate::scene::{Camera3D, Color, Fill, Scene, SceneElement, Stroke};
use crate::theme::get_palette;

use super::cabin::item_color;
use super::cabin_assets::asset_for_item;
use super::wireframe::{draw_fuselage_wireframe, framing};

#[derive(Clone)]
struct Face3D {
    points: Vec<[f64; 3]>,
    color: Color,
    outline: Color,
}

fn shade(color: Color, factor: f64, alpha: u8) -> Color {
    let channel = |value: u8| (f64::from(value) * factor).clamp(0.0, 255.0) as u8;
    Color::rgba(channel(color.r), channel(color.g), channel(color.b), alpha)
}

fn cuboid_faces(center: [f64; 3], size: [f64; 3], color: Color) -> Vec<Face3D> {
    let [cx, cy, cz] = center;
    let [hx, hy, hz] = [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5];
    let corners = [
        [cx - hx, cy - hy, cz - hz],
        [cx + hx, cy - hy, cz - hz],
        [cx + hx, cy + hy, cz - hz],
        [cx - hx, cy + hy, cz - hz],
        [cx - hx, cy - hy, cz + hz],
        [cx + hx, cy - hy, cz + hz],
        [cx + hx, cy + hy, cz + hz],
        [cx - hx, cy + hy, cz + hz],
    ];
    let definitions = [
        ([0, 1, 2, 3], 0.68),
        ([4, 7, 6, 5], 1.08),
        ([0, 4, 5, 1], 0.82),
        ([1, 5, 6, 2], 0.96),
        ([2, 6, 7, 3], 0.76),
        ([3, 7, 4, 0], 0.90),
    ];
    definitions
        .into_iter()
        .map(|(indices, factor)| Face3D {
            points: indices.into_iter().map(|index| corners[index]).collect(),
            color: shade(color, factor, 230),
            outline: shade(color, 0.48, 255),
        })
        .collect()
}

fn seat_centers(meta: &SeatMeta) -> Vec<f64> {
    let blocks = if meta.blocks.is_empty() {
        vec![meta.abreast.max(0)]
    } else {
        meta.blocks.clone()
    };
    let seat_count = blocks.iter().sum::<i64>().max(0);
    let total_width =
        seat_count as f64 * meta.seat_w + blocks.len().saturating_sub(1) as f64 * meta.aisle_w;
    let mut cursor = -total_width * 0.5;
    let mut remaining = meta.filled.max(0);
    let mut centers = Vec::with_capacity(remaining as usize);
    for (block_index, block) in blocks.iter().enumerate() {
        for _ in 0..(*block).max(0) {
            if remaining > 0 {
                centers.push(cursor + meta.seat_w * 0.5);
                remaining -= 1;
            }
            cursor += meta.seat_w;
        }
        if block_index + 1 < blocks.len() {
            cursor += meta.aisle_w;
        }
    }
    centers
}

fn seat_faces(item: &DeckItem, meta: &SeatMeta, color: Color) -> Vec<Face3D> {
    let floor = item.z - item.height * 0.5;
    let seat_width = (meta.seat_w * 0.82).max(0.2);
    let cushion_length = (item.length * 0.54).max(0.22);
    let cushion_height = item.height.clamp(0.18, 0.48) * 0.30;
    let cushion_z = floor + item.height.min(0.48) - cushion_height * 0.5;
    let back_height = (item.height * 0.68).max(0.35);
    let back_length = (item.length * 0.13).clamp(0.08, 0.18);
    let back_x = item.x + cushion_length * 0.42;
    let mut faces = Vec::new();
    for y in seat_centers(meta) {
        let y0 = item.y + y - seat_width * 0.5;
        let y1 = item.y + y + seat_width * 0.5;
        let cushion_x0 = item.x - cushion_length * 0.5;
        let cushion_x1 = item.x + cushion_length * 0.5;
        let cushion_top = cushion_z + cushion_height * 0.5;
        let back_x0 = back_x - back_length * 0.5;
        let back_top = floor + back_height;
        faces.push(Face3D {
            points: vec![
                [cushion_x0, y0, cushion_top],
                [cushion_x1, y0, cushion_top],
                [cushion_x1, y1, cushion_top],
                [cushion_x0, y1, cushion_top],
            ],
            color,
            outline: shade(color, 0.62, 255),
        });
        faces.push(Face3D {
            points: vec![
                [back_x0, y0, floor],
                [back_x0, y0, back_top],
                [back_x0, y1, back_top],
                [back_x0, y1, floor],
            ],
            color: shade(color, 0.88, 255),
            outline: shade(color, 0.58, 255),
        });
    }
    faces
}

fn item_faces(item: &DeckItem) -> Vec<Face3D> {
    let color = item_color(item);
    if let (ItemKind::SeatRow, ItemMeta::Seat(meta)) = (item.kind, &item.meta) {
        return seat_faces(item, meta, color);
    }
    if let Some(asset) = asset_for_item(item) {
        let is_bin = item.kind == ItemKind::OverheadBin;
        return asset
            .faces()
            .into_iter()
            .enumerate()
            .map(|(index, points)| Face3D {
                points,
                // Alternating side shading makes the crown-mounted side bins
                // legible against the fuselage wireframe without inventing a
                // lighting or GPU dependency.
                color: shade(color, if index % 2 == 0 { 1.12 } else { 0.88 }, 240),
                outline: if is_bin {
                    Color::rgba(213, 216, 220, 235)
                } else {
                    shade(color, 0.48, 255)
                },
            })
            .collect();
    }
    cuboid_faces(
        [item.x, item.y, item.z],
        [
            item.length.max(0.06),
            item.width.max(0.06),
            item.height.max(0.06),
        ],
        color,
    )
}

fn floor_faces(cabin: &CabinGeometry, layout: &PayloadLayout) -> Vec<Face3D> {
    let mut faces = Vec::new();
    for deck in cabin
        .passenger_decks
        .iter()
        .chain(std::iter::once(&cabin.lower_deck))
    {
        if layout.by_deck(deck.name).is_empty() {
            continue;
        }
        for index in 0..16 {
            let fraction0 = index as f64 / 16.0;
            let fraction1 = (index + 1) as f64 / 16.0;
            let x0 = cabin.cabin_start_x + (cabin.cabin_end_x - cabin.cabin_start_x) * fraction0;
            let x1 = cabin.cabin_start_x + (cabin.cabin_end_x - cabin.cabin_start_x) * fraction1;
            let w0 = cabin.usable_width(deck, x0) * 0.5;
            let w1 = cabin.usable_width(deck, x1) * 0.5;
            faces.push(Face3D {
                points: vec![
                    [x0, -w0, cabin.floor_z(deck, x0)],
                    [x1, -w1, cabin.floor_z(deck, x1)],
                    [x1, w1, cabin.floor_z(deck, x1)],
                    [x0, w0, cabin.floor_z(deck, x0)],
                ],
                color: Color::rgba(52, 73, 94, 72),
                outline: Color::rgba(52, 152, 219, 150),
            });
        }
    }
    faces
}

fn draw_faces(
    scene: &mut Scene,
    mut faces: Vec<Face3D>,
    camera: &Camera3D,
    center: [f64; 3],
    span: f64,
    viewport: (f64, f64, f64, f64),
) {
    faces.sort_by(|a, b| {
        let depth = |face: &Face3D| {
            face.points
                .iter()
                .map(|&point| camera.view_depth(point, center))
                .sum::<f64>()
                / face.points.len() as f64
        };
        depth(a).total_cmp(&depth(b))
    });
    for face in faces {
        scene.add(SceneElement::Polygon {
            points: face
                .points
                .into_iter()
                .map(|point| camera.project(point, center, span, viewport))
                .collect(),
            fill: Some(Fill::new(face.color)),
            stroke: Some(Stroke::new(face.outline, 0.45)),
        });
    }
}

/// Project the physical cabin furniture into an orbitable solid preview.
pub fn figure_cabin_payload_3d(
    layout: &PayloadLayout,
    plane: &Airplane,
    config: &AlasConfig,
    camera: Option<Camera3D>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    if layout.items.is_empty() {
        return super::cabin::empty(theme, "No payload layout available");
    }
    let Ok(cabin) = CabinGeometry::new(
        plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    ) else {
        return super::cabin::empty(theme, "Cabin geometry unavailable");
    };
    let mut scene = Scene::new(800.0, 520.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Cabin / Payload - 3D".to_owned());
    let camera = camera.unwrap_or_default();
    let viewport = (20.0, 25.0, 760.0, 410.0);
    let mut fit_points = Vec::new();
    if let Some(fuselage) = plane.fuselages.first() {
        for section in &fuselage.xsecs {
            for index in 0..12 {
                let theta = std::f64::consts::TAU * index as f64 / 12.0;
                fit_points.push([
                    section.xyz_c[0],
                    section.xyz_c[1] + section.width * 0.5 * theta.cos(),
                    section.xyz_c[2] + section.height * 0.5 * theta.sin(),
                ]);
            }
        }
    }
    let (x0, x1, y0, y1, z0, z1) = super::shared::airplane_bbox(plane);
    let (fallback_center, _) = framing(x0, x1, y0, y1, z0, z1);
    let center = camera.fit_center_to_points(&fit_points, fallback_center);
    let span = camera.fit_span_to_points(&fit_points, viewport, 0.05);
    if let Some(fuselage) = plane.fuselages.first() {
        draw_fuselage_wireframe(
            &mut scene,
            &camera,
            center,
            span,
            viewport,
            fuselage,
            Color::rgba(127, 140, 141, 135),
        );
    }
    let mut faces = floor_faces(&cabin, layout);
    faces.extend(layout.items.iter().flat_map(item_faces));
    draw_faces(&mut scene, faces, &camera, center, span, viewport);
    scene
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_rendered_as_polygons(faces: Vec<Face3D>) {
        let mut scene = Scene::new(200.0, 120.0, None);
        draw_faces(
            &mut scene,
            faces,
            &Camera3D::default(),
            [0.0, 0.0, 0.0],
            20.0,
            (0.0, 0.0, 200.0, 120.0),
        );
        assert!(!scene.elements.is_empty());
        assert!(scene
            .elements
            .iter()
            .all(|element| matches!(element, SceneElement::Polygon { .. })));
        assert!(!scene
            .elements
            .iter()
            .any(|element| matches!(element, SceneElement::Circle { .. })));
    }

    fn item(kind: ItemKind, meta: ItemMeta) -> DeckItem {
        DeckItem {
            kind,
            deck: "main",
            x: 10.0,
            y: 0.0,
            z: 1.0,
            length: 0.8,
            width: 2.0,
            mass: 0.0,
            height: 1.0,
            label: String::new(),
            meta,
        }
    }

    #[test]
    fn a_seat_row_expands_into_solid_cushions_and_backrests() {
        let meta = SeatMeta {
            cls: "Economy",
            abreast: 2,
            filled: 2,
            deck: "main",
            aisles: 1,
            blocks: vec![1, 1],
            seat_w: 0.46,
            aisle_w: 0.51,
        };
        let faces = item_faces(&item(ItemKind::SeatRow, ItemMeta::Seat(meta)));
        assert_eq!(faces.len(), 4, "two seats each use two visible surfaces");
        assert!(faces.iter().all(|face| face.points.len() == 4));
        assert_rendered_as_polygons(faces);
    }

    #[test]
    fn a_monument_is_rendered_as_six_polygon_faces() {
        let faces = item_faces(&item(ItemKind::Galley, ItemMeta::None));
        assert_eq!(faces.len(), 6);
        assert!(faces.iter().all(|face| face.points.len() == 4));
        assert_rendered_as_polygons(faces);
    }

    #[test]
    fn overhead_bins_use_profile_extrusions_and_high_contrast_outlines() {
        use alas_payload::layout::{OverheadBinMeta, OverheadBinType};

        let faces = item_faces(&item(
            ItemKind::OverheadBin,
            ItemMeta::OverheadBin(OverheadBinMeta {
                bin_type: OverheadBinType::Sidewall,
            }),
        ));
        assert_eq!(faces.len(), 8, "six profile edges plus two end caps");
        assert!(faces
            .iter()
            .all(|face| face.outline == Color::rgba(213, 216, 220, 235)));
        assert_rendered_as_polygons(faces);
    }

    #[test]
    fn ulds_use_the_polygonal_contour_instead_of_a_rectangular_solid() {
        use alas_payload::layout::ContainerMeta;

        let faces = item_faces(&item(
            ItemKind::Uld,
            ItemMeta::Container(ContainerMeta {
                uld: "AKE",
                fill: 0.8,
                color: "#e74c3c",
                net: Some(900.0),
            }),
        ));
        // Eight normalized profile vertices produce two caps and one quad per
        // edge. A six-face cuboid would be the old rectangular fallback.
        assert_eq!(faces.len(), 10);
        assert!(faces.iter().any(|face| face.points.len() == 8));
        assert_rendered_as_polygons(faces);
    }
}
