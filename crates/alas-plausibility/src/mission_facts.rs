// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! End-point facts of a solved mission, in SI units, as the mission rules read them.

use alas_mission::segments::SegmentKind;
use alas_mission::MissionResult;

const FT: f64 = 0.3048;
const SEA_LEVEL_DENSITY: f64 = 1.225;
/// Numerical tolerance on a leg that ends exactly at a rule altitude, m.
const ALTITUDE_TOLERANCE_M: f64 = 1.0;

/// Kind of a mission segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Climb.
    Climb,
    /// Level cruise.
    Cruise,
    /// Descent.
    Descent,
}

/// End-point facts of one flown segment, SI units.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentFacts {
    /// Segment tag.
    pub tag: String,
    /// Segment kind.
    pub kind: Kind,
    /// Whether this is a level speed-change leg between two other legs; the
    /// phase rules look through it.
    pub speed_change: bool,
    /// Start time, s.
    pub t0_s: f64,
    /// End time, s.
    pub t1_s: f64,
    /// Start altitude, m.
    pub h0_m: f64,
    /// End altitude, m.
    pub h1_m: f64,
    /// Start range, m.
    pub r0_m: f64,
    /// End range, m.
    pub r1_m: f64,
    /// Start Mach.
    pub mach0: f64,
    /// End Mach.
    pub mach1: f64,
    /// Start true airspeed, m/s.
    pub tas0_ms: f64,
    /// End true airspeed, m/s.
    pub tas1_ms: f64,
    /// Start mass, kg.
    pub mass0_kg: f64,
    /// End mass, kg.
    pub mass1_kg: f64,
    /// Largest equivalent airspeed below 10,000 ft in the segment, m/s (0 when
    /// the segment never goes below it).
    pub max_eas_low_alt_ms: f64,
    /// Highest Mach in the segment.
    pub max_mach: f64,
    /// Mean lift coefficient.
    pub cl_mean: f64,
    /// Mean lift-to-drag ratio.
    pub ld_mean: f64,
    /// Smallest point-to-point time step, s.
    pub min_step_t_s: f64,
    /// Smallest point-to-point range step, m.
    pub min_step_r_m: f64,
    /// Largest point-to-point mass gain, kg (negative when mass only falls).
    pub max_mass_gain_kg: f64,
}

/// A flown mission as the rules see it.
#[derive(Debug, Clone, PartialEq)]
pub struct MissionFacts {
    /// Segments flown, in order.
    pub segments: Vec<SegmentFacts>,
    /// Segments scheduled.
    pub scheduled: usize,
    /// Whether the load case ran out of fuel.
    pub fuel_exhausted: bool,
    /// Design cruise Mach of the configuration.
    pub design_mach: f64,
    /// Published time to climb of the type, when it has one.
    pub climb_reference: Option<super::climb_reference::ClimbReference>,
    /// Whether this is the priced route profile (commanded rung speeds whose
    /// changes are paid as kinetic-energy budgets) rather than a flown
    /// trajectory: the speed-continuity test does not apply to it.
    pub priced: bool,
}

impl MissionFacts {
    /// Extract the facts from a solved mission.
    pub fn from_result(mission: &MissionResult, design_mach: f64) -> Self {
        let mut segments = Vec::new();
        for segment in &mission.segments {
            let c = &segment.conditions;
            let n = c.time_s.len();
            if n == 0 {
                continue;
            }
            let kind = match segment.spec.kind {
                SegmentKind::Climb { .. } => Kind::Climb,
                SegmentKind::Cruise { .. } => Kind::Cruise,
                SegmentKind::Descent { .. } => Kind::Descent,
            };
            let last = n - 1;
            let mut max_eas = 0.0_f64;
            let mut max_mach = 0.0_f64;
            let (mut cl_sum, mut ld_sum) = (0.0, 0.0);
            for i in 0..n {
                if c.altitude_m[i] < 10_000.0 * FT - ALTITUDE_TOLERANCE_M {
                    max_eas = max_eas
                        .max(c.velocity_m_s[i] * (c.density_kg_m3[i] / SEA_LEVEL_DENSITY).sqrt());
                }
                max_mach = max_mach.max(c.mach[i]);
                cl_sum += c.lift_coefficient[i];
                ld_sum += c.lift_coefficient[i] / c.drag_coefficient[i];
            }
            let step = |values: &[f64]| values.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>();
            let min_of = |values: Vec<f64>| values.into_iter().fold(f64::INFINITY, f64::min);
            let max_of = |values: Vec<f64>| values.into_iter().fold(f64::NEG_INFINITY, f64::max);
            segments.push(SegmentFacts {
                tag: segment.spec.tag.clone(),
                kind,
                speed_change: segment.spec.tag.starts_with("speed_change_to_"),
                t0_s: c.time_s[0],
                t1_s: c.time_s[last],
                h0_m: c.altitude_m[0],
                h1_m: c.altitude_m[last],
                r0_m: c.aircraft_range_m[0],
                r1_m: c.aircraft_range_m[last],
                mach0: c.mach[0],
                mach1: c.mach[last],
                tas0_ms: c.velocity_m_s[0],
                tas1_ms: c.velocity_m_s[last],
                mass0_kg: c.total_mass_kg[0],
                mass1_kg: c.total_mass_kg[last],
                max_eas_low_alt_ms: max_eas,
                max_mach,
                cl_mean: cl_sum / n as f64,
                ld_mean: ld_sum / n as f64,
                min_step_t_s: min_of(step(&c.time_s)),
                min_step_r_m: min_of(step(&c.aircraft_range_m)),
                max_mass_gain_kg: max_of(step(&c.total_mass_kg)),
            });
        }
        Self {
            segments,
            scheduled: mission.scheduled_segment_count,
            fuel_exhausted: mission.fuel_exhaustion.is_some(),
            design_mach,
            climb_reference: None,
            priced: false,
        }
    }

    /// Ground distance flown, m.
    pub fn range_m(&self) -> f64 {
        match (self.segments.first(), self.segments.last()) {
            (Some(first), Some(last)) => last.r1_m - first.r0_m,
            _ => 0.0,
        }
    }

    /// Flight time, s.
    pub fn duration_s(&self) -> f64 {
        match (self.segments.first(), self.segments.last()) {
            (Some(first), Some(last)) => last.t1_s - first.t0_s,
            _ => 0.0,
        }
    }
}
