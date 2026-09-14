// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

pub(crate) fn execute_solver_stage<F>(
    adapter: &OpenFoamAdapter,
    case_dir: &Path,
    timeout_seconds: u64,
    cancel: &Arc<AtomicBool>,
    emit: &mut F,
    started: std::time::Instant,
    command_logs: &mut BTreeMap<String, String>,
    mesh_output: &mut String,
    stage: CfdStage,
    label: &str,
) -> Result<OpenFoamProcessStatus, String>
where
    F: FnMut(CfdRunEvent),
{
    let command = adapter.command("simpleFoam", Some(case_dir), &[])?;
    execute_resolved_stage(
        adapter,
        case_dir,
        timeout_seconds,
        cancel,
        emit,
        started,
        command_logs,
        mesh_output,
        stage,
        label,
        command,
    )
}

pub(crate) fn execute_solver_postprocess_stage<F>(
    adapter: &OpenFoamAdapter,
    case_dir: &Path,
    timeout_seconds: u64,
    cancel: &Arc<AtomicBool>,
    emit: &mut F,
    started: std::time::Instant,
    command_logs: &mut BTreeMap<String, String>,
    mesh_output: &mut String,
) -> Result<OpenFoamProcessStatus, String>
where
    F: FnMut(CfdRunEvent),
{
    let args: Vec<std::ffi::OsString> = ["-postProcess", "-latestTime"]
        .into_iter()
        .map(std::ffi::OsString::from)
        .collect();
    let command = adapter.command("simpleFoam", Some(case_dir), &args)?;
    execute_resolved_stage(
        adapter,
        case_dir,
        timeout_seconds,
        cancel,
        emit,
        started,
        command_logs,
        mesh_output,
        CfdStage::PostProcessing,
        "simpleFoam-postProcess",
        command,
    )
}

pub(crate) fn rewrite_solver_control_dict(
    case_dir: &Path,
    end_time: u32,
    start_from_latest: bool,
    write_interval: Option<u32>,
) -> Result<(), String> {
    let path = case_dir.join("system/controlDict");
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {} for solver stage: {error}", path.display()))?;
    let mut rewritten = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("startFrom ") {
            rewritten.push_str(if start_from_latest {
                "startFrom latestTime;"
            } else {
                "startFrom startTime;"
            });
        } else if trimmed.starts_with("endTime ") {
            rewritten.push_str(&format!("endTime {end_time};"));
        } else if write_interval.is_some() && line.starts_with("writeInterval ") {
            // Only the top-level case write interval controls field writes.
            // Function objects intentionally keep their own sampling cadence
            // (forceCoeffs/forces use every iteration); matching on the
            // trimmed text would rewrite those nested dictionaries as well.
            rewritten.push_str(&format!(
                "writeInterval {};",
                write_interval.unwrap_or_default().max(1)
            ));
        } else {
            rewritten.push_str(line);
        }
        rewritten.push('\n');
    }
    fs::write(&path, rewritten)
        .map_err(|error| format!("cannot write {} for solver stage: {error}", path.display()))
}

pub(crate) fn final_write_interval(max_iterations: u32, configured: u32) -> u32 {
    let configured = configured.max(1);
    let max_iterations = max_iterations.max(1);
    if max_iterations % configured == 0 {
        configured
    } else {
        max_iterations
    }
}

pub(crate) fn execute_stage<F>(
    adapter: &OpenFoamAdapter,
    case_dir: &Path,
    timeout_seconds: u64,
    cancel: &Arc<AtomicBool>,
    emit: &mut F,
    started: std::time::Instant,
    command_logs: &mut BTreeMap<String, String>,
    mesh_output: &mut String,
    stage: CfdStage,
    tool: &str,
    args: Vec<std::ffi::OsString>,
) -> Result<OpenFoamProcessStatus, String>
where
    F: FnMut(CfdRunEvent),
{
    let command = adapter.command(tool, Some(case_dir), &args)?;
    execute_resolved_stage(
        adapter,
        case_dir,
        timeout_seconds,
        cancel,
        emit,
        started,
        command_logs,
        mesh_output,
        stage,
        tool,
        command,
    )
}

pub(crate) fn execute_gmsh_stage<F>(
    adapter: &OpenFoamAdapter,
    case_dir: &Path,
    timeout_seconds: u64,
    cancel: &Arc<AtomicBool>,
    emit: &mut F,
    started: std::time::Instant,
    command_logs: &mut BTreeMap<String, String>,
    mesh_output: &mut String,
    stage: CfdStage,
    args: Vec<std::ffi::OsString>,
) -> Result<OpenFoamProcessStatus, String>
where
    F: FnMut(CfdRunEvent),
{
    let command = adapter.gmsh_command(case_dir, &args)?;
    execute_resolved_stage(
        adapter,
        case_dir,
        timeout_seconds,
        cancel,
        emit,
        started,
        command_logs,
        mesh_output,
        stage,
        "gmsh",
        command,
    )
}

