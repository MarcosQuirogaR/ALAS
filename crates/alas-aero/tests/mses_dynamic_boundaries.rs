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
use std::sync::atomic::{AtomicBool, Ordering};

use alas_aero::mses::{
    run_mses_polar, run_mses_polar_with_cancel, run_mses_pressure_distribution,
    run_mses_pressure_distribution_with_cancel,
    run_mses_pressure_distribution_with_checkpoint_and_cancel, MsesPolarPointStatus, MsesStatus,
};
use alas_config::MsesConfig;
use alas_geom::asb::airfoil::Airfoil;

const FAKE_MSES_SOURCE: &str = r##"
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let role = env::args()
        .next()
        .and_then(|path| Path::new(&path).file_stem().map(|value| value.to_owned()))
        .and_then(|value| value.to_str().map(str::to_owned))
        .unwrap_or_default();
    let mode = "__ALAS_FAKE_MSES_MODE__";
    if role == "mset" {
        // In fake mset, write a clean sentinel marker to mdat.case
        let _ = fs::write("mdat.case", "mset_mesh_clean");
        return;
    }
    if role == "mses" {
        if mode == "fail_all" {
            println!("Iteration 100 rms(dR): 0.128E+00 did not converge");
            return;
        }
        if mode == "bridge_ok" {
            // Every solve converges, including bounded intermediate bridge
            // hops, so a wide requested gap gets bridged rather than falling
            // back to a clean remesh.
            println!("Converged on tolerance");
            return;
        }
        if mode == "bridge_official_only" {
            // Only the exact requested angles converge; any intermediate
            // bridge hop (which lands strictly between them) must fail, so
            // the driver has to fall back to a clean target-angle remesh.
            let case_text = fs::read_to_string("mses.case").unwrap_or_default();
            let alpha: f64 = case_text
                .lines()
                .nth(2)
                .and_then(|line| line.split_whitespace().nth(2))
                .and_then(|token| token.parse().ok())
                .unwrap_or(f64::NAN);
            let is_official = [5.0_f64, 6.0, 7.0]
                .iter()
                .any(|&official| (official - alpha).abs() < 1e-6);
            if is_official {
                println!("Converged on tolerance");
            } else {
                println!("Iteration 100 rms(dR): 0.128E+00 did not converge");
            }
            return;
        }
        if mode == "pressure_bridge_recovery" {
            // The exact requested angle (6.0 deg) fails on its first ever
            // attempt (the plain retry-offset solve) but converges on any
            // later attempt (the exact-target recovery solve after
            // bridging). Every other angle (the 6.5 deg offset candidate)
            // converges immediately.
            let case_text = fs::read_to_string("mses.case").unwrap_or_default();
            let alpha: f64 = case_text
                .lines()
                .nth(2)
                .and_then(|line| line.split_whitespace().nth(2))
                .and_then(|token| token.parse().ok())
                .unwrap_or(f64::NAN);
            if (alpha - 6.0).abs() < 1e-6 {
                // Each retry offset (and the bridge-back attempt) runs in its
                // own fresh temporary work directory, so the counter must
                // live next to the fixture executable (shared across every
                // invocation) rather than in the current directory.
                let counter_path = env::current_exe()
                    .ok()
                    .and_then(|path| path.parent().map(|dir| dir.join("fake_exact_count.txt")));
                let count = counter_path
                    .as_ref()
                    .and_then(|path| fs::read_to_string(path).ok())
                    .and_then(|text| text.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if let Some(path) = &counter_path {
                    let _ = fs::write(path, (count + 1).to_string());
                }
                if count == 0 {
                    println!("Iteration 100 rms(dR): 0.128E+00 did not converge");
                } else {
                    println!("Converged on tolerance");
                }
                return;
            }
            println!("Converged on tolerance");
            return;
        }
        if mode == "pressure_bridge_never_exact" {
            // The exact requested angle never converges, no matter how many
            // times it is attempted; any offset candidate converges
            // immediately: the bridge-back attempt must fail honestly.
            let case_text = fs::read_to_string("mses.case").unwrap_or_default();
            let alpha: f64 = case_text
                .lines()
                .nth(2)
                .and_then(|line| line.split_whitespace().nth(2))
                .and_then(|token| token.parse().ok())
                .unwrap_or(f64::NAN);
            if (alpha - 6.0).abs() < 1e-6 {
                println!("Iteration 100 rms(dR): 0.128E+00 did not converge");
            } else {
                println!("Converged on tolerance");
            }
            return;
        }
        if mode == "pressure_bridge_cancel" {
            // The exact requested angle fails immediately; the +1.0 deg
            // offset converges immediately; the bridge hop toward the exact
            // target (6.5 deg, the midpoint) writes a deterministic sentinel file
            // to signal the test watcher, then waits to be terminated by cooperative
            // cancellation. This eliminates sleep races completely.
            let case_text = fs::read_to_string("mses.case").unwrap_or_default();
            let alpha: f64 = case_text
                .lines()
                .nth(2)
                .and_then(|line| line.split_whitespace().nth(2))
                .and_then(|token| token.parse().ok())
                .unwrap_or(f64::NAN);
            if (alpha - 6.0).abs() < 1e-6 {
                println!("Iteration 100 rms(dR): 0.128E+00 did not converge");
                return;
            }
            if (alpha - 6.5).abs() < 1e-6 {
                if let Some(parent) = env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
                    let _ = fs::write(parent.join("pressure_bridge_started.sentinel"), "started");
                }
                let start = std::time::Instant::now();
                while start.elapsed() < std::time::Duration::from_secs(10) {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                println!("Converged on tolerance");
                return;
            }
            println!("Converged on tolerance");
            return;
        }
        if mode == "cancel_after_point0" {
            // The first solve (requested point 0) returns immediately.
            // When the subsequent solve (the bridge hop toward point 1) starts,
            // it writes a deterministic sentinel file to notify the test watcher,
            // then waits to be terminated by cooperative cancellation.
            // This eliminates sleep races completely.
            let count = fs::read_to_string("fake_mses_count.txt")
                .ok()
                .and_then(|text| text.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let _ = fs::write("fake_mses_count.txt", (count + 1).to_string());
            if count >= 1 {
                if let Some(parent) = env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
                    let _ = fs::write(parent.join("bridge_hop_started.sentinel"), "started");
                }
                let start = std::time::Instant::now();
                while start.elapsed() < std::time::Duration::from_secs(10) {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            println!("Converged on tolerance");
            return;
        }
        if mode == "checkpoint_warm_start_fails" {
            // The warm start attempt from a checkpoint (which restores the checkpoint's
            // converged flowfield) fails to converge; subsequent cold-start retry meshes
            // (fresh from mset, containing "mset_mesh_clean") succeed. This tests bounded
            // fallback when a restored checkpoint anchor is not usable.
            let mdat = fs::read_to_string("mdat.case").unwrap_or_default();
            if mdat.contains("converged_checkpoint_flowfield") {
                println!("Iteration 100 rms(dR): 0.128E+00 checkpoint anchor did not converge");
                return;
            }
            if mdat.contains("mset_mesh_clean") {
                let _ = fs::write("mdat.case", "converged_checkpoint_flowfield");
                println!("Converged on tolerance");
                return;
            }
            println!("Iteration 100 rms(dR): 0.128E+00 checkpoint anchor did not converge");
            return;
        }
        if mode == "fail_warmup" {
            let count = fs::read_to_string("fake_mses_count.txt")
                .ok()
                .and_then(|text| text.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let _ = fs::write("fake_mses_count.txt", (count + 1).to_string());
            if count == 0 {
                // Point 0 initial solve fails
                let _ = fs::write("mdat.case", "point_0_diverged");
                println!("Iteration 100 rms(dR): 0.128E+00 Point 0 did not converge");
                return;
            } else if count == 1 {
                // Continuation warm-up fails and corrupts mdat.case
                let _ = fs::write("mdat.case", "failed_warmup_polluted_mdat");
                println!("Iteration 100 rms(dR): 0.128E+00 Warmup did not converge");
                return;
            }
            // Subsequent runs (e.g. Point 1 clean target solve) converge cleanly
            println!("Converged on tolerance");
            return;
        }
        println!("Converged on tolerance");
        return;
    }
    if role != "mplot" {
        return;
    }

    let count = fs::read_to_string("fake_mplot_count.txt")
        .ok()
        .and_then(|text| text.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let _ = fs::write("fake_mplot_count.txt", (count + 1).to_string());

    // The fake binary cannot tell which mplot menu option was requested (the
    // choice is piped over stdin, which this fixture never reads), so it
    // always writes both the pressure entry point's option-12/11 dump files
    // in addition to the polar summary line below: harmless for polar-only
    // tests, and it lets pressure-path tests reach a real converged replay.
    let _ = fs::write(
        "bl_dump.txt",
        "# x y s b0 Cp Ue rho Me\n\
         0.2 0.1 0.2 0 -0.8 0 0 1.2\n\
         0.8 0.1 0.8 0 -0.4 0 0 1.1\n\
         0.3 -0.1 0.3 0 0.2 0 0 0.7\n\
         1.2 -0.1 1.2 0 0.1 0 0 0.8\n",
    );
    let _ = fs::write(
        "flowfield.txt",
        "# x y rho p u v q M Cp\n\
         0.0 -0.5 1 1 1 0 1 0.7 0\n\
         1.0 -0.5 1 1 1 0 1 0.8 0\n\n\
         0.0 0.5 1 1 1 0 1 0.9 0\n\
         1.0 0.5 1 1 1 0 1 1.0 0\n",
    );

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
"##;

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
    // Transcript tests require a complete free-transition installation at
    // preflight. This header is only for the fake solver, never real physics.
    write_osmap_fixture(root);
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

fn write_osmap_fixture(root: &Path) {
    let mut header = Vec::new();
    for value in [12_i32, 28, 41, 18, 12, 224_i32] {
        header.extend_from_slice(&value.to_le_bytes());
    }
    fs::write(root.join("osmapDP.dat"), header)
        .expect("write double-precision osmapDP.dat fixture");
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
            write_osmap_fixture(&root);
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

#[test]
fn sweep_never_initializes_zero_or_center_large_incidence_jump() {
    let root = temporary_directory("no-zero-jump");
    fake_mses_installation(&root, "fail_all");
    let config = MsesConfig {
        mses_dir: root.display().to_string(),
        alpha_sweep_halfwidth_deg: 2.0,
        alpha_sweep_n_points: 3, // points: 4.0, 6.0, 8.0
        ..MsesConfig::default()
    };

    // Requested trim alpha is 6.0 deg; bracket is [4.0, 6.0, 8.0]
    let result = run_mses_polar(&airfoil(), 0.735, 6.0e7, 6.0, &config, &root);

    assert_eq!(result.status, MsesStatus::Error);
    assert!(!result.solver_attempts.is_empty());
    for attempt in &result.solver_attempts {
        // Assert no attempt commanded an arbitrary 0 deg incidence
        assert!(
            (attempt.alpha_deg - 0.0).abs() > 0.5,
            "sweep must not inject an arbitrary 0-deg restart: got alpha={}",
            attempt.alpha_deg
        );
        // Assert no attempt purpose mentions neutral or center restarts
        assert!(
            !attempt.purpose.contains("alpha=0 deg"),
            "purpose must not indicate a 0-deg restart: {}",
            attempt.purpose
        );
        assert!(
            !attempt.purpose.contains("center MSET restart"),
            "purpose must not indicate a center restart: {}",
            attempt.purpose
        );
        // Every attempt must remain within the bounded bracket
        assert!(
            attempt.alpha_deg >= 3.99 && attempt.alpha_deg <= 8.01,
            "attempt alpha {} is outside the requested sweep bracket",
            attempt.alpha_deg
        );
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn failed_warmup_state_is_isolated_with_clean_target_remesh() {
    let root = temporary_directory("failed-warmup-isolation");
    fake_mses_installation(&root, "fail_warmup");
    let config = MsesConfig {
        mses_dir: root.display().to_string(),
        // 0.5 deg halfwidth is the smallest the public sweep entry point
        // will honor (`alpha_sweep_halfwidth_deg.max(0.5)`), which makes the
        // 3-point spacing land exactly at MAX_CONTINUATION_STEP_DEG so the
        // warm-up/continuation paths stay exercised under the corrected,
        // MSES-guidance-conservative bound.
        alpha_sweep_halfwidth_deg: 0.5,
        alpha_sweep_n_points: 3, // points: 5.5, 6.0, 6.5
        ..MsesConfig::default()
    };

    let result = run_mses_polar(&airfoil(), 0.735, 6.0e7, 6.0, &config, &root);

    // Warmup at point 1 failed, but point 1 then remeshed cleanly at target angle and converged
    assert!(result.converged_alpha_count >= 1);
    assert_eq!(result.status, MsesStatus::PartialConvergence);

    // Check attempt sequence:
    // 1. Initial point 0 (5.5 deg) -> failed
    // 2. Unrequested continuation warm-up at point 1 (6.0 deg) -> failed
    // 3. Point 1 (6.0 deg) requested solve with clean target-angle MSET mesh -> converged!
    let warmup_attempt = result
        .solver_attempts
        .iter()
        .find(|a| a.purpose.contains("unrequested continuation warm-up"));
    assert!(
        warmup_attempt.is_some(),
        "sweep must record the bounded continuation warm-up attempt"
    );

    let clean_remesh_attempt = result.solver_attempts.iter().find(|a| {
        a.purpose.contains("clean target-angle MSET mesh")
            && a.purpose.contains("after failed continuation warm-up")
    });
    assert!(
        clean_remesh_attempt.is_some(),
        "driver must cleanly remesh at the target angle after a failed warm-up instead of reusing dirty mdat: {:?}",
        result.solver_attempts.iter().map(|a| &a.purpose).collect::<Vec<_>>()
    );
    let _ = fs::remove_dir_all(root);
}

/// A requested gap wider than `MAX_CONTINUATION_STEP_DEG` (0.5 deg) must not
/// simply fall back to a clean remesh: it should bridge through bounded
/// 0.5 deg intermediate hops, and those hops must never be counted as (or
/// substitute for) a requested point's own diagnostic or coefficients.
#[test]
fn a_one_degree_request_gap_bridges_through_bounded_half_degree_hops() {
    let root = temporary_directory("bridge-half-degree-hops");
    fake_mses_installation(&root, "bridge_ok");
    let config = MsesConfig {
        mses_dir: root.display().to_string(),
        alpha_sweep_halfwidth_deg: 1.0,
        alpha_sweep_n_points: 3, // points: 5.0, 6.0, 7.0: 1.0 deg gaps
        ..MsesConfig::default()
    };

    let result = run_mses_polar(&airfoil(), 0.735, 6.0e7, 6.0, &config, &root);

    assert_eq!(result.status, MsesStatus::Ok);
    assert_eq!(result.converged_alpha_count, 3);
    // The fake mplot summary always reports a fixed "alpha = 1.0" row (it
    // does not parse mses.case), so the converged coefficient count (not
    // its hardcoded alpha column) is what is meaningful here.
    assert_eq!(result.alpha_deg.len(), 3);

    // Exactly one requested-point diagnostic per requested angle, no
    // intermediate bridge hop ever becomes (or substitutes for) a point.
    let requested: Vec<f64> = result
        .point_diagnostics
        .iter()
        .map(|d| d.requested_alpha_deg)
        .collect();
    assert_eq!(requested, vec![5.0, 6.0, 7.0]);

    let bridge_hops: Vec<f64> = result
        .solver_attempts
        .iter()
        .filter(|a| {
            a.purpose
                .contains("bounded intermediate continuation bridge")
        })
        .map(|a| a.alpha_deg)
        .collect();
    assert_eq!(
        bridge_hops,
        vec![5.5, 6.5],
        "the two 1.0 deg gaps must each bridge through exactly one bounded 0.5 deg \
         intermediate hop rather than a clean remesh: {:?}",
        result
            .solver_attempts
            .iter()
            .map(|a| &a.purpose)
            .collect::<Vec<_>>()
    );
    let _ = fs::remove_dir_all(root);
}

/// When a bridge hop cannot converge (even after its bounded smaller-step
/// retry), the driver must restore the last known-good state and fall back
/// to a clean target-angle remesh at the actual next requested point,
/// never leaving a diverged intermediate state to pollute it.
#[test]
fn a_failed_bridge_restores_state_and_falls_back_to_a_clean_remesh() {
    let root = temporary_directory("bridge-failure-fallback");
    fake_mses_installation(&root, "bridge_official_only");
    let config = MsesConfig {
        mses_dir: root.display().to_string(),
        alpha_sweep_halfwidth_deg: 1.0,
        alpha_sweep_n_points: 3, // points: 5.0, 6.0, 7.0: 1.0 deg gaps
        ..MsesConfig::default()
    };

    let result = run_mses_polar(&airfoil(), 0.735, 6.0e7, 6.0, &config, &root);

    // Every requested point still converges: the bridge fails, but the
    // clean-target-mesh fallback recovers each one independently. (The fake
    // mplot summary always reports a fixed "alpha = 1.0" row rather than
    // parsing mses.case, so the count is what is meaningful here.)
    assert_eq!(result.status, MsesStatus::Ok);
    assert_eq!(result.converged_alpha_count, 3);
    assert_eq!(result.alpha_deg.len(), 3);

    let failed_bridge_attempts: Vec<_> = result
        .solver_attempts
        .iter()
        .filter(|a| {
            a.purpose
                .contains("bounded intermediate continuation bridge")
        })
        .collect();
    // Two gaps, each with one full-step attempt and one bounded half-step
    // retry, both failing, exactly four bridge attempts, all NotConverged.
    assert_eq!(failed_bridge_attempts.len(), 4);
    assert!(failed_bridge_attempts
        .iter()
        .all(|a| a.status == alas_aero::mses::MsesPolarPointStatus::NotConverged));

    let fallback_remeshes = result
        .solver_attempts
        .iter()
        .filter(|a| {
            a.purpose
                .contains("clean target-angle MSET mesh at requested alpha=6")
                || a.purpose
                    .contains("clean target-angle MSET mesh at requested alpha=7")
        })
        .count();
    assert_eq!(
        fallback_remeshes,
        2,
        "both gaps must fall back to a clean remesh at the actual next requested point: {:?}",
        result
            .solver_attempts
            .iter()
            .map(|a| &a.purpose)
            .collect::<Vec<_>>()
    );
    let _ = fs::remove_dir_all(root);
}

/// Cancellation that lands before a bridge hop must preserve whatever
/// requested points already converged and must never let the bridge record
/// (or count) an intermediate attempt.
#[test]
fn cancellation_before_a_bridge_hop_preserves_the_requested_partial() {
    let root = temporary_directory("bridge-cancellation");
    fake_mses_installation(&root, "cancel_after_point0");
    let config = MsesConfig {
        mses_dir: root.display().to_string(),
        alpha_sweep_halfwidth_deg: 1.0,
        alpha_sweep_n_points: 3, // points: 5.0, 6.0, 7.0: 1.0 deg gaps
        ..MsesConfig::default()
    };

    let sentinel = root.join("bridge_hop_started.sentinel");
    let cancel = AtomicBool::new(false);
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            // Wait for the sentinel indicating Point 0 has finished and the bridge hop
            // toward Point 1 has actually started. This eliminates the sleep race completely.
            let start = std::time::Instant::now();
            while !sentinel.exists() && start.elapsed() < std::time::Duration::from_secs(10) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            cancel.store(true, Ordering::Relaxed);
        });
        run_mses_polar_with_cancel(&airfoil(), 0.735, 6.0e7, 6.0, &config, &root, Some(&cancel))
    });

    assert_eq!(result.point_diagnostics.len(), 1);
    assert_eq!(result.point_diagnostics[0].requested_alpha_deg, 5.0);
    assert_eq!(result.converged_alpha_count, 1);
    assert!(
        result.solver_attempts.iter().all(|a| !a
            .purpose
            .contains("bounded intermediate continuation bridge")),
        "cancellation must land before any bridge hop is attempted or counted: {:?}",
        result
            .solver_attempts
            .iter()
            .map(|a| &a.purpose)
            .collect::<Vec<_>>()
    );
    assert!(result
        .error
        .as_deref()
        .is_some_and(|message| message.contains("cancelled")));
    let _ = fs::remove_dir_all(root);
}

/// A pressure candidate that only converges off-target must not be accepted
/// as-is: the driver should bridge back to the exact requested angle and
/// report the genuinely exact-target converged state when that succeeds.
#[test]
fn a_converged_offset_pressure_candidate_bridges_back_to_the_exact_requested_angle() {
    let root = temporary_directory("pressure-bridge-recovery");
    fake_mses_installation(&root, "pressure_bridge_recovery");
    let config = MsesConfig {
        mses_dir: root.display().to_string(),
        ..MsesConfig::default()
    };

    // Defaults try 0.0 (fails once), then 0.5 (converges): the offset that
    // must trigger the exact-target bridge-back.
    let result =
        run_mses_pressure_distribution(&airfoil(), 0.735, 6.0e7, 6.0, &config, &root, None);

    assert_eq!(result.status, MsesStatus::Ok);
    assert_eq!(result.requested_alpha_deg, 6.0);
    assert_eq!(result.alpha_deg, 6.0);
    assert!(
        result.is_exact_alpha(),
        "recovered result must be the exact requested angle"
    );
    assert!(
        !result.x_upper.is_empty(),
        "an exact-target result must carry real surface data"
    );

    let offset_attempt = result
        .solver_attempts
        .iter()
        .find(|a| a.alpha_deg == 6.5 && a.status == MsesPolarPointStatus::Converged);
    assert!(
        offset_attempt.is_some(),
        "the converged offset candidate must remain in the audit trail"
    );

    let recovery_attempt = result.solver_attempts.iter().find(|a| {
        a.purpose.contains("exact-target continuation solve")
            && a.status == MsesPolarPointStatus::Converged
    });
    assert!(
        recovery_attempt.is_some(),
        "the bridge-back solve at the exact requested angle must be recorded and converged: {:?}",
        result
            .solver_attempts
            .iter()
            .map(|a| &a.purpose)
            .collect::<Vec<_>>()
    );
    let _ = fs::remove_dir_all(root);
}

/// When the exact requested angle never converges even after bridging, the
/// driver must retain the genuinely converged off-target candidate rather
/// than discarding real evidence, but it must be labeled off-target, not
/// silently presented as the requested nominal condition.
#[test]
fn a_pressure_bridge_that_cannot_reach_the_exact_angle_stays_truthfully_off_target() {
    let root = temporary_directory("pressure-bridge-never-exact");
    fake_mses_installation(&root, "pressure_bridge_never_exact");
    let config = MsesConfig {
        mses_dir: root.display().to_string(),
        ..MsesConfig::default()
    };

    let result =
        run_mses_pressure_distribution(&airfoil(), 0.735, 6.0e7, 6.0, &config, &root, None);

    assert_eq!(result.status, MsesStatus::Ok);
    assert_eq!(result.requested_alpha_deg, 6.0);
    assert_eq!(
        result.alpha_deg, 6.5,
        "must report the real converged offset, not fabricate the exact angle"
    );
    assert!(
        !result.is_exact_alpha(),
        "an off-target result must never claim to be the exact requested angle"
    );
    // Real, converged off-target contours, not empty, not fabricated.
    assert!(!result.x_upper.is_empty());

    let recovery_attempt = result
        .solver_attempts
        .iter()
        .find(|a| a.purpose.contains("exact-target continuation solve"));
    assert!(
        recovery_attempt.is_some_and(|a| a.status == MsesPolarPointStatus::NotConverged),
        "the failed bridge-back attempt must be recorded honestly: {:?}",
        result
            .solver_attempts
            .iter()
            .map(|a| &a.purpose)
            .collect::<Vec<_>>()
    );
    let _ = fs::remove_dir_all(root);
}

/// Cancellation that lands during the exact-target bridge-back must be
/// reported as cancelled, never as a fabricated (exact or off-target)
/// pressure result.
#[test]
fn cancellation_during_the_pressure_bridge_is_reported_not_fabricated() {
    let root = temporary_directory("pressure-bridge-cancel");
    fake_mses_installation(&root, "pressure_bridge_cancel");
    let config = MsesConfig {
        mses_dir: root.display().to_string(),
        ..MsesConfig::default()
    };

    let sentinel = root.join("pressure_bridge_started.sentinel");
    let cancel = AtomicBool::new(false);
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            // Wait for the sentinel indicating the offset candidates finished and the bridge hop
            // toward the exact target has actually started. This eliminates the sleep race completely.
            let start = std::time::Instant::now();
            while !sentinel.exists() && start.elapsed() < std::time::Duration::from_secs(10) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            cancel.store(true, Ordering::Relaxed);
        });
        run_mses_pressure_distribution_with_cancel(
            &airfoil(),
            0.735,
            6.0e7,
            6.0,
            &config,
            &root,
            Some(&[0.0, 1.0]),
            Some(&cancel),
        )
    });

    assert_eq!(result.status, MsesStatus::Error);
    assert!(result
        .error
        .as_deref()
        .is_some_and(|message| message.contains("cancelled")));
    assert!(
        result.x_upper.is_empty(),
        "cancellation must never fabricate surface data"
    );
    assert!(result.cp_upper.is_empty());
    let _ = fs::remove_dir_all(root);
}

