// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Evidence-aware production verdict for generated fixed-wing UAVs.
//!
//! The implementation is split only to keep each source file within the
//! repository's review-size limit; this module is the stable public boundary.

#[path = "production_verdict_impl.rs"]
mod implementation;

pub use implementation::*;
