// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Subprocess helpers for driving external console tools from a windowed app.
//!
//! ALAS shells out to compiled console executables it does not reimplement --
//! MSES's `mset`/`mses`/`mplot`, VSPAERO, and Nastran -- to run analyses the
//! native code leaves to those tools. On Windows a GUI process has no console of its
//! own, so every such spawn would otherwise allocate a *new* console window
//! that flashes on screen, steals focus, and costs real time to create. At the
//! volumes an airfoil sweep reaches -- several solver invocations per
//! candidate, across dozens of candidates -- that is a storm of popping
//! terminals and a real slowdown, not a cosmetic detail.
//!
//! This crate is the one place every external-tool spawn in the workspace is
//! configured, so the windowless flag is applied here rather than remembered at
//! each call site.
//!
//! [`vspaero`] executes an extensionless native case directly, captures both
//! output streams, rejects stale or missing polar output, and publishes a
//! causal process status for product-level orchestration.

pub mod avl;
pub mod download;
pub mod flowunsteady;
pub mod openfoam;
pub mod process;
pub mod tools;
pub mod vspaero;

pub use tools::{ExecutableDiscovery, RunEnvironment, ToolLocator, ToolPreferences};
