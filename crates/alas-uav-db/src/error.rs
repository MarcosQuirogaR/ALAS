// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The error type shared by validators, importers and the converter.

/// A database, import or conversion failure with a human-readable cause.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct DbError {
    message: String,
}

impl DbError {
    /// Build an error from any message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The failure description.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Result alias used across the crate.
pub type DbResult<T> = Result<T, DbError>;
