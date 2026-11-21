// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wing-area finding: the declared maximum in a clean sheet, the
//! registered window in a redesign of a registered aircraft.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;

use super::{error, FindingCode, PhysicalFinding};

/// Append the [`FindingCode::WingAreaLimit`] error when the projected main
/// wing area is outside its limit.
pub(super) fn append(config: &AlasConfig, plane: &Airplane, findings: &mut Vec<PhysicalFinding>) {
    let projected_area_m2 = plane
        .wings
        .first()
        .map_or(f64::NAN, alas_geom::aircraft::wing::Wing::reference_area);
    // A redesigned registered aircraft is held within a fraction of the
    // registered wing's area on both sides, the optimizer's `wing_area` and
    // `wing_area_floor` rows; a clean sheet keeps the declared maximum.
    let registered_area = alas_opt::mdo::registered_wing_area(config);
    let upper_m2 = registered_area.map_or(config.requirements.max_wing_area_m2, |area| {
        area.projected_window_m2().1
    });
    let area_roundoff_m2 = upper_m2.abs().max(1.0) * 1.0e-12;
    if !projected_area_m2.is_finite() || projected_area_m2 > upper_m2 + area_roundoff_m2 {
        findings.push(error(
            FindingCode::WingAreaLimit,
            if registered_area.is_some() {
                "wing reference area exceeds the registered aircraft's by more than the allowed fraction"
            } else {
                "wing reference area exceeds the configured maximum"
            },
            Some(projected_area_m2),
            Some(upper_m2),
            "m^2",
        ));
    } else if let Some(lower_m2) = registered_area.map(|area| area.projected_window_m2().0) {
        if projected_area_m2 < lower_m2 - area_roundoff_m2 {
            findings.push(error(
                FindingCode::WingAreaLimit,
                "wing reference area is below the registered aircraft's by more than the allowed fraction",
                Some(projected_area_m2),
                Some(lower_m2),
                "m^2",
            ));
        }
    }
}
