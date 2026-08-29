// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless command line entry point and path resolution for ALAS.
//!
//! [`paths`] ports `alas/paths.py`: cross-platform path resolution for application
//! installation roots, user data directories, and external tools.
//!
//! [`cli`] ports `alas/cli.py`: command-line argument parsing and headless workflow dispatch.

pub mod cli;
pub mod paths;

pub use cli::{load_config, parse_args, run_cli, CliArgs};
pub use paths::{
    app_root, bundle_root, candidate_roots, data_roots, find_tool_dir, is_frozen,
    resolve_data_path, resolve_tool_dir, resolve_tool_exe, user_data_root, APP_DIR_ENV,
};
