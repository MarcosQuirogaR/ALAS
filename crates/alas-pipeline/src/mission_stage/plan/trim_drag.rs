// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The trim drag a plan pays for where its centre of gravity sits.
//!
//! The drag the mission flies on is trimmed at one centre of gravity (the
//! table's own, [`alas_opt::mdo::CandidateFuelArtifacts::trim_cg_x_m`]). A plan moves
//! the centre of gravity: the burn drifts it, a release or a refuelling jumps
//! it, and a trim tank holds it. The tail load that holds the pitching moment
//! changes with it, and with the tail load the induced drag of wing and tail
//! (see [`alas_opt::mdo::CgTrimDrag`] for the sources and the method). This reads the
//! centre of gravity of each flown leg from the tank history, finds the
//! increment of the trimmed induced drag at the leg's mean lift coefficient and
//! centre of gravity, and hands it back as a drag coefficient per leg for the
//! next flight.

use std::collections::HashMap;

use alas_config::AlasConfig;
use alas_mission::MissionResult;
use alas_opt::mdo::cg_trim_drag::CgTrimDrag;
use alas_opt::mdo::CandidateDrag;

use super::super::schedule::junction::family;
use super::super::schedule::speed_change::SPEED_CHANGE_PREFIX;
use crate::full_analysis::AnalysisReport;

/// The key of the leg a flown piece belongs to, as the plan built it: the tag
/// up to the stage number of `name@NN`. The guidance cuts a leg into pieces
/// after the increments are assigned (`..._upper`, `..._lower`, `..._lower_2`)
/// and the pieces inherit the leg's increment, so they all answer to its key.
pub(super) fn leg_key(tag: &str) -> &str {
    let whole = family(tag);
    if let Some(at) = whole.find('@') {
        let digits = whole[at + 1..]
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        return &whole[..at + 1 + digits];
    }
    // A tag with no stage number (a hand-built schedule): cut at the first piece mark.
    let cut = ["_upper", "_lower"]
        .iter()
        .filter_map(|mark| whole.find(mark))
        .min()
        .unwrap_or(whole.len());
    &whole[..cut]
}

/// What became of the trim-drag correction of a plan.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum TrimDragStatus {
    /// Not looked at: the plan was not flown to its end.
    #[default]
    NotChecked,
    /// The centre of gravity stays within the retrim tolerance of the one the
    /// drag was trimmed at: the drag stands as it is.
    WithinReference,
    /// The legs carry the increment of the trimmed induced drag at their own
    /// centre of gravity.
    Applied,
    /// The aircraft's drag is an external solver's polar: it has no centre of
    /// gravity dependence the plan could follow.
    ExternalPolar,
    /// The aircraft cannot be trimmed at a centre of gravity the plan reaches.
    CannotTrim(String),
}

/// The trim-drag correction of a flown plan.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrimDragReport {
    /// What became of it.
    pub status: TrimDragStatus,
    /// The station the drag was trimmed at, m (geometry axes).
    pub reference_cg_x_m: f64,
    /// Largest increment added to a leg, drag coefficient (reference area).
    pub max_increment_cd: f64,
    /// Whether the last pass still changed a leg's increment by a fifth of a drag
    /// count or more: the fuel was closed with a trim drag that had not settled.
    pub unsettled: bool,
    /// How many passes of the correction the plan took (each after the first
    /// flies the plan again with the increments of the pass before).
    pub passes: usize,
    /// The increment each leg of the flown plan carried, drag coefficient, by
    /// the leg's key (see [`TrimDragReport::increment_cd`]). The speed-change
    /// legs have no key of their own: they fly the mean of the configuration
    /// drag of the legs on either side, trim included, and read zero here.
    pub increments: HashMap<String, f64>,
    /// The increment of the trimmed induced drag against the centre-of-gravity
    /// station at the lift coefficient of the first cruise leg: `[station m,
    /// drag coefficient]`, from the most forward station of the plan to the most
    /// aft, so the trim drag a centre of gravity costs or saves can be read.
    pub curve: Vec<[f64; 2]>,
}