/// A genuinely converged polar checkpoint, reused for a later pressure call
/// on the same geometry/config/Mach/Re/OSMAP identity, must warm-start the
/// exact requested angle directly (skipping the cold clean-mesh
/// retry-offset search entirely) and still produce a real, exact-condition
/// Cp/Mach export.
#[test]
fn a_matching_polar_checkpoint_warm_starts_the_exact_pressure_target() {
    let root = temporary_directory("checkpoint-reuse");
    fake_mses_installation(&root, "bridge_ok");
    let config = MsesConfig {
        mses_dir: root.display().to_string(),
        alpha_sweep_halfwidth_deg: 1.0,
        alpha_sweep_n_points: 3, // points: 5.0, 6.0, 7.0
        ..MsesConfig::default()
    };
    let mach = 0.735;
    let reynolds = 6.0e7;

    let polar = run_mses_polar(&airfoil(), mach, reynolds, 6.0, &config, &root);
    assert_eq!(
        polar.status,
        MsesStatus::Ok,
        "setup: the polar sweep must fully converge"
    );
    let checkpoint = polar
        .checkpoints
        .iter()
        .find(|c| c.alpha_deg == 6.0)
        .expect("setup: a checkpoint at 6.0 deg must be captured");

    // A pressure target that is not any polar point or bridge hop, so a
    // converged result can only come from the checkpoint-anchored bridge
    // (checked positively below) or, if that failed, the ordinary cold-start
    // search (checked absent below).
    let pressure = run_mses_pressure_distribution_with_checkpoint_and_cancel(
        &airfoil(),
        mach,
        reynolds,
        6.7,
        &config,
        &root,
        None,
        Some(checkpoint),
        None,
    );

    assert_eq!(pressure.status, MsesStatus::Ok);
    assert_eq!(pressure.requested_alpha_deg, 6.7);
    assert_eq!(pressure.alpha_deg, 6.7);
    assert!(pressure.is_exact_alpha());

    let checkpoint_attempt = pressure
        .solver_attempts
        .iter()
        .find(|a| a.purpose.contains("reused converged polar checkpoint"));
    assert!(
        checkpoint_attempt.is_some_and(|a| a.status == MsesPolarPointStatus::Converged),
        "the checkpoint-anchored solve must be recorded and converged: {:?}",
        pressure
            .solver_attempts
            .iter()
            .map(|a| &a.purpose)
            .collect::<Vec<_>>()
    );
    assert!(
        !pressure.solver_attempts.iter().any(|a| a.purpose.contains("initial MSET mesh")),
        "the cold-start retry-offset search must never run when the checkpoint anchor succeeds: {:?}",
        pressure.solver_attempts.iter().map(|a| &a.purpose).collect::<Vec<_>>()
    );

    // Real, exact-condition Cp/Mach export, not empty, not fabricated.
    assert!(!pressure.x_upper.is_empty());
    assert!(!pressure.cp_upper.is_empty());
    assert!(!pressure.mach_upper.is_empty());
    let _ = fs::remove_dir_all(root);
}

