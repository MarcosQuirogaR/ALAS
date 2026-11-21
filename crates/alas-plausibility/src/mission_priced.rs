// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mission that sizes the aircraft (the priced route model of `alas-opt`)
//! read as a flown mission, and its consistency with the native flight.
//!
//! Two mission models exist. The native flight (`alas-pipeline` mission stage,
//! `alas-mission`) is reported as telemetry; the priced model
//! (`alas_opt::mdo::mission_model`) decides block fuel, take-off mass and the
//! closure of the design. The mission rules of [`super::mission`] therefore
//! apply to the priced profile too, and the two models, fed the same aircraft,
//! take-off mass and route, must agree on what the flight costs.
//!
//! The priced profile is a ladder of constant-speed segments whose speed
//! changes are kinetic-energy budgets the integrator pays on the following
//! steps; its commanded speed therefore steps from rung to rung and the
//! speed-continuity test of the native rules is not applied to it.
//!
//! The ground-roll allowance (an ICAO reference mode time, no distance) is not
//! part of the native flight; it is left out of both sides of the comparison
//! (the airborne leg from lift-off is compared) and reported separately.

use alas_opt::mdo::mission_model::{FlownStep, StepPhase};

use super::mission::{phase_kind, Kind, MissionFacts, SegmentFacts};
use super::{Findings, RuleInfo, Severity};

const FT: f64 = 0.3048;

pub(super) const P_FLOWN: RuleInfo = RuleInfo {
    id: "PRICED-NATIVE-FLOWN",
    statement: "The route model that sizes the aircraft can fly the route the native flight flies, from the native lift-off mass",
    source: "The priced model decides block fuel, take-off mass and the closure of the design; if it cannot fly the route the native mission flies, the sizing is not a mission of this aircraft and no consistency can be judged. A model that cannot be built or flown is a finding, never a skipped check (owner rule: no silent skips)",
};
pub(super) const P_DECLARED: RuleInfo = RuleInfo {
    id: "PRICED-DECLARED-LEVEL",
    statement: "A declared or design cruise level the priced route cannot fly (route too short for the profile, or the aircraft cannot climb to it at the flown mass) is flown at the highest level it can; the shortfall is reported, declared against flown level",
    source: "Owner decision 2026-10-23: the sizing closure converges on the highest reachable level instead of failing; a prescribed level is a ceiling the user works below (dispatch practice), and the lowering is a finding, never silent. MEDIUM above 1,000 ft (one level pair), HIGH above 5,000 ft",
};
pub(super) const P_FUEL: RuleInfo = RuleInfo {
    id: "PRICED-NATIVE-FUEL",
    statement: "Trip fuel of the priced route model that sizes the aircraft and of the native flight, same aircraft, take-off mass and route, from lift-off: MEDIUM when they differ by more than 2 %, HIGH by more than 5 %",
    source: "Both integrate the same longitudinal energy equation on the same polar family and engine deck; a trip-fuel error dF moves the closed take-off mass by about dF (1 + fuel-to-MTOW feedback), so 2 % of a 10-40 % fuel fraction is 0.2-0.8 % of MTOW. The 2 % and 5 % are engineering thresholds chosen up front, not published values and not a resolution of the sizing closure (owner rule: stated before the measurement, never relaxed to pass)",
};
pub(super) const P_TIME: RuleInfo = RuleInfo {
    id: "PRICED-NATIVE-TIME",
    statement: "Flight time of the priced model and of the native flight from lift-off: MEDIUM when they differ by more than 3 %, HIGH by more than 8 %",
    source: "Same integration and the same speed schedule, so the block time differs only by discretisation and by how the speed changes are paid; 3 % of a 1-10 h flight is 2-18 min, more than the whole of a level speed change. Engineering thresholds chosen up front, not published",
};
pub(super) const P_LEVEL: RuleInfo = RuleInfo {
    id: "PRICED-NATIVE-LEVEL",
    statement: "Highest cruise level of the priced model and of the native flight: MEDIUM when they differ by more than 1,000 ft, HIGH by more than 3,000 ft",
    source: "Both apply the same residual-climb criterion at top-of-climb mass (Jenkinson, Civil Jet Aircraft Design, Ch. 10 pp. 238-239); a difference of more than half a flight-level step pair means the two models fly different missions. Tolerance argued, not published",
};
pub(super) const P_FOOTPRINT: RuleInfo = RuleInfo {
    id: "PRICED-NATIVE-FOOTPRINT",
    statement: "Climb and descent ground distance (all climb and descent phases) of the priced model and of the native flight at the same cruise level: MEDIUM when they differ by more than 10 %, HIGH by more than 25 %",
    source: "The footprints set the cruise distance of a fixed route. At equal level and speed schedule the two models differ by discretisation (priced sub-rungs of one eighth of a CAS rung, 1,000-3,000 ft; native descent pieces of up to 2,625 ft) and by how speed changes are paid: one piece of a 30,000 ft climb or descent is 3-10 % of its ground distance, so 10 % is about one piece and 25 % is a different profile. Engineering thresholds chosen up front, not published",
};

