// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Behavioral contracts for the native AVL executor.

use super::*;

const FAKE_OUTPUT: &str =
    "012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789";

#[derive(Clone, Copy)]
enum FakeSolverBehavior {
    WritesOutputs,
    WritesNothing,
    Sleeps,
}

fn temporary_directory(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "alas-avl-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap_or_else(|error| panic!("create {}: {error}", root.display()));
    root
}

fn write_geometry(root: &Path) -> PathBuf {
    let geometry = root.join("case.avl");
    fs::write(&geometry, b"fake AVL geometry")
        .unwrap_or_else(|error| panic!("write {}: {error}", geometry.display()));
    geometry
}

fn write_fake_solver(root: &Path, behavior: FakeSolverBehavior, alpha_count: usize) -> PathBuf {
    #[cfg(windows)]
    {
        let path = root.join("fake_avl.cmd");
        let mut script = String::from("@echo off\r\n");
        script.push_str("echo fake stdout\r\n");
        script.push_str("echo fake stderr 1>&2\r\n");
        match behavior {
            FakeSolverBehavior::WritesOutputs => {
                for index in 0..alpha_count {
                    script.push_str(&format!(">case.avl.{index:03}.ft echo {FAKE_OUTPUT}\r\n"));
                    script.push_str(&format!(">case.avl.{index:03}.fs echo {FAKE_OUTPUT}\r\n"));
                }
                script.push_str(&format!(">case.avl.derivatives.txt echo {FAKE_OUTPUT}\r\n"));
                script.push_str(&format!(">case.avl.trim.ft echo {FAKE_OUTPUT}\r\n"));
            }
            FakeSolverBehavior::WritesNothing => {}
            FakeSolverBehavior::Sleeps => {
                script.push_str("ping -n 30 127.0.0.1 > NUL\r\n");
            }
        }
        script.push_str("exit /b 0\r\n");
        fs::write(&path, script)
            .unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
        path
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let path = root.join("fake_avl.sh");
        let mut script = String::from("#!/bin/sh\n");
        script.push_str("printf '%s\\n' 'fake stdout'\n");
        script.push_str("printf '%s\\n' 'fake stderr' >&2\n");
        match behavior {
            FakeSolverBehavior::WritesOutputs => {
                for index in 0..alpha_count {
                    script.push_str(&format!(
                        "printf '%s\\n' '{FAKE_OUTPUT}' > case.avl.{index:03}.ft\n"
                    ));
                    script.push_str(&format!(
                        "printf '%s\\n' '{FAKE_OUTPUT}' > case.avl.{index:03}.fs\n"
                    ));
                }
                script.push_str(&format!(
                    "printf '%s\\n' '{FAKE_OUTPUT}' > case.avl.derivatives.txt\n"
                ));
                script.push_str(&format!(
                    "printf '%s\\n' '{FAKE_OUTPUT}' > case.avl.trim.ft\n"
                ));
            }
            FakeSolverBehavior::WritesNothing => {}
            FakeSolverBehavior::Sleeps => {
                script.push_str("sleep 30\n");
            }
        }
        script.push_str("exit 0\n");
        fs::write(&path, script)
            .unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
        let mut permissions = fs::metadata(&path)
            .unwrap_or_else(|error| panic!("stat {}: {error}", path.display()))
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions)
            .unwrap_or_else(|error| panic!("make {} executable: {error}", path.display()));
        path
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (root, behavior, alpha_count);
        panic!("AVL executor tests need a Unix or Windows process host");
    }
}

#[test]
fn legacy_run_keeps_the_historical_paths_and_accepts_only_fresh_force_files() {
    let root = temporary_directory("legacy");
    let geometry = write_geometry(&root);
    let executable = write_fake_solver(&root, FakeSolverBehavior::WritesOutputs, 2);

    let result = run_avl(&executable, &geometry, &[0.0, 2.0], 5.0);

    assert_eq!(result.status, AvlProcessStatus::Completed);
    assert_eq!(result.session_path, root.join("case.avl.session.txt"));
    assert_eq!(result.stdout_path, root.join("case.avl.stdout.txt"));
    assert_eq!(result.stderr_path, root.join("case.avl.stderr.txt"));
    assert_eq!(
        result.force_paths,
        vec![root.join("case.avl.000.ft"), root.join("case.avl.001.ft")]
    );
    assert_eq!(result.output_paths, AvlOutputPaths::default());
    assert!(result.session_path.is_file());
    assert!(result.stdout_path.is_file());
    assert!(result.stderr_path.is_file());
    assert!(result.force_paths.iter().all(|path| path.is_file()));
    assert!(fs::read_to_string(&result.session_path)
        .unwrap_or_else(|error| panic!("read session: {error}"))
        .contains("MRF\n"));

    fs::remove_dir_all(&root).unwrap_or_else(|error| panic!("remove {}: {error}", root.display()));
}

