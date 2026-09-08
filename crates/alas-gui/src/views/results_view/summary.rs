// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Layout-derived aircraft metrics shown above the result figures.

mod findings;
use findings::{
    actual_label, affected_disciplines, finding_margin, finding_meaning, finding_next_step,
    finding_title, limit_label,
};

include!("summary_parts/part_01.rs");
include!("summary_parts/part_02.rs");
include!("summary_parts/part_03.rs");
