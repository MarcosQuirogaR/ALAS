// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Test fixtures fail at the assertion site when inputs or native outputs are malformed.
#![allow(clippy::expect_used)]
use super::*;

/// Replays a saved product case without changing its solver settings.
/// ALAS_MSES_REPLAY_DIR is its optimize directory; output goes to
/// ALAS_MSES_REPLAY_OUTPUT. Native executables and OSMAP must be installed.
#[test]
#[ignore = "requires saved aircraft case and installed native MSES"]
fn native_requested_bridge_recovery_replay() {
    let root = PathBuf::from(std::env::var("ALAS_MSES_REPLAY_DIR").expect("replay directory"));
    let output = PathBuf::from(std::env::var("ALAS_MSES_REPLAY_OUTPUT").expect("output path"));
    let input: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("input_config.json")).expect("saved config"),
    )
    .expect("valid config JSON");
    let config: MsesConfig = serde_json::from_value(input["mses"].clone()).expect("MSES config");
    let previous: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("mses/polar_diagnostics.json")).expect("saved polar"),
    )
    .expect("valid polar JSON");
    let text =
        std::fs::read_to_string(root.join("airfoils/optimized_root.dat")).expect("saved section");
    let coordinates = text
        .lines()
        .skip(1)
        .map(|line| {
            let values: Vec<f64> = line
                .split_whitespace()
                .map(|value| value.parse().expect("coordinate"))
                .collect();
            (values[0], values[1])
        })
        .collect();
    let airfoil = Airfoil::from_coordinates("morphed", coordinates)
        .repanel(crate::mses::N_POINTS_PER_SIDE)
        .expect("repanel saved section");
    let alphas: Vec<f64> = previous["points"]
        .as_array()
        .expect("requested points")
        .iter()
        .map(|point| point["requested_alpha_deg"].as_f64().expect("alpha"))
        .collect();
    let driver = Mses::new(airfoil, &config, Path::new(&config.mses_dir));
    let start = std::time::Instant::now();
    if std::env::var_os("ALAS_MSES_REPLAY_BRIDGE_ONLY").is_some() {
        // A controlled native checkpoint test of extraction during a bridge.
        // The failed diagnostic is synthetic; the mesh/flow/MPLOT are native.
        let anchor = alphas[3];
        let requested = anchor + 0.5;
        let re = previous["reynolds"].as_f64().expect("Re");
        let mach = previous["mach"].as_f64().expect("Mach");
        let workdir = WorkDir::new("alas_mses_bridge_replay_").expect("native workdir");
        let anchor_outcome = driver
            .solve_sweep(workdir.path(), &[anchor], re, mach, None)
            .expect("native anchor solve");
        assert_eq!(anchor_outcome.accumulated["alpha"].len(), 1);
        let mut recovery = RequestedPolarRecovery::new(&[requested]);
        recovery.visited_count = 1;
        let mut outcome = SweepOutcome::default();
        outcome.point_diagnostics.push(MsesPolarPointDiagnostic {
            requested_alpha_deg: requested,
            status: MsesPolarPointStatus::NotConverged,
            solver_output: "synthetic prior failed requested-point diagnostic".to_owned(),
        });
        driver
            .bridge_to_target_with_recovery(
                workdir.path(),
                anchor,
                anchor + 1.0,
                re,
                mach,
                None,
                &mut outcome.solver_attempts,
                Some(&mut recovery),
            )
            .expect("native bridge solve");
        recovery.finish(&mut outcome);
        let artifact = serde_json::json!({
            "elapsed_seconds": start.elapsed().as_secs_f64(),
            "anchor_alpha": anchor, "requested_alpha": requested,
            "columns": outcome.accumulated,
            "status": outcome.point_diagnostics[0].status.as_str(),
            "solver_output": outcome.point_diagnostics[0].solver_output,
            "checkpoint_count": outcome.checkpoints.len(),
            "attempts": outcome.solver_attempts.iter().map(|attempt| serde_json::json!({
                "alpha": attempt.alpha_deg, "purpose": attempt.purpose,
                "status": attempt.status.as_str(), "solver_output": attempt.solver_output,
            })).collect::<Vec<_>>(),
        });
        std::fs::write(
            output,
            serde_json::to_vec_pretty(&artifact).expect("serialize bridge"),
        )
        .expect("write bridge");
        assert_eq!(
            outcome.point_diagnostics[0].status,
            MsesPolarPointStatus::Converged
        );
        assert_eq!(outcome.checkpoints.len(), 1);
        assert!(validated_polar_columns(&outcome.accumulated, 1).is_ok());
        assert!(parse::is_converged(
            &outcome.point_diagnostics[0].solver_output
        ));
        return;
    }
    let result = driver.polar_with_cancel(
        &alphas,
        previous["reynolds"].as_f64().expect("Re"),
        previous["mach"].as_f64().expect("Mach"),
        None,
    );
    let artifact = serde_json::json!({
        "elapsed_seconds": start.elapsed().as_secs_f64(),
        "status": result.status.as_str(), "error": result.error,
        "requested_alpha": alphas, "converged_alpha": result.alpha_deg,
        "cl": result.cl, "cd": result.cd, "cm": result.cm,
        "points": result.point_diagnostics.iter().map(|point| serde_json::json!({
            "alpha": point.requested_alpha_deg, "status": point.status.as_str(),
            "solver_output": point.solver_output,
        })).collect::<Vec<_>>(),
        "attempts": result.solver_attempts.iter().map(|attempt| serde_json::json!({
            "alpha": attempt.alpha_deg, "purpose": attempt.purpose,
            "status": attempt.status.as_str(), "solver_output": attempt.solver_output,
        })).collect::<Vec<_>>(),
    });
    std::fs::write(
        output,
        serde_json::to_vec_pretty(&artifact).expect("serialize replay"),
    )
    .expect("write replay");
    assert_eq!(result.point_diagnostics.len(), alphas.len());
    assert_eq!(
        result.converged_alpha_count,
        result
            .point_diagnostics
            .iter()
            .filter(|point| point.status == MsesPolarPointStatus::Converged)
            .count()
    );
}