/// A checkpoint whose Mach/Reynolds (or, by the same identity check,
/// geometry/config/OSMAP) does not match the current driver instance must be
/// rejected outright (never trusted as a stale foreign anchor) and the
/// call must still succeed correctly through the ordinary cold-start search.
#[test]
fn a_mismatched_checkpoint_is_rejected_and_the_cold_start_search_still_succeeds() {
    let root = temporary_directory("checkpoint-mismatch");
    fake_mses_installation(&root, "bridge_ok");
    let config = MsesConfig {
        mses_dir: root.display().to_string(),
        alpha_sweep_halfwidth_deg: 0.5,
        alpha_sweep_n_points: 3,
        ..MsesConfig::default()
    };

    let polar = run_mses_polar(&airfoil(), 0.735, 6.0e7, 6.0, &config, &root);
    assert_eq!(
        polar.status,
        MsesStatus::Ok,
        "setup: the polar sweep must fully converge"
    );
    let checkpoint = polar
        .checkpoints
        .first()
        .expect("setup: at least one checkpoint must be captured");

    // Same airfoil/config, but a different Reynolds number: the identity
    // check must reject this checkpoint even though every other field matches.
    let pressure = run_mses_pressure_distribution_with_checkpoint_and_cancel(
        &airfoil(),
        0.735,
        7.0e7,
        6.2,
        &config,
        &root,
        None,
        Some(checkpoint),
        None,
    );

    assert_eq!(
        pressure.status,
        MsesStatus::Ok,
        "the cold-start fallback must still converge"
    );
    assert_eq!(pressure.alpha_deg, 6.2);
    assert!(
        !pressure
            .solver_attempts
            .iter()
            .any(|a| a.purpose.contains("reused converged polar checkpoint")),
        "a Reynolds-mismatched checkpoint must never be used: {:?}",
        pressure
            .solver_attempts
            .iter()
            .map(|a| &a.purpose)
            .collect::<Vec<_>>()
    );
    assert!(
        pressure
            .solver_attempts
            .iter()
            .any(|a| a.purpose.contains("initial MSET mesh")),
        "rejection must fall through to the ordinary cold-start search: {:?}",
        pressure
            .solver_attempts
            .iter()
            .map(|a| &a.purpose)
            .collect::<Vec<_>>()
    );
    let _ = fs::remove_dir_all(root);
}