pub(crate) fn execute_resolved_stage<F>(
    adapter: &OpenFoamAdapter,
    case_dir: &Path,
    timeout_seconds: u64,
    cancel: &Arc<AtomicBool>,
    emit: &mut F,
    started: std::time::Instant,
    command_logs: &mut BTreeMap<String, String>,
    mesh_output: &mut String,
    stage: CfdStage,
    tool: &str,
    command: alas_exec::openfoam::OpenFoamCommand,
) -> Result<OpenFoamProcessStatus, String>
where
    F: FnMut(CfdRunEvent),
{
    if cancel.load(Ordering::Relaxed) {
        return Ok(OpenFoamProcessStatus::Cancelled);
    }
    emit_event(
        emit,
        started,
        stage,
        CfdEventSeverity::Info,
        format!("Running {tool}..."),
    );
    command_logs.insert(
        format!("__executable_path:{tool}"),
        command.program.to_string_lossy().into_owned(),
    );
    let mut partial_line = String::new();
    let result = adapter.run_with_callback(
        &command,
        cancel,
        Duration::from_secs(timeout_seconds.clamp(1, 86_400)),
        |stream, chunk| {
            partial_line.push_str(&chunk);
            while let Some(newline) = partial_line.find('\n') {
                let line = partial_line[..newline].trim_end_matches('\r').to_owned();
                partial_line.drain(..=newline);
                if is_live_diagnostic(&line) {
                    let severity = if stream == alas_exec::openfoam::OpenFoamOutputStream::Stderr {
                        CfdEventSeverity::Warning
                    } else {
                        CfdEventSeverity::Info
                    };
                    let message = truncate_live_line(&format!("{tool}: {line}"));
                    emit_event(emit, started, stage, severity, message);
                }
            }
        },
    );
    if !partial_line.trim().is_empty() && is_live_diagnostic(partial_line.trim()) {
        emit_event(
            emit,
            started,
            stage,
            CfdEventSeverity::Info,
            truncate_live_line(&format!("{tool}: {}", partial_line.trim())),
        );
    }
    let combined = format!("{}\n{}", result.stdout, result.stderr);
    let log_key = unique_log_key(command_logs, tool);
    command_logs.insert(log_key.clone(), combined.clone());
    let log_dir = case_dir.join("logs");
    fs::create_dir_all(&log_dir)
        .map_err(|error| format!("cannot create logs directory: {error}"))?;
    let log_name = if log_dir.join(format!("{tool}.log")).exists() {
        let mut index = 2_u32;
        loop {
            let candidate = log_dir.join(format!("{tool}-{index}.log"));
            if !candidate.exists() {
                break candidate;
            }
            index = index.saturating_add(1);
        }
    } else {
        log_dir.join(format!("{tool}.log"))
    };
    fs::write(log_name, &combined).map_err(|error| format!("cannot write {tool} log: {error}"))?;
    if stage == CfdStage::QualityGate {
        *mesh_output = combined.clone();
    }
    let severity = match result.status {
        OpenFoamProcessStatus::Completed => CfdEventSeverity::Info,
        OpenFoamProcessStatus::Cancelled => CfdEventSeverity::Warning,
        _ => CfdEventSeverity::Error,
    };
    emit_event(
        emit,
        started,
        stage,
        severity,
        process_summary(tool, &result),
    );
    if result.status != OpenFoamProcessStatus::Completed {
        command_logs.insert(
            "__failure".to_owned(),
            format!("{tool}: {}", process_failure_detail(&result, &combined)),
        );
    }
    Ok(result.status)
}

pub(crate) fn unique_log_key(logs: &BTreeMap<String, String>, requested: &str) -> String {
    if !logs.contains_key(requested) {
        return requested.to_owned();
    }
    let mut index = 2_u32;
    loop {
        let candidate = format!("{requested}-{index}");
        if !logs.contains_key(&candidate) {
            return candidate;
        }
        index = index.saturating_add(1);
    }
}

pub(crate) fn process_failure_detail(
    result: &alas_exec::openfoam::OpenFoamProcessResult,
    combined: &str,
) -> String {
    let mut lines = combined
        .lines()
        .filter(|line| !line.trim().is_empty())
        .rev()
        .take(6)
        .collect::<Vec<_>>();
    lines.reverse();
    let tail = lines.join(" ");
    let tail = if tail.chars().count() > 800 {
        tail.chars().skip(tail.chars().count() - 800).collect()
    } else {
        tail
    };
    format!(
        "status={:?}, exit={:?}; {}",
        result.status,
        result.exit_code,
        if tail.is_empty() {
            "no utility output was captured".to_owned()
        } else {
            tail
        }
    )
}

pub(crate) fn emit_event<F>(
    emit: &mut F,
    started: std::time::Instant,
    stage: CfdStage,
    severity: CfdEventSeverity,
    message: String,
) where
    F: FnMut(CfdRunEvent),
{
    emit(CfdRunEvent {
        stage,
        severity,
        message,
        elapsed_seconds: started.elapsed().as_secs_f64(),
    });
}

pub(crate) fn process_summary(
    tool: &str,
    result: &alas_exec::openfoam::OpenFoamProcessResult,
) -> String {
    let detail = if result.stderr.trim().is_empty() {
        result.stdout.lines().last().unwrap_or("")
    } else {
        result.stderr.lines().last().unwrap_or("")
    };
    format!(
        "{tool}: {:?}, exit={:?}, elapsed={:.2}s{}",
        result.status,
        result.exit_code,
        result.elapsed_seconds,
        if detail.trim().is_empty() {
            String::new()
        } else {
            format!(", last output: {}", detail.trim())
        }
    )
}

fn is_live_diagnostic(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("time =")
        || lower.contains("solving for ")
        || lower.contains("continuity")
        || lower.contains("executiontime")
        || lower.contains("clocktime")
        || lower.contains("forcecoeff")
        || lower.contains("cd =")
        || lower.contains("cl =")
}

fn truncate_live_line(line: &str) -> String {
    const MAX_CHARS: usize = 600;
    if line.chars().count() <= MAX_CHARS {
        return line.to_owned();
    }
    let mut truncated = line.chars().take(MAX_CHARS - 3).collect::<String>();
    truncated.push_str("...");
    truncated
}
