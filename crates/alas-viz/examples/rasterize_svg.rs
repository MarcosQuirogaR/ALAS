// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rasterize an exported ALAS SVG for first-hand figure inspection.

use std::error::Error;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let source = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("supply an input SVG path")?;
    let destination = std::env::args_os()
        .nth(2)
        .map(PathBuf::from)
        .ok_or("supply an output PNG path")?;
    let svg = std::fs::read_to_string(&source)?;
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(&svg, &options)?;
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or("could not allocate PNG canvas")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    pixmap.save_png(destination)?;
    Ok(())
}
