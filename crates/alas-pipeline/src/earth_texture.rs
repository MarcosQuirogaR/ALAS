// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Optional Earth texture for globe and map figures.
//!
//! ALAS does not bundle an Earth raster. A user who wants textured globe and
//! map figures sets `ALAS_EARTH_TEXTURE` to the path of a local equirectangular
//! PNG (for example a copy of the NASA Blue Marble, a public-domain image).
//! Without it, or when the file cannot be read, figures render the plain
//! vector globe and flat map.

use std::sync::OnceLock;

/// Environment variable that names the optional Earth texture PNG.
pub const EARTH_TEXTURE_ENV: &str = "ALAS_EARTH_TEXTURE";

/// Raw bytes of the user-provided Earth texture, read once per process.
/// The one warning for an unreadable configured file goes to stderr by design.
#[allow(clippy::print_stderr)]
pub fn earth_texture_png() -> Option<&'static [u8]> {
    static BYTES: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    BYTES
        .get_or_init(|| {
            let path = std::env::var_os(EARTH_TEXTURE_ENV)?;
            match std::fs::read(&path) {
                Ok(bytes) => Some(bytes),
                Err(error) => {
                    eprintln!(
                        "warning: {EARTH_TEXTURE_ENV} is set but {} cannot be read ({error}); rendering without the Earth texture",
                        std::path::Path::new(&path).display()
                    );
                    None
                }
            }
        })
        .as_deref()
}
