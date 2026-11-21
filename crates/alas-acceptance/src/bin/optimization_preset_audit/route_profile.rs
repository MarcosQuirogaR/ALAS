// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Route-profile initialization shared by optimizer acceptance experiments.

use alas_config::AlasConfig;

/// The route-derived profile every front end loads a preset with
/// (`alas_mission::preset_load`). Aircraft-specific speeds and rates remain
/// unchanged.
pub(crate) fn initialize_route_profile(config: &mut AlasConfig) {
    alas_mission::preset_load::apply_route_profile(config);
}