impl MissionFacts {
    /// The facts of a priced leg, from its integration steps.
    ///
    /// Consecutive steps of one phase and one commanded speed form a segment
    /// (one planned rung). The ground roll is dropped and time is counted from
    /// lift-off; the level acceleration to the first rung and the take-off band
    /// belong to the first climb, the landing band to the descent.
    pub fn from_priced(steps: &[FlownStep], design_mach: f64) -> Self {
        let airborne: Vec<&FlownStep> = steps
            .iter()
            .filter(|s| s.phase != StepPhase::Ground)
            .collect();
        let time_zero_s = airborne.first().map_or(0.0, |s| s.time_s[0]);
        let mut segments: Vec<SegmentFacts> = Vec::new();
        let mut run: Vec<&FlownStep> = Vec::new();
        let flush = |run: &mut Vec<&FlownStep>, segments: &mut Vec<SegmentFacts>| {
            if !run.is_empty() {
                let tag = format!("priced_{}", segments.len() + 1);
                segments.push(segment_of(&tag, run, time_zero_s));
                run.clear();
            }
        };
        for step in airborne {
            let new_rung = run.last().is_some_and(|last| {
                is_level_change(last)
                    || is_level_change(step)
                    || is_path_piece(last)
                    || is_path_piece(step)
                    || kind_of(last.phase) != kind_of(step.phase)
                    || (step.phase != StepPhase::Takeoff
                        && (last.tas_m_s[1] - step.tas_m_s[0]).abs() > 1.0e-9)
            });
            if new_rung {
                flush(&mut run, &mut segments);
            }
            run.push(step);
        }
        flush(&mut run, &mut segments);
        let scheduled = segments.len();
        Self {
            segments,
            scheduled,
            fuel_exhausted: false,
            design_mach,
            climb_reference: None,
            priced: true,
        }
    }

    /// Trip fuel from the first to the last point, kg.
    pub fn fuel_kg(&self) -> f64 {
        match (self.segments.first(), self.segments.last()) {
            (Some(first), Some(last)) => first.mass0_kg - last.mass1_kg,
            _ => 0.0,
        }
    }
}

/// A speed change along a climb (the take-off climb or a path piece): one step that changes speed
/// and gains altitude.
fn is_path_piece(step: &FlownStep) -> bool {
    matches!(step.phase, StepPhase::Takeoff | StepPhase::Climb)
        && (step.altitude_m[1] - step.altitude_m[0]).abs() > 1.0e-9
        && (step.tas_m_s[1] - step.tas_m_s[0]).abs() > 1.0e-9
}

/// A level speed change at the restriction altitude: a climb or descent step that gains no altitude
/// and changes speed.
fn is_level_change(step: &FlownStep) -> bool {
    matches!(step.phase, StepPhase::Climb | StepPhase::Descent)
        && (step.altitude_m[1] - step.altitude_m[0]).abs() < 1.0e-9
        && (step.tas_m_s[1] - step.tas_m_s[0]).abs() > 1.0e-9
}

fn kind_of(phase: StepPhase) -> Kind {
    match phase {
        StepPhase::Ground | StepPhase::Takeoff | StepPhase::Climb => Kind::Climb,
        StepPhase::Cruise => Kind::Cruise,
        StepPhase::Descent | StepPhase::Landing => Kind::Descent,
    }
}

