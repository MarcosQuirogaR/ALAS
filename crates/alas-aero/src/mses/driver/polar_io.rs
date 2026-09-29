// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// Validate the accumulated MPlot polar schema before publishing it.
///
/// Each converged MPlot invocation must provide every public polar column.
/// Checking both length and finiteness here keeps this boundary defensive if
/// aggregation changes later, and prevents a missing/ragged column from being
/// silently replaced with fabricated zeros.
pub(super) fn validated_polar_columns(
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

pub(super) fn polar_completion_status(requested: usize, converged: usize) -> MsesStatus {
    if requested > 0 && converged == requested {
        MsesStatus::Ok
    } else if converged > 0 {
        MsesStatus::PartialConvergence
    } else {
        MsesStatus::Error
    }
}

pub(super) fn installation_message(directory: &Path, status: MsesStatus) -> String {
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
pub(super) fn read_lossy(path: &Path) -> String {
    match std::fs::read(path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => String::new(),
    }
}
