// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in process-contract evidence against the installed native VSPAERO.

use std::fs;
use std::path::{Path, PathBuf};

use alas_exec::vspaero::{run_vspaero, VspaeroProcessStatus};

#[test]
#[ignore = "requires an installed OpenVSP 3.51.2 VSPAERO distribution"]
fn installed_native_solver_produces_a_fresh_polar_without_wrapper_result_keys() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = std::env::var_os("ALAS_VSPAERO_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("external tools/OpenVSP-3.51.2-win64/vspaero.exe"));
    let source_case = std::env::var_os("ALAS_VSPAERO_CASE")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("outputs/vspaero_installed_probe/TR1208-API"));
    assert!(executable.is_file(), "missing {}", executable.display());
    for extension in ["vspgeom", "vkey", "vspaero"] {
        assert!(
            source_case.with_extension(extension).is_file(),
            "missing {}",
            source_case.with_extension(extension).display()
        );
    }

    let work_dir = std::env::temp_dir().join(format!(
        "alas-installed-vspaero-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    fs::create_dir_all(&work_dir)
        .unwrap_or_else(|error| panic!("create {}: {error}", work_dir.display()));
    let case_path = work_dir.join("TR1208-API");
    for extension in ["vspgeom", "vkey", "vspaero"] {
        copy_case_file(&source_case, &case_path, extension);
    }

    let result = run_vspaero(&executable, &case_path, 4, 120.0);
    assert_eq!(
        result.status,
        VspaeroProcessStatus::Completed,
        "{:?}",
        result.error
    );
    assert!(result
        .polar_path
        .metadata()
        .is_ok_and(|meta| meta.len() > 100));
    assert_eq!(
        result.wake_mode,
        result.wake_settings.map(|settings| settings.mode())
    );
    let stdout = fs::read_to_string(&result.stdout_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", result.stdout_path.display()));
    assert!(stdout.contains("VSPAERO v.7.2.2"));

    fs::remove_dir_all(&work_dir)
        .unwrap_or_else(|error| panic!("remove {}: {error}", work_dir.display()));
}

fn copy_case_file(source_case: &Path, target_case: &Path, extension: &str) {
    let source = source_case.with_extension(extension);
    let target = target_case.with_extension(extension);
    fs::copy(&source, &target).unwrap_or_else(|error| {
        panic!("copy {} to {}: {error}", source.display(), target.display())
    });
}
