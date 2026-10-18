// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Level-flight thrust at the cruise levels the mission plan flies.
//!
//! The design cruise altitude of the requirements is the representative
//! (mid-cruise) level of a step-climbing aircraft, not a level the mission
//! flies: the planner (`optimum_step` policy) starts at the highest level
//! that holds the 300 ft/min residual climb at the top-of-climb mass, which
//! for a heavy aircraft is below the design level, and steps up as fuel
//! burns. Owner decision 2026-10-14: the `cruise_thrust` check judges the
//! levels the plan actually flies. Every level cruise segment of the
//! closure's frozen plan (initial level and each step level), at the mass the
//! mission model reaches it with and the true airspeed it commands (Mach
//! `tas / a(h)`, ISA), must be flyable at the maximum-cruise rating; the
//! constraint is the worst one. Within a level the mass only falls, so the
//! first segment of the level, the heaviest, is the binding one.
//!
//! The requirement per level is the same formula as the design-level check
//! (`required_sls_tw_level`, `(D/W)(m/m_TO)/r`, with the deck's
//! maximum-cruise ratio `r` and the candidate drag at the level's `CL` and
//! Mach), so the check differs only in the flight states it is evaluated at.
//! The states are on the day the mission flies (ISA plus the departure
//! airport's deviation, `SegmentMissionModel::isa_deviation_c`): the mission
//! applies it to every segment, so the Mach of the commanded true airspeed,
//! the density behind the lift coefficient and the deck thrust are those of
//! that day, the same state the mission planner's ceiling evaluates. The
//! design-level fallback stays on the ISA day.

use alas_config::AlasConfig;

use super::cruise_deck::{ambient, deck_cruise_at, DeckCruiseRequirements};
use super::oei_deck::EngineOutModel;
use crate::mdo::sizing::candidate_model::candidate_mission_model;
use crate::mdo::types::{CandidateDrag, CandidateFuelArtifacts};

/// One cruise level of the plan with its level-flight requirement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlannedCruiseLevel {
    /// Level, m.
    pub altitude_m: f64,
    /// Cruise Mach the plan commands at the level (on the mission's day).
    pub mach: f64,
    /// Temperature deviation from ISA of the day the mission flies, K.
    pub isa_deviation_c: f64,
    /// Mass on reaching the level (the heaviest at it), kg.
    pub mass_kg: f64,
    /// Required installed sea-level-static `T/W` over the take-off weight.
    pub required_tw: f64,
}

/// The level-flight requirement over the levels the mission plan flies.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedCruise {
    /// The deck requirement at the governing (worst) level; its `level_tw`
    /// is the constraint value.
    pub worst: DeckCruiseRequirements,
    /// The governing level.
    pub worst_level: PlannedCruiseLevel,
    /// Every distinct level the plan flies, in flight order.
    pub levels: Vec<PlannedCruiseLevel>,
}

impl PlannedCruise {
    /// The initial cruise level of the plan.
    pub fn initial(&self) -> PlannedCruiseLevel {
        self.levels[0]
    }

    /// A one-line description of the levels checked and the governing one.
    pub fn describe(&self) -> String {
        let worst = self.worst_level;
        format!(
            "the plan's {} cruise level(s) (initial {:.0} m, governing {:.0} m at Mach {:.3} and {:.0} kg)",
            self.levels.len(),
            self.initial().altitude_m,
            worst.altitude_m,
            worst.mach,
            worst.mass_kg
        )
    }
}

/// The level-flight requirement at every level the candidate's frozen plan
/// flies from `takeoff_mass_kg`, on the wing of `s_ref_m2`.
///
/// # Errors
///
/// No frozen plan, the model or trip rejection, an unusable state, or the
/// deck rejection, as text. The caller then falls back to the design level.
pub fn planned_cruise_requirements(
    model: &EngineOutModel,
    config: &AlasConfig,
    artifacts: &CandidateFuelArtifacts,
    takeoff_mass_kg: f64,
    s_ref_m2: f64,
) -> Result<PlannedCruise, String> {
    let mission = candidate_mission_model(config, artifacts)?;
    let states = mission.planned_cruise_states(takeoff_mass_kg)?;
    let gravity_m_s2 = config.requirements.gravity_m_s2;
    let isa_deviation_c = mission.isa_deviation_c;
    let mut seen = std::collections::HashSet::new();
    let mut levels = Vec::new();
    let mut worst: Option<(DeckCruiseRequirements, PlannedCruiseLevel)> = None;
    for state in states {
        if !seen.insert((state.altitude_m.to_bits(), state.tas_m_s.to_bits())) {
            continue;
        }
        let mach = state.tas_m_s / ambient(state.altitude_m, isa_deviation_c).1;
        let deck = deck_cruise_at(
            model,
            &artifacts.drag,
            gravity_m_s2,
            (state.altitude_m, mach, isa_deviation_c),
            (takeoff_mass_kg, state.mass_kg),
            s_ref_m2,
        )?;
        let level = PlannedCruiseLevel {
            altitude_m: state.altitude_m,
            mach,
            isa_deviation_c,
            mass_kg: state.mass_kg.min(takeoff_mass_kg),
            required_tw: deck.level_tw,
        };
        if worst.is_none_or(|(_, w)| level.required_tw > w.required_tw) {
            worst = Some((deck, level));
        }
        levels.push(level);
    }
    let (worst, worst_level) = worst.ok_or_else(|| "the plan has no cruise segment".to_owned())?;
    Ok(PlannedCruise {
        worst,
        worst_level,
        levels,
    })
}

/// Grid of the search for the highest level, m (1,000 ft).
const LEVEL_SEARCH_STEP_M: f64 = 304.8;

/// Highest level at which level flight at the maximum-cruise rating holds
/// for `level`'s mass and Mach with `available_tw` installed (diagnostic):
/// the largest altitude with `required_tw <= available_tw`, found on a
/// 1,000 ft grid from the plan's level and refined by bisection to 1 m.
/// `None` when no level down to sea level holds.
pub fn highest_cruise_level_m(
    model: &EngineOutModel,
    drag: &CandidateDrag,
    gravity_m_s2: f64,
    level: &PlannedCruiseLevel,
    (takeoff_mass_kg, s_ref_m2): (f64, f64),
    available_tw: f64,
) -> Option<f64> {
    let holds = |altitude_m: f64| {
        deck_cruise_at(
            model,
            drag,
            gravity_m_s2,
            (altitude_m, level.mach, level.isa_deviation_c),
            (takeoff_mass_kg, level.mass_kg),
            s_ref_m2,
        )
        .is_ok_and(|deck| deck.level_tw <= available_tw)
    };
    let (mut low_m, mut high_m);
    if holds(level.altitude_m) {
        low_m = level.altitude_m;
        high_m = low_m + LEVEL_SEARCH_STEP_M;
        while holds(high_m) {
            low_m = high_m;
            high_m += LEVEL_SEARCH_STEP_M;
            if high_m > 25_000.0 {
                return Some(low_m);
            }
        }
    } else {
        high_m = level.altitude_m;
        low_m = high_m - LEVEL_SEARCH_STEP_M;
        while !holds(low_m) {
            high_m = low_m;
            low_m -= LEVEL_SEARCH_STEP_M;
            if low_m < 0.0 {
                return None;
            }
        }
    }
    while high_m - low_m > 1.0 {
        let mid_m = 0.5 * (low_m + high_m);
        if holds(mid_m) {
            low_m = mid_m;
        } else {
            high_m = mid_m;
        }
    }
    Some(low_m)
}