fn segment_of(tag: &str, run: &[&FlownStep], time_zero_s: f64) -> SegmentFacts {
    let (first, last) = (run[0], run[run.len() - 1]);
    let max_of = |f: &dyn Fn(&FlownStep) -> f64| run.iter().map(|s| f(s)).fold(0.0_f64, f64::max);
    let measured: Vec<&&FlownStep> = run.iter().filter(|s| s.lift_to_drag > 0.0).collect();
    let m = measured.len().max(1) as f64;
    let low_alt_eas = run
        .iter()
        .filter(|s| s.altitude_m[0].min(s.altitude_m[1]) < 10_000.0 * FT - 1.0)
        .map(|s| s.eas_m_s)
        .fold(0.0_f64, f64::max);
    let min_of =
        |f: &dyn Fn(&FlownStep) -> f64| run.iter().map(|s| f(s)).fold(f64::INFINITY, f64::min);
    SegmentFacts {
        tag: tag.to_owned(),
        kind: kind_of(first.phase),
        speed_change: run.len() == 1 && is_level_change(run[0]),
        t0_s: first.time_s[0] - time_zero_s,
        t1_s: last.time_s[1] - time_zero_s,
        h0_m: first.altitude_m[0],
        h1_m: last.altitude_m[1],
        r0_m: first.range_m[0],
        r1_m: last.range_m[1],
        mach0: first.mach,
        mach1: last.mach,
        tas0_ms: first.tas_m_s[0],
        tas1_ms: last.tas_m_s[1],
        mass0_kg: first.mass_kg[0],
        mass1_kg: last.mass_kg[1],
        max_eas_low_alt_ms: low_alt_eas,
        max_mach: max_of(&|s| s.mach),
        cl_mean: measured.iter().map(|s| s.lift_coefficient).sum::<f64>() / m,
        ld_mean: measured.iter().map(|s| s.lift_to_drag).sum::<f64>() / m,
        min_step_t_s: min_of(&|s| s.time_s[1] - s.time_s[0]),
        min_step_r_m: min_of(&|s| s.range_m[1] - s.range_m[0]),
        max_mass_gain_kg: run
            .iter()
            .map(|s| s.mass_kg[1] - s.mass_kg[0])
            .fold(f64::NEG_INFINITY, f64::max),
    }
}

/// The numbers the native-versus-priced consistency rules compare.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LegTotals {
    /// Trip fuel from lift-off, kg.
    pub fuel_kg: f64,
    /// Flight time from lift-off, s.
    pub time_s: f64,
    /// Highest altitude reached, m.
    pub top_m: f64,
    /// Ground distance of every climb phase, m.
    pub climb_m: f64,
    /// Ground distance of every descent phase, m.
    pub descent_m: f64,
    /// Fuel burned in the climb, cruise and descent phases, kg (diagnostic).
    pub phase_fuel_kg: [f64; 3],
    /// Time spent in the climb, cruise and descent phases, s (diagnostic).
    pub phase_time_s: [f64; 3],
}

impl LegTotals {
    /// The totals of a mission as the rules see it.
    pub fn of(m: &MissionFacts) -> Self {
        let mut totals = Self {
            fuel_kg: m.fuel_kg(),
            time_s: m.duration_s(),
            top_m: 0.0,
            climb_m: 0.0,
            descent_m: 0.0,
            phase_fuel_kg: [0.0; 3],
            phase_time_s: [0.0; 3],
        };
        for (index, s) in m.segments.iter().enumerate() {
            totals.top_m = totals.top_m.max(s.h0_m).max(s.h1_m);
            let slot = match phase_kind(m, index) {
                Kind::Climb => {
                    totals.climb_m += s.r1_m - s.r0_m;
                    0
                }
                Kind::Cruise => 1,
                Kind::Descent => {
                    totals.descent_m += s.r1_m - s.r0_m;
                    2
                }
            };
            totals.phase_fuel_kg[slot] += s.mass0_kg - s.mass1_kg;
            totals.phase_time_s[slot] += s.t1_s - s.t0_s;
        }
        totals
    }
}

fn graded(relative: f64, medium: f64, high: f64) -> Option<Severity> {
    let r = relative.abs();
    if r > high {
        Some(Severity::High)
    } else if r > medium {
        Some(Severity::Medium)
    } else {
        None
    }
}

