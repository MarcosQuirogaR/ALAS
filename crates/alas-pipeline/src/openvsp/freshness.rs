// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Whether a native OpenVSP output file is both freshly written by the run
//! that just finished and structurally the format it claims to be.

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

pub(super) fn is_native_vsp3(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    bytes.len() > 100
        && String::from_utf8_lossy(&bytes[..bytes.len().min(8192)]).contains("<Vsp_Geometry>")
}

pub(super) fn is_fresh_native_vsp3(path: &Path, run_started: SystemTime) -> bool {
    is_fresh_file(path, run_started) && is_native_vsp3(path)
}

pub(super) fn is_native_vspgeom(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let header = String::from_utf8_lossy(&bytes[..bytes.len().min(64)]);
    bytes.len() > 32
        && header
            .lines()
            .next()
            .is_some_and(|line| line.trim() == "# vspgeom v3")
}

pub(super) fn is_fresh_native_vspgeom(path: &Path, run_started: SystemTime) -> bool {
    is_fresh_file(path, run_started) && is_native_vspgeom(path)
}

pub(super) fn is_fresh_native_png(path: &Path, run_started: SystemTime) -> bool {
    if !is_fresh_file(path, run_started) {
        return false;
    }
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    if bytes.len() < 33 || bytes[..8] != *b"\x89PNG\r\n\x1a\n" {
        return false;
    }
    let mut offset: usize = 8;
    let mut saw_ihdr = false;
    while offset.checked_add(12).is_some_and(|end| end <= bytes.len()) {
        let chunk_length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let data_start = offset + 8;
        let Some(data_end) = data_start.checked_add(chunk_length) else {
            return false;
        };
        let Some(chunk_end) = data_end.checked_add(4) else {
            return false;
        };
        if chunk_end > bytes.len() {
            return false;
        }
        let chunk_type = &bytes[offset + 4..offset + 8];
        if !saw_ihdr {
            if chunk_type != b"IHDR" || chunk_length != 13 {
                return false;
            }
            let width = u32::from_be_bytes([
                bytes[data_start],
                bytes[data_start + 1],
                bytes[data_start + 2],
                bytes[data_start + 3],
            ]);
            let height = u32::from_be_bytes([
                bytes[data_start + 4],
                bytes[data_start + 5],
                bytes[data_start + 6],
                bytes[data_start + 7],
            ]);
            if width == 0 || height == 0 {
                return false;
            }
            saw_ihdr = true;
        }
        if chunk_type == b"IEND" {
            return saw_ihdr && chunk_length == 0 && chunk_end == bytes.len();
        }
        offset = chunk_end;
    }
    false
}

pub(super) fn is_fresh_file(path: &Path, run_started: SystemTime) -> bool {
    let Ok(modified) = fs::metadata(path).and_then(|metadata| metadata.modified()) else {
        return false;
    };
    let earliest_allowed = run_started
        .checked_sub(Duration::from_secs(2))
        .unwrap_or(run_started);
    modified >= earliest_allowed
}

pub(super) fn preview_failure_reason(stdout: &str, path: &Path) -> String {
    if stdout.contains("ALAS_OPENVSP_PREVIEW_UNAVAILABLE") {
        return format!(
            "OpenVSP completed the native project, but its runtime has no graphics-capable GUI build; no CAD preview was written to {}",
            path.display()
        );
    }
    format!(
        "OpenVSP completed the native project, but no fresh valid PNG preview was written to {}",
        path.display()
    )
}

pub(super) fn text_tail(text: &str) -> String {
    text.lines()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" | ")
}
