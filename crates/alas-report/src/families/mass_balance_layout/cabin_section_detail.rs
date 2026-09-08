// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reusable, backend-neutral vector assets for the transverse cabin view.

use crate::scene::{Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};

use super::cabin_section::SectionMap;

pub(super) fn polygon(
    scene: &mut Scene,
    points: impl IntoIterator<Item = [f64; 2]>,
    fill: Color,
    stroke: Color,
    width: f64,
) {
    scene.add(SceneElement::Polygon {
        points: points.into_iter().collect(),
        fill: Some(Fill::new(fill)),
        stroke: Some(Stroke::new(stroke, width)),
    });
}

pub(super) fn line(scene: &mut Scene, a: [f64; 2], b: [f64; 2], color: Color, width: f64) {
    scene.add(SceneElement::Line {
        p1: a,
        p2: b,
        stroke: Stroke::new(color, width),
    });
}

pub(super) fn rect(
    scene: &mut Scene,
    map: SectionMap,
    bounds: [f64; 4],
    fill: Color,
    stroke: Color,
    radius: f64,
) {
    let a = map.point(bounds[0], bounds[3]);
    let b = map.point(bounds[1], bounds[2]);
    scene.add(SceneElement::Rect {
        x: a[0],
        y: a[1],
        width: b[0] - a[0],
        height: b[1] - a[1],
        rx: radius,
        fill: Some(Fill::new(fill)),
        stroke: Some(Stroke::new(stroke, 0.9)),
    });
}

pub(super) fn ellipse(map: SectionMap, width: f64, height: f64, zc: f64) -> Vec<[f64; 2]> {
    (0..=128)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / 128.0;
            map.point(width * 0.5 * t.cos(), zc + height * 0.5 * t.sin())
        })
        .collect()
}

/// Structural skin, insulation blanket, air gap and moulded liner.
pub(super) fn shell(
    scene: &mut Scene,
    map: SectionMap,
    width: f64,
    height: f64,
    zc: f64,
    wall: f64,
) {
    let layers = [
        (0.0, "#748091", "#d8e0e8"),
        (wall * 0.18, "#d4a84f", "#f5d889"),
        (wall * 0.48, "#35404a", "#53616d"),
        (wall * 0.72, "#d9dee3", "#f7f9fb"),
    ];
    for (inset, fill, edge) in layers {
        polygon(
            scene,
            ellipse(
                map,
                (width - 2.0 * inset).max(0.2),
                (height - 2.0 * inset).max(0.2),
                zc,
            ),
            Color::from_hex(fill),
            Color::from_hex(edge),
            1.0,
        );
    }
    polygon(
        scene,
        ellipse(
            map,
            (width - 2.0 * wall).max(0.2),
            (height - 2.0 * wall).max(0.2),
            zc,
        ),
        Color::from_hex("#151a20"),
        Color::from_hex("#eef2f5"),
        1.1,
    );
}

/// A transverse cut through the window belt: outer pane, pressure pane, reveal and trim.
pub(super) fn windows(
    scene: &mut Scene,
    map: SectionMap,
    width: f64,
    height: f64,
    zc: f64,
    floor: f64,
) {
    // Fallback datum: the aperture centre is 0.95 m above the local deck
    // floor.  Keep this isolated so a future station-indexed window datum can
    // be passed here without changing the drawing asset.
    let z = window_center_z(floor, height, zc);
    let normalized = ((z - zc) / (height * 0.5)).clamp(-0.92, 0.92);
    let y = width * 0.5 * (1.0 - normalized * normalized).sqrt();
    for side in [-1.0, 1.0] {
        // Ellipse outward normal and tangent.  The complete window stack is
        // constructed in this local frame, so it follows the fuselage radius.
        let yc = side * y;
        let ny0 = yc / (width * width * 0.25);
        let nz0 = (z - zc) / (height * height * 0.25);
        let nlen = ny0.hypot(nz0);
        let (ny, nz) = (ny0 / nlen, nz0 / nlen);
        let (ty, tz) = (-nz, ny);
        let local = |t: f64, n: f64| [yc + ty * t + ny * n, z + tz * t + nz * n];
        let outer = [
            local(-0.25, -0.01),
            local(0.25, -0.01),
            local(0.25, -0.14),
            local(-0.25, -0.14),
        ];
        polygon(
            scene,
            outer.into_iter().map(|p| map.point(p[0], p[1])),
            Color::from_hex("#263a47"),
            Color::from_hex("#d8e3e8"),
            1.2,
        );
        polygon(
            scene,
            [
                local(-0.19, -0.025),
                local(0.19, -0.025),
                local(0.19, -0.095),
                local(-0.19, -0.095),
            ]
            .into_iter()
            .map(|p| map.point(p[0], p[1])),
            Color::from_hex("#8fc3d6"),
            Color::from_hex("#eef8fb"),
            1.0,
        );
        // Pressure pane and reveal sides make the wall thickness legible.
        line(
            scene,
            map.point(local(-0.18, -0.105)[0], local(-0.18, -0.105)[1]),
            map.point(local(0.18, -0.105)[0], local(0.18, -0.105)[1]),
            Color::from_hex("#d8f1f7"),
            1.0,
        );
        line(
            scene,
            map.point(local(-0.25, -0.02)[0], local(-0.25, -0.02)[1]),
            map.point(local(-0.19, -0.14)[0], local(-0.19, -0.14)[1]),
            Color::from_hex("#abb5bd"),
            1.2,
        );
        line(
            scene,
            map.point(local(0.25, -0.02)[0], local(0.25, -0.02)[1]),
            map.point(local(0.19, -0.14)[0], local(0.19, -0.14)[1]),
            Color::from_hex("#abb5bd"),
            1.2,
        );
    }
}