#[test]
fn separate_namespaces_isolate_parallel_runs_with_the_same_geometry_stem() {
    let root = temporary_directory("namespaces");
    let geometry = write_geometry(&root);
    let executable = write_fake_solver(&root, FakeSolverBehavior::WritesOutputs, 2);
    let first_namespace = root.join("vlm-comparison");
    let second_namespace = root.join("avl-comparison");

    let (first, second) = std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            run_avl_in_namespace(&executable, &geometry, &[0.0, 2.0], 5.0, &first_namespace)
        });
        let second = scope.spawn(|| {
            run_avl_in_namespace(&executable, &geometry, &[0.0, 2.0], 5.0, &second_namespace)
        });
        (
            first
                .join()
                .unwrap_or_else(|_| panic!("first AVL namespace panicked")),
            second
                .join()
                .unwrap_or_else(|_| panic!("second AVL namespace panicked")),
        )
    });

    assert_eq!(first.status, AvlProcessStatus::Completed);
    assert_eq!(second.status, AvlProcessStatus::Completed);
    assert_eq!(first.session_path.parent(), Some(first_namespace.as_path()));
    assert_eq!(
        second.session_path.parent(),
        Some(second_namespace.as_path())
    );
    assert_ne!(first.session_path, second.session_path);
    assert!(first.force_paths.iter().all(|path| path.is_file()));
    assert!(second.force_paths.iter().all(|path| path.is_file()));
    assert!(!root.join("case.avl.000.ft").exists());
    assert!(!root.join("case.avl.session.txt").exists());

    fs::remove_dir_all(&root).unwrap_or_else(|error| panic!("remove {}: {error}", root.display()));
}

#[test]
fn stale_force_and_log_files_are_removed_before_a_new_run() {
    let root = temporary_directory("stale");
    let geometry = write_geometry(&root);
    let executable = write_fake_solver(&root, FakeSolverBehavior::WritesNothing, 1);
    for path in [
        root.join("case.avl.000.ft"),
        root.join("case.avl.session.txt"),
        root.join("case.avl.stdout.txt"),
        root.join("case.avl.stderr.txt"),
    ] {
        fs::write(&path, vec![b'x'; 256])
            .unwrap_or_else(|error| panic!("write stale {}: {error}", path.display()));
    }

    let result = run_avl(&executable, &geometry, &[0.0], 5.0);

    assert_eq!(result.status, AvlProcessStatus::OutputMissing);
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("fresh requested output files")));
    assert!(!result.force_paths[0].exists());
    assert!(result.session_path.is_file());
    assert!(result.stdout_path.is_file());
    assert!(result.stderr_path.is_file());
    let stdout = fs::read_to_string(&result.stdout_path)
        .unwrap_or_else(|error| panic!("read stdout: {error}"));
    assert_eq!(stdout.trim(), "fake stdout");

    fs::remove_dir_all(&root).unwrap_or_else(|error| panic!("remove {}: {error}", root.display()));
}

