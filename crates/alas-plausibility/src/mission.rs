// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission-profile plausibility rules.
//!
//! Consecutive segments of one kind are merged into a phase first: a climb
//! flown as three constant-speed segments is one climb to the pilot, and its
//! average rate and path angle are what the rules judge.

pub use super::mission_facts::{Kind, MissionFacts, SegmentFacts};

use super::{Findings, RuleInfo, Severity};

const FT: f64 = 0.3048;
const KT: f64 = 0.514_444;
const FPM: f64 = FT / 60.0;

pub(super) const M_COMPLETE: RuleInfo = RuleInfo {
    id: "MISSION-COMPLETE",
    statement: "Every scheduled segment is flown; the load case does not run out of fuel",
    source: "Conservation of mass; a partial mission is not a result",
};
pub(super) const M_MONOTONIC: RuleInfo = RuleInfo {
    id: "MISSION-MONOTONIC",
    statement: "Time and range never decrease; aircraft mass never increases (no fuel gain)",
    source: "Kinematics; fuel is only burned in a mission without payload release",
};
pub(super) const M_CRUISE_PRESENT: RuleInfo = RuleInfo {
    id: "MISSION-CRUISE-PRESENT",
    statement: "A route longer than 300 km contains a cruise phase (level flight, distance > 0)",
    source: "Owner rule 2026-10-19 (E195-E2 baseline: climb then continuous descent, no cruise)",
};
pub(super) const M_CRUISE_SHARE: RuleInfo = RuleInfo {
    id: "MISSION-CRUISE-SHARE",
    statement: "On a route longer than 1,500 km the cruise phase is at least 40 % of the ground distance and 30 % of the flight time",
    source: "Typical transport mission: climb and descent together cover about 300-500 km (Raymer, Aircraft Design, Ch. 6 mission profile; Torenbeek Ch. 8)",
};
pub(super) const M_CLIMB_RATE: RuleInfo = RuleInfo {
    id: "MISSION-CLIMB-RATE",
    statement: "Average rate of the initial climb (before the first cruise) when it gains more than 3,000 ft: jets 1,500-3,000 ft/min usual (owner band), MEDIUM outside 1,200-3,500, HIGH outside 800-5,000. A type with a published time to climb (factsheet/AFM, stated altitude and weight; data/climb_references.tsv) is compared with it at the corresponding weight instead on the slow side: MEDIUM if more than 10 % slower, HIGH if more than 25 %; a turboprop without one uses the mean rate of the one cited turboprop datum (ATR 72-600 factsheet 2020 p.2). The climb ends at top of climb: a level acceleration after it belongs to cruise. When the model's climb ends below the quoted altitude the rest is climbed on the linear trend of its own rate of climb against altitude, not at its mean rate; the sea-level-to-field part is taken at the sea-level rate and the weight scaling is linear, both also in the model's favour when it is lighter. A reference comparison, never a tuning target (owner decision)",
    source: "Owner rule 2026-10-19; transport jet and turboprop climb performance (Roskam Part VII, Sec. 5.1; AFM climb tables)",
};
pub(super) const M_DESCENT_RATE: RuleInfo = RuleInfo {
    id: "MISSION-DESCENT-RATE",
    statement: "Average rate of a descent phase losing more than 3,000 ft: 1,500-3,500 ft/min usual (owner band); MEDIUM outside 1,200-4,000; HIGH outside 900-6,000 (below 900 ft/min the descent gradient is under 1.4 degrees, a glide no idle-thrust descent flies)",
    source: "Owner rule 2026-10-19; standard 3 degree descent profile, pressurisation limit about 500 ft/min cabin",
};
pub(super) const M_DESCENT_TIME: RuleInfo = RuleInfo {
    id: "MISSION-DESCENT-TIME",
    statement: "A single descent phase lasts at most 27 min (MEDIUM above, HIGH above 35 min)",
    source: "Cruise-to-approach descent from FL410 at 1,500-2,000 ft/min takes 20-27 min; owner flagged a 40 min descent as absurd",
};
pub(super) const M_SPEED_CHANGE: RuleInfo = RuleInfo {
    id: "MISSION-SPEED-CHANGE",
    statement: "A level speed change between two legs lasts at most 5 min (MEDIUM above, HIGH above 10 min)",
    source: "BADA 3.8 User Manual Sec. 5.2 (printed p. 33): nominal level acceleration is bounded by 2 ft/s^2, so 100 kt takes about 1.5 min; published procedures accelerate or decelerate in 1-3 min (engineering band, no single source)",
};
pub(super) const M_PATH_ANGLE: RuleInfo = RuleInfo {
    id: "MISSION-PATH-ANGLE",
    statement: "Mean flight-path angle of a descent phase 1.5-6 degrees; of the initial climb 1.5-12 degrees (MEDIUM outside)",
    source: "Idle-descent gradient about 3 degrees (3:1 rule); CS 25.121 / Roskam climb gradients",
};
pub(super) const M_STEP_CLIMB: RuleInfo = RuleInfo {
    id: "MISSION-STEP-CLIMB",
    statement:
        "A climb after the first cruise (step climb) only occurs on routes longer than 2,500 km",
    source:
        "Step climbs follow fuel burn (Torenbeek Sec. 9; ICAO/Eurocontrol cruise-level practice)",
};
pub(super) const M_CONTINUITY: RuleInfo = RuleInfo {
    id: "MISSION-CONTINUITY",
    statement: "At each segment boundary altitude, range, time and mass are continuous (1 m, 1 km, 1 s, 0.5 % tolerance) and true airspeed jumps by at most 20 m/s (MEDIUM above, HIGH above 50 m/s = 97 kt)",
    source: "Trajectory continuity; a jump is an unsolved or mis-scheduled interface",
};
pub(super) const M_MACH: RuleInfo = RuleInfo {
    id: "MISSION-MACH",
    statement: "Mach never exceeds the design cruise Mach by more than 0.03 (MEDIUM) and never exceeds 0.95 (HIGH)",
    source: "Design cruise Mach is the speed limit the aircraft is sized for (VMO/MMO sit about 0.05-0.07 above)",
};
pub(super) const M_LOW_ALT_SPEED: RuleInfo = RuleInfo {
    id: "MISSION-LOW-ALT-SPEED",
    statement: "Equivalent airspeed below 10,000 ft does not exceed 270 kt (250 kt CAS limit plus tolerance)",
    source: "14 CFR 91.117 / ICAO Annex 2 speed restriction below 10,000 ft",
};
pub(super) const M_CRUISE_LD: RuleInfo = RuleInfo {
    id: "MISSION-CRUISE-LD",
    statement: "Cruise lift-to-drag ratio 8-25 usual; MEDIUM outside 6-28; HIGH outside 4-35",
    source: "Raymer Table 3.6 and Torenbeek Sec. 7 (transport L/D(max) 14-22); BWB and glider-class excluded by class",
};
pub(super) const M_CRUISE_CL: RuleInfo = RuleInfo {
    id: "MISSION-CRUISE-CL",
    statement: "Cruise lift coefficient 0.2-0.8 usual; MEDIUM outside; HIGH outside 0.1-1.1",
    source: "Cruise CL of transports 0.4-0.6, buffet limit about 1.0 (Torenbeek Sec. 7.4)",
};