/// Compare the native flight with the priced model and record the gaps.
/// Differences are `(native - priced) / priced`.
pub fn check_consistency(out: &mut Findings, native: &LegTotals, priced: &LegTotals) {
    let relative = |a: f64, b: f64| (a - b) / b;
    let fuel = relative(native.fuel_kg, priced.fuel_kg);
    if let Some(sev) = graded(fuel, 0.02, 0.05) {
        out.add(
            P_FUEL.id,
            sev,
            format!(
                "native trip fuel {:.0} kg against priced {:.0} kg ({:+.1} %)",
                native.fuel_kg,
                priced.fuel_kg,
                100.0 * fuel
            ),
        );
    }
    let time = relative(native.time_s, priced.time_s);
    if let Some(sev) = graded(time, 0.03, 0.08) {
        out.add(
            P_TIME.id,
            sev,
            format!(
                "native flight time {:.1} min against priced {:.1} min ({:+.1} %)",
                native.time_s / 60.0,
                priced.time_s / 60.0,
                100.0 * time
            ),
        );
    }
    let level_ft = (native.top_m - priced.top_m) / FT;
    if let Some(sev) = {
        let a = level_ft.abs();
        if a > 3000.0 {
            Some(Severity::High)
        } else if a > 1000.0 {
            Some(Severity::Medium)
        } else {
            None
        }
    } {
        out.add(
            P_LEVEL.id,
            sev,
            format!(
                "native top level {:.0} ft against priced {:.0} ft ({level_ft:+.0} ft)",
                native.top_m / FT,
                priced.top_m / FT
            ),
        );
    }
    for (name, a, b) in [
        ("climb", native.climb_m, priced.climb_m),
        ("descent", native.descent_m, priced.descent_m),
    ] {
        let gap = relative(a, b);
        if let Some(sev) = graded(gap, 0.10, 0.25) {
            out.add(
                P_FOOTPRINT.id,
                sev,
                format!(
                    "native {name} distance {:.0} km against priced {:.0} km ({:+.1} %)",
                    a / 1e3,
                    b / 1e3,
                    100.0 * gap
                ),
            );
        }
    }
}

/// Record that the priced model could not be built or could not fly the
/// native flight's route: a HIGH finding of [`P_FLOWN`].
pub fn record_unflown(out: &mut Findings, stage: &str, error: &str) {
    out.add(
        P_FLOWN.id,
        Severity::High,
        format!("the priced mission model {stage}: {error}"),
    );
}

/// Report a declared or design cruise level the priced route flew lower than
/// declared: `declared_m` against the highest level `flown_m` of the priced ladder.
pub fn check_declared_level(out: &mut Findings, declared_m: f64, flown_m: f64) {
    let shortfall_ft = (declared_m - flown_m) / FT;
    let severity = if shortfall_ft > 5000.0 {
        Severity::High
    } else if shortfall_ft > 1000.0 {
        Severity::Medium
    } else {
        return;
    };
    out.add(
        P_DECLARED.id,
        severity,
        format!(
            "declared cruise level {:.0} ft, flown {:.0} ft (-{shortfall_ft:.0} ft)",
            declared_m / FT,
            flown_m / FT
        ),
    );
}

/// Run the native mission rules on the priced profile. The findings are
/// recorded under the case label `<case>#priced` (see [`PRICED_SUFFIX`]), so a
/// native and a priced finding of one rule stay distinct.
pub fn check_priced_profile(out: &mut Findings, priced: &MissionFacts) {
    super::mission::check_mission(out, priced);
}

/// Suffix of the case label under which the priced profile is judged.
pub const PRICED_SUFFIX: &str = "#priced";

