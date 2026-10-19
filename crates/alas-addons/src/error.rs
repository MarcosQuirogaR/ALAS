// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Errors reported by the add-on subsystem.

use thiserror::Error;

/// Everything that can go wrong loading, running or applying an add-on.
#[derive(Debug, Error)]
pub enum AddonError {
    /// `addon.toml` is malformed or violates the manifest rules.
    #[error("invalid manifest: {0}")]
    Manifest(String),
    /// A file could not be read or written.
    #[error("{0}")]
    Io(String),
    /// The supplied inputs do not match the manifest.
    #[error("invalid inputs: {0}")]
    Inputs(String),
    /// The script failed to compile or raised an error.
    #[error("script error: {0}")]
    Script(String),
    /// The script ran past its wall-clock limit.
    #[error("timed out after {0:.1} s")]
    Timeout(f64),
    /// The script exceeded its operation budget.
    #[error("operation limit exceeded ({0} operations)")]
    OperationLimit(u64),
    /// The child process could not be started or failed.
    #[error("process error: {0}")]
    Process(String),
    /// The add-on returned something that is not a valid output set.
    #[error("invalid outputs: {0}")]
    Outputs(String),
    /// A requested change was refused.
    #[error("{0}")]
    Apply(String),
}
