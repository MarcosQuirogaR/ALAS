// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! End-to-end test of the one-click NASTRAN-95 build: the installer
//! unpacks a source archive, patches it, compiles it with the platform toolchain,
//! prunes it, runs the real closed-form check (`alas-struct`) and installs it.
//!
//! The test runs the **bundled manifest entry** (pinned hashes, build steps, patch,
//! keep list and check name); only the download URLs are replaced by loopback ones
//! that serve local copies of the real archives, so a local file that does not hash
//! to the shipped pin fails the install. Ignored by default: it needs
//!
//! * `ALAS_TEST_NASTRAN95_SOURCE`: a `.tar.gz` of the pinned source revision;
//! * Windows only: `ALAS_TEST_TOOLCHAIN_ZIP`, the winlibs GCC zip that the bundled
//!   manifest pins (gfortran, CMake and Ninja inside);
//! * Linux: `gfortran`, `cmake` and `ninja` on `PATH`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::thread;

use alas_exec::installer::{
    current_platform, install_plan, run_install, ConsentReceipt, InstallContext, Manifest,
    PlanItemState, ToolResult, TransferOptions,
};
use alas_struct::nastran95::{installation_self_check, INSTALL_CHECK_NAME};

/// Serve `body` on loopback for every request (no ranges: a fresh download).
fn serve(body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut request = Vec::new();
            let mut byte = [0_u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                if stream.read(&mut byte).unwrap_or(0) == 0 {
                    break;
                }
                request.push(byte[0]);
            }
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    format!("http://127.0.0.1:{port}/archive")
}

/// The bundled manifest with its `nastran95` download URLs replaced by loopback
/// servers that serve the given local copies. Hashes, sizes, steps, patch, keep
/// list and check name are the shipped ones.
fn bundled_manifest_served_locally(source: Vec<u8>, toolchain: Option<Vec<u8>>) -> Manifest {
    let mut manifest = Manifest::bundled().expect("bundled manifest");
    let platform = current_platform();
    let entry = manifest
        .tools
        .iter_mut()
        .find(|tool| tool.id == "nastran95")
        .expect("the bundled manifest has nastran95");
    entry
        .artifacts
        .get_mut(platform)
        .expect("a bundled source artifact for this platform")
        .url = serve(source);
    if let Some(zip) = toolchain {
        let build = entry.build.as_mut().expect("a build recipe");
        let archives = build
            .toolchain
            .get_mut(platform)
            .expect("a bundled toolchain for this platform");
        archives[0].url = serve(zip);
    }
    manifest
}

/// The shipped recipe names the check the application registers, and every
/// platform that builds has a source pin and steps.
#[test]
fn the_bundled_recipe_names_the_check_the_application_registers() {
    let manifest = Manifest::bundled().expect("bundled manifest");
    let entry = manifest.tool("nastran95").expect("nastran95");
    let build = entry.build.as_ref().expect("build recipe");
    assert_eq!(build.check.as_deref(), Some(INSTALL_CHECK_NAME));
    for platform in ["windows-x86_64", "linux-x86_64"] {
        assert!(entry.artifacts[platform].sha256.is_some(), "{platform}");
        assert!(!build.steps[platform].is_empty(), "{platform}");
    }
    assert!(build.toolchain["windows-x86_64"][0].sha256.is_some());
}

#[test]
#[ignore = "needs ALAS_TEST_NASTRAN95_SOURCE (the real pinned source tar.gz) and a compiler toolchain"]
fn the_installer_builds_nastran95_and_the_closed_form_check_passes() {
    // `ALAS_TEST_STRIP_PATH=1` leaves only the Windows system folders on PATH, so a
    // system CMake/Ninja/gfortran cannot stand in for the pinned toolchain's.
    if std::env::var_os("ALAS_TEST_STRIP_PATH").is_some() && cfg!(windows) {
        std::env::set_var("PATH", r"C:\Windows\System32;C:\Windows");
    }
    let source = std::fs::read(std::env::var("ALAS_TEST_NASTRAN95_SOURCE").expect("source path"))
        .expect("read the source archive");
    let toolchain = cfg!(windows).then(|| {
        std::fs::read(std::env::var("ALAS_TEST_TOOLCHAIN_ZIP").expect("toolchain zip"))
            .expect("read the toolchain zip")
    });
    let manifest = bundled_manifest_served_locally(source, toolchain);

    let root: PathBuf = std::env::temp_dir().join(format!("alas-n95-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut ctx =
        InstallContext::new(&root).with_check(INSTALL_CHECK_NAME, installation_self_check);
    ctx.options = TransferOptions {
        allow_loopback_http: true,
    };
    let only = vec!["nastran95".to_owned()];
    let plan = install_plan(&manifest, &ctx, Some(&only));
    assert_eq!(
        plan.items[0].state,
        PlanItemState::Install,
        "{:?}",
        plan.items[0]
    );
    let consent = ConsentReceipt::for_plan(&plan);
    let cancel = AtomicBool::new(false);
    let reports = run_install(&manifest, &ctx, &plan, &consent, &cancel, &mut |_| {}).expect("run");
    let ToolResult::Installed { executable, health } = &reports[0].result else {
        panic!("expected a real build, got {:?}", reports[0].result);
    };
    assert!(executable.is_file());
    assert!(
        health.detail.contains("matches beam theory"),
        "{}",
        health.detail
    );
    let record = std::fs::read_to_string(ctx.tools_dir().join("NASTRAN-95/ALAS-BUILD.txt"))
        .expect("build record");
    assert!(
        record.contains("060ed900") && record.contains("not redistributable"),
        "{record}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
