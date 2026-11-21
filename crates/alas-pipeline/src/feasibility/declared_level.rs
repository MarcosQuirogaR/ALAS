// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Warning when a declared (or design) cruise level could not be flown: the dispatched trip
//! flies the highest level the aircraft can climb to at the dispatched mass, and the user is told which level that was. Warning only. The level is the
//! dispatch stage's own (`PolicyClosureCase::declared_level`), not a second evaluation; a trip
//! that could not be flown is reported as a check that could not be evaluated.

use alas_units::FOOT;

use super::{warning, FindingCode, PhysicalFinding};
use crate::mission_stage::dispatch::DeclaredLevelCheck;

/// Shortfall below which the lowering is a rounding of the level search, m (1,000 ft).
const REPORTED_SHORTFALL_M: f64 = 1_000.0 * FOOT;

/// Append the finding for `check`, if there is one to make.
pub(super) fn declared_level_finding(
    check: Option<&DeclaredLevelCheck>,
    takeoff_mass_kg: f64,
    findings: &mut Vec<PhysicalFinding>,
) {
    let Some(check) = check else {
        return;
    };
    match &check.flown_m {
        Ok(flown_m) if check.declared_m - flown_m > REPORTED_SHORTFALL_M => {
            findings.push(warning(
                FindingCode::DeclaredCruiseLevelLowered,
                format!(
                    "the declared cruise level {:.0} ft cannot be flown at the dispatched take-off mass {takeoff_mass_kg:.0} kg; the route is flown at {:.0} ft, {:.0} ft lower; warning only",
                    check.declared_m / FOOT,
                    flown_m / FOOT,
                    (check.declared_m - flown_m) / FOOT
                ),
                Some(*flown_m),
                Some(check.declared_m),
                "m",
            ));
        }
        Ok(_) => {}
        Err(reason) => findings.push(warning(
            FindingCode::DeclaredCruiseLevelUnchecked,
            format!(
                "the declared cruise level {:.0} ft could not be checked against the dispatched trip: {reason}",
                check.declared_m / FOOT
            ),
            None,
            Some(check.declared_m),
            "m",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(check: DeclaredLevelCheck) -> Vec<PhysicalFinding> {
        let mut findings = Vec::new();
        declared_level_finding(Some(&check), 78_000.0, &mut findings);
        findings
    }

    /// A lowered level is a warning with both levels; a level flown as declared (or within a
    /// level pair) is nothing; a trip that could not be flown is a check that could not be
    /// evaluated, never silence.
    #[test]
    fn a_lowered_level_is_a_warning_and_a_failed_check_is_not_silent() {
        let lowered = run(DeclaredLevelCheck {
            declared_m: 12_000.0,
            flown_m: Ok(12_000.0 - 3_000.0 * FOOT),
        });
        assert_eq!(lowered.len(), 1);
        assert_eq!(lowered[0].code, FindingCode::DeclaredCruiseLevelLowered);
        assert!(
            lowered[0].message.contains("39370 ft"),
            "{}",
            lowered[0].message
        );
        assert!(
            lowered[0].message.contains("36370 ft"),
            "{}",
            lowered[0].message
        );
        assert!(run(DeclaredLevelCheck {
            declared_m: 12_000.0,
            flown_m: Ok(12_000.0 - 500.0 * FOOT),
        })
        .is_empty());
        let failed = run(DeclaredLevelCheck {
            declared_m: 12_000.0,
            flown_m: Err("the trip is too short".to_owned()),
        });
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].code, FindingCode::DeclaredCruiseLevelUnchecked);
        assert!(failed[0].message.contains("could not be checked"));
        assert!(failed[0].message.contains("too short"));
    }
}
