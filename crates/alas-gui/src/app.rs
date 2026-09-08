// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Main `eframe::App` desktop application shell.
//!
//! Reproduces the reference desktop app's layout: a File/View/Help menu bar,
//! a left navigation tree, an optional right-hand 3D preview dock, a run log
//! and control bar along the bottom, and a central content pane that routes
//! to the active [`crate::nav::Page`].

include!("app_parts/part_01.rs");
include!("app_parts/part_02.rs");
