// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

/// Validate the accumulated MPlot polar schema before publishing it.
///
/// Each converged MPlot invocation must provide every public polar column.
/// Checking both length and finiteness here keeps this boundary defensive if
/// aggregation changes later, and prevents a missing/ragged column from being
/// silently replaced with fabricated zeros.
fn validated_polar_columns(
    accumulated: &HashMap<String, Vec<f64>>,
    expected_len: usize,
) -> Result<[Vec<f64>; 8], String> {
    let mut columns = Vec::with_capacity(parse::POLAR_REQUIRED_COLUMNS.len());
    for &key in &parse::POLAR_REQUIRED_COLUMNS {
        let Some(values) = accumulated.get(key) else {
            return Err(format!(
                "mplot polar output missing required column '{key}'"
            ));
        };
        if values.len() != expected_len {
            return Err(format!(
                "mplot polar column '{key}' has {} values for {expected_len} alpha points",
                values.len()
            ));
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(format!(
                "mplot polar column '{key}' is non-finite or malformed"
            ));
        }
        columns.push(values.clone());
    }
    columns
        .try_into()
        .map_err(|_| "internal MPlot polar schema length mismatch".to_owned())
}

fn polar_completion_status(requested: usize, converged: usize) -> MsesStatus {
    if requested > 0 && converged == requested {
        MsesStatus::Ok
    } else if converged > 0 {
        MsesStatus::PartialConvergence
    } else {
        MsesStatus::Error
    }
}

fn installation_message(directory: &Path, status: MsesStatus) -> String {
    match status {
        MsesStatus::Absent => format!("MSES installation was not found at {}", directory.display()),
        MsesStatus::Incomplete => format!(
            "MSES installation at {} is incomplete; required mset.exe, mses.exe, and mplot.exe",
            directory.display()
        ),
        _ => format!(
            "MSES installation at {} is unavailable",
            directory.display()
        ),
    }
}

/// Read a file as text, replacing invalid bytes rather than failing: the
/// counterpart of upstream's `open(..., errors="replace")`. The dump files are
/// ASCII, so the replacement never fires; it is faithfulness, not a fix.
fn read_lossy(path: &Path) -> String {
    match std::fs::read(path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => String::new(),
    }
}

