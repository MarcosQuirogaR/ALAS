// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Whether the propulsion deck answered the points of a flown mission inside
//! its evidence domain.
//!
//! Every deck evaluation carries the deck's own validity statement
//! ([`alas_prop::system::ValidityStatus`]). The flight reads force and fuel
//! flow only, so the statement is read again here at the control points of
//! the converged segments and summed up for the run: thrust and fuel that
//! rest on an extrapolated deck are then said to do so.

use alas_mission::segments::MissionAnalyses;
use alas_mission::MissionResult;

/// The propulsion deck's validity over the flown mission points.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PropulsionDeckValidity {
    /// No flight was read (none was flown, or none of its segments converged).
    #[default]
    NotEvaluated,
    /// Every flown point lies inside the deck's evidence domain.
    Valid {
        /// Control points read.
        points: usize,
    },
    /// At least one flown point lies outside the deck's evidence domain.
    Extrapolated {
        /// Control points read.
        points: usize,
        /// Control points the deck answered by extrapolation.
        extrapolated_points: usize,
        /// Tags of the segments holding such a point, in flight order.
        segments: Vec<String>,
        /// The deck's reason at the first such point.
        first_reason: String,
    },
    /// The deck could not be read again at a flown point, so its validity
    /// there is not known.
    Unknown {
        /// Tag of the first segment with such a point.
        segment: String,
        /// The deck's error at that point.
        reason: String,
    },
}

impl PropulsionDeckValidity {
    /// Read the deck's validity at every control point of the converged
    /// segments of `result`, and record it per point on the result
    /// (`Conditions::propulsion_extrapolated`). The summary below counts those
    /// same flags: `extrapolated_points` is the number of `true` flags.
    pub(super) fn of_flight(result: &mut MissionResult, analyses: &MissionAnalyses) -> Self {
        let mut points = 0_usize;
        let mut extrapolated_points = 0_usize;
        let mut segments: Vec<String> = Vec::new();
        let mut first_reason: Option<String> = None;
        let mut unknown: Option<(String, String)> = None;
        for index in 0..result.segments.len() {
            if !result.solutions.get(index).is_some_and(|s| s.converged) {
                continue;
            }
            let statuses = result.segments[index].propulsion_validity(analyses);
            let mut flags = vec![false; statuses.len()];
            for (point, status) in statuses.into_iter().enumerate() {
                points += 1;
                let segment = &result.segments[index];
                match status {
                    Ok(status) => {
                        if let Some(reason) = status.extrapolation_reason() {
                            flags[point] = true;
                            extrapolated_points += 1;
                            if segments.last() != Some(&segment.spec.tag) {
                                segments.push(segment.spec.tag.clone());
                            }
                            first_reason.get_or_insert_with(|| reason.to_owned());
                        }
                    }
                    Err(error) => {
                        unknown
                            .get_or_insert_with(|| (segment.spec.tag.clone(), error.to_string()));
                    }
                }
            }
            result.segments[index].conditions.propulsion_extrapolated = flags;
        }
        if let Some(first_reason) = first_reason {
            return Self::Extrapolated {
                points,
                extrapolated_points,
                segments,
                first_reason,
            };
        }
        if let Some((segment, reason)) = unknown {
            return Self::Unknown { segment, reason };
        }
        if points == 0 {
            Self::NotEvaluated
        } else {
            Self::Valid { points }
        }
    }

    /// The warning text of a flight that is not known to lie inside the
    /// deck's evidence domain; `None` for a valid or unread flight.
    pub fn warning(&self, flight: &str) -> Option<String> {
        match self {
            Self::NotEvaluated | Self::Valid { .. } => None,
            Self::Extrapolated {
                points,
                extrapolated_points,
                segments,
                first_reason,
            } => Some(format!(
                "{flight} flew {extrapolated_points} of {points} control points outside the propulsion deck's evidence domain (segments: {}): thrust and fuel flow there are extrapolated; first reason: {first_reason}",
                segments.join(", ")
            )),
            Self::Unknown { segment, reason } => Some(format!(
                "{flight}: the propulsion deck's validity could not be read at a flown point of segment {segment}: {reason}"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_extrapolated_or_unread_flight_has_a_warning() {
        assert_eq!(
            PropulsionDeckValidity::NotEvaluated.warning("the flight"),
            None
        );
        assert_eq!(
            PropulsionDeckValidity::Valid { points: 32 }.warning("the flight"),
            None
        );
        let extrapolated = PropulsionDeckValidity::Extrapolated {
            points: 32,
            extrapolated_points: 3,
            segments: vec!["climb_1".to_owned(), "cruise_step_1".to_owned()],
            first_reason: "Mach above the deck".to_owned(),
        };
        let text = extrapolated
            .warning("the flight")
            .unwrap_or_else(|| panic!("an extrapolated flight has a warning"));
        assert!(text.contains("3 of 32"), "{text}");
        assert!(text.contains("climb_1, cruise_step_1"), "{text}");
        assert!(text.contains("Mach above the deck"), "{text}");
        let unknown = PropulsionDeckValidity::Unknown {
            segment: "descent_1".to_owned(),
            reason: "invalid input".to_owned(),
        };
        assert!(unknown
            .warning("the flight")
            .is_some_and(|text| text.contains("descent_1")));
    }
}