/// Mean increment of every leg, weighted by the time each of its pieces lasts:
/// `pieces` are `(tag, increment, duration s)`. The speed-change legs are left
/// out: they are not assigned an increment of their own (the junction takes the
/// mean of the legs on either side), so the key the flight carries for them is not
/// one the increments are looked up by, and comparing it would unsettle a run
/// that has settled.
fn weighted_by_leg<'a>(
    pieces: impl IntoIterator<Item = (&'a str, f64, f64)>,
) -> HashMap<String, f64> {
    let mut sums: HashMap<String, (f64, f64)> = HashMap::new();
    for (tag, increment, duration_s) in pieces {
        if tag.starts_with(SPEED_CHANGE_PREFIX) {
            continue;
        }
        let entry = sums.entry(leg_key(tag).to_owned()).or_insert((0.0, 0.0));
        entry.0 += increment * duration_s;
        entry.1 += duration_s;
    }
    sums.into_iter()
        .map(|(key, (sum, duration_s))| (key, sum / duration_s))
        .collect()
}

impl TrimDragReport {
    /// The increment the flown piece `tag` carried, drag coefficient (zero when
    /// the leg carried none).
    #[must_use]
    pub fn increment_cd(&self, tag: &str) -> f64 {
        self.increments.get(leg_key(tag)).copied().unwrap_or(0.0)
    }
}

/// The increments to add to the legs, by leg family, and how they came about.
pub(super) struct Correction {
    pub by_family: HashMap<String, f64>,
    pub report: TrimDragReport,
}

/// The increment of every leg family of `result` given the centre of gravity
/// `leg_cg_x_m` of each of its legs. `tables` keeps the trimmed tables from
/// one call to the next.
pub(super) fn correction(
    config: &AlasConfig,
    report: &AnalysisReport,
    tables: &mut Option<CgTrimDrag>,
    result: &MissionResult,
    leg_cg_x_m: &[[f64; 2]],
) -> Correction {
    let mut out = Correction {
        by_family: HashMap::new(),
        report: TrimDragReport::default(),
    };
    let Ok(artifacts) = report.fuel.artifacts(config, &report.design) else {
        return out;
    };
    let reference = artifacts.trim_cg_x_m;
    out.report.reference_cg_x_m = reference;
    let means: Vec<f64> = leg_cg_x_m
        .iter()
        .map(|[start, end]| 0.5 * (start + end))
        .collect();
    let finite = || means.iter().copied().filter(|x| x.is_finite());
    let lowest = finite().fold(f64::INFINITY, f64::min);
    let highest = finite().fold(f64::NEG_INFINITY, f64::max);
    if !lowest.is_finite() {
        return out;
    }
    let tolerance_m =
        config.optimizer.objective.retrim_cg_tolerance_pct_mac / 100.0 * report.airplane.c_ref;
    if (lowest - reference).abs() <= tolerance_m && (highest - reference).abs() <= tolerance_m {
        out.report.status = TrimDragStatus::WithinReference;
        return out;
    }
    let outside = |tables: &CgTrimDrag| {
        let [low, high] = tables.station_range_m();
        lowest < low - 1.0e-9 || highest > high + 1.0e-9
    };
    // A later flight can reach stations beyond the ones the first showed: the
    // tables are then built again over the wider range, not extrapolated.
    if tables.as_ref().is_some_and(outside) {
        *tables = None;
    }
    if tables.is_none() {
        let mass_kg = cruise_mass_kg(result);
        match CgTrimDrag::build(
            config,
            &artifacts,
            &report.airplane,
            &report.design,
            [lowest, highest],
            mass_kg,
        ) {
            Ok(built) => *tables = Some(built),
            Err(message) => {
                out.report.status = if matches!(artifacts.drag, CandidateDrag::External(_)) {
                    TrimDragStatus::ExternalPolar
                } else {
                    TrimDragStatus::CannotTrim(message)
                };
                return out;
            }
        }
    }
    let Some(tables) = tables.as_ref() else {
        return out;
    };
    let cruise_mach = config.requirements.cruise_mach;
    let mut pieces = Vec::new();
    for (segment, station) in result.segments.iter().zip(&means) {
        let lift = &segment.conditions.lift_coefficient;
        if !station.is_finite() || lift.is_empty() {
            continue;
        }
        let mean_cl = lift.iter().sum::<f64>() / lift.len() as f64;
        let mach = &segment.conditions.mach;
        let mean_mach = if mach.is_empty() {
            cruise_mach
        } else {
            mach.iter().sum::<f64>() / mach.len() as f64
        };
        let increment = tables.increment_cd(mean_cl, mean_mach, *station);
        let time = &segment.conditions.time_s;
        let duration_s = match (time.first(), time.last()) {
            (Some(first), Some(last)) if last > first => last - first,
            _ => 1.0,
        };
        pieces.push((segment.spec.tag.as_str(), increment, duration_s));
    }
    for (key, mean) in weighted_by_leg(pieces) {
        out.report.max_increment_cd = out.report.max_increment_cd.max(mean.abs());
        out.by_family.insert(key, mean);
    }
    let cruise_cl = result
        .segments
        .iter()
        .find(|segment| segment.spec.tag.starts_with("cruise"))
        .map_or(0.5, |segment| {
            let lift = &segment.conditions.lift_coefficient;
            lift.iter().sum::<f64>() / lift.len().max(1) as f64
        });
    out.report.curve = (0..=SAMPLES)
        .map(|i| {
            let station = lowest + (highest - lowest) * i as f64 / SAMPLES as f64;
            [
                station,
                tables.increment_cd(cruise_cl, cruise_mach, station),
            ]
        })
        .collect();
    out.report.status = TrimDragStatus::Applied;
    out
}

