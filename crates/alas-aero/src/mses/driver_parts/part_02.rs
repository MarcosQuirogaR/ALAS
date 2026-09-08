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

/// Read a file as text, replacing invalid bytes rather than failing -- the
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
}

