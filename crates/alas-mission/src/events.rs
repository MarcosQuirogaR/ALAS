// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Flying a mission whose mass changes between segments.
//!
//! A payload drop, a launched store, an air-to-air refuelling or an offload
//! changes the mass the next segment starts from, in one instant. A segment is
//! solved from the state the one before it handed forward, so the whole
//! coupling is a change of that state's mass: [`Mission::evaluate_with_changes`]
//! applies one [`MassChange`] before each segment that has one.

use crate::segments::{Initials, MissionAnalyses, Segment};
use crate::solve::{converge_root, FuelExhaustion, Mission, MissionError, MissionResult};

/// A change of the aircraft's mass at a segment boundary.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MassChange {
    /// Mass that is not fuel added (positive) or removed (negative), kg: a
    /// store hung on, a pallet dropped. The lowest reachable mass of the
    /// fuel-exhaustion check moves by it.
    pub dry_kg: f64,
    /// Fuel received (positive) or given away (negative), kg, in flight.
    pub fuel_kg: f64,
}

impl Mission {
    /// Fly the mission with `changes[i]` applied to the mass the segment `i`
    /// starts from. A change before the first segment is not applied here (the
    /// first segment starts from `analyses.takeoff_mass_kg`, which the caller
    /// sets); a missing or zero entry changes nothing, so an empty slice is
    /// [`Mission::evaluate`].
    ///
    /// The fuel-exhaustion check follows the changes: the floor
    /// `analyses.minimum_mass_kg` is moved by the dry mass added so far, and
    /// the fuel it reports as available and burned counts the fuel received.
    ///
    /// # Errors
    ///
    /// As [`Mission::evaluate`].
    pub fn evaluate_with_changes(
        &self,
        analyses: &MissionAnalyses,
        changes: &[MassChange],
    ) -> Result<MissionResult, MissionError> {
        let mut segments = Vec::with_capacity(self.schedule.len());
        let mut solutions = Vec::with_capacity(self.schedule.len());
        let mut initials: Option<Initials> = None;
        let mut fuel_exhaustion = None;
        let (mut dry_delta_kg, mut fuel_delta_kg) = (0.0, 0.0);

        for (segment_index, spec) in self.schedule.iter().enumerate() {
            let tag = spec.tag.clone();
            if let (Some(change), Some(state)) = (changes.get(segment_index), initials.as_mut()) {
                state.total_mass_kg += change.dry_kg + change.fuel_kg;
                dry_delta_kg += change.dry_kg;
                fuel_delta_kg += change.fuel_kg;
            }
            let mut segment =
                Segment::new(spec.clone(), initials).map_err(|source| MissionError::Setup {
                    tag: tag.clone(),
                    source,
                })?;
            let solution = converge_root(&mut segment, analyses)?;
            segment.finalize();

            if let Some(floor_kg) = analyses.minimum_mass_kg {
                let minimum_mass_kg = floor_kg + dry_delta_kg;
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
                        available_fuel_kg: analyses.takeoff_mass_kg - floor_kg + fuel_delta_kg,
                        burned_fuel_kg: analyses.takeoff_mass_kg + dry_delta_kg + fuel_delta_kg
                            - crossing_mass_kg,
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