fn recovered_test_point(alpha: f64) -> RecoveredPolarPoint {
    RecoveredPolarPoint {
        summary: parse::POLAR_REQUIRED_COLUMNS
            .iter()
            .map(|&key| (key.to_owned(), if key == "alpha" { alpha } else { 0.01 }))
            .collect(),
        solver_output: "Converged on tolerance during bridge".to_owned(),
        checkpoint: None,
    }
}

#[test]
fn bridge_recovery_preserves_requested_order_and_does_not_duplicate_points() {
    let alphas = [1.0, 2.0, 3.0];
    let mut recovery = RequestedPolarRecovery::new(&alphas);
    recovery.visited_count = 3;
    // The middle point is recovered only after the last regular point.
    recovery.points[0] = Some(recovered_test_point(1.0));
    recovery.points[2] = Some(recovered_test_point(3.0));
    assert_eq!(recovery.missing_requested_index(2.0), Some(1));
    recovery.points[1] = Some(recovered_test_point(2.0));
    assert_eq!(recovery.missing_requested_index(2.0), None);
    let mut outcome = SweepOutcome {
        point_diagnostics: alphas
            .iter()
            .map(|&requested_alpha_deg| MsesPolarPointDiagnostic {
                requested_alpha_deg,
                status: MsesPolarPointStatus::NotConverged,
                solver_output: "original failed solve".to_owned(),
            })
            .collect(),
        ..SweepOutcome::default()
    };
    recovery.finish(&mut outcome);
    assert_eq!(outcome.accumulated["alpha"], alphas);
    assert!(outcome.point_diagnostics.iter().all(|point| {
        point.status == MsesPolarPointStatus::Converged
            && point.solver_output.contains("during bridge")
    }));
    assert!(validated_polar_columns(&outcome.accumulated, 3).is_ok());
}

#[test]
fn bridge_recovery_excludes_nearby_unrequested_and_unvisited_angles() {
    let mut recovery = RequestedPolarRecovery::new(&[1.0, 2.0]);
    recovery.visited_count = 1;
    assert_eq!(
        recovery.missing_requested_index(1.0 + f64::EPSILON),
        Some(0)
    );
    assert_eq!(recovery.missing_requested_index(1.000001), None);
    assert_eq!(recovery.missing_requested_index(1.5), None);
    assert_eq!(recovery.missing_requested_index(2.0), None);
}

#[test]
fn bridge_convergence_without_extracted_row_remains_failed() {
    let mut outcome = SweepOutcome::default();
    outcome.point_diagnostics.push(MsesPolarPointDiagnostic {
        requested_alpha_deg: 2.0,
        status: MsesPolarPointStatus::NotConverged,
        solver_output: "original failed solve".to_owned(),
    });
    outcome.solver_attempts.push(MsesSolverAttempt {
        alpha_deg: 2.0,
        purpose: "later bridge".to_owned(),
        status: MsesPolarPointStatus::Converged,
        solver_output: "Converged on tolerance".to_owned(),
    });
    RequestedPolarRecovery::new(&[2.0]).finish(&mut outcome);
    assert!(outcome.accumulated.is_empty());
    assert_eq!(
        outcome.point_diagnostics[0].status,
        MsesPolarPointStatus::NotConverged
    );
}