/// A run of consecutive segments of one kind.
#[derive(Debug, Clone, Copy)]
pub(super) struct Phase {
    pub(super) kind: Kind,
    pub(super) dt_s: f64,
    pub(super) dh_m: f64,
    pub(super) dr_m: f64,
}

/// The kind of phase a segment belongs to. A level speed-change leg belongs to
/// the phase it sits in: that of its neighbours when they agree (a deceleration
/// between two descent rungs is part of the descent, the acceleration after
/// take-off part of the climb), and level flight at a cruise level when one of
/// them is a cruise leg (the acceleration to the cruise speed at the top of the
/// climb, the deceleration before the top of descent).
pub(super) fn phase_kind(m: &MissionFacts, index: usize) -> Kind {
    let s = &m.segments[index];
    if !s.speed_change {
        return s.kind;
    }
    let neighbour = |range: &mut dyn Iterator<Item = usize>| {
        range
            .map(|i| &m.segments[i])
            .find(|n| !n.speed_change)
            .map(|n| n.kind)
    };
    let before = neighbour(&mut (0..index).rev());
    let after = neighbour(&mut (index + 1..m.segments.len()));
    match (before, after) {
        (Some(a), Some(b)) if a == b => a,
        (Some(Kind::Cruise), Some(_)) | (Some(_), Some(Kind::Cruise)) => Kind::Cruise,
        (Some(a), _) => a,
        (None, Some(b)) => b,
        (None, None) => s.kind,
    }
}

