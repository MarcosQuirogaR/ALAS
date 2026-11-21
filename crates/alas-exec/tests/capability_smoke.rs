// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Capability smoke test, family `tools`: tool preferences persist, and a tool
//! that is not installed resolves to nothing rather than to a guess.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use alas_exec::{ToolLocator, ToolPreferences};

#[test]
fn capability_smoke_tools_preferences_persist_and_missing_tools_stay_absent() {
    let root = std::env::temp_dir().join(format!("alas-tools-smoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let locator = ToolLocator::new(root.join("app"), root.join("user"));

    let preferences = ToolPreferences {
        avl_exe: Some("C:/nowhere/avl.exe".to_owned()),
        ..ToolPreferences::default()
    };
    locator.save_preferences(&preferences).unwrap();
    assert_eq!(locator.load_preferences(), preferences);

    let missing = Path::new("C:/nowhere/not-installed");
    let environment = locator.resolve_environment(missing, missing, missing, missing, missing);
    assert!(environment.nastran_exe.is_none());
    assert!(environment.avl_exe.is_none());
    let _ = std::fs::remove_dir_all(&root);
}
