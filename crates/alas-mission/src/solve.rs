// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Methods/Missions/Segments/converge_root.py,
// mission analysis model/Methods/Missions/Segments/Common/Sub_Segments.py and
// mission analysis model/Analyses/Mission/Sequential_Segments.py.
// Upstream: mission analysis model 2.5.2, LGPL-2.1 (relicensed under GPL-2.0-or-later per
// LGPL-2.1 section 3; compatible with this program's AGPL-3.0-or-later).
// Reference: alas @ rust-port-baseline.

//! Solving a segment, and flying a mission one segment at a time.
//!
//! [`converge_root`] is the whole of mission analysis model's solver interface: pack the
//! unknowns, hand the residual to a root finder, and record whether it
//! converged. The root finder is `scipy.optimize.fsolve`, which is MINPACK's
//! `hybrd` -- [`alas_math::hybrd`]'s own green row -- called with the
//! segment's `tolerance_solution` and with SciPy's substitutions for the two
//! settings mission analysis model leaves unset.
//!
//! [`Mission::evaluate`] is `Sequential_Segments`: each segment is initialized
//! against the one before it, solved on its own, finalized, and its final
//! state handed forward. There is no outer iteration -- a segment's answer is
//! never revisited once the next one has started -- which is what makes the
//! mass at the end of each segment the entire coupling between them.
//!
//! # Why the failure exits are kept apart
//!
//! `converge_root` collapses MINPACK's four unhappy exits into one: it prints
//! the message and sets `converged = False`. That is reproduced -- a segment
//! either converged or did not -- but the [`alas_math::hybrd::Status`] is kept
//! alongside, because the four say different things about what to do next, and
//! a mission that stops converging is diagnosed from them.

use alas_math::hybrd::{self, Settings, Status};

use crate::segments::{MissionAnalyses, Segment, SegmentError, SegmentSpec};

/// How one segment's solve came out.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentSolution {
    /// Whether the root finder reported success -- the only thing upstream
    /// keeps.
    pub converged: bool,
    /// Which of MINPACK's five exits it took.
    pub status: Status,
    /// Residual evaluations spent.
    pub evaluations: usize,
    /// The commanded-throttle boundary was reached before force balance.
    pub throttle_limited: bool,
}

/// Why a mission could not be flown.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MissionError {
    /// A segment could not be set up.
    #[error("segment {tag}: {source}")]
    Setup {
        /// Which segment.
        tag: String,
        /// What went wrong.
        #[source]
        source: SegmentError,
    },
    /// The root finder declined the system outright, which is MINPACK's
    /// `info = 0` and the case SciPy raises over.
    ///
    /// The field is `reason` rather than `source` because
    /// [`hybrd::HybrdError`] is a plain enum rather than an error type: it
    /// describes a system the routine will not start on, not a failure it
    /// suffered part-way.
    #[error("segment {tag}: the root finder rejected the system ({reason:?})")]
    Solver {
        /// Which segment.
        tag: String,
        /// What it objected to.
        reason: hybrd::HybrdError,
    },
}

/// Solve one segment's two-unknown system, as `converge_root` does.
///
/// The segment is left holding the point the search stopped at and the
/// conditions that point produces, whether or not it converged: MINPACK
/// returns its best point either way and mission analysis model keeps it. Because the last
/// residual evaluation is not necessarily at the final point, the chain is run
/// once more at the answer before returning, so the conditions a caller reads
/// are the ones the reported unknowns produce.
///
/// # Errors
///
/// [`MissionError::Solver`] when the root finder will not start.
pub fn converge_root(
    segment: &mut Segment,
    analyses: &MissionAnalyses,
) -> Result<SegmentSolution, MissionError> {
    let start = segment.pack_unknowns();
    let settings = Settings {
        xtol: segment.numerics.tolerance_solution,
        // mission analysis model leaves `max_evaluations` at `0.` and `step_size` at `None`,
        // and SciPy substitutes its own defaults for both. `alas_math::hybrd`
        // spells that substitution `None`; a nonzero budget upstream would be
        // an explicit cap and is carried through as one.
        max_evaluations: (segment.numerics.max_evaluations > 0.0)
            .then_some(segment.numerics.max_evaluations)
            .map(|budget| budget as usize),
        step_size: segment.numerics.step_size,
        ..Settings::default()
    };

    let solution = {
        let segment = &mut *segment;
        hybrd::solve(
            |point, residual| {
                segment.unpack_unknowns(point);
                if analyses.enforce_throttle_envelope {
                    for throttle in &mut segment.throttle {
                        *throttle = throttle.clamp(0.0, 1.0);
                    }
                }
                segment.iterate(analyses);
                residual.copy_from_slice(&segment.pack_residuals());
            },
            &start,
            &settings,
        )
    }
    .map_err(|reason| MissionError::Solver {
        tag: segment.spec.tag.clone(),
        reason,
    })?;

    segment.unpack_unknowns(&solution.x);
    let requested_above_limit = segment
        .throttle
        .iter()
        .any(|throttle| throttle.is_finite() && *throttle > 1.0);
    if analyses.enforce_throttle_envelope {
        for throttle in &mut segment.throttle {
            *throttle = throttle.clamp(0.0, 1.0);
        }
    }
    segment.iterate(analyses);

    let throttle_within_available_envelope = !analyses.enforce_throttle_envelope
        || segment
            .conditions
            .throttle
            .iter()
            .all(|throttle| throttle.is_finite() && *throttle >= 0.0 && *throttle <= 1.0);
    let converged = solution.status.is_converged() && throttle_within_available_envelope;
    let throttle_limited = analyses.enforce_throttle_envelope
        && !converged
        && (requested_above_limit
            || segment
                .conditions
                .throttle
                .iter()
                .any(|throttle| throttle.is_finite() && *throttle >= 1.0 - 1.0e-9));
    segment.numerics.converged = Some(converged);
    if !converged {
        tracing::warn!(
            segment = %segment.spec.tag,
            status = ?solution.status,
            evaluations = solution.evaluations,
            "segment did not converge within the solver and propulsion envelopes"
        );
    }

    Ok(SegmentSolution {
        converged,
        status: solution.status,
        evaluations: solution.evaluations,
        throttle_limited,
    })
}

