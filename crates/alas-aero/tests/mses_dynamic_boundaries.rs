// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Dynamic public-boundary probes for malformed MSES transcripts and timeout
//! handling.
//!
//! The helper executable is compiled from a tiny repository-local source
//! string at test time.  It is not an MSES implementation: it only reproduces
//! the process shape (`mset`, `mses`, and `mplot`) so the public driver reaches
//! its transcript-validation boundary without requiring a separately licensed
//! solver installation.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use alas_aero::mses::{run_mses_polar, MsesStatus};
use alas_config::MsesConfig;
use alas_geom::asb::airfoil::Airfoil;

const FAKE_MSES_SOURCE: &str = r#"
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let role = env::args()
        .next()
        .and_then(|path| Path::new(&path).file_stem().map(|value| value.to_owned()))
        .and_then(|value| value.to_str().map(str::to_owned))
        .unwrap_or_default();
    if role == "mses" {
        println!("Converged on tolerance");
        return;
    }
    if role != "mplot" {
        return;
    }

    let mode = "__ALAS_FAKE_MSES_MODE__";
    let count = fs::read_to_string("fake_mplot_count.txt")
        .ok()
        .and_then(|text| text.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let _ = fs::write("fake_mplot_count.txt", (count + 1).to_string());

    let complete = "alpha = 1.0 CL = 0.5 CD = 0.03 CM = -0.02 CDv = 0.02 CDw = 0.01 top Xtr = 0.4 bot Xtr = 0.6";
    let missing = "alpha = 1.0 CL = 0.5 CD = 0.03 CM = -0.02 CDv = 0.02 top Xtr = 0.4 bot Xtr = 0.6";
    let malformed = "alpha = 1.0 CL = malformed CD = 0.03 CM = -0.02 CDv = 0.02 CDw = 0.01 top Xtr = 0.4 bot Xtr = 0.6";
    let nonfinite = "alpha = 1.0 CL = NaN CD = 0.03 CM = -0.02 CDv = 0.02 CDw = 0.01 top Xtr = 0.4 bot Xtr = 0.6";
    let output = match mode {
        "missing" => missing,
        "malformed" => malformed,
        "nonfinite" => nonfinite,
        // The first point is complete and the next point omits CDw.  Before
        // the strict row check, this would have left CDw one value short in
        // the public accumulator; it must now fail at the transcript boundary.
        "ragged" if count > 0 => missing,
        _ => complete,
    };
    println!("{output}");
}
"#;

fn temporary_directory(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "alas-mses-dynamic-{label}-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("the dynamic MSES scratch directory can be created");
    path
}

fn fake_mses_installation(root: &Path, mode: &str) {
    let source = root.join("fake_mses.rs");
    let helper = root.join("fake_mses_helper.exe");
    let source_text = FAKE_MSES_SOURCE.replace("__ALAS_FAKE_MSES_MODE__", mode);
    fs::write(&source, source_text).expect("fake MSES source can be written");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = Command::new(rustc)
        .args([
            "--edition",
            "2021",
            source.to_str().expect("source path is UTF-8"),
            "-o",
            helper.to_str().expect("helper path is UTF-8"),
        ])
        .status()
        .expect("rustc can be launched for the local fixture");
    assert!(status.success(), "the local MSES fixture must compile");

    for name in ["mset.exe", "mses.exe", "mplot.exe"] {
        fs::copy(&helper, root.join(name)).expect("fake MSES executable can be copied");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        for name in ["mset.exe", "mses.exe", "mplot.exe"] {
            let path = root.join(name);
            let mut permissions = fs::metadata(&path)
                .expect("fake MSES executable metadata is readable")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).expect("fake MSES executable is executable");
        }
    }
}

fn airfoil() -> Airfoil {
    Airfoil::from_name("naca2412").expect("the built-in NACA section is available")
}

#[test]
fn public_mses_polar_rejects_each_malformed_transcript_shape() {
    for (mode, point_count) in [
        ("missing", 1),
        ("ragged", 3),
        ("malformed", 1),
        ("nonfinite", 1),
    ] {
        let root = temporary_directory(mode);
        fake_mses_installation(&root, mode);
        let config = MsesConfig {
            mses_dir: root.display().to_string(),
            alpha_sweep_n_points: point_count,
            ..MsesConfig::default()
        };

        let result = run_mses_polar(&airfoil(), 0.3, 5.0e6, 2.0, &config, &root);

        assert_eq!(result.status, MsesStatus::ParseFailure, "mode={mode}");
        assert!(
            !result.has_usable_data(),
            "mode={mode} fabricated data leaked"
        );
        assert_eq!(result.converged_alpha_count, 0, "mode={mode}");
        assert!(result.alpha_deg.is_empty(), "mode={mode}");
        assert!(result.cl.is_empty(), "mode={mode}");
        assert!(
            result
                .error
                .as_deref()
                .is_some_and(|message| message.contains("mplot")),
            "mode={mode} should retain a parser diagnostic: {:?}",
            result.error
        );
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn public_mses_polar_reports_invalid_timeout_without_launching_a_tool() {
    for field in ["mset", "mses"] {
        for seconds in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let root = temporary_directory(&format!("invalid-timeout-{field}"));
            let mut config = MsesConfig::default();
            if field == "mset" {
                for name in ["mset.exe", "mses.exe", "mplot.exe"] {
                    fs::write(root.join(name), b"placeholder").expect("placeholder is written");
                }
                config.timeout_mset_s = seconds;
            } else {
                // A valid mset/mses fixture gets far enough to prove that the
                // invalid flow-solve timeout is rejected before mses launch.
                fake_mses_installation(&root, "complete");
                config.timeout_mses_s = seconds;
            }

            let result = run_mses_polar(&airfoil(), 0.3, 5.0e6, 2.0, &config, &root);

            assert_eq!(
                result.status,
                MsesStatus::Error,
                "field={field} seconds={seconds:?}"
            );
            assert!(
                !result.has_usable_data(),
                "field={field} seconds={seconds:?}"
            );
            assert!(
                result
                    .error
                    .as_deref()
                    .is_some_and(|message| message.contains("invalid process timeout")),
                "field={field} seconds={seconds:?} should retain the typed timeout diagnostic: {:?}",
                result.error
            );
            let _ = fs::remove_dir_all(root);
        }
    }
}
