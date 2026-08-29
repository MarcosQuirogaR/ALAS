// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parity test for `alas-app::paths` against `golden/app/paths.json`.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use alas_app::paths::{
    find_tool_dir, resolve_data_path, resolve_tool_dir, resolve_tool_exe, APP_DIR_ENV,
};
use alas_testkit::{load, Comparison, Tier};
use serde::Deserialize;

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "alas_path_test_{}_{}_{}",
            std::process::id(),
            nanos,
            count
        ));
        fs::create_dir_all(&path).expect("temp dir created");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[derive(Debug, Deserialize)]
struct Fixture {
    resolve_tool_dir: HashMap<String, String>,
    find_tool_dir: HashMap<String, Option<String>>,
    resolve_data_path: HashMap<String, String>,
    resolve_tool_exe: HashMap<String, Option<String>>,
}

fn normalize(p: PathBuf, root: &Path) -> String {
    if let Ok(rel) = p.strip_prefix(root) {
        let s = rel.to_string_lossy().replace('\\', "/");
        if s.is_empty() {
            ".".to_owned()
        } else {
            s
        }
    } else {
        let s = p.to_string_lossy().replace('\\', "/");
        if s.is_empty() {
            ".".to_owned()
        } else {
            s
        }
    }
}

#[test]
fn paths_match_reference_fixture() {
    let fixture: Fixture = load("app", "paths");

    let temp_dir = TempDir::new();
    let root = temp_dir
        .path
        .canonicalize()
        .unwrap_or_else(|_| temp_dir.path.clone());

    fs::create_dir_all(root.join("bin")).expect("bin created");
    fs::create_dir_all(root.join("data")).expect("data created");
    fs::write(root.join("data/airports.json"), "{}").expect("airports.json written");
    fs::create_dir_all(root.join("external tools/MSES")).expect("MSES created");
    fs::write(root.join("external tools/MSES/mses.exe"), "bin").expect("mses.exe written");
    fs::create_dir_all(root.join("external tools/suave_runner")).expect("suave_runner created");

    // Point ALAS_APP_DIR to the temporary isolated environment
    env::set_var(APP_DIR_ENV, &root);

    let mut cmp = Comparison::new("paths", Tier::Exact);

    for (input, expected) in &fixture.resolve_tool_dir {
        let path_obj = if input.ends_with("data") && input.len() > 10 {
            root.join("data")
        } else {
            PathBuf::from(input)
        };
        let actual = normalize(resolve_tool_dir(&path_obj), &root);
        cmp.exact(&format!("resolve_tool_dir({input:?})"), &actual, expected);
    }

    for (input, expected) in &fixture.find_tool_dir {
        let path_obj = if input.ends_with("data") && input.len() > 10 {
            root.join("data")
        } else {
            PathBuf::from(input)
        };
        let actual = find_tool_dir(&path_obj).map(|p| normalize(p, &root));
        cmp.exact(&format!("find_tool_dir({input:?})"), &actual, expected);
    }

    for (input, expected) in &fixture.resolve_data_path {
        let path_obj = if input.ends_with("data") && input.len() > 10 {
            root.join("data")
        } else {
            PathBuf::from(input)
        };
        let actual = normalize(resolve_data_path(&path_obj), &root);
        cmp.exact(&format!("resolve_data_path({input:?})"), &actual, expected);
    }

    for (input, expected) in &fixture.resolve_tool_exe {
        let actual = resolve_tool_exe(Path::new(input), &root).map(|p| normalize(p, &root));
        cmp.exact(&format!("resolve_tool_exe({input:?})"), &actual, expected);
    }

    env::remove_var(APP_DIR_ENV);

    cmp.finish();
}