fn phases(m: &MissionFacts) -> Vec<Phase> {
    let mut out: Vec<Phase> = Vec::new();
    for (index, s) in m.segments.iter().enumerate() {
        let kind = phase_kind(m, index);
        let dt = s.t1_s - s.t0_s;
        let dh = s.h1_m - s.h0_m;
        let dr = s.r1_m - s.r0_m;
        // A zero-length cruise between two climbs is a schedule placeholder,
        // not a cruise: it must neither split a climb nor count as cruise.
        if !s.speed_change && s.kind == Kind::Cruise && dr < 1.0e3 && dt < 60.0 {
            continue;
        }
        match out.last_mut() {
            Some(p) if p.kind == kind => {
                p.dt_s += dt;
                p.dh_m += dh;
                p.dr_m += dr;
            }
            _ => out.push(Phase {
                kind,
                dt_s: dt,
                dh_m: dh,
                dr_m: dr,
            }),
        }
    }
    out
}

fn banded(value: f64, usual: (f64, f64), medium: (f64, f64), high: (f64, f64)) -> Option<Severity> {
    if value < high.0 || value > high.1 {
        Some(Severity::High)
    } else if value < medium.0 || value > medium.1 {
        Some(Severity::Medium)
    } else if value < usual.0 || value > usual.1 {
        Some(Severity::Low)
    } else {
        None
    }
}

