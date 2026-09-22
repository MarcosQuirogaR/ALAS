// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Flying one schedule with bounded guidance revisions.
//!
//! A segment that does not converge, or that needs more than full throttle,
//! is not a flown segment. Rather than propagating it, the flight replans
//! the failed leg from the propulsion envelope: a shallower climb, a slower
//! cruise, a gentler descent: closes the route distance again, and flies
//! once more, up to a bounded number of revisions. The same loop serves the
//! dispatch closure, which flies the route at several takeoff masses, and
//! the final flight the pipeline reports.

use alas_mission::segments::{SegmentKind, SegmentSpec};
use alas_mission::{Mission, MissionRequest, MissionResult};

use super::schedule::close_schedule_distance;
use super::{adapt_failed_climb, adapt_failed_cruise, adapt_failed_descent, MissionAnalyses};

/// Upper bound on guidance revisions before the last result is returned as
/// it stands, converged or not.
const MAX_GUIDANCE_REVISIONS: usize = 24;

/// Fly `schedule` against `analyses`, replanning failed legs from the
/// available propulsion envelope until the mission completes or the
/// revision budget is spent.
///
/// The returned result is the last flight attempted; a caller reads its
/// `completed_summary` to learn whether the route was actually closed.
pub(super) fn fly_with_guidance(
    mut schedule: Vec<SegmentSpec>,
    request: &MissionRequest,
    analyses: &MissionAnalyses,
) -> Result<MissionResult, String> {
    for revision in 0..=MAX_GUIDANCE_REVISIONS {
        let result = Mission {
            schedule: schedule.clone(),
        }
        .evaluate(analyses)
        .map_err(|error| format!("native mission failed: {error}"))?;
        if result.completed_summary().is_some() || revision == MAX_GUIDANCE_REVISIONS {
            return Ok(result);
        }

        let Some(index) = result.segments.len().checked_sub(1) else {
            return Ok(result);
        };
        let segment_tag = schedule[index].tag.clone();
        let solution = &result.solutions[index];
        let adapted = if !solution.converged || solution.throttle_limited {
            match schedule[index].kind {
                SegmentKind::Climb { .. } => {
                    adapt_failed_climb(&mut schedule, index, &result.segments[index]).map(
                        |change| {
                            tracing::info!(
                                segment = %segment_tag,
                                revision,
                                old_rate_m_s = change.old_rate_m_s,
                                new_rate_m_s = change.new_rate_m_s,
                                old_end_altitude_m = change.old_end_altitude_m,
                                new_end_altitude_m = change.new_end_altitude_m,
                                old_speed_m_s = change.old_speed_m_s,
                                new_speed_m_s = change.new_speed_m_s,
                                "replanned unconverged climb from the available propulsion envelope"
                            );
                        },
                    )
                }
                SegmentKind::Cruise { .. } => {
                    adapt_failed_cruise(&mut schedule, index).map(|change| {
                        tracing::info!(
                            segment = %segment_tag,
                            revision,
                            old_speed_m_s = change.old_speed_m_s,
                            new_speed_m_s = change.new_speed_m_s,
                            "replanned unconverged cruise below the propulsion envelope"
                        );
                    })
                }
                SegmentKind::Descent { .. } => {
                    // The refusal's own cause chooses the direction: a rung
                    // the engine cannot hold *down* to needs to be flown more
                    // shallowly, and every other failure needs it steeper.
                    adapt_failed_descent(&mut schedule, index, solution.idle_floor_limited).map(
                        |change| {
                            tracing::info!(
                                segment = %segment_tag,
                                revision,
                                old_rate_m_s = change.old_rate_m_s,
                                new_rate_m_s = change.new_rate_m_s,
                                below_idle = solution.idle_floor_limited,
                                "replanned unconverged descent from the available propulsion envelope"
                            );
                        },
                    )
                }
            }
        } else {
            None
        };
        if adapted.is_none() {
            tracing::warn!(
                segment = %segment_tag,
                revision,
                status = ?solution.status,
                converged = solution.converged,
                throttle_limited = solution.throttle_limited,
                "mission guidance could not find a bounded profile revision"
            );
            return Ok(result);
        }
        close_schedule_distance(&mut schedule, request)?;
    }
    unreachable!("bounded guidance revision loop always returns")
}
