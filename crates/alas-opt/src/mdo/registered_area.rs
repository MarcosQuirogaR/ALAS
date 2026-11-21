// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wing-area window of a redesigned registered aircraft.
//!
//! A reference adaptation (and the baseline sandbox) redesigns a registered
//! aircraft, so its wing reference area is held within a fixed fraction of
//! the registered wing's own: the registered preset design vector built on
//! the configuration's geometry, the same builder and the same reference
//! wing convention as the candidate, so the comparison carries no modelling
//! bias. The window replaces `requirements.max_wing_area_m2` in those modes;
//! a clean-sheet design keeps the declared maximum.

use alas_config::{AlasConfig, DesignMode};

/// Half-width of the window, a fraction of the registered area.
pub const REGISTERED_WING_AREA_FRACTION: f64 = 0.05;

/// The registered wing's area for a configuration whose mode redesigns a
/// registered aircraft, keyed by the complete configuration.
static REGISTERED_AREA: super::nominal_cache::NominalCache<RegisteredWingArea> =
    super::nominal_cache::NominalCache::new();

/// The registered wing's reference areas, m^2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegisteredWingArea {
    /// The built aircraft's `s_ref`, which includes the projection of any
    /// non-planar tip device.
    pub s_ref_m2: f64,
    /// The main wing's projected planform reference area.
    pub projected_m2: f64,
}

impl RegisteredWingArea {
    /// `(lower, upper)` bounds on the candidate's `s_ref`, m^2.
    #[must_use]
    pub fn s_ref_window_m2(&self) -> (f64, f64) {
        window(self.s_ref_m2)
    }

    /// `(lower, upper)` bounds on the candidate's projected main-wing area,
    /// m^2.
    #[must_use]
    pub fn projected_window_m2(&self) -> (f64, f64) {
        window(self.projected_m2)
    }
}

fn window(area_m2: f64) -> (f64, f64) {
    (
        area_m2 * (1.0 - REGISTERED_WING_AREA_FRACTION),
        area_m2 * (1.0 + REGISTERED_WING_AREA_FRACTION),
    )
}

/// The registered wing's areas when `config` redesigns a registered aircraft
/// (`reference_adaptation` or `baseline_sandbox`), `None` in a clean sheet or
/// when the preset or its geometry cannot be resolved.
#[must_use]
pub fn registered_wing_area(config: &AlasConfig) -> Option<RegisteredWingArea> {
    if !matches!(
        config.optimizer.design_space.mode,
        DesignMode::ReferenceAdaptation | DesignMode::BaselineSandbox
    ) {
        return None;
    }
    REGISTERED_AREA.get_or_resolve(config, |config| {
        let design = alas_config::presets::get(&config.preset)
            .ok()?
            .design_vector;
        let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .ok()?;
        let projected_m2 = plane
            .wings
            .first()
            .map(alas_geom::aircraft::wing::Wing::reference_area)?;
        let area = RegisteredWingArea {
            s_ref_m2: plane.s_ref,
            projected_m2,
        };
        (area.s_ref_m2.is_finite()
            && area.s_ref_m2 > 0.0
            && area.projected_m2.is_finite()
            && area.projected_m2 > 0.0)
            .then_some(area)
    })
}

/// The upper bound on the wing's reference area `s_ref` the residual table
/// enforces for `config`, m^2: the registered window's upper edge when
/// `config` redesigns a registered aircraft, the declared
/// `requirements.max_wing_area_m2` otherwise. Search-side gates and
/// projections read it so the window is reachable where it lies above the
/// declared maximum.
#[must_use]
pub fn wing_area_upper_m2(config: &AlasConfig) -> f64 {
    registered_wing_area(config).map_or(config.requirements.max_wing_area_m2, |area| {
        area.s_ref_window_m2().1
    })
}
