// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Coupled calculations behind the UAV production-verdict boundary.
//!
//! Positive pitching moment is nose-up and positive lift coefficient is
//! upward, so longitudinal static stability requires `dCm/dCL < 0`: an
//! increase in lift must create a restoring nose-down moment. The production
//! geometry has no elevator primitive. Pitch trim is therefore claimed only
//! for an explicitly evidenced trimmable-horizontal-tail incidence range.

include!("production_verdict_impl_parts/part_01.rs");
include!("production_verdict_impl_parts/part_02.rs");