/// Fly the priced route model at the native flight's lift-off mass over its
/// route and return the steps of the profile it prices.
///
/// The priced trip starts at brake release and pays the ground-roll fuel before
/// lift-off; the native flight starts at lift-off. The brake-release mass is
/// therefore the native initial mass plus the roll fuel (two passes: the roll
/// fuel depends only weakly on the mass).
///
/// # Errors
///
/// The priced model could not fly the route, with its reason.
pub fn priced_flight(
    model: &alas_opt::mdo::SegmentMissionModel,
    native: &MissionFacts,
) -> Result<Vec<FlownStep>, String> {
    let first = native.segments.first().ok_or("native mission is empty")?;
    let range_m = native.range_m();
    let lift_off_kg = first.mass0_kg;
    let mut brake_release_kg = lift_off_kg;
    let mut flown = None;
    for _ in 0..2 {
        let leg = model
            .fly_trip(brake_release_kg, range_m)
            .map_err(|e| format!("priced trip: {e}"))?;
        let roll_kg = leg
            .steps
            .iter()
            .find(|s| s.phase == StepPhase::Ground)
            .map_or(0.0, |s| s.mass_kg[0] - s.mass_kg[1]);
        brake_release_kg = lift_off_kg + roll_kg;
        flown = Some(leg);
    }
    let leg = flown.ok_or("priced trip was not flown")?;
    Ok(leg.steps)
}

