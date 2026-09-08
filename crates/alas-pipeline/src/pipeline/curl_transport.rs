// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! HTTPS transport used by the route-planning boundary.

use std::process::Command;

use alas_exec::process::NoConsoleWindow;
use alas_route::SimbriefTransport;

/// HTTPS transport supplied by the operating system's curl installation.
pub(super) struct SystemCurlTransport;

impl SimbriefTransport for SystemCurlTransport {
    fn fetch(&self, url: &str, timeout_s: f64) -> Result<String, String> {
        let executable = if cfg!(windows) { "curl.exe" } else { "curl" };
        let timeout = timeout_s.clamp(1.0, 120.0).to_string();
        let connect_timeout = timeout_s.clamp(1.0, 10.0).to_string();
        let output = Command::new(executable)
            .args([
                "--silent",
                "--show-error",
                // Keep an HTTP error body: SimBrief uses it for actionable
                // account/plan diagnostics (for example, no plan on file).
                "--fail-with-body",
                "--location",
                "--proto",
                "=https",
                "--connect-timeout",
                &connect_timeout,
                "--max-time",
                &timeout,
                // A short retry budget covers transient DNS/TLS/5xx failures
                // without turning an optional route tier into a long stall.
                "--retry",
                "2",
                "--retry-delay",
                "1",
                "--retry-max-time",
                &timeout,
                "--user-agent",
                "ALAS/1.0 (SimBrief route import)",
                url,
            ])
            .no_window()
            .output()
            .map_err(|error| format!("cannot launch {executable}: {error}"))?;
        if !output.status.success() {
            let body = bounded_diagnostic(&String::from_utf8_lossy(&output.stdout));
            let stderr = bounded_diagnostic(&String::from_utf8_lossy(&output.stderr));
            let detail = match (body.is_empty(), stderr.is_empty()) {
                (true, true) => "no response body or stderr".to_owned(),
                (false, true) => format!("response: {body}"),
                (true, false) => format!("curl: {stderr}"),
                (false, false) => format!("response: {body}; curl: {stderr}"),
            };
            return Err(format!(
                "SimBrief request failed ({}): {detail}",
                output.status
            ));
        }
        String::from_utf8(output.stdout)
            .map_err(|error| format!("SimBrief returned non-UTF-8 data: {error}"))
    }
}

fn bounded_diagnostic(text: &str) -> String {
    const MAX_CHARS: usize = 2_000;
    let text = text.trim();
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut result = String::with_capacity(text.len().min(MAX_CHARS));
    result.push(first);
    result.extend(chars.take(MAX_CHARS - 1));
    if result.chars().count() < text.chars().count() {
        result.push_str(" \u{2026}");
    }
    result.replace(['\r', '\n'], " ")
}