/// A checkpoint whose Mach number differs from the pressure request must be
/// rejected outright, and the solve must proceed via cold-start search.
#[test]
fn a_mach_mismatched_checkpoint_is_rejected_and_falls_back_to_cold_start() {
    let root = temporary_directory("checkpoint-mach-mismatch");
    fake_mses_installation(&root, "bridge_ok");
    let config = MsesConfig {
        mses_dir: root.display().to_string(),
        alpha_sweep_halfwidth_deg: 0.5,
        alpha_sweep_n_points: 3,
        ..MsesConfig::default()
    };

    let polar = run_mses_polar(&airfoil(), 0.735, 6.0e7, 6.0, &config, &root);
    assert_eq!(
        polar.status,
        MsesStatus::Ok,
        "setup: polar sweep must converge"
    );
    let checkpoint = polar
        .checkpoints
        .first()
        .expect("at least one checkpoint captured");

    // Same airfoil, Re, and config, but different Mach (0.780 vs 0.735)
    let pressure = run_mses_pressure_distribution_with_checkpoint_and_cancel(
        &airfoil(),
        0.780,
        6.0e7,
        6.2,
        &config,
        &root,
        None,
        Some(checkpoint),
        None,
    );

    assert_eq!(
        pressure.status,
        MsesStatus::Ok,
        "cold-start fallback must converge"
    );
    assert_eq!(pressure.alpha_deg, 6.2);
    assert!(
        !pressure
            .solver_attempts
            .iter()
            .any(|a| a.purpose.contains("reused converged polar checkpoint")),
        "a Mach-mismatched checkpoint must never be used: {:?}",
        pressure
            .solver_attempts
            .iter()
            .map(|a| &a.purpose)
            .collect::<Vec<_>>()
    );
    assert!(
        pressure
            .solver_attempts
            .iter()
            .any(|a| a.purpose.contains("initial MSET mesh")),
        "rejection must fall through to the cold-start search: {:?}",
        pressure
            .solver_attempts
            .iter()
            .map(|a| &a.purpose)
            .collect::<Vec<_>>()
    );
    let _ = fs::remove_dir_all(root);
}