/// Stations of the reported increment curve, beyond the first.
const SAMPLES: usize = 4;

/// The mass the cruise is flown at, kg: the start of the first cruise leg, else
/// of the first leg.
fn cruise_mass_kg(result: &MissionResult) -> f64 {
    let leg = result
        .segments
        .iter()
        .find(|segment| segment.spec.tag.starts_with("cruise"))
        .or_else(|| result.segments.first());
    leg.and_then(|segment| segment.conditions.total_mass_kg.first().copied())
        .unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::{leg_key, weighted_by_leg};

    #[test]
    fn the_pieces_of_a_leg_are_time_weighted_into_one_key_and_speed_changes_are_left_out() {
        let by_leg = weighted_by_leg([
            ("cruise@03", 1.0e-4, 3_000.0),
            ("cruise@03_upper", 4.0e-4, 1_000.0),
            ("cruise@03_upper_upper", 8.0e-4, 1_000.0),
            ("speed_change_to_cruise@03", 9.9e-4, 120.0),
            ("climb@02", 2.0e-5, 600.0),
        ]);
        assert_eq!(by_leg.len(), 2, "{by_leg:?}");
        let cruise = (1.0e-4 * 3_000.0 + 4.0e-4 * 1_000.0 + 8.0e-4 * 1_000.0) / 5_000.0;
        assert!((by_leg["cruise@03"] - cruise).abs() < 1.0e-15);
        assert_eq!(by_leg["climb@02"], 2.0e-5);
        assert!(by_leg.keys().all(|key| !key.starts_with("speed_change")));
    }

    #[test]
    fn every_piece_the_guidance_cuts_from_a_leg_answers_to_the_key_of_the_leg() {
        for tag in [
            "step_climb_1@02",
            "step_climb_1@02_upper",
            "step_climb_1@02_upper_upper",
            "step_climb_1@02_upper_upper_upper_upper",
            "step_climb_1@02_lower",
            "step_climb_1@02_lower_2",
            "step_climb_1@02_path_entry",
            "step_climb_1@02_upper_path_exit",
        ] {
            assert_eq!(leg_key(tag), "step_climb_1@02", "{tag}");
        }
        assert_eq!(leg_key("descent_3@07_lower_2"), "descent_3@07");
        assert_eq!(leg_key("cruise@03"), "cruise@03");
        // Different legs and different stages keep different keys.
        assert_ne!(leg_key("step_climb_1@02_upper"), leg_key("step_climb_2@02"));
        assert_ne!(leg_key("cruise@03_upper"), leg_key("cruise@13"));
        // A hand-built tag with no stage number is cut at the first piece mark.
        assert_eq!(leg_key("initial_climb_upper_upper"), "initial_climb");
        assert_eq!(leg_key("takeoff"), "takeoff");
    }
}
