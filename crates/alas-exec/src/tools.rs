// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! External-tool discovery and the typed environment passed to a run.
//!
//! The executable, GUI and pipeline must make the same decision
//! about where an optional solver lives. Keeping discovery here means a
//! packaged executable and a development checkout resolve the same way, and a
//! run receives one named value rather than a list of unrelated `Option`s.

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Whether configured and automatic external-tool resolution is enabled.
///
/// `ALAS_TOOL_DISCOVERY=disabled` isolates a run from installed tools.
/// Direct calls to explicit tool runners remain available.
pub fn tool_discovery_enabled() -> bool {
    let mode = env::var("ALAS_TOOL_DISCOVERY").ok();
    discovery_mode_enabled(mode.as_deref())
}

fn discovery_mode_enabled(mode: Option<&str>) -> bool {
    !mode.is_some_and(|mode| mode.eq_ignore_ascii_case("disabled"))
}

mod types;
pub use types::*;
mod locator;
mod nastran;
mod platform;
use nastran::*;
use platform::*;
#[cfg(test)]
mod external_tests;
