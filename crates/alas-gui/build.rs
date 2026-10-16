// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Link settings scoped to the examples.
//!
//! The visual-tour example (`render_gui_tour`) builds the whole shell state
//! and paints its first frames on the main thread, which winit requires, and
//! that overflows the 1 MiB default stack of a Windows process. The product
//! binary and the tests are not affected, so only the examples get a larger
//! stack, 256 MiB of reserved address space (committed on use).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc");
    if msvc {
        println!("cargo:rustc-link-arg-examples=/STACK:268435456");
    }
}
