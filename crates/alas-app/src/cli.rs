// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/cli.py
//! Command-line argument parsing, desktop GUI invocation, and headless execution dispatch.

// The CLI binary and orchestration driver legitimately prints output and error summaries to stdout and stderr.
#![allow(clippy::print_stdout, clippy::print_stderr)]

include!("cli_parts/part_01.rs");
include!("cli_parts/part_02.rs");
