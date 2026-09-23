// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Typed run events, stage timers and external-tool diagnostics emitted by
//! the pipeline coordinator.

use std::time::Instant;

use alas_aero::mses::MsesPolarResult;
use alas_exec::RunEnvironment;

use crate::avl::AvlAnalysisResult;
use crate::flowunsteady::FlowUnsteadyAnalysisResult;
use crate::openvsp::OpenVspExportResult;
use crate::runs::{RunEvent, RunEventKind, RunEventSeverity};
use crate::structural::StructuralAnalysisResult;
use crate::vspaero::VspaeroAnalysisResult;

const PIPELINE_STAGE_COUNT: u8 = 7;

pub(super) fn emit_event(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    event: RunEvent,
) {
    if let Some(callback) = events {
        callback(RunEvent {
            elapsed_ms: run_clock.elapsed().as_millis() as u64,
            ..event
        });
    }
}

pub(super) fn begin_stage(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    index: u8,
    stage: &str,
    message: &str,
) -> Instant {
    emit_event(
        events,
        run_clock,
        RunEvent {
            stage: stage.to_owned(),
            message: message.to_owned(),
            fraction: Some(0.0),
            kind: RunEventKind::StageStarted,
            severity: RunEventSeverity::Info,
            stage_index: Some(index),
            stage_count: Some(PIPELINE_STAGE_COUNT),
            elapsed_ms: 0,
            duration_ms: None,
        },
    );
    Instant::now()
}

pub(super) fn finish_stage(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    stage_clock: Instant,
    index: u8,
    stage: &str,
) {
    let duration_ms = stage_clock.elapsed().as_millis() as u64;
    emit_event(
        events,
        run_clock,
        RunEvent {
            stage: stage.to_owned(),
            message: format!("Completed in {:.3} s", duration_ms as f64 / 1_000.0),
            fraction: Some(1.0),
            kind: RunEventKind::StageCompleted,
            severity: RunEventSeverity::Info,
            stage_index: Some(index),
            stage_count: Some(PIPELINE_STAGE_COUNT),
            elapsed_ms: 0,
            duration_ms: Some(duration_ms),
        },
    );
}

/// Start a detailed downstream component timer.
///
/// Component events intentionally do not carry the seven-stage index. They
/// are children of the top-level `downstream` stage and are rendered as a
/// separate, indented timing list by the desktop console.
pub(crate) fn begin_component(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    stage: &str,
    message: &str,
) -> Instant {
    emit_event(
        events,
        run_clock,
        RunEvent {
            stage: stage.to_owned(),
            message: message.to_owned(),
            fraction: Some(0.0),
            kind: RunEventKind::StageStarted,
            severity: RunEventSeverity::Info,
            stage_index: None,
            stage_count: None,
            elapsed_ms: 0,
            duration_ms: None,
        },
    );
    Instant::now()
}

/// Finish a detailed downstream component timer.
pub(crate) fn finish_component(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    stage_clock: Instant,
    stage: &str,
    status: &str,
) {
    let duration_ms = stage_clock.elapsed().as_millis() as u64;
    let message = if status.eq_ignore_ascii_case("skipped") {
        status.to_owned()
    } else {
        format!("{status} in {:.3} s", duration_ms as f64 / 1_000.0)
    };
    emit_event(
        events,
        run_clock,
        RunEvent {
            stage: stage.to_owned(),
            message,
            fraction: Some(1.0),
            kind: RunEventKind::StageCompleted,
            severity: RunEventSeverity::Info,
            stage_index: None,
            stage_count: None,
            elapsed_ms: 0,
            duration_ms: Some(duration_ms),
        },
    );
}

pub(super) fn emit_diagnostic(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    stage: &str,
    message: &str,
) {
    emit_diagnostic_with_severity(events, run_clock, stage, message, RunEventSeverity::Info);
}

pub(super) fn emit_diagnostic_with_severity(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    stage: &str,
    message: &str,
    severity: RunEventSeverity,
) {
    emit_event(
        events,
        run_clock,
        RunEvent {
            stage: stage.to_owned(),
            message: message.to_owned(),
            fraction: None,
            kind: RunEventKind::Diagnostic,
            severity,
            stage_index: None,
            stage_count: Some(PIPELINE_STAGE_COUNT),
            elapsed_ms: 0,
            duration_ms: None,
        },
    );
}

/// Report an optional artifact that could not be written.
///
/// The run continues without it, so the failure must reach the run log and
/// not only the library trace: otherwise a missing file reads the same as a
/// stage that was never requested.
pub(super) fn warn_artifact_failure(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    stage: &str,
    artifact: &str,
    error: &std::io::Error,
) {
    tracing::warn!(%error, artifact, "optional artifact export failed");
    emit_diagnostic_with_severity(
        events,
        run_clock,
        stage,
        &format!("{artifact} export failed: {error}"),
        RunEventSeverity::Warning,
    );
}

