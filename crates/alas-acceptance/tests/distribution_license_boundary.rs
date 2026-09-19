// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in license-boundary acceptance for an assembled standalone package.
//!
//! `xtask`'s own tests cover the packaging *code*: that the tool inventory
//! names the never-bundled programs, that incomplete NASTRAN-95 staging is
//! reported rather than filled in, and that the source allowlist rejects
//! `external tools/`. This suite checks the *produced artifact* instead, which
//! is the claim a download actually makes: what the package directory
//! contains, and whether its manifest describes that content honestly.
//!
//! Like the runtime matrix beside it, the tests run only when
//! `ALAS_W55_PACKAGE_DIR` names a package assembled by `cargo xtask dist`.
//! Without it there is nothing to accept, and a checkout must never be
//! mistaken for a distribution.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const PACKAGE_ENV: &str = "ALAS_W55_PACKAGE_DIR";

/// Executable stems that must never appear inside a package: their licences
/// do not authorize redistribution by this project. MSES is the named case in
/// the download contract; the rest are the other process-boundary tools.
const NEVER_BUNDLED_EXECUTABLES: &[&str] = &[
    "mset",
    "mses",
    "mplot",
    "vsp",
    "vspaero",
    "openvsp",
    "nastran20",
    "patran",
    "msc",
];

/// Manifest tool keys that must be recorded as never bundled by this project.
const NEVER_BUNDLED_TOOLS: &[&str] = &[
    "mses",
    "vspaero",
    "msc_nastran",
    "msc_patran",
    "flowunsteady",
];

/// The manifest field holding the external-tool inventory.
const TOOL_INVENTORY: &str = "external_tools";

fn package_dir() -> Option<PathBuf> {
    let path = PathBuf::from(env::var_os(PACKAGE_ENV)?);
    assert!(
        path.is_dir(),
        "{PACKAGE_ENV} does not name a directory: {}",
        path.display()
    );
    Some(path)
}

fn release_manifest(package: &Path) -> Value {
    let path = package.join("RELEASE-MANIFEST.json");
    let text = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "release manifest is readable at {}: {error}",
            path.display()
        )
    });
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("release manifest is valid JSON: {error}"))
}

fn files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).expect("package directory is readable") {
            let path = entry.expect("package entry is readable").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn is_executable(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("exe" | "bat" | "cmd" | "com")
    )
}

#[test]
fn no_package_file_is_a_tool_this_project_may_not_redistribute() {
    let Some(package) = package_dir() else {
        eprintln!("skipped: {PACKAGE_ENV} is not set");
        return;
    };

    let offenders: Vec<String> = files(&package)
        .into_iter()
        .filter(|path| is_executable(path))
        .filter(|path| NEVER_BUNDLED_EXECUTABLES.contains(&stem(path).as_str()))
        .map(|path| path.display().to_string())
        .collect();

    assert!(
        offenders.is_empty(),
        "package contains programs this project has no redistribution licence for: {offenders:?}"
    );
}

#[test]
fn the_manifest_records_every_never_bundled_tool_with_a_reason() {
    let Some(package) = package_dir() else {
        eprintln!("skipped: {PACKAGE_ENV} is not set");
        return;
    };

    let manifest = release_manifest(&package);
    let tools = manifest
        .get(TOOL_INVENTORY)
        .and_then(Value::as_object)
        .expect("the release manifest carries a tool inventory");

    for name in NEVER_BUNDLED_TOOLS {
        let entry = tools
            .get(*name)
            .unwrap_or_else(|| panic!("the tool inventory names {name}"));
        let status = entry.get("status").and_then(Value::as_str);
        assert_eq!(
            status,
            Some("user_supplied"),
            "{name} must be recorded as user supplied, not {status:?}"
        );
        let reason = entry
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(
            reason.contains("THIRD-PARTY-NOTICES.md"),
            "{name} must point at the notices file; reason was {reason:?}"
        );
    }
}

/// A bundled program must arrive with the terms that let it be bundled. AVL
/// is GPL-2.0, so its unchanged executable travels with its corresponding
/// source archive and licence text; NASTRAN-95 is only ever bundled with its
/// complete reviewed staging, and is otherwise absent with a recorded reason.
#[test]
fn a_bundled_tool_carries_its_corresponding_source_and_licence() {
    let Some(package) = package_dir() else {
        eprintln!("skipped: {PACKAGE_ENV} is not set");
        return;
    };

    let manifest = release_manifest(&package);
    let tools = manifest
        .get(TOOL_INVENTORY)
        .and_then(Value::as_object)
        .expect("the release manifest carries a tool inventory");
    let tool_dir = package.join("external tools");

    let avl = tools.get("avl").expect("the inventory names AVL");
    match avl.get("status").and_then(Value::as_str) {
        Some("bundled") => {
            for name in ["avl352.exe", "avl3.52.tgz", "AVL-GPL-2.0.txt"] {
                assert!(
                    tool_dir.join(name).is_file(),
                    "a bundled AVL must ship {name}"
                );
            }
        }
        other => assert!(
            avl.get("reason").and_then(Value::as_str).is_some(),
            "a non-bundled AVL must record why; status was {other:?}"
        ),
    }

    let nastran = tools
        .get("nastran95")
        .expect("the inventory names NASTRAN-95");
    match nastran.get("status").and_then(Value::as_str) {
        Some("bundled") => {
            assert!(
                tool_dir
                    .join("NASTRAN-95")
                    .join("build")
                    .join("bin")
                    .is_dir(),
                "a bundled NASTRAN-95 must ship its reviewed staging tree"
            );
        }
        Some("not_bundled") => {
            assert!(
                !tool_dir.join("NASTRAN-95").exists(),
                "NASTRAN-95 is recorded as not bundled, so no staging may be shipped"
            );
            assert!(
                nastran.get("reason").and_then(Value::as_str).is_some(),
                "an omitted NASTRAN-95 must record why"
            );
        }
        other => panic!("unexpected NASTRAN-95 bundle status: {other:?}"),
    }
}

/// The AGPL obligation the package itself carries: the corresponding source
/// snapshot and the notices a recipient audits it with.
#[test]
fn the_package_ships_its_own_source_and_notices() {
    let Some(package) = package_dir() else {
        eprintln!("skipped: {PACKAGE_ENV} is not set");
        return;
    };

    for name in [
        "LICENSE",
        "NOTICE",
        "THIRD-PARTY-NOTICES.md",
        "SOURCE-MANIFEST.json",
        "RELEASE-MANIFEST.json",
    ] {
        assert!(package.join(name).is_file(), "the package must ship {name}");
    }
    assert!(
        package.join("source").join("alas").join("crates").is_dir(),
        "the package must ship the corresponding source snapshot"
    );

    // The snapshot is the source of the shipped binary, so it must not smuggle
    // an external tool or local evidence back in through the allowlist. Only
    // the path *inside* the snapshot is examined: the checkout this package
    // was built in can itself live under any directory name.
    let snapshot_root = package.join("source").join("alas");
    let snapshot_offenders: Vec<String> = files(&snapshot_root)
        .into_iter()
        .filter_map(|path| {
            let relative = path.strip_prefix(&snapshot_root).ok()?.to_path_buf();
            relative
                .components()
                .any(|component| {
                    matches!(
                        component.as_os_str().to_str(),
                        Some("external tools" | ".agent" | "outputs" | "dist" | "target")
                    )
                })
                .then(|| relative.display().to_string())
        })
        .collect();
    assert!(
        snapshot_offenders.is_empty(),
        "the source snapshot contains excluded paths: {snapshot_offenders:?}"
    );
}