pub(super) fn window_center_z(floor: f64, height: f64, zc: f64) -> f64 {
    (floor + 0.95).clamp(zc - height * 0.40, zc + height * 0.40)
}

pub(super) fn label(scene: &mut Scene, value: impl Into<String>, pos: [f64; 2], size: f64) {
    scene.add(SceneElement::Text {
        text: value.into(),
        pos,
        font_size: size,
        color: Color::from_hex("#f4f7f8"),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });
}

pub(super) fn seat(
    scene: &mut Scene,
    map: SectionMap,
    y: f64,
    floor: f64,
    width: f64,
    color: Color,
) {
    let h = width * 0.42;
    let frame = Color::from_hex("#aab2b9");
    // Pedestal, legs and cross brace.
    for dy in [-h * 0.58, h * 0.58] {
        line(
            scene,
            map.point(y + dy, floor + 0.04),
            map.point(y + dy * 0.76, floor + 0.31),
            frame,
            2.0,
        );
    }
    line(
        scene,
        map.point(y - h * 0.6, floor + 0.12),
        map.point(y + h * 0.6, floor + 0.12),
        frame,
        1.3,
    );
    // Pan, back shell, cushions and headrest.
    rect(
        scene,
        map,
        [y - h, y + h, floor + 0.28, floor + 0.48],
        Color::from_hex("#232c35"),
        frame,
        4.0,
    );
    rect(
        scene,
        map,
        [y - h * 0.9, y + h * 0.9, floor + 0.36, floor + 1.02],
        Color::from_hex("#29343e"),
        frame,
        6.0,
    );
    rect(
        scene,
        map,
        [y - h * 0.82, y + h * 0.82, floor + 0.43, floor + 0.68],
        color,
        Color::from_hex("#182028"),
        5.0,
    );
    rect(
        scene,
        map,
        [y - h * 0.78, y + h * 0.78, floor + 0.70, floor + 0.94],
        color,
        Color::from_hex("#182028"),
        5.0,
    );
    rect(
        scene,
        map,
        [y - h * 0.70, y + h * 0.70, floor + 0.94, floor + 1.10],
        Color::from_hex("#d8dde1"),
        Color::from_hex("#57616a"),
        4.0,
    );
    line(
        scene,
        map.point(y - h, floor + 0.59),
        map.point(y - h * 1.2, floor + 0.59),
        frame,
        2.0,
    );
    line(
        scene,
        map.point(y + h, floor + 0.59),
        map.point(y + h * 1.2, floor + 0.59),
        frame,
        2.0,
    );
}

pub(super) fn person(scene: &mut Scene, map: SectionMap, y: f64, floor: f64, height: f64) {
    let suit = Color::from_hex("#91a0a8");
    scene.add(SceneElement::Circle {
        center: map.point(y, floor + height * 0.91),
        radius: height * map.scale * 0.065,
        fill: Some(Fill::new(Color::from_hex("#c99b7a"))),
        stroke: Some(Stroke::new(Color::from_hex("#4c5860"), 0.8)),
    });
    polygon(
        scene,
        [
            [y - height * 0.11, floor + height * 0.72],
            [y + height * 0.11, floor + height * 0.72],
            [y + height * 0.08, floor + height * 0.38],
            [y - height * 0.08, floor + height * 0.38],
        ]
        .into_iter()
        .map(|p| map.point(p[0], p[1])),
        suit,
        Color::from_hex("#59656d"),
        1.0,
    );
    for dy in [-0.055, 0.055] {
        line(
            scene,
            map.point(y + height * dy, floor),
            map.point(y + height * dy * 0.75, floor + height * 0.39),
            suit,
            4.2,
        );
    }
    for dy in [-0.12, 0.12] {
        line(
            scene,
            map.point(y + height * dy, floor + height * 0.42),
            map.point(y + height * dy * 0.82, floor + height * 0.70),
            suit,
            3.2,
        );
    }
}

pub(super) fn floor(scene: &mut Scene, map: SectionMap, half_width: f64, z: f64) {
    rect(
        scene,
        map,
        [-half_width, half_width, z - 0.075, z + 0.035],
        Color::from_hex("#7a858c"),
        Color::from_hex("#e2e7ea"),
        0.0,
    );
    rect(
        scene,
        map,
        [-half_width, half_width, z + 0.036, z + 0.07],
        Color::from_hex("#303940"),
        Color::from_hex("#65727a"),
        0.0,
    );
    let mut y = -half_width + 0.08;
    while y < half_width {
        line(
            scene,
            map.point(y, z - 0.07),
            map.point(y, z + 0.03),
            Color::from_hex("#aeb8be"),
            0.55,
        );
        y += 0.18;
    }
}