/// Run every mission rule on `m`.
pub fn check_mission(out: &mut Findings, m: &MissionFacts) {
    if m.segments.is_empty() {
        out.add(
            M_COMPLETE.id,
            Severity::High,
            "no mission segments were flown".into(),
        );
        return;
    }
    if m.fuel_exhausted || m.segments.len() < m.scheduled {
        out.add(
            M_COMPLETE.id,
            Severity::High,
            format!(
                "{} of {} scheduled segments flown, fuel exhausted: {}",
                m.segments.len(),
                m.scheduled,
                m.fuel_exhausted
            ),
        );
    }
    check_monotonic_and_continuity(out, m);

    let range = m.range_m();
    let duration = m.duration_s();
    let ph = phases(m);
    let cruise_dist: f64 = ph
        .iter()
        .filter(|p| p.kind == Kind::Cruise)
        .map(|p| p.dr_m)
        .sum();
    let cruise_time: f64 = ph
        .iter()
        .filter(|p| p.kind == Kind::Cruise)
        .map(|p| p.dt_s)
        .sum();
    if range > 300e3 && cruise_dist < 1.0e3 {
        out.add(
            M_CRUISE_PRESENT.id,
            Severity::High,
            format!(
                "route {:.0} km has {:.1} km of cruise over {} climb/descent phases",
                range / 1e3,
                cruise_dist / 1e3,
                ph.len()
            ),
        );
    } else if range > 1500e3 && (cruise_dist < 0.4 * range || cruise_time < 0.3 * duration) {
        out.add(
            M_CRUISE_SHARE.id,
            Severity::Medium,
            format!(
                "cruise is {:.0} % of the {:.0} km distance and {:.0} % of the {:.0} min flight",
                100.0 * cruise_dist / range,
                range / 1e3,
                100.0 * cruise_time / duration,
                duration / 60.0
            ),
        );
    }

    let mut seen_cruise = false;
    for p in &ph {
        match p.kind {
            Kind::Cruise => seen_cruise = true,
            Kind::Climb => {
                if seen_cruise && range < 2500e3 && p.dh_m > 600.0 * FT {
                    out.add(
                        M_STEP_CLIMB.id,
                        Severity::Medium,
                        format!(
                            "step climb of {:.0} ft on a {:.0} km route",
                            p.dh_m / FT,
                            range / 1e3
                        ),
                    );
                }
                if !seen_cruise && p.dh_m > 3000.0 * FT && p.dt_s > 0.0 {
                    let rate = p.dh_m / p.dt_s / FPM;
                    // A type with a published time to climb (or a class reference) is
                    // judged against it on the slow side; the fast
                    // side keeps the band.
                    let referenced = m.climb_reference.is_some();
                    let bands = if referenced {
                        (1500.0, 3000.0, 0.0)
                    } else {
                        (1500.0, 3000.0, 800.0)
                    };
                    let band = banded(
                        rate,
                        (bands.0, bands.1),
                        (1200.0, 3500.0),
                        (bands.2, 5000.0),
                    );
                    let band = band.filter(|_| !referenced || rate > bands.1);
                    if let Some(sev) = band {
                        out.add(
                            M_CLIMB_RATE.id,
                            sev,
                            format!(
                                "climb of {:.0} ft in {:.1} min averages {rate:.0} ft/min",
                                p.dh_m / FT,
                                p.dt_s / 60.0
                            ),
                        );
                    }
                    if let Some((slower, note)) = super::mission_climb::slower_than_reference(m, p)
                    {
                        let sev = if slower > 0.25 {
                            Some(Severity::High)
                        } else if slower > 0.10 {
                            Some(Severity::Medium)
                        } else {
                            None
                        };
                        if let Some(sev) = sev {
                            out.add_measured(
                                M_CLIMB_RATE.id,
                                sev,
                                "SlowerThanReference",
                                Some(100.0 * slower),
                                format!("climb {:.3} % slower than {note}", 100.0 * slower),
                            );
                        }
                    }
                }
                if !seen_cruise && p.dh_m > 3000.0 * FT && p.dr_m > 0.0 {
                    let angle = (p.dh_m / p.dr_m).atan().to_degrees();
                    if !(1.5..=12.0).contains(&angle) {
                        out.add(
                            M_PATH_ANGLE.id,
                            Severity::Medium,
                            format!(
                                "climb path angle {angle:.1} deg over {:.0} km",
                                p.dr_m / 1e3
                            ),
                        );
                    }
                }
            }
            Kind::Descent => {
                let loss = -p.dh_m;
                if loss > 3000.0 * FT && p.dt_s > 0.0 {
                    let rate = loss / p.dt_s / FPM;
                    if let Some(sev) =
                        banded(rate, (1500.0, 3500.0), (1200.0, 4000.0), (900.0, 6000.0))
                    {
                        out.add(
                            M_DESCENT_RATE.id,
                            sev,
                            format!(
                                "descent of {:.0} ft in {:.1} min averages {rate:.0} ft/min",
                                loss / FT,
                                p.dt_s / 60.0
                            ),
                        );
                    }
                }
                if p.dt_s > 27.0 * 60.0 {
                    let sev = if p.dt_s > 35.0 * 60.0 {
                        Severity::High
                    } else {
                        Severity::Medium
                    };
                    out.add(
                        M_DESCENT_TIME.id,
                        sev,
                        format!("continuous descent lasts {:.1} min", p.dt_s / 60.0),
                    );
                }
                if loss > 3000.0 * FT && p.dr_m > 0.0 {
                    let angle = (loss / p.dr_m).atan().to_degrees();
                    if !(1.5..=6.0).contains(&angle) {
                        out.add(
                            M_PATH_ANGLE.id,
                            Severity::Medium,
                            format!(
                                "descent path angle {angle:.1} deg over {:.0} km",
                                p.dr_m / 1e3
                            ),
                        );
                    }
                }
            }
        }
    }

    for s in m.segments.iter().filter(|s| s.speed_change) {
        let minutes = (s.t1_s - s.t0_s) / 60.0;
        if minutes > 5.0 {
            let sev = if minutes > 10.0 {
                Severity::High
            } else {
                Severity::Medium
            };
            out.add(
                M_SPEED_CHANGE.id,
                sev,
                format!(
                    "{}: a level speed change of {:.0} m/s lasts {minutes:.1} min over {:.0} km",
                    s.tag,
                    (s.tas1_ms - s.tas0_ms).abs(),
                    (s.r1_m - s.r0_m) / 1e3
                ),
            );
        }
    }

    for s in &m.segments {
        if s.max_mach > 0.95 {
            out.add(
                M_MACH.id,
                Severity::High,
                format!("{}: Mach {:.3}", s.tag, s.max_mach),
            );
        } else if s.max_mach > m.design_mach + 0.03 {
            out.add(
                M_MACH.id,
                Severity::Medium,
                format!(
                    "{}: Mach {:.3} above design cruise Mach {:.3}",
                    s.tag, s.max_mach, m.design_mach
                ),
            );
        }
        if s.max_eas_low_alt_ms > 270.0 * KT {
            out.add(
                M_LOW_ALT_SPEED.id,
                Severity::Medium,
                format!(
                    "{}: EAS {:.0} kt below 10,000 ft",
                    s.tag,
                    s.max_eas_low_alt_ms / KT
                ),
            );
        }
        if s.kind == Kind::Cruise && !s.speed_change {
            if let Some(sev) = banded(s.ld_mean, (8.0, 25.0), (6.0, 28.0), (4.0, 35.0)) {
                out.add(
                    M_CRUISE_LD.id,
                    sev,
                    format!("{}: cruise L/D {:.1}", s.tag, s.ld_mean),
                );
            }
            if let Some(sev) = banded(s.cl_mean, (0.2, 0.8), (0.2, 0.8), (0.1, 1.1)) {
                out.add(
                    M_CRUISE_CL.id,
                    sev,
                    format!("{}: cruise CL {:.2}", s.tag, s.cl_mean),
                );
            }
        }
    }
}

