// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parametric free-turbine cycle preview.
use crate::scene::Scene;
use alas_config::AlasConfig;

pub(super) fn figure_turboprop_cycle_preview(config: &AlasConfig, theme: Option<&str>) -> Scene {
    super::ts_preview::turboprop_preview(config, theme)
}