/// A checkpoint whose airfoil section differs from the pressure request must be
/// rejected outright, falling through to cold-start search.
#[test]
fn an_airfoil_geometry_mismatched_checkpoint_is_rejected_and_falls_back_to_cold_start() {
    let root = temporary_directory("checkpoint-geom-mismatch");
    fake_mses_installation(&root, "bridge_ok");
    let config = MsesConfig {
        mses_dir: root.display().to_string(),
        alpha_sweep_halfwidth_deg: 0.5,
        alpha_sweep_n_points: 3,
        ..MsesConfig::default()
    };

    let polar = run_mses_polar(&airfoil(), 0.735, 6.0e7, 6.0, &config, &root);
    assert_eq!(
        polar.status,
        MsesStatus::Ok,
        "setup: polar sweep must converge"
    );
    let checkpoint = polar.checkpoints.first().expect("checkpoint captured");

    // Different section: NACA 0012 instead of NACA 2412
    let other_airfoil = Airfoil::from_name("naca0012").expect("naca0012 is available");
    let pressure = run_mses_pressure_distribution_with_checkpoint_and_cancel(
        &other_airfoil,
        0.735,
        6.0e7,
        6.2,
        &config,
        &root,
        None,
        Some(checkpoint),
        None,
    );

    assert_eq!(
        pressure.status,
        MsesStatus::Ok,
        "cold-start fallback must converge"
    );
    assert_eq!(pressure.alpha_deg, 6.2);
    assert!(
        !pressure
            .solver_attempts
            .iter()
            .any(|a| a.purpose.contains("reused converged polar checkpoint")),
        "a geometry-mismatched checkpoint must never be used: {:?}",
        pressure
            .solver_attempts
            .iter()
            .map(|a| &a.purpose)
            .collect::<Vec<_>>()
    );
    assert!(
        pressure
            .solver_attempts
            .iter()
            .any(|a| a.purpose.contains("initial MSET mesh")),
        "rejection must fall through to the cold-start search: {:?}",
        pressure
            .solver_attempts
            .iter()
            .map(|a| &a.purpose)
            .collect::<Vec<_>>()
    );
    let _ = fs::remove_dir_all(root);
}