// Test fixtures use `expect`/`expect_err` so malformed cases fail at the
// assertion site; this allowance is intentionally scoped to the test module.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_sweep_is_not_reported_as_ok() {
        assert_eq!(
            polar_completion_status(7, 4),
            MsesStatus::PartialConvergence
        );
    }

    #[test]
    fn complete_and_empty_sweeps_have_distinct_statuses() {
        assert_eq!(polar_completion_status(7, 7), MsesStatus::Ok);
        assert_eq!(polar_completion_status(7, 0), MsesStatus::Error);
    }

    #[test]
    fn subprocess_timeout_becomes_a_timeout_status() {
        let failure = MsesFailure::from_run(RunError::Timeout {
            command: "mset.exe".to_owned(),
            seconds: 1.0,
        });
        assert_eq!(failure.status, MsesStatus::Timeout);
    }

    #[test]
    fn subprocess_spawn_failure_becomes_a_launch_failure_status() {
        let failure = MsesFailure::from_run(RunError::Spawn {
            command: "mset.exe".to_owned(),
            source: std::io::Error::other("test launch failure"),
        });
        assert_eq!(failure.status, MsesStatus::LaunchFailure);
    }

    #[test]
    fn malformed_output_becomes_a_parse_failure_status() {
        assert_eq!(
            MsesFailure::parse("bad output").status,
            MsesStatus::ParseFailure
        );
    }

    fn complete_accumulated_polar() -> HashMap<String, Vec<f64>> {
        parse::POLAR_REQUIRED_COLUMNS
            .into_iter()
            .map(|key| {
                let values = if key == "CDw" {
                    vec![0.0, 0.0]
                } else {
                    vec![1.0, 2.0]
                };
                (key.to_owned(), values)
            })
            .collect()
    }

    #[test]
    fn validated_polar_columns_keep_zero_wave_drag() {
        let accumulated = complete_accumulated_polar();
        let columns = validated_polar_columns(&accumulated, 2).expect("complete polar");
        assert_eq!(columns[0], vec![1.0, 2.0]);
        assert_eq!(columns[5], vec![0.0, 0.0]);
    }

    #[test]
    fn validated_polar_columns_reject_ragged_required_data() {
        let mut accumulated = complete_accumulated_polar();
        accumulated.insert("CD".to_owned(), vec![0.03]);
        let error = validated_polar_columns(&accumulated, 2)
            .expect_err("a ragged coefficient column must be rejected");
        assert_eq!(
            error,
            "mplot polar column 'CD' has 1 values for 2 alpha points"
        );
    }

    #[test]
    fn validated_polar_columns_reject_nonfinite_required_data() {
        let mut accumulated = complete_accumulated_polar();
        accumulated.insert("CL".to_owned(), vec![1.0, f64::NAN]);
        let error = validated_polar_columns(&accumulated, 2)
            .expect_err("a non-finite coefficient column must be rejected");
        assert_eq!(error, "mplot polar column 'CL' is non-finite or malformed");
    }

    fn write_osmap_header(path: &Path, table_record_length: i32) {
        let mut header = Vec::new();
        for value in [12_i32, 28, 41, 18, 12, table_record_length] {
            header.extend_from_slice(&value.to_le_bytes());
        }
        std::fs::write(path, header).expect("write OSMAP fixture header");
    }

    #[test]
    fn osmap_header_distinguishes_double_and_single_precision_resources() {
        let workdir = WorkDir::new("alas_osmap_test_").expect("temporary OSMAP directory");
        let double = workdir.path().join("osmapDP.dat");
        let single = workdir.path().join("osmap.dat");
        write_osmap_header(&double, 224);
        write_osmap_header(&single, 112);
        assert!(inspect_osmap(&double).is_ok());
        let error = inspect_osmap(&single).expect_err("single-precision map must be rejected");
        assert!(error.contains("single-precision"));
    }

    #[test]
    fn relative_osmap_diagnostics_are_resolved_from_the_host_working_directory() {
        let relative = PathBuf::from("alas-definitely-missing-osmap.dat");
        let expected = std::env::current_dir()
            .expect("the test has a working directory")
            .join(&relative);
        let error = osmap_candidate(relative, "test").expect_err("the fixture is absent");
        assert!(
            error.contains(&expected.display().to_string()),
            "relative resource paths must be normalized before child launch: {error}"
        );
    }

    #[test]
    fn free_transition_requires_a_compatible_map_but_forced_transition_does_not() {
        let workdir =
            WorkDir::new("alas_osmap_resolution_test_").expect("temporary OSMAP directory");
        let mut config = MsesConfig {
            mses_dir: workdir.path().display().to_string(),
            ..MsesConfig::default()
        };
        let missing = resolve_osmap(&config, workdir.path());
        assert!(missing.required);
        assert_eq!(missing.status, MsesOsmapStatus::Missing);

        config.xtr_upper = 0.5;
        config.xtr_lower = 0.5;
        let forced = resolve_osmap(&config, workdir.path());
        assert!(!forced.required);
        assert_eq!(forced.status, MsesOsmapStatus::NotRequired);
    }

    #[test]
    fn release_asset_is_used_without_mses_directory_or_working_directory_dependence() {
        let package = WorkDir::new("alas_osmap_package_test_").expect("temporary package root");
        let mses_dir = package.path().join("external tools").join("MSES");
        std::fs::create_dir_all(&mses_dir).expect("temporary MSES directory");
        let asset = package
            .path()
            .join("assets")
            .join("mses")
            .join("osmapDP.dat");
        std::fs::create_dir_all(asset.parent().expect("asset parent")).expect("asset directory");
        write_osmap_header(&asset, 224);

        let config = MsesConfig {
            mses_dir: mses_dir.display().to_string(),
            ..MsesConfig::default()
        };
        let selection = resolve_osmap(&config, &mses_dir);
        assert_eq!(selection.status, MsesOsmapStatus::Available);
        assert_eq!(selection.path.as_deref(), Some(asset.as_path()));
        assert!(selection
            .diagnostic
            .as_deref()
            .is_some_and(|text| text.contains("bundled ALAS")));
    }

    #[test]
    fn a_finite_pressure_table_without_osmap_is_not_valid_free_transition_evidence() {
        let mut result = MsesPressureResult {
            status: MsesStatus::Ok,
            solver_attempts: vec![MsesSolverAttempt {
                alpha_deg: 2.0,
                purpose: "requested".to_owned(),
                status: MsesPolarPointStatus::Converged,
                solver_output: "Converged on tolerance".to_owned(),
            }],
            osmap_required: true,
            osmap_status: MsesOsmapStatus::Missing,
            ..MsesPressureResult::default()
        };
        assert!(!result.transition_model_is_valid());
        assert!(!result.has_convergence_evidence());
        assert!(!result.is_valid_for_presentation());
        result.osmap_status = MsesOsmapStatus::Available;
        assert!(result.transition_model_is_valid());
        assert!(result.has_convergence_evidence());
        assert!(result.is_valid_for_presentation());
    }

    #[test]
    fn continuation_step_bound_rejects_large_incidence_jumps() {
        const {
            assert!(
                MAX_CONTINUATION_STEP_DEG <= 0.5,
                "continuation step bound must match MSES continuation guidance (<= 0.5 deg)"
            );
        }
        let tight_sweep_step: f64 = 0.5;
        assert!(tight_sweep_step <= MAX_CONTINUATION_STEP_DEG);
        let default_sweep_step: f64 = 1.0;
        assert!(default_sweep_step > MAX_CONTINUATION_STEP_DEG);
        let center_restart_step: f64 = 3.0;
        assert!(center_restart_step > MAX_CONTINUATION_STEP_DEG);
        let neutral_zero_step: f64 = 6.636;
        assert!(neutral_zero_step > MAX_CONTINUATION_STEP_DEG);
    }

    #[test]
    fn bridge_to_target_honors_cancellation_before_any_intermediate_solve() {
        let workdir = WorkDir::new("alas_bridge_cancel_test_").expect("temporary directory");
        std::fs::write(
            workdir.path().join("mdat.case.last_converged"),
            b"fake converged checkpoint",
        )
        .expect("seed a converged checkpoint to restore from");

        let airfoil = Airfoil::from_name("naca2412").expect("the built-in NACA section is available");
        let config = MsesConfig::default();
        let driver = Mses::new(airfoil, &config, Path::new("nonexistent-mses-dir"));
        let cancel = AtomicBool::new(true);
        let mut attempts = Vec::new();

        // A 2.0 deg gap is well past MAX_CONTINUATION_STEP_DEG, so a live
        // bridge would need at least one intermediate hop; if cancellation
        // were not checked first, this would try to spawn `mses.exe` at a
        // path that does not exist.
        let outcome = driver
            .bridge_to_target(workdir.path(), 0.0, 2.0, 5.0e6, 0.3, Some(&cancel), &mut attempts)
            .expect("a pre-cancelled bridge must short-circuit rather than spawn a process");

        assert!(matches!(outcome, BridgeOutcome::Cancelled));
        assert!(
            attempts.is_empty(),
            "a cancelled bridge must never record an intermediate attempt"
        );
    }
}
