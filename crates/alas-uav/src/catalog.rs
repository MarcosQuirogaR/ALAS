// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Normalized, source-provenanced UAV component records.
//!
//! Retail pages mix physical specifications with volatile commercial state.
//! Only physical fields enter this schema; prices and stock are deliberately
//! absent because neither changes whether an aircraft can fly. Optional
//! fields are intentional. A missing motor current or servo current remains
//! `None`, allowing the feasibility layer to report an unverified constraint
//! instead of substituting a plausible value.

include!("catalog_parts/part_01.rs");
include!("catalog_parts/part_02.rs");
