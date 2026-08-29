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
        let output = Command::new(executable)
            .args([
                "--silent",
                "--show-error",
                "--fail",
                "--location",
                "--proto",
                "=https",
                "--max-time",
                &timeout,
                url,
            ])
            .no_window()
            .output()
            .map_err(|error| format!("cannot launch {executable}: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "SimBrief request failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        String::from_utf8(output.stdout)
            .map_err(|error| format!("SimBrief returned non-UTF-8 data: {error}"))
    }
}
