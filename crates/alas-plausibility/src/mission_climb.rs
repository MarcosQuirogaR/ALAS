// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The initial climb against a published time to climb.

use super::mission::{phase_kind, Phase};
use super::mission_facts::{Kind, MissionFacts};

/// Time from the start of the flight to first reaching `altitude_m` in the
/// initial climb (up to the first cruise phase), s; `None` when the climb ends
/// below it.
fn time_to_altitude_s(m: &MissionFacts, altitude_m: f64) -> Option<f64> {
    let start_s = m.segments.first()?.t0_s;
    for (index, s) in m.segments.iter().enumerate() {
        if phase_kind(m, index) == Kind::Cruise {
            return None;
        }
        if s.h1_m >= altitude_m && s.h1_m > s.h0_m {
            let share = ((altitude_m - s.h0_m) / (s.h1_m - s.h0_m)).clamp(0.0, 1.0);
            return Some(s.t0_s + share * (s.t1_s - s.t0_s) - start_s);
        }
    }
    None
}

/// Time, s, to climb from the top of the model's climb (`top_m`, reached after
/// `top_s`) to `altitude_m`, extending the model's own rate of climb with its
/// trend: a straight line through the rate of the last climb legs flown on
/// their own schedule (not path-change pieces) against altitude, which is how the
/// rate of climb falls to zero at the ceiling. A trend that does not fall is
/// replaced by the rate at the top (an upper bound). `None` without a climb leg.
fn extended_time_s(m: &MissionFacts, top_m: f64, top_s: f64, altitude_m: f64) -> Option<f64> {
    const LEGS: usize = 8;
    let mut points: Vec<(f64, f64)> = Vec::new();
    for (index, s) in m.segments.iter().enumerate() {
        if phase_kind(m, index) == Kind::Cruise {
            break;
        }
        if s.kind == Kind::Climb && !s.speed_change && !s.tag.contains("_path_") && s.t1_s > s.t0_s
        {
            let rate = (s.h1_m - s.h0_m) / (s.t1_s - s.t0_s);
            if rate > 0.0 {
                points.push((0.5 * (s.h0_m + s.h1_m), rate));
            }
        }
    }
    let recent = &points[points.len().saturating_sub(LEGS)..];
    let n = recent.len() as f64;
    let (mean_h, mean_r) = recent
        .iter()
        .fold((0.0, 0.0), |(h, r), p| (h + p.0 / n, r + p.1 / n));
    let variance: f64 = recent.iter().map(|p| (p.0 - mean_h).powi(2)).sum();
    let slope = if variance > 0.0 {
        recent
            .iter()
            .map(|p| (p.0 - mean_h) * (p.1 - mean_r))
            .sum::<f64>()
            / variance
    } else {
        0.0
    };
    let remaining_m = altitude_m - top_m;
    if slope >= 0.0 {
        let (_, top_rate) = *recent.last()?;
        return Some(top_s + remaining_m / top_rate);
    }
    // The fitted line at the top and at the target altitude: t = (1/slope) ln(r_target/r_top).
    let rate_at = |h: f64| mean_r + slope * (h - mean_h);
    let (rate_top, rate_target) = (rate_at(top_m), rate_at(altitude_m));
    if rate_top <= 0.0 || rate_target <= 0.0 {
        return Some(f64::INFINITY);
    }
    Some(top_s + (rate_target / rate_top).ln() / slope)
}

/// How much slower than its published time to climb the initial climb `p` is
/// (a fraction; negative when faster) and what it was compared with. A type
/// with a published time (`data/climb_references.tsv`) is compared with it at
/// the corresponding weight; a class reference (a turboprop without a row of
/// its own) by mean rate. When the model's climb ends below the quoted
/// altitude, the rest is climbed on the trend of its own rate of climb (see
/// `extended_time_s`), not at its mean rate.
pub(super) fn slower_than_reference(m: &MissionFacts, p: &Phase) -> Option<(f64, String)> {
    let first = m.segments.first()?;
    let reference = m.climb_reference.as_ref()?;
    let span_m = reference.altitude_m - first.h0_m;
    if span_m <= 0.0 || p.dh_m <= 0.0 || p.dt_s <= 0.0 {
        return None;
    }
    let (reference_s, label) = if reference.class {
        (
            reference.time_s * span_m / reference.altitude_m,
            format!("the turboprop class reference ({})", reference.source),
        )
    } else {
        let seconds = reference.time_from_field_s(first.h0_m, first.mass0_kg);
        (
            seconds,
            format!(
                "the published time ({:.1} min from the field at {:.0} kg: {})",
                seconds / 60.0,
                first.mass0_kg,
                reference.source
            ),
        )
    };
    let model_s = time_to_altitude_s(m, reference.altitude_m)
        .or_else(|| extended_time_s(m, first.h0_m + p.dh_m, p.dt_s, reference.altitude_m))?;
    Some((model_s / reference_s - 1.0, label))
}
