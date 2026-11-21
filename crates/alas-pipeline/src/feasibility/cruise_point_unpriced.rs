// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The findings of a reported cruise point no design mission was priced
//! for, or whose priced mission has no flown state at its mid-cruise mass.

use alas_config::AlasConfig;

use crate::full_analysis::{AnalysisReport, CruisePointMission};

use super::{warning, FindingCode, PhysicalFinding};

/// The warnings on `report`'s cruise point: [`finding`] and
/// [`level_unplanned_finding`].
pub(super) fn findings(config: &AlasConfig, report: &AnalysisReport) -> Vec<PhysicalFinding> {
    finding(report)
        .into_iter()
        .chain(level_unplanned_finding(config, report))
        .collect()
}

/// The [`FindingCode::CruisePointLevelUnplanned`] warning of a priced cruise
/// point that stands at the design cruise altitude and Mach because no flown
/// state of its mid-cruise mass was found (`CruisePointBasis::level_unplanned`):
/// the lift coefficient and lift-to-drag ratio then describe a condition the
/// mission need not fly. `None` otherwise.
fn level_unplanned_finding(
    config: &AlasConfig,
    report: &AnalysisReport,
) -> Option<PhysicalFinding> {
    let basis = report.cruise_point_basis()?;
    let req = &config.requirements;
    basis.level_unplanned().then(|| {
        warning(
            FindingCode::CruisePointLevelUnplanned,
            format!(
                "no flown state of the design mission's mid-cruise mass ({:.0} kg) was found, so the reported cruise point stands at the design cruise altitude {:.0} m and Mach {:.3}, which need not be a level or speed the mission flies: its lift coefficient and lift-to-drag ratio describe that condition, not the flight",
                basis.mid_cruise_mass_kg, req.cruise_altitude_m, req.cruise_mach
            ),
            Some(req.cruise_altitude_m),
            None,
            "m",
        )
    })
}

/// The [`FindingCode::CruisePointUnpriced`] warning of a report whose cruise
/// point stayed at the mean of the take-off and zero-fuel masses because its
/// design mission could not be closed: the cruise lift coefficient, drag,
/// lift-to-drag ratio and trip fuel then describe no flown mission. `None`
/// for a priced point or a report that records none.
fn finding(report: &AnalysisReport) -> Option<PhysicalFinding> {
    let basis = report.cruise_point_basis()?;
    (basis.mission == CruisePointMission::AllFuelBurned).then(|| {
        warning(
            FindingCode::CruisePointUnpriced,
            "the design mission could not be closed, so the reported cruise point is the mean of the take-off and zero-fuel masses with all loaded fuel burned, not a flown mission: its lift coefficient, lift-to-drag ratio and trip fuel are not a mission result",
            Some(basis.trip_fuel_kg),
            None,
            "kg",
        )
    })
}