fn check_monotonic_and_continuity(out: &mut Findings, m: &MissionFacts) {
    for s in &m.segments {
        if s.min_step_t_s < -1e-9 || s.min_step_r_m < -1.0 || s.max_mass_gain_kg > 1.0 {
            out.add(
                M_MONOTONIC.id,
                Severity::High,
                format!(
                    "{}: min dt {:.2} s, min dr {:.1} m, max mass gain {:.1} kg",
                    s.tag, s.min_step_t_s, s.min_step_r_m, s.max_mass_gain_kg
                ),
            );
        }
    }
    for pair in m.segments.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let dh = (b.h0_m - a.h1_m).abs();
        let dr = (b.r0_m - a.r1_m).abs();
        let dt = (b.t0_s - a.t1_s).abs();
        let dm = (b.mass0_kg - a.mass1_kg).abs() / a.mass1_kg.max(1.0);
        let dv = (b.tas0_ms - a.tas1_ms).abs();
        let dv = if m.priced { 0.0 } else { dv };
        if dh > 1.0 || dr > 1.0e3 || dt > 1.0 || dm > 0.005 || dv > 20.0 {
            out.add(
                M_CONTINUITY.id,
                if dv > 50.0 { Severity::High } else { Severity::Medium },
                format!(
                    "{} -> {}: jump dh {:.1} m, dr {:.1} m, dt {:.1} s, dmass {:.2} %, dTAS {:.1} m/s",
                    a.tag,
                    b.tag,
                    dh,
                    dr,
                    dt,
                    100.0 * dm,
                    dv
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test helper: one argument per fact of the synthetic segment.
    #[allow(clippy::too_many_arguments)]
    fn seg(
        tag: &str,
        kind: Kind,
        t: (f64, f64),
        h_ft: (f64, f64),
        r_km: (f64, f64),
        mach: f64,
    ) -> SegmentFacts {
        SegmentFacts {
            tag: tag.into(),
            kind,
            speed_change: false,
            t0_s: t.0 * 60.0,
            t1_s: t.1 * 60.0,
            h0_m: h_ft.0 * FT,
            h1_m: h_ft.1 * FT,
            r0_m: r_km.0 * 1e3,
            r1_m: r_km.1 * 1e3,
            mach0: mach,
            mach1: mach,
            tas0_ms: mach * 295.0,
            tas1_ms: mach * 295.0,
            mass0_kg: 70_000.0 - t.0 * 40.0,
            mass1_kg: 70_000.0 - t.1 * 40.0,
            max_eas_low_alt_ms: 0.0,
            max_mach: mach,
            cl_mean: 0.5,
            ld_mean: 17.0,
            min_step_t_s: 1.0,
            min_step_r_m: 1.0,
            max_mass_gain_kg: -0.1,
        }
    }

    fn facts(segments: Vec<SegmentFacts>) -> MissionFacts {
        MissionFacts {
            segments,
            scheduled: 3,
            fuel_exhausted: false,
            design_mach: 0.78,
            climb_reference: None,
            priced: false,
        }
    }

    fn run(m: &MissionFacts) -> Findings {
        let mut out = Findings::new("synthetic");
        check_mission(&mut out, m);
        out
    }

    /// A 2,000 km transport profile: climb to FL350 in 17 min, long cruise,
    /// descent in 20 min.
    fn healthy() -> MissionFacts {
        facts(vec![
            seg(
                "climb",
                Kind::Climb,
                (0.0, 17.0),
                (0.0, 35_000.0),
                (0.0, 140.0),
                0.78,
            ),
            seg(
                "cruise",
                Kind::Cruise,
                (17.0, 147.0),
                (35_000.0, 35_000.0),
                (140.0, 1_700.0),
                0.78,
            ),
            seg(
                "descent",
                Kind::Descent,
                (147.0, 169.0),
                (35_000.0, 0.0),
                (1_700.0, 1_900.0),
                0.78,
            ),
        ])
    }

    #[test]
    fn a_climb_is_judged_against_the_published_time_to_climb_at_the_corresponding_weight() {
        let build = |minutes: f64| {
            let mut climb = seg(
                "climb",
                Kind::Climb,
                (0.0, minutes),
                (2_000.0, 17_000.0),
                (0.0, 120.0),
                0.3,
            );
            climb.mass0_kg = 22_800.0;
            let mut accelerate = seg(
                "speed_change_to_cruise_step_1",
                Kind::Cruise,
                (minutes, minutes + 2.0),
                (17_000.0, 17_000.0),
                (120.0, 140.0),
                0.4,
            );
            accelerate.speed_change = true;
            let cruise = seg(
                "cruise_step_1",
                Kind::Cruise,
                (minutes + 2.0, minutes + 60.0),
                (17_000.0, 17_000.0),
                (140.0, 600.0),
                0.4,
            );
            let mut m = facts(vec![climb, accelerate, cruise]);
            m.climb_reference = super::super::climb_reference::reference_for("ATR72-600");
            m
        };
        // Reference from a 2,000 ft field at 22,800 kg: about 16 min.
        assert!(!run(&build(15.0)).has("MISSION-CLIMB-RATE"));
        assert_eq!(
            run(&build(18.5)).severity_of("MISSION-CLIMB-RATE"),
            Some(Severity::Medium)
        );
        assert_eq!(
            run(&build(21.0)).severity_of("MISSION-CLIMB-RATE"),
            Some(Severity::High)
        );
        // The acceleration after the top of climb is cruise, not climb.
        let facts = build(15.0);
        let climb_phase = phases(&facts)
            .into_iter()
            .find(|p| p.kind == Kind::Climb)
            .map_or(f64::NAN, |p| p.dt_s);
        assert_eq!(climb_phase, 15.0 * 60.0);
    }

    #[test]
    fn a_climb_ending_below_the_reference_altitude_is_extended_on_its_rate_trend() {
        // 2,000 -> 10,000 ft in 7 min, 10,000 -> 15,000 ft in 8 min (the rate falls from 1,143 to
        // 625 ft/min). The mean rate (867 ft/min) would put FL170 at 17.3 min, +8 % on the 16.0 min
        // reference: no finding. On the trend of the rate it is about 21 min, +30 %: HIGH.
        let mut low = seg(
            "climb_a",
            Kind::Climb,
            (0.0, 7.0),
            (2_000.0, 10_000.0),
            (0.0, 40.0),
            0.3,
        );
        low.mass0_kg = 22_800.0;
        let high = seg(
            "climb_b",
            Kind::Climb,
            (7.0, 15.0),
            (10_000.0, 15_000.0),
            (40.0, 100.0),
            0.3,
        );
        let cruise = seg(
            "cruise_step_1",
            Kind::Cruise,
            (15.0, 75.0),
            (15_000.0, 15_000.0),
            (100.0, 600.0),
            0.4,
        );
        let mut m = facts(vec![low, high, cruise]);
        m.climb_reference = super::super::climb_reference::reference_for("ATR72-600");
        assert_eq!(
            run(&m).severity_of("MISSION-CLIMB-RATE"),
            Some(Severity::High)
        );
    }

    #[test]
    fn a_long_level_speed_change_is_flagged_by_its_duration() {
        let build = |minutes: f64| {
            let mut change = seg(
                "speed_change_to_cruise_step_1",
                Kind::Cruise,
                (20.0, 20.0 + minutes),
                (28_000.0, 28_000.0),
                (100.0, 160.0),
                0.5,
            );
            change.speed_change = true;
            change.tas1_ms = change.tas0_ms + 40.0;
            facts(vec![change])
        };
        assert_eq!(
            run(&build(11.0)).severity_of("MISSION-SPEED-CHANGE"),
            Some(Severity::High)
        );
        assert_eq!(
            run(&build(6.0)).severity_of("MISSION-SPEED-CHANGE"),
            Some(Severity::Medium)
        );
        assert!(!run(&build(3.0)).has("MISSION-SPEED-CHANGE"));
    }

    #[test]
    fn a_level_speed_change_counts_inside_the_descent_it_sits_in() {
        // 20 min + 3 min level deceleration + 13 min descent: one 36 min
        // procedure, HIGH above 35 min; without the level leg it is 33 min.
        let mut level = seg(
            "speed_change_to_descent_2",
            Kind::Cruise,
            (170.0, 173.0),
            (15_000.0, 15_000.0),
            (1_800.0, 1_820.0),
            0.5,
        );
        level.speed_change = true;
        let build = |with_level: bool| {
            let mut parts = vec![
                seg(
                    "climb",
                    Kind::Climb,
                    (0.0, 17.0),
                    (0.0, 35_000.0),
                    (0.0, 140.0),
                    0.78,
                ),
                seg(
                    "cruise",
                    Kind::Cruise,
                    (17.0, 150.0),
                    (35_000.0, 35_000.0),
                    (140.0, 1_700.0),
                    0.78,
                ),
                seg(
                    "descent_1",
                    Kind::Descent,
                    (150.0, 170.0),
                    (35_000.0, 15_000.0),
                    (1_700.0, 1_800.0),
                    0.7,
                ),
            ];
            if with_level {
                parts.push(level.clone());
            }
            parts.push(seg(
                "descent_2",
                Kind::Descent,
                (
                    if with_level { 173.0 } else { 170.0 },
                    if with_level { 186.0 } else { 183.0 },
                ),
                (15_000.0, 0.0),
                (1_820.0, 1_900.0),
                0.5,
            ));
            facts(parts)
        };
        assert_eq!(
            run(&build(true)).severity_of("MISSION-DESCENT-TIME"),
            Some(Severity::High)
        );
        assert_ne!(
            run(&build(false)).severity_of("MISSION-DESCENT-TIME"),
            Some(Severity::High)
        );
    }

    #[test]
    fn healthy_profile_has_no_findings() {
        let out = run(&healthy());
        assert!(out.items().is_empty(), "{:?}", out.items());
    }

    #[test]
    fn climb_then_continuous_descent_without_cruise_is_high() {
        // The E195-E2 reference case: 24 min to 33,000 ft, 40 min descent, no cruise.
        let m = facts(vec![
            seg(
                "climb",
                Kind::Climb,
                (0.0, 24.0),
                (0.0, 33_000.0),
                (0.0, 200.0),
                0.70,
            ),
            seg(
                "descent",
                Kind::Descent,
                (24.0, 64.0),
                (33_000.0, 0.0),
                (200.0, 1_000.0),
                0.70,
            ),
        ]);
        let out = run(&m);
        assert_eq!(
            out.severity_of("MISSION-CRUISE-PRESENT"),
            Some(Severity::High)
        );
        assert!(out.has("MISSION-DESCENT-TIME"));
        assert!(
            out.has("MISSION-CLIMB-RATE"),
            "1,375 ft/min is below the owner band"
        );
        assert!(
            out.has("MISSION-DESCENT-RATE"),
            "825 ft/min is below the owner band"
        );
    }

    #[test]
    fn step_climb_on_a_short_route_is_flagged_and_on_a_long_route_is_not() {
        let step = |far: f64| {
            facts(vec![
                seg(
                    "climb",
                    Kind::Climb,
                    (0.0, 17.0),
                    (0.0, 33_000.0),
                    (0.0, 140.0),
                    0.70,
                ),
                seg(
                    "cruise",
                    Kind::Cruise,
                    (17.0, 100.0),
                    (33_000.0, 33_000.0),
                    (140.0, 140.0 + far / 2.0),
                    0.78,
                ),
                seg(
                    "step",
                    Kind::Climb,
                    (100.0, 106.0),
                    (33_000.0, 37_000.0),
                    (140.0 + far / 2.0, 160.0 + far / 2.0),
                    0.78,
                ),
                seg(
                    "cruise2",
                    Kind::Cruise,
                    (106.0, 200.0),
                    (37_000.0, 37_000.0),
                    (160.0 + far / 2.0, far),
                    0.78,
                ),
                seg(
                    "descent",
                    Kind::Descent,
                    (200.0, 222.0),
                    (37_000.0, 0.0),
                    (far, far + 150.0),
                    0.70,
                ),
            ])
        };
        assert!(run(&step(1_500.0)).has("MISSION-STEP-CLIMB"));
        assert!(!run(&step(5_000.0)).has("MISSION-STEP-CLIMB"));
    }

    #[test]
    fn boundary_jumps_and_mass_gain_are_flagged() {
        let mut m = healthy();
        m.segments[1].tas0_ms += 40.0;
        m.segments[2].max_mass_gain_kg = 50.0;
        let out = run(&m);
        assert!(out.has("MISSION-CONTINUITY"));
        assert_eq!(out.severity_of("MISSION-MONOTONIC"), Some(Severity::High));
    }

    #[test]
    fn mach_and_low_altitude_speed_and_lift_to_drag_are_judged() {
        let mut m = healthy();
        m.segments[1].max_mach = 0.84;
        m.segments[0].max_eas_low_alt_ms = 160.0;
        m.segments[1].ld_mean = 3.0;
        m.segments[1].cl_mean = 1.3;
        let out = run(&m);
        assert_eq!(out.severity_of("MISSION-MACH"), Some(Severity::Medium));
        assert!(out.has("MISSION-LOW-ALT-SPEED"));
        assert_eq!(out.severity_of("MISSION-CRUISE-LD"), Some(Severity::High));
        assert_eq!(out.severity_of("MISSION-CRUISE-CL"), Some(Severity::High));
    }

    #[test]
    fn partial_mission_is_high() {
        let mut m = healthy();
        m.scheduled = 6;
        m.fuel_exhausted = true;
        assert_eq!(
            run(&m).severity_of("MISSION-COMPLETE"),
            Some(Severity::High)
        );
    }
}
