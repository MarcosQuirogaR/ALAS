// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The exported SVG shares one embedded Blue Marble definition between image
//! references; the rasterized pixels must stay what the per-reference inline
//! form produced.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_report::{render_svg, Color, Scene, SceneElement};

fn rasterize(svg: &str, w: u32, h: u32) -> Vec<u8> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).expect("svg");
    let mut pixmap = tiny_skia::Pixmap::new(w, h).expect("canvas");
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    pixmap.data().to_vec()
}

fn texture_scene() -> Scene {
    let mut scene = Scene::new(400.0, 300.0, Some(Color::rgb(0, 0, 0)));
    let image = |x, y, width, height, source_rect| SceneElement::Image {
        source: "embedded://nasa-blue-marble".to_owned(),
        x,
        y,
        width,
        height,
        source_rect,
    };
    scene.add(image(10.0, 10.0, 180.0, 90.0, None));
    scene.add(image(10.0, 120.0, 200.0, 120.0, None));
    scene.add(image(
        220.0,
        20.0,
        160.0,
        100.0,
        Some([0.25, 0.25, 0.25, 0.25]),
    ));
    scene.add(image(
        220.0,
        150.0,
        160.0,
        120.0,
        Some([0.1, 0.3, 0.6, 0.5]),
    ));
    scene
}

#[test]
fn shared_texture_definition_rasterizes_like_inline_references() {
    let svg = render_svg(&texture_scene());
    assert_eq!(
        svg.matches("data:image/png;base64,").count(),
        1,
        "the texture data URI must appear once per document"
    );
    let pixels = rasterize(&svg, 400, 300);
    assert!(pixels.iter().any(|byte| *byte != 0));

    // The per-reference form this replaced, written out by hand.
    let start = svg.find("data:image/png;base64,").expect("data uri");
    let uri = &svg[start..start + svg[start..].find('"').expect("uri end")];
    let inline = |x: &str, y: &str, w: &str, h: &str, crop: Option<&str>| match crop {
        None => format!(
            r#"<image href="{uri}" x="{x}" y="{y}" width="{w}" height="{h}" preserveAspectRatio="xMidYMid meet"/>"#
        ),
        Some(view_box) => format!(
            r#"<svg x="{x}" y="{y}" width="{w}" height="{h}" viewBox="{view_box}" preserveAspectRatio="none" overflow="hidden"><image href="{uri}" x="0" y="0" width="1" height="1" preserveAspectRatio="none"/></svg>"#
        ),
    };
    let reference = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400.0 300.0" width="400.0" height="300.0"><rect width="400.0" height="300.0" fill="#000000" fill-opacity="1.000"/>{}{}{}{}</svg>"##,
        inline("10.00", "10.00", "180.00", "90.00", None),
        inline("10.00", "120.00", "200.00", "120.00", None),
        inline(
            "220.00",
            "20.00",
            "160.00",
            "100.00",
            Some("0.25000000 0.25000000 0.25000000 0.25000000")
        ),
        inline(
            "220.00",
            "150.00",
            "160.00",
            "120.00",
            Some("0.10000000 0.30000000 0.60000000 0.50000000")
        ),
    );
    let expected = rasterize(&reference, 400, 300);
    assert!(
        pixels == expected,
        "shared-definition pixels differ from the inline form"
    );
}