/// A checkpoint whose solver configuration differs (e.g. n_crit)
/// must be rejected, and the call must still succeed through cold start.
#[test]
fn a_config_mismatched_checkpoint_is_rejected_and_falls_back_to_cold_start() {
    let root = temporary_directory("checkpoint-config-mismatch");
    fake_mses_installation(&root, "bridge_ok");
    let config_polar = MsesConfig {
        mses_dir: root.display().to_string(),
        n_crit: 9.0,
        alpha_sweep_halfwidth_deg: 0.5,
        alpha_sweep_n_points: 3,
        ..MsesConfig::default()
    };

    let polar = run_mses_polar(&airfoil(), 0.735, 6.0e7, 6.0, &config_polar, &root);
    assert_eq!(polar.status, MsesStatus::Ok);
    let checkpoint = polar.checkpoints.first().expect("checkpoint captured");

    // Different n_crit configuration
    let config_pressure = MsesConfig {
        mses_dir: root.display().to_string(),
        n_crit: 10.0,
        ..MsesConfig::default()
    };
    let pressure = run_mses_pressure_distribution_with_checkpoint_and_cancel(
        &airfoil(),
        0.735,
        6.0e7,
        6.2,
        &config_pressure,
        &root,
        None,
        Some(checkpoint),
        None,
    );

    assert_eq!(pressure.status, MsesStatus::Ok);
    assert_eq!(pressure.alpha_deg, 6.2);
    assert!(
        !pressure
            .solver_attempts
            .iter()
            .any(|a| a.purpose.contains("reused converged polar checkpoint")),
        "a config-mismatched checkpoint must never be used"
    );
    assert!(
        pressure
            .solver_attempts
            .iter()
            .any(|a| a.purpose.contains("initial MSET mesh")),
        "rejection must fall through to the cold-start search"
    );
    let _ = fs::remove_dir_all(root);
}

