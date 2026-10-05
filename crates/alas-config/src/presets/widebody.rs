// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/presets.py (the twin-aisle entries)

//! Twin-aisle aircraft presets, grouped by manufacturer.

mod airbus;
mod american;
mod b747;

pub use airbus::{a340_300, a380_800};
pub use american::{b787_9, dc_10};
pub use b747::b747_400;
