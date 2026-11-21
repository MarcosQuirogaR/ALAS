// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Embed native Windows branding independently of runtime window decoration.

/// Stack reserve of the executable's main thread on Windows: 8 MiB, the
/// default of the other desktop platforms.
const MAIN_THREAD_STACK_BYTES: usize = 8 * 1024 * 1024;

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

    // The desktop interface runs on the main thread (the window system requires
    // it), and Windows gives that thread the 1 MiB the executable header
    // reserves. Start-up needs more than that in the harness profile: an
    // executable built in this profile ended with "thread 'main' has overflowed
    // its stack" before its window opened, and opened with a 2 MiB reserve.
    // Analyses already run on their own 256 MiB threads; this reserve is
    // address space only, committed page by page as it is used.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/STACK:{MAIN_THREAD_STACK_BYTES}");
    } else {
        println!("cargo:rustc-link-arg-bins=-Wl,--stack,{MAIN_THREAD_STACK_BYTES}");
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