#[test]
fn optional_output_channels_have_stable_session_and_file_contracts() {
    let root = temporary_directory("channels");
    let geometry = write_geometry(&root);
    let executable = write_fake_solver(&root, FakeSolverBehavior::WritesOutputs, 1);
    let channels = AvlOutputChannels {
        strip_forces: true,
        stability_derivatives: true,
        trim_commands: Some(vec![
            "C1".to_owned(),
            "G 9.8".to_owned(),
            "D 1.2".to_owned(),
            "M 100.0".to_owned(),
        ]),
    };
    let options = AvlRunOptions::in_namespace(root.join("channels")).with_output_channels(channels);

    let result = run_avl_with_options(&executable, &geometry, &[1.5], 5.0, &options);

    assert_eq!(result.status, AvlProcessStatus::Completed);
    assert_eq!(result.output_paths.strip_forces.len(), 1);
    assert!(result.output_paths.strip_forces[0].is_file());
    assert!(result
        .output_paths
        .derivatives
        .as_ref()
        .is_some_and(|path| path.is_file()));
    assert!(result
        .output_paths
        .trim
        .as_ref()
        .is_some_and(|path| path.is_file()));
    let session = fs::read_to_string(&result.session_path)
        .unwrap_or_else(|error| panic!("read session: {error}"));
    assert!(session.contains("FS\ncase.avl.000.fs\n"));
    assert!(session.contains("C1\nG 9.8\nD 1.2\nM 100.0\nX\nFT\n"));
    assert!(session.contains("ST\ncase.avl.derivatives.txt\n"));
    assert!(session.ends_with("\nQUIT\n"));

    fs::remove_dir_all(&root).unwrap_or_else(|error| panic!("remove {}: {error}", root.display()));
}

#[test]
fn a_timeout_kills_the_solver_tree_and_retains_the_failure_evidence() {
    let root = temporary_directory("timeout");
    let geometry = write_geometry(&root);
    let executable = write_fake_solver(&root, FakeSolverBehavior::Sleeps, 1);

    let result = run_avl(&executable, &geometry, &[0.0], 0.1);

    assert_eq!(result.status, AvlProcessStatus::TimedOut);
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("process tree force-killed")));
    assert!(result.session_path.is_file());
    assert!(result.stdout_path.is_file());
    assert!(result.stderr_path.is_file());

    fs::remove_dir_all(&root).unwrap_or_else(|error| panic!("remove {}: {error}", root.display()));
}

#[test]
fn a_timeout_beyond_the_clock_range_runs_without_a_deadline() {
    // `Instant::now() + Duration::MAX` panics; a caller asking for an
    // effectively unbounded run must get one rather than a crashed worker.
    let root = temporary_directory("unbounded");
    let geometry = write_geometry(&root);
    let executable = write_fake_solver(&root, FakeSolverBehavior::WritesOutputs, 1);

    let result = run_avl(&executable, &geometry, &[0.0], 1.0e30);

    assert_eq!(result.status, AvlProcessStatus::Completed);
    fs::remove_dir_all(&root).unwrap_or_else(|error| panic!("remove {}: {error}", root.display()));
}

#[test]
fn an_unremovable_stale_artifact_is_a_launch_failure_not_a_rejected_deck() {
    let root = temporary_directory("stale-directory");
    let geometry = write_geometry(&root);
    let executable = write_fake_solver(&root, FakeSolverBehavior::WritesOutputs, 1);
    let blocking = root.join("case.avl.000.ft");
    fs::create_dir_all(&blocking)
        .unwrap_or_else(|error| panic!("create {}: {error}", blocking.display()));

    let result = run_avl(&executable, &geometry, &[0.0], 5.0);

    assert_eq!(result.status, AvlProcessStatus::LaunchFailed);
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("cannot remove stale")));
    fs::remove_dir_all(&root).unwrap_or_else(|error| panic!("remove {}: {error}", root.display()));
}

#[test]
fn an_unusable_timeout_is_an_invalid_timeout_and_nothing_is_launched() {
    let root = temporary_directory("invalid-timeout");
    let geometry = write_geometry(&root);
    let executable = write_fake_solver(&root, FakeSolverBehavior::WritesOutputs, 1);

    for timeout_seconds in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -1.0] {
        let result = run_avl(&executable, &geometry, &[0.0], timeout_seconds);

        assert_eq!(
            result.status,
            AvlProcessStatus::InvalidTimeout,
            "{timeout_seconds}"
        );
        assert!(result
            .error
            .as_deref()
            .is_some_and(|error| error.starts_with("AVL timeout must be")));
        // The fake solver would have written its force file had it run.
        assert!(!root.join("case.avl.000.ft").exists(), "{timeout_seconds}");
    }
    fs::remove_dir_all(&root).unwrap_or_else(|error| panic!("remove {}: {error}", root.display()));
}
