// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Guard against child processes that flash a console window (review item
//! B12): every `Command::new(` in the GUI and application crates must be
//! configured with `NoConsoleWindow::no_window()` (the shared helper of
//! `alas-exec`), which is what keeps a GUI-launched tool from opening a
//! console on Windows. Test code is exempt; the process helpers themselves
//! live in `alas-exec`, which is not scanned.

use std::path::{Path, PathBuf};

/// How many lines after `Command::new(` the `no_window()` call may follow
/// (the builder chain or the statements configuring the same command).
const CONFIGURATION_WINDOW: usize = 40;

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn is_test_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    name == "tests.rs"
        || name.ends_with("_tests.rs")
        || path.components().any(|part| part.as_os_str() == "tests")
}

/// Spawn sites without a console-hiding call, as `file:line` strings.
fn unhidden_spawns(files: &[PathBuf]) -> Vec<String> {
    let mut found = Vec::new();
    for path in files.iter().filter(|path| !is_test_file(path)) {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let lines: Vec<&str> = text.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            // Code after the first test module is test code.
            if line.trim_start().starts_with("#[cfg(test)]") {
                break;
            }
            let code = line.trim_start();
            if code.starts_with("//") || !code.contains("Command::new(") {
                continue;
            }
            let end = (index + CONFIGURATION_WINDOW).min(lines.len());
            if !lines[index..end]
                .iter()
                .any(|each| each.contains("no_window"))
            {
                found.push(format!("{}:{}", path.display(), index + 1));
            }
        }
    }
    found
}

#[test]
fn every_direct_process_spawn_hides_its_console() {
    let gui = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&gui, &mut files);
    if let Some(app) = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|crates| crates.join("alas-app").join("src"))
    {
        rust_files(&app, &mut files);
    }
    assert!(files.len() > 50, "the scan must find the sources");
    let offenders = unhidden_spawns(&files);
    assert!(
        offenders.is_empty(),
        "process spawn without `no_window()` (use alas_exec::process::NoConsoleWindow): {offenders:?}"
    );
}

#[test]
fn the_guard_flags_a_spawn_that_does_not_hide_its_console() {
    let dir = std::env::temp_dir().join(format!("alas-spawn-guard-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let bad = dir.join("bad.rs");
    std::fs::write(
        &bad,
        "fn f() {\n    let _ = std::process::Command::new(\"x\").spawn();\n}\n",
    )
    .expect("write");
    let good = dir.join("good.rs");
    std::fs::write(
        &good,
        "fn f() {\n    let _ = std::process::Command::new(\"x\").no_window().spawn();\n}\n",
    )
    .expect("write");
    assert_eq!(unhidden_spawns(&[bad.clone(), good]).len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
