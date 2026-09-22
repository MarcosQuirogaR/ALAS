// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Embed native Windows branding independently of runtime window decoration.

fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    println!("cargo:rerun-if-changed=../../app_icon.ico");

    // Check the target, not the build-script host, so non-Windows targets never
    // require an installed Windows resource compiler.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }

    winresource::WindowsResource::new()
        .set_icon("../../app_icon.ico")
        .set("ProductName", "ALAS")
        .set("FileDescription", "ALAS")
        .set("InternalName", "ALAS")
        .set("OriginalFilename", "ALAS.exe")
        .set("FileVersion", env!("CARGO_PKG_VERSION"))
        .set("ProductVersion", env!("CARGO_PKG_VERSION"))
        .set("CompanyName", env!("CARGO_PKG_AUTHORS"))
        .set("LegalCopyright", env!("CARGO_PKG_AUTHORS"))
        .set("Comments", env!("CARGO_PKG_LICENSE"))
        .compile()
}
