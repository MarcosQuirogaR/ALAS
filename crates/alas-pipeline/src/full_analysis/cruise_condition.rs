// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The level-flight condition the reported cruise points are evaluated at
//! (`super::cruise_point`, `super::route_point`): the state the mission flies
//! the point's mass in, or the design cruise condition when no mission plan
//! is available.
//!
//! Units: altitudes m, dynamic pressure Pa, masses kg; coefficients on the
//! report's reference area.

use super::*;

/// A level-flight condition a cruise point is evaluated at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CruiseCondition {
    /// Pressure altitude, m.
    pub altitude_m: f64,
    /// Flight Mach number.
    pub mach: f64,
}

impl CruiseCondition {
    /// The design cruise condition: `requirements.cruise_altitude_m` at
    /// `requirements.cruise_mach`.
    pub(crate) fn design(config: &AlasConfig) -> Self {
        Self {
            altitude_m: config.requirements.cruise_altitude_m,
            mach: config.requirements.cruise_mach,
        }
    }

    /// Dynamic pressure, Pa: `0.5 rho V^2` of the flown Mach on the standard
    /// atmosphere at the level, which is `0.5 gamma p M^2` on any day.
    fn dynamic_pressure_pa(self) -> f64 {
        let atmo = Atmosphere::new(self.altitude_m);
        let v = self.mach * atmo.speed_of_sound();
        0.5 * atmo.density() * v * v
    }
}

/// Level-flight lift coefficient of `report`'s aircraft at `mass_kg` in the
/// condition `at`.
pub(super) fn cruise_cl_of_mass(
    config: &AlasConfig,
    report: &AnalysisReport,
    mass_kg: f64,
    at: CruiseCondition,
) -> f64 {
    let q = at.dynamic_pressure_pa();
    crate::cruise_mass::cruise_cl_at_mass(
        mass_kg,
        config.requirements.gravity_m_s2,
        q,
        report.airplane.s_ref,
    )
}

/// Total drag coefficient of the trimmed drag table at lift coefficient `cl`
/// in the flown condition `flown` (the mission's own drag there), else at
/// the design cruise Mach and the table's reference altitude, trim and wave
/// drag included; `None` when `report` carries no built fuel artifacts (and
/// so no table) or the table has no finite positive value there.
pub(crate) fn trimmed_table_cd(
    config: &AlasConfig,
    report: &AnalysisReport,
    cl: f64,
    flown: Option<CruiseCondition>,
) -> Option<f64> {
    let artifacts = report.fuel.built_artifacts()?;
    let table = artifacts.drag.table()?;
    let (mach, altitude_m) = flown.map_or(
        (config.requirements.cruise_mach, table.reference_altitude_m()),
        |at| (at.mach, at.altitude_m),
    );
    let cd = table.cd(cl, mach, altitude_m);
    (cd.is_finite() && cd > 0.0).then_some(cd)
}