/// From multiple polar checkpoints, closest_checkpoint selects the nearest
/// finite angle using total_cmp and warm-starts the pressure distribution from it.
#[test]
fn closest_checkpoint_selection_prefers_nearest_finite_polar_anchor_with_total_cmp() {
    let root = temporary_directory("checkpoint-closest-selection");
    fake_mses_installation(&root, "bridge_ok");
    let config = MsesConfig {
        mses_dir: root.display().to_string(),
        alpha_sweep_halfwidth_deg: 1.0,
        alpha_sweep_n_points: 3, // 5.0, 6.0, 7.0 deg
        ..MsesConfig::default()
    };

    let polar = run_mses_polar(&airfoil(), 0.735, 6.0e7, 6.0, &config, &root);
    assert_eq!(polar.status, MsesStatus::Ok);
    assert_eq!(polar.checkpoints.len(), 3);

    // Target 6.1 is closest to 6.0 (diff 0.1 vs 1.1 to 5.0 and 0.9 to 7.0)
    let closest = polar
        .closest_checkpoint(6.1)
        .expect("closest checkpoint for 6.1");
    assert_eq!(closest.alpha_deg, 6.0);

    // Target 6.8 is closest to 7.0 (diff 0.2 vs 0.8 to 6.0)
    let closest_hi = polar
        .closest_checkpoint(6.8)
        .expect("closest checkpoint for 6.8");
    assert_eq!(closest_hi.alpha_deg, 7.0);

    // Non-finite targets yield None
    assert!(polar.closest_checkpoint(f64::NAN).is_none());
    assert!(polar.closest_checkpoint(f64::INFINITY).is_none());

    // Solve pressure at 6.1 with closest checkpoint
    let pressure = run_mses_pressure_distribution_with_checkpoint_and_cancel(
        &airfoil(),
        0.735,
        6.0e7,
        6.1,
        &config,
        &root,
        None,
        Some(closest),
        None,
    );

    assert_eq!(pressure.status, MsesStatus::Ok);
    assert_eq!(pressure.alpha_deg, 6.1);
    assert!(pressure.is_exact_alpha());

    // Verify warm-start anchor came specifically from 6.0 deg
    let attempt = pressure
        .solver_attempts
        .iter()
        .find(|a| a.purpose.contains("reused converged polar checkpoint"))
        .expect("warm-start attempt recorded");
    assert!(attempt
        .purpose
        .contains("originally converged at alpha=6.000000 deg"));
    assert!(!pressure
        .solver_attempts
        .iter()
        .any(|a| a.purpose.contains("initial MSET mesh")));
    let _ = fs::remove_dir_all(root);
}

