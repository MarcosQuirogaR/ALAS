// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! External-tool failures of a finished run, stated in one list.
//!
//! A native solver that aborts (for example AVL's "Vortex array overflow")
//! leaves the run itself valid but its overlay absent. The failure is
//! reported here so the CLI log, the run manifest and the GUI never show a
//! silent success.

use crate::openvsp::OpenVspExportStatus;
use crate::PipelineResult;

/// Statuses that mean "the tool did not run or did not deliver" and that the
/// user did not cause by leaving the tool unconfigured or by cancelling.
const NON_FAILURES: [&str; 5] = [
    "not_configured",
    "cancelled",
    "completed_comparable",
    "completed_not_comparable",
    "absent",
];

/// One line per external tool that ran or was requested and failed.
pub fn tool_failures(result: &PipelineResult) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(export) = &result.openvsp_export {
        // An unverified script means no OpenVSP runtime was configured.
        if !matches!(
            export.status,
            OpenVspExportStatus::Vsp3Materialized
                | OpenVspExportStatus::ScriptWrittenRuntimeUnverified
        ) {
            lines.push(line(
                "OpenVSP geometry export",
                export.status.as_str(),
                export.runtime_error.as_deref(),
            ));
        }
    }
    if let Some(vspaero) = &result.vspaero_result {
        push(
            &mut lines,
            "VSPAERO",
            vspaero.status.as_str(),
            vspaero.error.as_deref(),
        );
    }
    if let Some(avl) = &result.avl_result {
        push(
            &mut lines,
            "AVL (take-off)",
            avl.status.as_str(),
            avl.error.as_deref(),
        );
        if let Some(cruise) = &avl.linear_cruise {
            push(
                &mut lines,
                "AVL (linear cruise)",
                cruise.status.as_str(),
                cruise.error.as_deref(),
            );
        }
    }
    if let Some(flow) = &result.flowunsteady_result {
        push(
            &mut lines,
            "FLOWUnsteady",
            flow.status.as_str(),
            flow.error.as_deref(),
        );
    }
    lines
}

fn push(lines: &mut Vec<String>, tool: &str, status: &str, error: Option<&str>) {
    if !NON_FAILURES.contains(&status) {
        lines.push(line(tool, status, error));
    }
}

fn line(tool: &str, status: &str, error: Option<&str>) -> String {
    match error {
        Some(error) if !error.is_empty() => format!("{tool} failed ({status}): {error}"),
        _ => format!("{tool} failed ({status})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_and_comparison_outcomes_are_not_failures() {
        let mut lines = Vec::new();
        for status in NON_FAILURES {
            push(&mut lines, "AVL", status, None);
        }
        assert!(lines.is_empty());
    }

    #[test]
    fn solver_abort_is_a_failure_with_its_message() {
        let mut lines = Vec::new();
        push(
            &mut lines,
            "AVL",
            "solver_failed",
            Some("AVL aborted: SDUPL: Vortex array overflow"),
        );
        assert_eq!(
            lines,
            ["AVL failed (solver_failed): AVL aborted: SDUPL: Vortex array overflow"]
        );
    }
}
