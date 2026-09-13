// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Embedded native application branding.
//!
//! The source PNGs live at the workspace root so the desktop shell and the
//! release package consume the same artwork.  This module keeps the loading
//! paths together and gives the native window a square, transparent canvas
//! for the wide three-stripe mark instead of allowing an operating system
//! icon API to distort it.
//!
//! The main symbol and the smaller symbol/wordmark are exposed as separate
//! images (rather than one fused composite) so callers such as the boot
//! splash can position each independently -- see
//! `alas-gui/src/views/overlays.rs::show_splash`.

use egui::IconData;

const APP_LOGO_PNG: &[u8] = include_bytes!("../../../app_logo.png");
const APP_TEXT_LOGO_PNG: &[u8] = include_bytes!("../../../app_text_logo.png");

/// Build the native options used by the production shell and GUI audit
/// examples, including the embedded ALAS window and task-bar icon.
pub fn native_options(
    title: impl Into<String>,
    inner_size: [f32; 2],
    min_inner_size: [f32; 2],
) -> eframe::NativeOptions {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(inner_size)
        .with_min_inner_size(min_inner_size);
    if let Some(icon) = icon_data() {
        viewport = viewport.with_icon(icon);
    }
    eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    }
}

/// Return the three-stripe mark as a native icon without changing its aspect
/// ratio.  The source mark is wide, while Windows and most desktop shells
/// expect a square icon; transparent padding preserves the source geometry.
fn icon_data() -> Option<IconData> {
    let source = eframe::icon_data::from_png_bytes(APP_LOGO_PNG).ok()?;
    if source.width == 0 || source.height == 0 {
        return None;
    }

    // IconData documents dimensions in multiples of four.  Round the square
    // canvas up while retaining every source pixel at its native aspect ratio.
    let side = source.width.max(source.height).checked_add(3)? / 4 * 4;
    let side_usize = side as usize;
    let source_width = source.width as usize;
    let source_height = source.height as usize;
    let row_bytes = source_width.checked_mul(4)?;
    let canvas_bytes = side_usize.checked_mul(side_usize)?.checked_mul(4)?;
    let mut rgba = vec![0; canvas_bytes];
    let x_offset = (side_usize - source_width) / 2;
    let y_offset = (side_usize - source_height) / 2;

    for row in 0..source_height {
        let source_start = row.checked_mul(row_bytes)?;
        let destination_start = ((row + y_offset) * side_usize + x_offset).checked_mul(4)?;
        rgba[destination_start..destination_start + row_bytes]
            .copy_from_slice(&source.rgba[source_start..source_start + row_bytes]);
    }

    Some(IconData {
        rgba,
        width: side,
        height: side,
    })
}

/// Embedded horizontal three-stripe and ALAS wordmark for native headers.
pub(crate) fn text_logo_image(ctx: &egui::Context) -> Option<egui::Image<'static>> {
    Some(egui::Image::from_texture(&cached_texture(
        ctx,
        "alas-native-text-logo",
        APP_TEXT_LOGO_PNG,
    )?))
}

/// Embedded three-stripe mark on its own, for surfaces that position it
/// independently of the wordmark (the boot splash's centred main symbol).
pub(crate) fn logo_image(ctx: &egui::Context) -> Option<egui::Image<'static>> {
    Some(egui::Image::from_texture(&cached_texture(
        ctx,
        "alas-native-logo",
        APP_LOGO_PNG,
    )?))
}

/// The three-stripe mark's source width and height, in pixels, so a caller
/// can fit it into a layout box without distorting its aspect ratio.
pub(crate) fn logo_natural_size() -> Option<egui::Vec2> {
    let decoded = eframe::icon_data::from_png_bytes(APP_LOGO_PNG).ok()?;
    if decoded.width == 0 || decoded.height == 0 {
        return None;
    }
    Some(egui::Vec2::new(decoded.width as f32, decoded.height as f32))
}

/// Decode an embedded PNG through eframe's already-linked PNG decoder and keep
/// one GPU texture per egui context.  egui's core intentionally has no PNG
/// image loader installed by default, so `Image::from_bytes` would render its
/// error glyph even though the bytes are present.
fn cached_texture(
    ctx: &egui::Context,
    texture_name: &'static str,
    bytes: &'static [u8],
) -> Option<egui::TextureHandle> {
    let id = egui::Id::new(texture_name);
    if let Some(texture) = ctx.data(|data| data.get_temp::<egui::TextureHandle>(id)) {
        return Some(texture);
    }

    let decoded = eframe::icon_data::from_png_bytes(bytes).ok()?;
    if decoded.width == 0 || decoded.height == 0 {
        return None;
    }
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [decoded.width as usize, decoded.height as usize],
        &decoded.rgba,
    );
    let texture = ctx.load_texture(texture_name, image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
    Some(texture)
}

#[cfg(test)]
mod tests {
    use super::{icon_data, logo_natural_size, APP_LOGO_PNG};

    #[test]
    fn logo_natural_size_matches_the_decoded_png_and_stays_wide() {
        let source = eframe::icon_data::from_png_bytes(APP_LOGO_PNG).expect("embedded logo PNG");
        let size = logo_natural_size().expect("embedded logo natural size");

        assert_eq!(size.x, source.width as f32);
        assert_eq!(size.y, source.height as f32);
        // The three-stripe mark is a wide horizontal glyph; a caller fitting
        // it by aspect ratio depends on that shape, not just nonzero size.
        assert!(size.x > size.y);
    }

    #[test]
    fn window_icon_is_square_and_keeps_transparent_padding() {
        let source = eframe::icon_data::from_png_bytes(APP_LOGO_PNG).expect("embedded logo PNG");
        let icon = icon_data().expect("embedded logo icon");

        assert_eq!(icon.width, icon.height);
        assert_eq!(icon.width % 4, 0);
        assert!(icon.width >= source.width.max(source.height));

        let top_left_alpha = icon.rgba[3];
        assert_eq!(top_left_alpha, 0);
    }
}