// Coordinated analysis inputs are kept explicit at this integration boundary.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_tool_diagnostics(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    environment: &RunEnvironment,
    openvsp: Option<&OpenVspExportResult>,
    vspaero: Option<&VspaeroAnalysisResult>,
    avl: Option<&AvlAnalysisResult>,
    flowunsteady: Option<&FlowUnsteadyAnalysisResult>,
    structures: Option<&StructuralAnalysisResult>,
    mses: Option<&MsesPolarResult>,
) {
    let configured = [
        ("OpenVSP", environment.openvsp_exe.as_deref()),
        ("VSPAERO", environment.vspaero_exe.as_deref()),
        ("AVL", environment.avl_exe.as_deref()),
        ("FLOWUnsteady", environment.flowunsteady_exe.as_deref()),
        ("MSES", environment.mses_dir.as_deref()),
        ("Nastran", environment.nastran_exe.as_deref()),
        ("MSC solver", environment.nastran_solver.as_deref()),
        ("Patran", environment.patran_exe.as_deref()),
    ];
    for (tool, path) in configured {
        let message = path.map_or_else(
            || format!("{tool}: not configured"),
            |path| format!("{tool}: resolved {}", path.display()),
        );
        emit_diagnostic_with_severity(
            events,
            run_clock,
            "external_tools",
            &message,
            if path.is_some() {
                RunEventSeverity::Info
            } else {
                RunEventSeverity::Warning
            },
        );
    }
    for (tool, status) in [
        (
            "OpenVSP",
            openvsp.map(|value| {
                status_with_detail(
                    format!("{:?}", value.status),
                    value.runtime_error.as_deref(),
                )
            }),
        ),
        (
            "VSPAERO",
            vspaero.map(|value| {
                status_with_detail(format!("{:?}", value.status), value.error.as_deref())
            }),
        ),
        (
            "AVL",
            avl.map(|value| {
                status_with_detail(format!("{:?}", value.status), value.error.as_deref())
            }),
        ),
        (
            "FLOWUnsteady",
            flowunsteady.map(|value| {
                status_with_detail(format!("{:?}", value.status), value.error.as_deref())
            }),
        ),
        (
            "Structures",
            structures
                .map(|value| status_with_detail(value.status.clone(), value.error.as_deref())),
        ),
        (
            "MSES",
            mses.map(|value| {
                status_with_detail(format!("{:?}", value.status), value.error.as_deref())
            }),
        ),
    ] {
        emit_diagnostic(
            events,
            run_clock,
            "external_tools",
            &format!(
                "{tool}: {}",
                status.unwrap_or_else(|| "not requested".to_owned())
            ),
        );
    }

    // The overall structural result stays `ok` when the analytical sizing
    // succeeded; the individual native solver outcomes are surfaced as well
    // so a missing MSC DLL is not hidden behind the analytical answer.
    if let Some(structures) = structures {
        for (tool, outcome) in [
            (
                "MSC Nastran SOL 101",
                structures.nastran.as_ref().map(|result| {
                    (
                        result.static_solve.status,
                        result.static_solve.error.as_deref(),
                    )
                }),
            ),
            (
                "MSC Nastran SOL 103",
                structures
                    .nastran
                    .as_ref()
                    .map(|result| (result.modes.status, result.modes.error.as_deref())),
            ),
            (
                "NASTRAN-95 SOL 101",
                structures.nastran95.as_ref().map(|result| {
                    (
                        result.static_solve.status,
                        result.static_solve.error.as_deref(),
                    )
                }),
            ),
            (
                "NASTRAN-95 SOL 103",
                structures
                    .nastran95
                    .as_ref()
                    .map(|result| (result.modes.status, result.modes.error.as_deref())),
            ),
        ] {
            if let Some((status, error)) = outcome {
                emit_diagnostic_with_severity(
                    events,
                    run_clock,
                    "external_tools",
                    &format!(
                        "{tool}: {}",
                        status_with_detail(status.as_str().to_owned(), error)
                    ),
                    if status == alas_struct::nastran::ResultStatus::Ok {
                        RunEventSeverity::Info
                    } else {
                        RunEventSeverity::Warning
                    },
                );
            }
        }
        if let Some(patran) = structures.patran.as_ref() {
            emit_diagnostic_with_severity(
                events,
                run_clock,
                "external_tools",
                &format!(
                    "Patran: {}",
                    status_with_detail(patran.status.clone(), patran.error.as_deref())
                ),
                if patran.status.eq_ignore_ascii_case("ok") {
                    RunEventSeverity::Info
                } else {
                    RunEventSeverity::Warning
                },
            );
        }
    }
}

fn status_with_detail(status: String, error: Option<&str>) -> String {
    let Some(error) = error.map(str::trim).filter(|error| !error.is_empty()) else {
        return status;
    };
    const MAX_CHARS: usize = 1_000;
    let detail = error.chars().take(MAX_CHARS).collect::<String>();
    if detail.chars().count() < error.chars().count() {
        format!("{status}: {detail} \u{2026}")
    } else {
        format!("{status}: {detail}")
    }
}
