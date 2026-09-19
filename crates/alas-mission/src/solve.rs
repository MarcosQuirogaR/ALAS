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
//! `hybrd` ([`alas_math::hybrd`]'s own green row) called with the
//! segment's `tolerance_solution` and with SciPy's substitutions for the two
//! settings mission analysis model leaves unset.
//!
//! [`Mission::evaluate`] is `Sequential_Segments`: each segment is initialized
//! against the one before it, solved on its own, finalized, and its final
//! state handed forward. There is no outer iteration: a segment's answer is
//! never revisited once the next one has started, which is what makes the
//! mass at the end of each segment the entire coupling between them.
//!
//! # Why the failure exits are kept apart
//!
//! `converge_root` collapses MINPACK's four unhappy exits into one: it prints
//! the message and sets `converged = False`. That is reproduced: a segment
//! either converged or did not, but the [`alas_math::hybrd::Status`] is kept
//! alongside, because the four say different things about what to do next, and
//! a mission that stops converging is diagnosed from them.

use alas_math::hybrd::{self, Settings, Status};

use crate::segments::{MissionAnalyses, Segment, SegmentError, SegmentSpec};

/// How one segment's solve came out.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentSolution {
    /// Whether the root finder reported success: the only thing upstream
    /// keeps.
    pub converged: bool,
    /// Which of MINPACK's five exits it took.
    pub status: Status,
    /// Residual evaluations spent.
    pub evaluations: usize,
    /// The commanded-throttle boundary was reached before force balance.
    pub throttle_limited: bool,
    /// Largest finite throttle fraction the root finder *asked for*, or `NaN`
    /// when none was finite.
    ///
    /// [`Self::throttle_limited`] alone cannot say whether a refusal is a
    /// physical shortfall or a diverging root find: it is set when any point
    /// reaches `1.0` *and* the solve did not converge, which is true of both.
    /// The magnitude separates them. A peak just above one is an aeroplane
    /// that is marginally short of thrust on the commanded trajectory; a peak
    /// far above one is the solver having left the physical domain, and the
    /// two have different owners and different fixes. Recorded, not gated on.
    ///
    /// **This is the request, not the delivered command.** While the envelope
    /// is enforced the throttle is clamped into `[0, 1]` *after* the root
    /// find, before the final `iterate` (see the clamp loop below), and the
    /// residual closure itself no longer clamps, so `conditions.throttle` can
    /// never exceed one and a peak read from there is always exactly `1.000`
    /// whenever the clamp was active — which reports that the limit was hit and nothing about how far
    /// short the aeroplane fell. The first version of this field did read it
    /// from there and was measured returning that useless `1.000`.
    pub peak_throttle: f64,
    /// Smallest finite throttle fraction the root finder *asked for*, or
    /// `NaN` when none was finite. The lower-bound counterpart of
    /// [`Self::peak_throttle`], read at the same point and for the same
    /// reason.
    pub minimum_throttle: f64,
    /// The deck's own flight-idle floor, as a normalized-force fraction, at
    /// the control point whose request fell below it; `NaN` when no request
    /// did.
    ///
    /// A propulsion deck's normalized-force command is not bounded below by
    /// zero: both shipped technologies refuse to deliver less than flight
    /// idle, so a whole band of commands above zero produces the identical
    /// force. Measured on the product turbofan deck over an A320-200
    /// `descent_1`, that band is `[0, 0.075]` at the top of the rung and
    /// `[0, 0.092]` at the bottom. Reporting the floor next to
    /// [`Self::minimum_throttle`] is what lets a reader see that a refused
    /// request was *below the engine's idle*, rather than merely negative.
    pub available_throttle_floor: f64,
    /// The solver asked for less force than the propulsion deck's flight-idle
    /// floor delivers, so the commanded trajectory cannot be flown on thrust
    /// alone. The lower-bound counterpart of [`Self::throttle_limited`].
    pub idle_floor_limited: bool,
    /// How many control points asked for less force than the deck delivers
    /// at flight idle; zero when none did.
    ///
    /// This is deliberately a count rather than a magnitude. The *magnitude*
    /// of a sub-idle command carries no information: the continuation that
    /// lets the root find escape the flat band also lets it settle far
    /// outside the deck domain, and the same A320-200 `descent_1` was
    /// measured stopping at -7.3, -37.6 and -173.9 on three variants of that
    /// continuation. Whether the whole rung or one node is below the floor
    /// does not move like that, and it is what separates a schedule that is
    /// wrong throughout from one that only grazes the floor at an end point.
    pub sub_idle_points: usize,
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
                // The command is no longer clamped here. `common::update_thrust`
                // keeps the propulsion deck inside its own domain and continues
                // the force linearly above the rating, so the residual still
                // varies with a command the aeroplane cannot deliver. Clamping
                // at this level instead made the residual flat above one - the
                // Jacobian column vanished and `hybrd` stalled rather than
                // reporting the shortfall. The envelope itself is unchanged and
                // is still enforced after the solve.
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
    // The *requested* peak, read before the clamp below and before the final
    // `iterate` writes deliverable commands into `conditions`. The published
    // conditions are clamped on purpose - an engine cannot be shown running
    // above its rating - so reading the peak from there would report the
    // clamp rather than the demand and could not say how far short the
    // aeroplane fell.
    let peak_requested_throttle = segment
        .throttle
        .iter()
        .copied()
        .filter(|throttle| throttle.is_finite())
        .fold(f64::NAN, f64::max);
    let minimum_requested_throttle = segment
        .throttle
        .iter()
        .copied()
        .filter(|throttle| throttle.is_finite())
        .fold(f64::NAN, f64::min);
    let requested_above_limit = segment
        .throttle
        .iter()
        .any(|throttle| throttle.is_finite() && *throttle > 1.0);
    let requested_throttle = segment.throttle.clone();
    if analyses.enforce_throttle_envelope {
        for throttle in &mut segment.throttle {
            *throttle = throttle.clamp(0.0, 1.0);
        }
    }
    segment.iterate(analyses);

    // The deck's own flight-idle floor, read from the published evaluation.
    // Reading it there rather than at the unclamped request costs nothing and
    // loses nothing: a request below the floor stays below the floor after a
    // clamp into `[0, 1]`, because the floor is itself inside that interval,
    // so the deck raises the same limit and reports the same fraction. A
    // request at or above the floor leaves `available_throttle_floor` at zero
    // and cannot be below it.
    let (sub_idle_points, available_throttle_floor) = requested_throttle
        .iter()
        .zip(segment.conditions.available_throttle_floor.iter())
        .filter(|(throttle, floor)| {
            throttle.is_finite() && floor.is_finite() && **throttle < **floor
        })
        .fold((0_usize, f64::NAN), |(count, worst), (_, floor)| {
            (count + 1, worst.max(*floor))
        });
    let requested_below_limit = analyses.enforce_throttle_envelope && sub_idle_points > 0;

    let throttle_within_available_envelope = !analyses.enforce_throttle_envelope
        || segment
            .conditions
            .throttle
            .iter()
            .all(|throttle| throttle.is_finite() && *throttle >= 0.0 && *throttle <= 1.0);
    // A root the solver could only reach by asking for a force the aeroplane
    // cannot produce is not a flown segment, and that holds at *both* bounds.
    // This was implicit while the command was clamped inside the residual,
    // because no such root existed to find; now that the force is continued
    // outside the deck's domain one does, and the contract has to say so
    // explicitly. This only ever *tightens*: a segment that converged before
    // did so with every command the deck answered directly, so both flags
    // were already false for it.
    //
    // The lower flag is not "the command went negative". A deck's lowest
    // deliverable force is flight idle, which is a *positive* fraction of the
    // rating - 0.075 to 0.092 over a measured A320-200 `descent_1` - so the
    // historical `[0, 1]` envelope declared commands available that no engine
    // will hold. Without this a root found at, say, 0.03 would have been
    // published as converged and inside the envelope while the aeroplane was
    // actually sitting at idle producing 2.5 times the commanded force.
    //
    // Both flags are read **only where the envelope is enforced**, which is
    // what [`MissionAnalyses::enforce_throttle_envelope`] has always said:
    // the frozen SUAVE compatibility path "keeps the reference solver's
    // converged flag even where its historical engine sizing produces
    // throttle above one". Applying the upper flag unconditionally instead
    // turned that fixture's `initial_climb` non-converged, which stopped the
    // frozen mission after two of its twelve segments and made the recorded
    // block fuel 3 954 kg against the reference's 82 816 kg. Product missions
    // are unaffected: they enforce the envelope, so the clause reads exactly
    // as before.
    let converged = solution.status.is_converged()
        && throttle_within_available_envelope
        && (!analyses.enforce_throttle_envelope
            || (!requested_above_limit && !requested_below_limit));
    let throttle_limited = analyses.enforce_throttle_envelope
        && !converged
        && (requested_above_limit
            || segment
                .conditions
                .throttle
                .iter()
                .any(|throttle| throttle.is_finite() && *throttle >= 1.0 - 1.0e-9));
    let idle_floor_limited = !converged && requested_below_limit;
    segment.numerics.converged = Some(converged);
    if !converged {
        tracing::warn!(
            segment = %segment.spec.tag,
            status = ?solution.status,
            evaluations = solution.evaluations,
            "segment did not converge within the solver and propulsion envelopes"
        );
    }

    let peak_throttle = peak_requested_throttle;

    Ok(SegmentSolution {
        converged,
        status: solution.status,
        evaluations: solution.evaluations,
        throttle_limited,
        peak_throttle,
        minimum_throttle: minimum_requested_throttle,
        available_throttle_floor,
        idle_floor_limited,
        sub_idle_points,
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
    /// Whether every field consumed by the mission figures is present and
    /// finite for a completed mission.
    ///
    /// `completed_summary` intentionally validates only scalar mission totals.
    /// The figure family also indexes drag breakdowns and force/aero columns,
    /// so it needs this stricter boundary before flattening conditions.
    pub fn figure_data_ready(&self) -> bool {
        self.completed_summary().is_some()
            && self.segments.iter().all(|segment| {
                let c = &segment.conditions;
                let n = c.len();
                c.time_s.len() == n
                    && c.altitude_m.len() == n
                    && c.total_mass_kg.len() == n
                    && c.velocity_m_s.len() == n
                    && c.density_kg_m3.len() == n
                    && c.mach.len() == n
                    && c.vehicle_mass_rate_kg_s.len() == n
                    && c.aircraft_range_m.len() == n
                    && c.angle_of_attack_rad.len() == n
                    && c.lift_coefficient.len() == n
                    && c.drag_coefficient.len() == n
                    && c.throttle.len() == n
                    && c.body_inertial_rotations_rad.len() == n
                    && c.wind_lift_force_vector_n.len() == n
                    && c.wind_drag_force_vector_n.len() == n
                    && c.thrust_force_vector_n.len() == n
                    && c.drag_breakdown.len() == n
                    && c.time_s
                        .iter()
                        .chain(&c.altitude_m)
                        .chain(&c.total_mass_kg)
                        .chain(&c.velocity_m_s)
                        .chain(&c.density_kg_m3)
                        .chain(&c.mach)
                        .chain(&c.vehicle_mass_rate_kg_s)
                        .chain(&c.aircraft_range_m)
                        .chain(&c.angle_of_attack_rad)
                        .chain(&c.lift_coefficient)
                        .chain(&c.drag_coefficient)
                        .chain(&c.throttle)
                        .all(|value| value.is_finite())
                    && c.body_inertial_rotations_rad
                        .iter()
                        .chain(&c.wind_lift_force_vector_n)
                        .chain(&c.wind_drag_force_vector_n)
                        .chain(&c.thrust_force_vector_n)
                        .all(|vector| vector.iter().all(|value| value.is_finite()))
                    && c.drag_breakdown.iter().all(|drag| {
                        [
                            drag.parasite_total,
                            drag.induced_total,
                            drag.compressible_total,
                            drag.miscellaneous_total,
                            drag.total,
                        ]
                        .iter()
                        .all(|value| value.is_finite())
                    })
            })
    }

    /// Why [`Self::completed_summary`] would refuse, or `None` when it would
    /// not refuse.
    ///
    /// The refusal itself is a plain `Option`, which is the right shape for a
    /// caller that only needs the totals and the wrong shape for a caller that
    /// has to tell a reader what went wrong. The fuel-policy closure is the
    /// second kind: when it cannot price a policy it reports only "the route
    /// could not be flown at N kg", and the surviving evidence then says
    /// nothing about whether the aeroplane ran out of fuel, sat on its
    /// throttle stop, or failed to converge — three findings with three
    /// different owners. This names the **cause**, not the symptom.
    ///
    /// Ordering matters here and is deliberately *not* the order
    /// [`Self::completed_summary`] tests its conditions in. `fly` breaks out
    /// of the segment loop as soon as a segment exhausts fuel or fails to
    /// converge, so a short segment list is a *consequence* of that break. A
    /// refusal that reported the count first would say "only 11 of 12
    /// scheduled segments produced a solution" and hide the segment that
    /// actually failed — which is what the first version of this function
    /// did, measured on the AVE finalist. The failing segment is therefore
    /// examined before the count.
    ///
    /// No gate moves: every condition below is one `completed_summary`
    /// already refuses on.
    pub fn completion_refusal(&self) -> Option<String> {
        if self.scheduled_segment_count == 0 {
            return Some("no mission segment was scheduled".to_owned());
        }
        if let Some(exhaustion) = &self.fuel_exhaustion {
            return Some(format!(
                "fuel was exhausted flying segment {} of {} ({}): {:.1} kg burned of {:.1} kg available",
                exhaustion.segment_index + 1,
                self.scheduled_segment_count,
                exhaustion.segment_tag,
                exhaustion.burned_fuel_kg,
                exhaustion.available_fuel_kg
            ));
        }
        // The segment the loop stopped on, named with the reason it stopped.
        if let Some((index, solution)) = self.solutions.iter().enumerate().find(|(_, solution)| {
            !solution.converged || solution.throttle_limited || solution.idle_floor_limited
        }) {
            let tag = self
                .segments
                .get(index)
                .map_or("<unknown>", |segment| segment.spec.tag.as_str());
            // The idle floor is named before the upper stop because the two
            // can coincide on a segment whose solve wandered across both, and
            // the lower one is the more specific statement: it says the
            // commanded trajectory needs *less* force than the engine will
            // deliver at flight idle, which is a schedule or a drag-device
            // question rather than a thrust shortfall.
            if solution.idle_floor_limited {
                let points = self
                    .segments
                    .get(index)
                    .map_or(solution.sub_idle_points, |segment| segment.conditions.len());
                // No magnitude of the *command* is quoted. It is an
                // out-of-domain iterate: the force continuation that lets the
                // root find escape the flat band below the floor also lets it
                // settle anywhere outside the deck's domain, and the same
                // A320-200 `descent_1` was measured stopping at -7.3, -37.6
                // and -173.9 on three variants of that continuation. Quoting
                // it would put a number that depends only on the search path
                // in front of a reader who would read it as a shortfall.
                return Some(format!(
                    "segment {} of {} ({tag}) required less force than the engine delivers at \
                     flight idle, at {} of {points} control points: the deck's own idle floor \
                     is {:.3} of the rating there ({:?}, {} residual evaluations)",
                    index + 1,
                    self.scheduled_segment_count,
                    solution.sub_idle_points,
                    solution.available_throttle_floor,
                    solution.status,
                    solution.evaluations,
                ));
            }
            let cause = if solution.throttle_limited {
                "reached the commanded-throttle boundary before force balance converged"
            } else {
                "did not converge"
            };
            return Some(format!(
                "segment {} of {} ({tag}) {cause} ({:?}, {} residual evaluations, peak throttle {:.3})",
                index + 1,
                self.scheduled_segment_count,
                solution.status,
                solution.evaluations,
                solution.peak_throttle
            ));
        }
        if self.segments.len() != self.scheduled_segment_count
            || self.solutions.len() != self.scheduled_segment_count
        {
            return Some(format!(
                "only {} of {} scheduled segments produced a solution",
                self.solutions.len().min(self.segments.len()),
                self.scheduled_segment_count
            ));
        }
        if self.segments.iter().any(|segment| {
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
        }) {
            return Some("a segment produced too few or non-finite samples".to_owned());
        }
        None
    }

    /// Return scalar mission results only when all requested segments completed.
    ///
    /// Partial, exhausted, unconverged, throttle-limited, or non-finite
    /// telemetry deliberately produces `None`; [`Self::completion_refusal`]
    /// names which of those it was.
    pub fn completed_summary(&self) -> Option<CompletedMissionSummary> {
        if self.scheduled_segment_count == 0
            || self.fuel_exhaustion.is_some()
            || self.segments.len() != self.scheduled_segment_count
            || self.solutions.len() != self.scheduled_segment_count
            || self.solutions.iter().any(|solution| {
                !solution.converged || solution.throttle_limited || solution.idle_floor_limited
            })
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

    /// Every refusal `completed_summary` makes must be attributable.
    ///
    /// The fuel-policy closure reports this refusal to the user as the reason
    /// a design was not certified, so "could not be flown" without a cause is
    /// an unactionable finding. The two must also stay in step: a refusal with
    /// no reason, or a reason with no refusal, is the pair drifting apart.
    #[test]
    fn a_refused_mission_summary_always_names_the_condition_it_refused_on() {
        let empty_schedule = MissionResult {
            segments: Vec::new(),
            solutions: Vec::new(),
            scheduled_segment_count: 0,
            fuel_exhaustion: None,
        };
        let partial = MissionResult {
            segments: Vec::new(),
            solutions: Vec::new(),
            scheduled_segment_count: 1,
            fuel_exhaustion: None,
        };
        for result in [&empty_schedule, &partial] {
            assert_eq!(result.completed_summary(), None);
            let refusal = result
                .completion_refusal()
                .unwrap_or_else(|| panic!("a refused summary names its condition"));
            assert!(!refusal.is_empty());
        }
        assert!(
            empty_schedule
                .completion_refusal()
                .is_some_and(|reason| reason.contains("segment was scheduled")),
            "an empty schedule is reported as an empty schedule"
        );
        assert!(
            partial
                .completion_refusal()
                .is_some_and(|reason| reason.contains("of 1 scheduled segments")),
            "a partial mission with no solved segment is reported by how much of it is missing"
        );
    }

    /// The refusal must name the segment that failed, not the short list that
    /// failing produced.
    ///
    /// `fly` breaks out of the segment loop the moment a segment does not
    /// converge, so a truncated solution list is the *consequence*. Reporting
    /// the count first is what the first version of this function did, and on
    /// the AVE finalist it produced "only 11 of 12 scheduled segments produced
    /// a solution" — true, and silent about which segment failed and why. A
    /// reader of that finding cannot tell a throttle stop from a root-finder
    /// failure, and those have different owners.
    #[test]
    fn a_truncated_mission_is_reported_by_the_segment_that_stopped_it() {
        let solved = SegmentSolution {
            converged: true,
            status: Status::Converged,
            evaluations: 12,
            throttle_limited: false,
            peak_throttle: 0.42,
            minimum_throttle: 0.31,
            available_throttle_floor: f64::NAN,
            idle_floor_limited: false,
            sub_idle_points: 0,
        };
        let stalled = SegmentSolution {
            converged: false,
            status: Status::MaxEvaluations,
            evaluations: 400,
            throttle_limited: false,
            peak_throttle: 0.87,
            minimum_throttle: 0.55,
            available_throttle_floor: f64::NAN,
            idle_floor_limited: false,
            sub_idle_points: 0,
        };
        let result = MissionResult {
            segments: Vec::new(),
            solutions: vec![solved, stalled],
            scheduled_segment_count: 12,
            fuel_exhaustion: None,
        };

        assert_eq!(result.completed_summary(), None);
        let refusal = result
            .completion_refusal()
            .unwrap_or_else(|| panic!("a refused summary names its condition"));
        assert!(
            refusal.contains("segment 2 of 12") && refusal.contains("did not converge"),
            "the refusal must name the failing segment and its cause: {refusal}"
        );
        assert!(
            refusal.contains("MaxEvaluations"),
            "the root-finder exit is what distinguishes a budget from a stall: {refusal}"
        );
        assert!(
            !refusal.contains("only 1 of 12"),
            "the truncated list is the consequence, not the reported cause: {refusal}"
        );
    }

    /// The reported peak must be the throttle the solver *asked for*.
    ///
    /// `converge_root`'s throttle unknowns are clamped into `[0, 1]` after the
    /// root find rather than inside the residual closure, so the published
    /// peak of `conditions.throttle` is exactly `1.000` whenever the clamp was
    /// active
    /// — for a 2 % shortfall and a 200 % one alike. Reading it from there is
    /// what the first version of this field did, and the measured AVE case
    /// duly reported `peak throttle 1.000`, which cannot discriminate a
    /// marginal thrust deficit from a diverging root find. A value above one
    /// must therefore be representable in this field and must survive into
    /// the refusal text.
    #[test]
    fn the_reported_peak_throttle_is_the_request_and_can_exceed_the_limit() {
        let over = SegmentSolution {
            converged: false,
            status: Status::NoProgressSinceIterations,
            evaluations: 152,
            throttle_limited: true,
            peak_throttle: 1.37,
            minimum_throttle: 0.91,
            available_throttle_floor: f64::NAN,
            idle_floor_limited: false,
            sub_idle_points: 0,
        };
        let result = MissionResult {
            segments: Vec::new(),
            solutions: vec![over],
            scheduled_segment_count: 12,
            fuel_exhaustion: None,
        };

        let refusal = result
            .completion_refusal()
            .unwrap_or_else(|| panic!("a refused summary names its condition"));
        assert!(
            refusal.contains("peak throttle 1.370"),
            "the request must reach the reader, not the clamp: {refusal}"
        );
    }

    /// A lower-bound refusal must report the force, not the iterate.
    ///
    /// The A320-200's measured blocker reported *"did not converge
    /// (NoProgressSinceIterations, 257 residual evaluations, peak throttle
    /// -44.505)"*, and not one number in that sentence survives inspection.
    /// "Peak" is the maximum, so on a segment whose whole request is below
    /// the floor it names the *least* violating point. The magnitude is
    /// wherever the search happened to stop: the same aeroplane and the same
    /// rung were measured stopping at -7.3, -37.6 and -173.9 on three
    /// variants of the same continuation. And nothing in it says what the
    /// aeroplane could not do. The refusal therefore quotes the boundary and
    /// how much of the rung crossed it, and quotes no command at all.
    #[test]
    fn a_sub_idle_refusal_reports_the_boundary_and_not_the_iterate() {
        let floored = SegmentSolution {
            converged: false,
            status: Status::NoProgressSinceIterations,
            evaluations: 257,
            throttle_limited: false,
            peak_throttle: -0.031,
            minimum_throttle: -0.184,
            available_throttle_floor: 0.0841,
            idle_floor_limited: true,
            sub_idle_points: 16,
        };
        let result = MissionResult {
            segments: Vec::new(),
            solutions: vec![floored],
            scheduled_segment_count: 12,
            fuel_exhaustion: None,
        };

        assert_eq!(result.completed_summary(), None);
        let refusal = result
            .completion_refusal()
            .unwrap_or_else(|| panic!("a refused summary names its condition"));
        assert!(
            refusal.contains("flight idle"),
            "the cause is the engine's own floor, not a generic stall: {refusal}"
        );
        assert!(
            refusal.contains("idle floor is 0.084") && refusal.contains("at 16 of 16"),
            "the boundary and how much of the rung crossed it must reach the reader: {refusal}"
        );
        assert!(
            !refusal.contains("peak throttle")
                && !refusal.contains("-0.184")
                && !refusal.contains("-0.031"),
            "an out-of-domain iterate is not the finding and must not be quoted: {refusal}"
        );
    }

    /// A segment sitting on the idle floor is not a flown segment even when
    /// the root finder reports success.
    ///
    /// This is the silent case the lower bound closed. Every command below
    /// the floor produces the identical force, so a root can be found at a
    /// command the engine will never hold - and before the floor was known,
    /// `converged` and the `[0, 1]` envelope check both accepted it, so a
    /// published mission could show a throttle of 0.03 next to the thrust of
    /// 0.084.
    #[test]
    fn a_root_below_the_idle_floor_cannot_publish_a_completed_summary() {
        let floored = SegmentSolution {
            converged: false,
            status: Status::Converged,
            evaluations: 41,
            throttle_limited: false,
            peak_throttle: 0.031,
            minimum_throttle: 0.029,
            available_throttle_floor: 0.0841,
            idle_floor_limited: true,
            sub_idle_points: 0,
        };
        let result = MissionResult {
            segments: Vec::new(),
            solutions: vec![floored],
            scheduled_segment_count: 1,
            fuel_exhaustion: None,
        };

        assert_eq!(result.completed_summary(), None);
        assert!(result
            .completion_refusal()
            .is_some_and(|refusal| refusal.contains("flight idle")));
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