/// A whole mission: an ordered list of segments over one aircraft.
pub struct Mission {
    /// The schedule, in the order it is flown.
    pub schedule: Vec<SegmentSpec>,
}

/// A flown mission.
#[derive(Debug, Clone, PartialEq)]
pub struct MissionResult {
    /// Each segment, holding the conditions its solved unknowns produce.
    pub segments: Vec<Segment>,
    /// How each segment's solve came out, aligned with [`Self::segments`].
    pub solutions: Vec<SegmentSolution>,
    /// Number of segments in the requested schedule.
    ///
    /// This remains larger than `segments.len()` when evaluation stops after
    /// fuel exhaustion or a failed solve, so consumers cannot mistake partial
    /// diagnostic telemetry for a completed flight.
    pub scheduled_segment_count: usize,
    /// The first segment that consumed more fuel than the load case carried.
    /// A populated value means the returned telemetry is deliberately partial:
    /// later segments were not propagated below the dry-mass floor.
    pub fuel_exhaustion: Option<FuelExhaustion>,
}

/// Scalar outputs that are valid only for a fully completed mission.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompletedMissionSummary {
    /// Aircraft mass at the first mission control point, kg.
    pub takeoff_mass_kg: f64,
    /// Aircraft mass at the last mission control point, kg.
    pub landing_mass_kg: f64,
    /// Fuel consumed by the complete modeled mission, kg.
    pub trip_fuel_kg: f64,
    /// Sum of segment time spans, s.
    pub block_time_s: f64,
    /// Range recorded at the final mission control point, m.
    pub distance_flown_m: f64,
}

/// Where a mission first crossed its usable-fuel boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct FuelExhaustion {
    /// Zero-based schedule position of the segment that crossed the boundary.
    pub segment_index: usize,
    /// Human-readable segment tag.
    pub segment_tag: String,
    /// Fuel available in the selected takeoff load case, kg.
    pub available_fuel_kg: f64,
    /// Fuel burned by the first control point found below the dry-mass floor,
    /// kg.
    pub burned_fuel_kg: f64,
    /// OEW plus payload for this load case, kg.
    pub minimum_mass_kg: f64,
}

impl MissionResult {
    /// Return scalar mission results only when all requested segments completed.
    ///
    /// Partial, exhausted, unconverged, throttle-limited, or non-finite
    /// telemetry deliberately produces `None`.
    pub fn completed_summary(&self) -> Option<CompletedMissionSummary> {
        if self.scheduled_segment_count == 0
            || self.fuel_exhaustion.is_some()
            || self.segments.len() != self.scheduled_segment_count
            || self.solutions.len() != self.scheduled_segment_count
            || self
                .solutions
                .iter()
                .any(|solution| !solution.converged || solution.throttle_limited)
            || self.segments.iter().any(|segment| {
                segment.conditions.total_mass_kg.len() < 2
                    || segment.conditions.time_s.len() < 2
                    || segment.conditions.aircraft_range_m.is_empty()
                    || segment
                        .conditions
                        .total_mass_kg
                        .iter()
                        .chain(&segment.conditions.time_s)
                        .chain(&segment.conditions.aircraft_range_m)
                        .any(|value| !value.is_finite())
            })
        {
            return None;
        }

        let takeoff_mass_kg = self.initial_mass_kg();
        let landing_mass_kg = self.final_mass_kg();
        let trip_fuel_kg = takeoff_mass_kg - landing_mass_kg;
        let block_time_s = self.block_time_s();
        let distance_flown_m = self
            .segments
            .last()?
            .conditions
            .aircraft_range_m
            .last()
            .copied()?;
        (takeoff_mass_kg.is_finite()
            && takeoff_mass_kg > 0.0
            && landing_mass_kg.is_finite()
            && landing_mass_kg > 0.0
            && trip_fuel_kg.is_finite()
            && trip_fuel_kg >= 0.0
            && block_time_s.is_finite()
            && block_time_s >= 0.0
            && distance_flown_m.is_finite()
            && distance_flown_m >= 0.0)
            .then_some(CompletedMissionSummary {
                takeoff_mass_kg,
                landing_mass_kg,
                trip_fuel_kg,
                block_time_s,
                distance_flown_m,
            })
    }