// The tests assert on facts they just built, so a failed unwrap is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn step(
        phase: StepPhase,
        t: (f64, f64),
        h: (f64, f64),
        r_km: (f64, f64),
        tas: f64,
    ) -> FlownStep {
        FlownStep {
            phase,
            time_s: [t.0, t.1],
            altitude_m: [h.0, h.1],
            range_m: [r_km.0 * 1e3, r_km.1 * 1e3],
            tas_m_s: [tas; 2],
            mach: 0.7,
            eas_m_s: 150.0,
            mass_kg: [70_000.0 - t.0 * 0.8, 70_000.0 - t.1 * 0.8],
            lift_coefficient: 0.5,
            drag_n: 40_000.0,
            thrust_n: 40_000.0,
            lift_to_drag: 17.0,
            bound_limited: false,
        }
    }

    /// Ground roll, climb at two speeds, cruise, descent.
    fn trip() -> Vec<FlownStep> {
        vec![
            step(StepPhase::Ground, (0.0, 42.0), (0.0, 0.0), (0.0, 0.0), 0.0),
            step(
                StepPhase::Climb,
                (42.0, 600.0),
                (0.0, 5_000.0),
                (0.0, 40.0),
                150.0,
            ),
            step(
                StepPhase::Climb,
                (600.0, 1_500.0),
                (5_000.0, 10_000.0),
                (40.0, 120.0),
                200.0,
            ),
            step(
                StepPhase::Cruise,
                (1_500.0, 6_000.0),
                (10_000.0, 10_000.0),
                (120.0, 1_000.0),
                230.0,
            ),
            step(
                StepPhase::Descent,
                (6_000.0, 7_500.0),
                (10_000.0, 0.0),
                (1_000.0, 1_100.0),
                180.0,
            ),
        ]
    }

    /// The level acceleration at the speed restriction is a segment of its own, flagged as a level
    /// speed change, between the rungs it joins.
    #[test]
    fn a_level_acceleration_is_its_own_speed_change_segment() {
        let mut level = step(
            StepPhase::Climb,
            (600.0, 660.0),
            (3_048.0, 3_048.0),
            (40.0, 45.0),
            150.0,
        );
        level.tas_m_s = [150.0, 200.0];
        let steps = vec![
            step(StepPhase::Ground, (0.0, 42.0), (0.0, 0.0), (0.0, 0.0), 0.0),
            step(
                StepPhase::Climb,
                (42.0, 600.0),
                (0.0, 3_048.0),
                (0.0, 40.0),
                150.0,
            ),
            level,
            step(
                StepPhase::Climb,
                (660.0, 1_500.0),
                (3_048.0, 10_000.0),
                (45.0, 120.0),
                200.0,
            ),
        ];
        let facts = MissionFacts::from_priced(&steps, 0.78);
        let flags: Vec<bool> = facts.segments.iter().map(|s| s.speed_change).collect();
        assert_eq!(flags, vec![false, true, false]);
        assert!((facts.segments[1].t1_s - facts.segments[1].t0_s - 60.0).abs() < 1e-9);
    }

    #[test]
    fn the_priced_steps_become_segments_from_lift_off() {
        let facts = MissionFacts::from_priced(&trip(), 0.78);
        assert!(facts.priced);
        // Two climb rungs (a speed change starts a segment), cruise, descent.
        assert_eq!(
            facts.segments.len(),
            4,
            "{:?}",
            facts.segments.iter().map(|s| &s.tag).collect::<Vec<_>>()
        );
        let first = &facts.segments[0];
        assert_eq!(first.kind, Kind::Climb);
        assert_eq!(
            first.t0_s, 0.0,
            "time counts from lift-off, not brake release"
        );
        assert!((facts.duration_s() - (7_500.0 - 42.0)).abs() < 1e-9);
        assert!((facts.range_m() - 1_100e3).abs() < 1e-6);
        assert!((facts.fuel_kg() - 0.8 * (7_500.0 - 42.0)).abs() < 1e-6);
        assert_eq!(facts.segments[2].kind, Kind::Cruise);
        assert!((facts.segments[2].ld_mean - 17.0).abs() < 1e-12);
    }

    #[test]
    fn the_priced_profile_is_judged_by_the_mission_rules_without_the_speed_jump_test() {
        let facts = MissionFacts::from_priced(&trip(), 0.78);
        let mut out = Findings::new("synthetic#priced");
        check_priced_profile(&mut out, &facts);
        // The 150 -> 200 -> 230 -> 180 m/s rung changes are energy budgets, not jumps.
        assert!(!out.has("MISSION-CONTINUITY"), "{:?}", out.items());
    }

    #[test]
    fn consistency_gaps_are_graded_at_their_stated_tolerances() {
        let priced = LegTotals {
            fuel_kg: 1_000.0,
            time_s: 3_600.0,
            top_m: 10_000.0,
            climb_m: 200e3,
            descent_m: 200e3,
            phase_fuel_kg: [0.0; 3],
            phase_time_s: [0.0; 3],
        };
        let grade = |native: LegTotals| {
            let mut out = Findings::new("synthetic");
            check_consistency(&mut out, &native, &priced);
            out
        };
        assert!(grade(priced).items().is_empty());
        let near = LegTotals {
            fuel_kg: 1_019.0,
            time_s: 3_600.0 * 1.029,
            top_m: 10_000.0 + 900.0 * FT,
            ..priced
        };
        assert!(grade(near).items().is_empty());
        let medium = LegTotals {
            fuel_kg: 1_030.0,
            time_s: 3_600.0 * 1.05,
            top_m: 10_000.0 + 1_500.0 * FT,
            climb_m: 225e3,
            ..priced
        };
        let out = grade(medium);
        for rule in [P_FUEL.id, P_TIME.id, P_LEVEL.id, P_FOOTPRINT.id] {
            assert_eq!(out.severity_of(rule), Some(Severity::Medium), "{rule}");
        }
        let high = LegTotals {
            fuel_kg: 940.0,
            time_s: 3_600.0 * 0.9,
            top_m: 10_000.0 - 3_500.0 * FT,
            descent_m: 130e3,
            ..priced
        };
        let out = grade(high);
        for rule in [P_FUEL.id, P_TIME.id, P_LEVEL.id, P_FOOTPRINT.id] {
            assert_eq!(out.severity_of(rule), Some(Severity::High), "{rule}");
        }
    }

    /// A priced model that cannot be built or flown is a HIGH finding of
    /// `PRICED-NATIVE-FLOWN`, not a skipped check.
    #[test]
    fn an_unflown_priced_model_is_a_high_finding() {
        let mut out = Findings::new("synthetic");
        record_unflown(
            &mut out,
            "cannot fly the native route",
            "priced trip: too short",
        );
        assert_eq!(out.severity_of(P_FLOWN.id), Some(Severity::High));
        assert!(out.items()[0].observation.contains("too short"));
    }

    /// A declared level flown lower is a finding with both levels in the text.
    #[test]
    fn a_declared_level_flown_lower_is_reported() {
        let mut out = Findings::new("synthetic");
        check_declared_level(&mut out, 12_000.0, 12_000.0 - 500.0 * FT);
        assert!(out.items().is_empty());
        check_declared_level(&mut out, 12_000.0, 12_000.0 - 2_000.0 * FT);
        assert_eq!(out.severity_of(P_DECLARED.id), Some(Severity::Medium));
        let text = &out.items()[0].observation;
        assert!(
            text.contains("declared cruise level 39370 ft, flown 37370 ft"),
            "{text}"
        );
    }
}
