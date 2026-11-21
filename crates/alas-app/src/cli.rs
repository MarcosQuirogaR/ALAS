// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Command-line argument parsing, desktop GUI invocation, and headless execution dispatch.

// The CLI binary and orchestration driver legitimately prints output and error summaries to stdout and stderr.
#![allow(clippy::print_stdout, clippy::print_stderr)]

mod args;
mod execute;
mod plots;
mod run;
mod summaries;
#[cfg(test)]
mod tests;
mod tools_install;

pub use args::{load_config, parse_args, CliArgs};
pub use run::run_cli;

pub(crate) use args::{apply_cli_tool_preferences, read_config_document};
pub(crate) use execute::{execute_run, RunOutcome};
