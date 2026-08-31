// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! External-tool discovery and the typed environment passed to a run.
//!
//! The executable, GUI and pipeline used to each make a different decision
//! about where an optional solver lived. Keeping discovery here means a
//! packaged executable and a development checkout resolve the same way, and a
//! run receives one named value rather than a list of unrelated `Option`s.

include!("tools_parts/part_01.rs");
include!("tools_parts/part_02.rs");
include!("tools_parts/part_03.rs");
