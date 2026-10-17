// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The report flag for an improved-climb take-off safety speed.

use alas_perf::performance::ImprovedClimbSelection;

use super::{warning, FindingCode, PhysicalFinding};

/// A warning stating the speed the second segment is flown at when it is above
/// the CS 25.107 schedule, and the field length charged for it. `None` when the
/// schedule speed stands.
pub(super) fn finding(selection: Option<ImprovedClimbSelection>) -> Option<PhysicalFinding> {
    let selection = selection.filter(|selection| selection.improved)?;
    if !selection.meets {
        return Some(warning(
            FindingCode::ImprovedClimbV2,
            format!(
                "improved climb cannot meet the CS 25.121(b) gradient: at the best-climb speed {:.3} VSR (schedule {:.3}) the required static T/W is {:.4} against {:.4} at the schedule speed, so the schedule speed stands and the second-segment finding is reported there",
                selection.v2_over_vsr,
                selection.schedule_v2_over_vsr,
                selection.required,
                selection.required_at_schedule,
            ),
            Some(selection.v2_over_vsr),
            Some(selection.schedule_v2_over_vsr),
            "V2/VSR",
        ));
    }
    let outcome = "meets the CS 25.121(b) gradient";
    Some(warning(
        FindingCode::ImprovedClimbV2,
        format!(
            "take-off safety speed raised from {:.3} to {:.3} VSR (improved climb, Jenkinson Ch. 10 overspeeding, Torenbeek Advanced Aircraft Design Sec. 9.3.2): the required static T/W falls from {:.4} to {:.4}, which {outcome}; the jet take-off field length is charged by the square of the speed ratio (x{:.3}) in the field checks",
            selection.schedule_v2_over_vsr,
            selection.v2_over_vsr,
            selection.required_at_schedule,
            selection.required,
            selection.field_length_factor(),
        ),
        Some(selection.v2_over_vsr),
        Some(selection.schedule_v2_over_vsr),
        "V2/VSR",
    ))
}