    /// Mass at the very start, kg.
    pub fn initial_mass_kg(&self) -> f64 {
        self.segments
            .first()
            .and_then(|segment| segment.conditions.total_mass_kg.first().copied())
            .unwrap_or_default()
    }

    /// Mass at the very end, kg.
    pub fn final_mass_kg(&self) -> f64 {
        self.segments
            .last()
            .and_then(|segment| segment.conditions.total_mass_kg.last().copied())
            .unwrap_or_default()
    }

    /// Fuel burned over the whole mission, kg.
    pub fn fuel_burned_kg(&self) -> f64 {
        self.initial_mass_kg() - self.final_mass_kg()
    }

    /// Total time in the air, seconds: the sum of the segment spans rather
    /// than the difference of the endpoints, matching upstream's summary.
    pub fn block_time_s(&self) -> f64 {
        self.segments
            .iter()
            .map(|segment| {
                let time = &segment.conditions.time_s;
                match (time.last(), time.first()) {
                    (Some(&last), Some(&first)) => last - first,
                    _ => 0.0,
                }
            })
            .sum()
    }
}

impl Mission {
    /// Fly the mission: `expand_sub_segments` then `sequential_sub_segments`.
    ///
    /// Each segment is built against its predecessor's final state, solved,
    /// finalized, and handed forward. Product validity cannot be recovered by
    /// propagating an unconverged state, so the first failed segment is kept as
    /// partial diagnostic telemetry and terminates the flown trajectory.
    ///
    /// # Errors
    ///
    /// [`MissionError::Setup`] if a segment could not be built and
    /// [`MissionError::Solver`] if the root finder would not start on one.
    pub fn evaluate(&self, analyses: &MissionAnalyses) -> Result<MissionResult, MissionError> {
        let mut segments = Vec::with_capacity(self.schedule.len());
        let mut solutions = Vec::with_capacity(self.schedule.len());
        let mut initials = None;
        let mut fuel_exhaustion = None;

        for (segment_index, spec) in self.schedule.iter().enumerate() {
            let tag = spec.tag.clone();
            let mut segment =
                Segment::new(spec.clone(), initials).map_err(|source| MissionError::Setup {
                    tag: tag.clone(),
                    source,
                })?;
            let solution = converge_root(&mut segment, analyses)?;
            segment.finalize();

            if let Some(minimum_mass_kg) = analyses.minimum_mass_kg {
                let crossing_mass_kg = segment
                    .conditions
                    .total_mass_kg
                    .iter()
                    .copied()
                    .find(|mass| *mass < minimum_mass_kg);
                if let Some(crossing_mass_kg) = crossing_mass_kg {
                    fuel_exhaustion = Some(FuelExhaustion {
                        segment_index,
                        segment_tag: tag,
                        available_fuel_kg: analyses.takeoff_mass_kg - minimum_mass_kg,
                        burned_fuel_kg: analyses.takeoff_mass_kg - crossing_mass_kg,
                        minimum_mass_kg,
                    });
                }
            }

            let next_initials = segment.initials_for_next();
            let converged = solution.converged;
            segments.push(segment);
            solutions.push(solution);
            if fuel_exhaustion.is_some() || !converged {
                break;
            }
            initials = Some(next_initials);
        }

        Ok(MissionResult {
            segments,
            solutions,
            scheduled_segment_count: self.schedule.len(),
            fuel_exhaustion,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_telemetry_cannot_publish_a_completed_summary() {
        let result = MissionResult {
            segments: Vec::new(),
            solutions: Vec::new(),
            scheduled_segment_count: 1,
            fuel_exhaustion: None,
        };

        assert_eq!(result.completed_summary(), None);
    }

    #[test]
    fn an_empty_schedule_is_not_a_completed_mission() {
        let result = MissionResult {
            segments: Vec::new(),
            solutions: Vec::new(),
            scheduled_segment_count: 0,
            fuel_exhaustion: None,
        };

        assert_eq!(result.completed_summary(), None);
    }
}
