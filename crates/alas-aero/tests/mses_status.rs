// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! MSES installation and result-status characterization tests.
//!
//! These tests exercise the optional external-tool boundary without inventing
//! solver output. Installed success remains in the ignored parity test, where
//! real MPlot boundary-layer and flowfield exports are required.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use alas_aero::mses::{run_mses_polar, MsesStatus};
use alas_config::MsesConfig;
use alas_geom::asb::airfoil::Airfoil;

fn airfoil() -> Airfoil {
    Airfoil::from_name("naca2412").expect("the built-in NACA section is available")
}

fn temporary_directory(label: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("alas-mses-status-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("the test scratch directory can be created");
    path
}

fn config() -> MsesConfig {
    MsesConfig::default()
}

#[test]
fn disabled_mses_reports_disabled_without_inspecting_the_installation() {
    let mut config = config();
    config.enabled = false;
    let path = Path::new("this-installation-does-not-need-to-exist");
    let result = run_mses_polar(&airfoil(), 0.3, 5.0e6, 2.0, &config, path);
    assert_eq!(result.status, MsesStatus::Disabled);
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("disabled")));
}

#[test]
fn absent_mses_reports_absent() {
    let root = temporary_directory("absent");
    let missing = root.join("MSES");
    let result = run_mses_polar(&airfoil(), 0.3, 5.0e6, 2.0, &config(), &missing);
    assert_eq!(result.status, MsesStatus::Absent);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn incomplete_mses_reports_incomplete() {
    let root = temporary_directory("incomplete");
    fs::write(root.join("mset.exe"), b"placeholder").expect("placeholder is written");
    let result = run_mses_polar(&airfoil(), 0.3, 5.0e6, 2.0, &config(), &root);
    assert_eq!(result.status, MsesStatus::Incomplete);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn present_but_unlaunchable_mses_reports_launch_failure() {
    let root = temporary_directory("launch-failure");
    for name in ["mset.exe", "mses.exe", "mplot.exe"] {
        fs::write(root.join(name), b"not an executable").expect("placeholder is written");
    }

    #[cfg(windows)]
    let launch_guards = {
        use std::os::windows::fs::OpenOptionsExt;

        ["mset.exe", "mses.exe", "mplot.exe"]
            .into_iter()
            .map(|name| {
                fs::OpenOptions::new()
                    .read(true)
                    .share_mode(0)
                    .open(root.join(name))
                    .expect("exclusive test handle is opened")
            })
            .collect::<Vec<_>>()
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        for name in ["mset.exe", "mses.exe", "mplot.exe"] {
            let path = root.join(name);
            let mut permissions = fs::metadata(&path)
                .expect("placeholder metadata is readable")
                .permissions();
            permissions.set_mode(0o600);
            fs::set_permissions(path, permissions).expect("execute permission is removed");
        }
    }

    let result = run_mses_polar(&airfoil(), 0.3, 5.0e6, 2.0, &config(), &root);
    assert_eq!(result.status, MsesStatus::LaunchFailure);
    #[cfg(windows)]
    drop(launch_guards);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn every_public_mses_status_has_a_stable_diagnostic_string() {
    let expected = [
        (MsesStatus::NotRun, "not_run"),
        (MsesStatus::Disabled, "disabled"),
        (MsesStatus::Absent, "absent"),
        (MsesStatus::Incomplete, "incomplete"),
        (MsesStatus::LaunchFailure, "launch_failure"),
        (MsesStatus::Timeout, "timeout"),
        (MsesStatus::ParseFailure, "parse_failure"),
        (MsesStatus::Ok, "ok"),
        (MsesStatus::PartialConvergence, "partial_convergence"),
        (MsesStatus::Error, "error"),
    ];
    for (status, text) in expected {
        assert_eq!(status.as_str(), text);
    }
}
