// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/sidecar/runs.py
// Reference: alas @ rust-port-baseline.

//! Pipeline run state tracking, bounded registry, and result summarization.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

/// Machine-readable lifecycle classification for a pipeline event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunEventKind {
    /// A stage has begun.
    StageStarted,
    /// A stage completed successfully.
    StageCompleted,
    /// Intermediate progress within a stage.
    #[default]
    Progress,
    /// A diagnostic about configuration, tools, or artifacts.
    Diagnostic,
}

/// Severity carried independently from the event's display text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunEventSeverity {
    /// Normal execution information.
    #[default]
    Info,
    /// A recoverable condition or unavailable optional capability.
    Warning,
    /// A terminal failure.
    Error,
}

/// Progress and status event emitted during a pipeline execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunEvent {
    /// Stage identifier (e.g., "baseline", "optimization", "full_analysis", "mission").
    pub stage: String,
    /// Human-readable message or progress update.
    pub message: String,
    /// Percentage progress (0.0 to 1.0), if applicable.
    pub fraction: Option<f64>,
    /// Typed lifecycle classification.
    #[serde(default)]
    pub kind: RunEventKind,
    /// Typed severity classification.
    #[serde(default)]
    pub severity: RunEventSeverity,
    /// One-based top-level stage number, when this is a stage event.
    #[serde(default)]
    pub stage_index: Option<u8>,
    /// Number of top-level stages in this execution contract.
    #[serde(default)]
    pub stage_count: Option<u8>,
    /// Milliseconds since pipeline execution began.
    #[serde(default)]
    pub elapsed_ms: u64,
    /// Completed stage duration in milliseconds.
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

/// Lifecycle state and captured results of a single pipeline execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunState {
    /// Unique identifier for this run.
    pub id: String,
    /// Current execution status (`"pending"`, `"running"`, `"done"`, `"error"`, `"cancelled"`).
    pub status: String,
    /// Error message if status is `"error"`.
    pub error: Option<String>,
    /// Accumulated progress events.
    pub events: Vec<RunEvent>,
    /// Final JSON summary of the run outcome.
    pub summary: Option<serde_json::Value>,
}

impl RunState {
    /// Create a new pending run.
    pub fn new(id: String) -> Self {
        Self {
            id,
            status: "pending".to_owned(),
            error: None,
            events: Vec::new(),
            summary: None,
        }
    }
}

/// Thread-safe in-memory store for recent pipeline runs.
#[derive(Debug, Clone)]
pub struct RunRegistry {
    runs: Arc<Mutex<HashMap<String, RunState>>>,
    order: Arc<Mutex<Vec<String>>>,
    max_capacity: usize,
}

impl Default for RunRegistry {
    fn default() -> Self {
        Self::new(50)
    }
}

impl RunRegistry {
    /// Create a registry that retains up to `max_capacity` runs.
    pub fn new(max_capacity: usize) -> Self {
        Self {
            runs: Arc::new(Mutex::new(HashMap::new())),
            order: Arc::new(Mutex::new(Vec::new())),
            max_capacity,
        }
    }

    /// Register a new run ID.
    pub fn register(&self, id: String) {
        let mut runs = match self.runs.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let mut order = match self.order.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };

        if !runs.contains_key(&id) {
            order.push(id.clone());
            runs.insert(id.clone(), RunState::new(id));
            if order.len() > self.max_capacity {
                let oldest = order.remove(0);
                runs.remove(&oldest);
            }
        }
    }

    /// Update status of a run.
    pub fn set_status(&self, id: &str, status: &str, error: Option<String>) {
        let mut runs = match self.runs.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(run) = runs.get_mut(id) {
            run.status = status.to_owned();
            run.error = error;
        }
    }

    /// Add an event to a run's event log.
    pub fn add_event(&self, id: &str, event: RunEvent) {
        let mut runs = match self.runs.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(run) = runs.get_mut(id) {
            run.events.push(event);
        }
    }

    /// Set the final summary result of a run and mark it done.
    pub fn complete(&self, id: &str, summary: serde_json::Value) {
        let mut runs = match self.runs.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(run) = runs.get_mut(id) {
            run.status = "done".to_owned();
            run.summary = Some(summary);
        }
    }

    /// Retrieve a snapshot of a run.
    pub fn get(&self, id: &str) -> Option<RunState> {
        let runs = match self.runs.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        runs.get(id).cloned()
    }

    /// List all runs in registration order.
    pub fn list(&self) -> Vec<RunState> {
        let runs = match self.runs.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let order = match self.order.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        order
            .iter()
            .filter_map(|id| runs.get(id).cloned())
            .collect()
    }
}