/// When a checkpoint is provided but its warm-start solve diverges or fails,
/// the driver must not fail: it must boundedly fall back to the cold-start
/// retry-offset search and succeed, preserving provenance and recording all attempts.
#[test]
fn checkpoint_warm_start_failure_gracefully_falls_back_to_cold_start_mesh() {
    let root = temporary_directory("checkpoint-fallback");
    fake_mses_installation(&root, "checkpoint_warm_start_fails");
    write_osmap_fixture(&root);
    let config = MsesConfig {
        mses_dir: root.display().to_string(),
        alpha_sweep_halfwidth_deg: 0.5,
        alpha_sweep_n_points: 3, // points at 5.5, 6.0, 6.5 deg
        ..MsesConfig::default()
    };

    let polar = run_mses_polar(&airfoil(), 0.735, 6.0e7, 6.0, &config, &root);
    assert_eq!(polar.status, MsesStatus::Ok);
    let checkpoint = polar
        .closest_checkpoint(6.0)
        .expect("checkpoint captured at 6.0 deg");
    assert_eq!(checkpoint.alpha_deg, 6.0);

    let pressure = run_mses_pressure_distribution_with_checkpoint_and_cancel(
        &airfoil(),
        0.735,
        6.0e7,
        6.0,
        &config,
        &root,
        None,
        Some(checkpoint),
        None,
    );

    assert_eq!(pressure.status, MsesStatus::Ok, "must succeed via fallback");
    assert_eq!(pressure.alpha_deg, 6.0);
    assert!(pressure.is_exact_alpha());

    // Both the failed warm-start attempt and the successful cold-start attempt must be recorded
    let warm_attempt = pressure
        .solver_attempts
        .iter()
        .find(|a| a.purpose.contains("reused converged polar checkpoint"))
        .expect("warm-start attempt must be recorded");
    assert_eq!(
        warm_attempt.status,
        MsesPolarPointStatus::NotConverged,
        "warm-start attempt must fail"
    );

    let cold_attempt = pressure
        .solver_attempts
        .iter()
        .find(|a| a.purpose.contains("initial MSET mesh"))
        .expect("cold-start fallback attempt must be recorded");
    assert_eq!(
        cold_attempt.status,
        MsesPolarPointStatus::Converged,
        "cold-start fallback must converge"
    );
    let _ = fs::remove_dir_all(root);
}

/// Simulates the exact pipeline choreography: run_mses_polar followed by
/// run_mses_pressure_distribution_with_checkpoint_and_cancel with polar.closest_checkpoint(alpha).
/// Verifies that exact target angle reuses the checkpoint directly, produces coherent exports,
/// and leaves no stale disk state.
#[test]
fn pipeline_pattern_polar_then_pressure_reuses_closest_converged_checkpoint() {
    let root = temporary_directory("pipeline-integration-pattern");
    fake_mses_installation(&root, "bridge_ok");
    write_osmap_fixture(&root);
    let config = MsesConfig {
        mses_dir: root.display().to_string(),
        alpha_sweep_halfwidth_deg: 1.0,
        alpha_sweep_n_points: 3, // 5.0, 6.0, 7.0 deg
        ..MsesConfig::default()
    };
    let mach = 0.735;
    let reynolds = 6.0e7;
    let target_alpha = 6.0;

    let polar = run_mses_polar_with_cancel(
        &airfoil(),
        mach,
        reynolds,
        target_alpha,
        &config,
        &root,
        None,
    );
    assert_eq!(polar.status, MsesStatus::Ok);
    assert!(polar.has_usable_data());
    assert_eq!(polar.checkpoints.len(), 3);

    // Closest checkpoint to exact 6.0 deg is the 6.0 deg checkpoint
    let best_cp = polar.closest_checkpoint(target_alpha);
    assert!(best_cp.is_some());
    assert_eq!(best_cp.unwrap().alpha_deg, 6.0);

    let pressure = run_mses_pressure_distribution_with_checkpoint_and_cancel(
        &airfoil(),
        mach,
        reynolds,
        target_alpha,
        &config,
        &root,
        None,
        best_cp,
        None,
    );

    assert_eq!(pressure.status, MsesStatus::Ok);
    assert_eq!(pressure.requested_alpha_deg, target_alpha);
    assert_eq!(pressure.alpha_deg, target_alpha);
    assert!(pressure.is_exact_alpha());
    assert!(pressure.transition_model_is_valid());
    assert!(pressure.is_valid_for_presentation());
    assert!(!pressure.x_upper.is_empty());
    assert!(!pressure.cp_upper.is_empty());
    assert!(!pressure.mach_upper.is_empty());
    assert!(!pressure.field_x.is_empty());

    // Exact match target: no bridging hops needed, warm start directly solves target
    assert!(
        !pressure.solver_attempts.iter().any(|a| a
            .purpose
            .contains("bounded intermediate continuation bridge")),
        "exact target match does not need intermediate bridging hops: {:?}",
        pressure
            .solver_attempts
            .iter()
            .map(|a| &a.purpose)
            .collect::<Vec<_>>()
    );
    assert!(
        !pressure
            .solver_attempts
            .iter()
            .any(|a| a.purpose.contains("initial MSET mesh")),
        "cold-start mesh search must not run when warm start succeeds"
    );
    let _ = fs::remove_dir_all(root);
}
