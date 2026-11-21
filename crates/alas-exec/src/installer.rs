// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One-click installer for the external tools ALAS can drive.
//!
//! The installer is policy, not magic. A bundled manifest
//! (`data/tool-manifest.json`) lists every external tool and says whether ALAS
//! may fetch it (`automatic`), whether it still waits for a maintainer to pin a
//! reviewed archive (`pending_review`) or whether it is licence-gated or needs
//! administrator rights (`guided`, steps only). Automatic entries carry a URL
//! and a SHA-256 per platform; an entry without a pinned hash is never
//! downloaded, so the installer fails closed.
//!
//! The flow for one tool is download (resumable, curl) -> verify SHA-256 ->
//! list and extract with the platform `tar` into a staging directory -> check
//! the tree for escapes -> move into `<user data>/external tools/<directory>` ->
//! health check (run the tool on a tiny case) -> record in
//! `installed-tools.json`. Any failure removes the new directory and restores
//! the previous one. The existing [`crate::ToolLocator`] already searches
//! `<user data>/external tools`, so a tool installed here is discovered without
//! editing any path.
//!
//! Nothing downloads without a [`ConsentReceipt`], which can only be built from
//! the exact [`InstallPlan`] the user was shown.

mod build;
mod engine;
mod extract;
mod health;
mod identity;
mod manifest;
mod patch;
mod plan;
mod registry;
mod transfer;

#[cfg(test)]
// The tests build fixtures and assert on them; a failed setup call is a test-authoring error.
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests;

pub use build::{SelfCheck, SelfChecks};
pub use engine::{
    run_install, uninstall_tool, InstallContext, ProgressEvent, ToolReport, ToolResult,
};
pub use health::{run_health_check, HealthReport};
pub use identity::toolchain_leftover_note;
pub use manifest::{
    current_platform, ArchiveFormat, Artifact, HealthSpec, InstallKind, Manifest, ToolEntry,
};
pub use plan::{install_plan, ConsentReceipt, InstallPlan, PlanDownload, PlanItem, PlanItemState};
pub use registry::{InstalledRecord, Registry};
pub use transfer::TransferOptions;

/// Why an install step failed. The message is user-facing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InstallError {
    /// The manifest is unreadable or inconsistent.
    #[error("tool manifest error: {0}")]
    Manifest(String),
    /// A tool has no pinned archive for this platform.
    #[error("{0}")]
    Unavailable(String),
    /// Network transfer failed.
    #[error("download failed: {0}")]
    Download(String),
    /// The downloaded bytes do not match the pinned SHA-256.
    #[error(
        "SHA-256 mismatch for {name}: expected {expected}, got {actual}; the file was discarded"
    )]
    HashMismatch {
        /// Tool display name.
        name: String,
        /// Pinned hash.
        expected: String,
        /// Computed hash.
        actual: String,
    },
    /// The archive could not be listed or extracted safely.
    #[error("archive rejected: {0}")]
    Archive(String),
    /// Filesystem failure.
    #[error("filesystem error: {0}")]
    Io(String),
    /// A source build step failed.
    #[error("build failed: {0}")]
    Build(String),
    /// The installed tool failed its tiny-case run.
    #[error("health check failed: {0}")]
    Health(String),
    /// The user asked to stop.
    #[error("cancelled")]
    Cancelled,
}
