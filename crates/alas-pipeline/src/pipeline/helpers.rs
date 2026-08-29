// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small persistence and validation helpers for the pipeline coordinator.
//!
//! Keeping these helpers beside, rather than inside, the coordinator leaves
//! the stage ordering readable and prevents the orchestration module from
//! becoming a second export implementation.

use std::collections::BTreeMap;
use std::path::Path;

use alas_aero::mses::{MsesPolarResult, MsesPressureResult};
use alas_config::design_variables::DesignVector;
#[cfg(test)]
use alas_config::AlasConfig;
use serde_json::json;

pub(super) fn persist_mses_raw_exports(
    result: &MsesPressureResult,
    output_dir: &Path,
) -> std::io::Result<()> {
    if result.raw_bl_dump.is_empty() && result.raw_flowfield_dump.is_empty() {
        return Ok(());
    }
    let directory = output_dir.join("mses");
    std::fs::create_dir_all(&directory)?;
    if !result.raw_bl_dump.is_empty() {
        std::fs::write(directory.join("bl_dump.txt"), &result.raw_bl_dump)?;
    }
    if !result.raw_flowfield_dump.is_empty() {
        std::fs::write(directory.join("flowfield.txt"), &result.raw_flowfield_dump)?;
    }
    Ok(())
}

/// Persist the exact requested alpha schedule and MSES transcript per point.
///
/// Coefficient arrays deliberately omit non-converged points. This sibling
/// artifact records their solver evidence without altering those arrays or the
/// existing raw `mplot` figure exports.
pub(super) fn persist_mses_polar_diagnostics(
    result: &MsesPolarResult,
    output_dir: &Path,
) -> std::io::Result<()> {
    if result.point_diagnostics.is_empty() {
        return Ok(());
    }
    let directory = output_dir.join("mses");
    std::fs::create_dir_all(&directory)?;
    let points = result
        .point_diagnostics
        .iter()
        .map(|point| {
            json!({
                "requested_alpha_deg": point.requested_alpha_deg,
                "status": point.status.as_str(),
                "solver_output": point.solver_output,
            })
        })
        .collect::<Vec<_>>();
    let document = json!({
        "status": result.status.as_str(),
        "requested_alpha_count": result.requested_alpha_count,
        "converged_alpha_count": result.converged_alpha_count,
        "points": points,
    });
    let text = serde_json::to_string_pretty(&document).map_err(std::io::Error::other)?;
    std::fs::write(directory.join("polar_diagnostics.json"), text)
}

pub(super) fn validate_bounds(bounds: &[(f64, f64)]) -> Result<(), String> {
    let expected = DesignVector::bounds().len();
    if bounds.len() != expected {
        return Err(format!(
            "design space has {} bound pairs; expected {expected}",
            bounds.len()
        ));
    }
    for (index, &(lower, upper)) in bounds.iter().enumerate() {
        if !lower.is_finite() || !upper.is_finite() || lower > upper {
            return Err(format!(
                "design-space bound {index} is invalid: [{lower}, {upper}]"
            ));
        }
    }
    Ok(())
}

pub(super) fn add_manifest_artifact(
    artifacts: &mut BTreeMap<String, String>,
    output_dir: &Path,
    name: &str,
    path: &Path,
) {
    let value = path
        .strip_prefix(output_dir)
        .unwrap_or(path)
        .display()
        .to_string();
    artifacts.insert(name.to_owned(), value);
}

#[cfg(test)]
pub(super) fn optimizer_config(
    config: &AlasConfig,
    seed: Option<u64>,
) -> Result<AlasConfig, String> {
    let mut effective = config.clone();
    if let Some(seed) = seed {
        effective.optimizer.solver.seed = Some(
            i64::try_from(seed)
                .map_err(|_| "optimizer seed exceeds the supported integer range")?,
        );
    }
    Ok(effective)
}
