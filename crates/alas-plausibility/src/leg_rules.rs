// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The take-off climb and the speed changes at the 10,000 ft limit, native against priced.
//!
//! Both mission models fly the same legs there: the take-off climb (a constant-speed climb at the
//! safety speed from the field to the acceleration height, then an acceleration along the climb to
//! the band speed with BADA's energy share, at the take-off rating), the climb-side speed change
//! at the limit (an acceleration along the climb, or a level change where the engine cannot
//! climb) and the descent-side level deceleration at flight idle. The rules here read the two
//! legs of each side and compare their entry and exit state, duration, ground distance and fuel.

use alas_opt::mdo::mission_model::{FlownStep, StepPhase};

use super::mission::{Kind, MissionFacts, SegmentFacts};
use super::{Findings, RuleInfo, Severity};

const FT: f64 = 0.3048;

/// The take-off climb of the two models agrees (see the module).
pub const P_TAKEOFF: RuleInfo = RuleInfo {
    id: "PRICED-NATIVE-TAKEOFF-CLIMB",
    statement: "The take-off climb (constant-speed climb at the safety speed to the acceleration height, then the acceleration along the climb to the band speed) of the priced model and of the native flight, same aeroplane and mass: entry and exit speed within 1 m/s, exit altitude within 15 m, duration, ground distance and fuel within 5 % (MEDIUM above, HIGH above 10 %)",
    source: "Both models fly the same two-part leg at the take-off rating, with BADA's energy share for the acceleration (alas_mission::segments::acceleration_climb_altitude_gain_m), so the end altitude follows from the speeds alone; the remaining differences are discretisation (the native leg is cut in pieces, the priced one in 8 steps per part) and the deck point each books. 1 m/s, 15 m (50 ft, the band stretch) and 5 % are engineering thresholds chosen up front, not published values",
};
/// The speed changes at the 10,000 ft limit of the two models agree (see the module).
pub const P_RESTRICTION: RuleInfo = RuleInfo {
    id: "PRICED-NATIVE-RESTRICTION",
    statement: "The speed change at the 10,000 ft limit, climb side (acceleration along the climb, or a level change) and descent side (level deceleration at flight idle), of the priced model and of the native flight: the same form of leg in both, entry altitude within 15 m, entry and exit speed within 3 m/s, duration, ground distance and fuel within 15 % (MEDIUM above, HIGH above 30 %)",
    source: "The native junction logic and the priced model cut the same legs (alas_config::mission::MINIMUM_SPEED_CHANGE_M_S, MINIMUM_PATH_CHANGE_RATE_M_S); the descent leg starts from the speed the descent schedule gives at the limit. The legs last 30-90 s and burn 5-500 kg, so a sub-rung of the priced ladder (1,000-3,000 ft) or a native piece moves them by a few percent: 15 % and 30 % are engineering thresholds chosen up front, not published values",
};

/// Entry and exit state and cost of one leg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LegFacts {
    /// Entry true airspeed, m/s.
    pub v0_m_s: f64,
    /// Exit true airspeed, m/s.
    pub v1_m_s: f64,
    /// Entry altitude, m.
    pub h0_m: f64,
    /// Exit altitude, m.
    pub h1_m: f64,
    /// Duration, s.
    pub time_s: f64,
    /// Ground distance, m.
    pub distance_m: f64,
    /// Fuel, kg.
    pub fuel_kg: f64,
}

impl LegFacts {
    fn of_native(first: &SegmentFacts, last: &SegmentFacts) -> Self {
        Self {
            v0_m_s: first.tas0_ms,
            v1_m_s: last.tas1_ms,
            h0_m: first.h0_m,
            h1_m: last.h1_m,
            time_s: last.t1_s - first.t0_s,
            distance_m: last.r1_m - first.r0_m,
            fuel_kg: first.mass0_kg - last.mass1_kg,
        }
    }

    fn of_priced(step: &FlownStep) -> Self {
        Self {
            v0_m_s: step.tas_m_s[0],
            v1_m_s: step.tas_m_s[1],
            h0_m: step.altitude_m[0],
            h1_m: step.altitude_m[1],
            time_s: step.time_s[1] - step.time_s[0],
            distance_m: step.range_m[1] - step.range_m[0],
            fuel_kg: step.mass_kg[0] - step.mass_kg[1],
        }
    }
}

/// The native take-off climb: the legs from the start of the flight to the end of the
/// acceleration piece (`takeoff_climb*` and `takeoff_path_entry*`).
pub fn native_takeoff_leg(native: &MissionFacts) -> Option<LegFacts> {
    let in_leg = |s: &SegmentFacts| {
        s.tag.starts_with("takeoff_climb") || s.tag.starts_with("takeoff_path_entry")
    };
    let first = native.segments.first().filter(|s| in_leg(s))?;
    let last = native.segments.iter().take_while(|s| in_leg(s)).last()?;
    Some(LegFacts::of_native(first, last))
}

/// The priced take-off climb: the first airborne step that accelerates in the take-off phase.
pub fn priced_takeoff_leg(steps: &[FlownStep]) -> Option<LegFacts> {
    steps
        .iter()
        .find(|s| s.phase == StepPhase::Takeoff && s.tas_m_s[1] > s.tas_m_s[0])
        .map(LegFacts::of_priced)
}

/// The native speed-change legs at the 10,000 ft limit: `(climb side, descent side)`.
pub fn native_restriction_legs(native: &MissionFacts) -> (Option<LegFacts>, Option<LegFacts>) {
    let limit_m = alas_config::mission::SPEED_RESTRICTION_ALTITUDE_M;
    let climb = native
        .segments
        .iter()
        .find(|s| {
            s.kind == Kind::Climb
                && s.tag.contains("_path_entry")
                && !s.tag.starts_with("takeoff")
                && s.tas1_ms > s.tas0_ms + 3.0
                && (s.h0_m - limit_m).abs() < 1_000.0 * FT
        })
        .or_else(|| {
            native.segments.iter().find(|s| {
                s.tag.starts_with("speed_change_to_")
                    && s.tas1_ms > s.tas0_ms + 3.0
                    && (s.h0_m - limit_m).abs() < 1_000.0 * FT
            })
        })
        .map(|s| LegFacts::of_native(s, s));
    let descent = native
        .segments
        .iter()
        .find(|s| {
            s.tag.starts_with("speed_change_to_")
                && s.tas1_ms < s.tas0_ms - 3.0
                && (s.h0_m - limit_m).abs() < 5.0
        })
        .map(|s| LegFacts::of_native(s, s));
    (climb, descent)
}

/// The priced speed-change legs at the 10,000 ft limit: `(climb side, descent side)`.
pub fn priced_restriction_legs(steps: &[FlownStep]) -> (Option<LegFacts>, Option<LegFacts>) {
    let limit_m = alas_config::mission::SPEED_RESTRICTION_ALTITUDE_M;
    let climb = steps
        .iter()
        .find(|s| {
            s.phase == StepPhase::Climb
                && s.tas_m_s[1] > s.tas_m_s[0] + 3.0
                && (s.altitude_m[0] - limit_m).abs() < 1_000.0 * FT
        })
        .map(LegFacts::of_priced);
    let descent = steps
        .iter()
        .find(|s| {
            s.phase == StepPhase::Descent
                && s.tas_m_s[1] < s.tas_m_s[0] - 3.0
                && (s.altitude_m[0] - limit_m).abs() < 5.0
                && (s.altitude_m[1] - s.altitude_m[0]).abs() < 1.0e-9
        })
        .map(LegFacts::of_priced);
    (climb, descent)
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

/// The largest relative gap among duration, distance and fuel, as `(name, gap)`.
fn worst_gap(native: &LegFacts, priced: &LegFacts) -> (&'static str, f64) {
    [
        ("duration", native.time_s, priced.time_s),
        ("distance", native.distance_m, priced.distance_m),
        ("fuel", native.fuel_kg, priced.fuel_kg),
    ]
    .into_iter()
    .map(|(name, a, b)| (name, (a - b) / b.abs().max(f64::MIN_POSITIVE)))
    .fold(("duration", 0.0), |worst, item| {
        if item.1.abs() > worst.1.abs() {
            item
        } else {
            worst
        }
    })
}

/// Compare the take-off climb and the speed changes at the limit of the native flight and of the
/// priced trip whose steps are `steps`, and record the gaps.
pub fn check_legs(out: &mut Findings, native: &MissionFacts, steps: &[FlownStep]) {
    check_takeoff(out, native_takeoff_leg(native), priced_takeoff_leg(steps));
    check_restriction(
        out,
        native_restriction_legs(native),
        priced_restriction_legs(steps),
    );
}

/// Compare the take-off climb of the two models and record the gaps.
pub fn check_takeoff(out: &mut Findings, native: Option<LegFacts>, priced: Option<LegFacts>) {
    let (native, priced) = match (native, priced) {
        (Some(n), Some(p)) => (n, p),
        (None, None) => return,
        (n, p) => {
            out.add(
                P_TAKEOFF.id,
                Severity::High,
                format!(
                    "the take-off climb is flown by one model only (native {}, priced {})",
                    n.is_some(),
                    p.is_some()
                ),
            );
            return;
        }
    };
    let mut problems = Vec::new();
    if (native.v0_m_s - priced.v0_m_s).abs() > 1.0 {
        problems.push(format!(
            "entry speed {:.1} m/s against {:.1} m/s",
            native.v0_m_s, priced.v0_m_s
        ));
    }
    if (native.v1_m_s - priced.v1_m_s).abs() > 1.0 {
        problems.push(format!(
            "exit speed {:.1} m/s against {:.1} m/s",
            native.v1_m_s, priced.v1_m_s
        ));
    }
    if (native.h1_m - priced.h1_m).abs() > 15.0 {
        problems.push(format!(
            "exit altitude {:.0} m against {:.0} m",
            native.h1_m, priced.h1_m
        ));
    }
    let (name, gap) = worst_gap(&native, &priced);
    let severity = graded(gap, 0.05, 0.10);
    if severity.is_some() {
        problems.push(format!("{name} {:+.1} %", 100.0 * gap));
    }
    if !problems.is_empty() {
        out.add(
            P_TAKEOFF.id,
            severity.unwrap_or(Severity::Medium),
            format!(
                "native {} against priced {}: {}",
                native.line("take-off climb"),
                priced.line("take-off climb"),
                problems.join("; ")
            ),
        );
    }
}

/// Compare the speed changes at the 10,000 ft limit of the two models and record the gaps.
pub fn check_restriction(
    out: &mut Findings,
    native: (Option<LegFacts>, Option<LegFacts>),
    priced: (Option<LegFacts>, Option<LegFacts>),
) {
    for (side, n, p) in [
        ("climb", native.0, priced.0),
        ("descent", native.1, priced.1),
    ] {
        let (n, p) = match (n, p) {
            (Some(n), Some(p)) => (n, p),
            (None, None) => continue,
            (n, p) => {
                out.add(
                    P_RESTRICTION.id,
                    Severity::High,
                    format!(
                        "the {side}-side speed change at the limit is flown by one model only (native {}, priced {})",
                        n.is_some(),
                        p.is_some()
                    ),
                );
                continue;
            }
        };
        let mut problems = Vec::new();
        if (n.h0_m - p.h0_m).abs() > 15.0 {
            problems.push(format!(
                "entry altitude {:.0} m against {:.0} m",
                n.h0_m, p.h0_m
            ));
        }
        if (n.v0_m_s - p.v0_m_s).abs() > 3.0 {
            problems.push(format!(
                "entry speed {:.1} m/s against {:.1} m/s",
                n.v0_m_s, p.v0_m_s
            ));
        }
        if (n.v1_m_s - p.v1_m_s).abs() > 3.0 {
            problems.push(format!(
                "exit speed {:.1} m/s against {:.1} m/s",
                n.v1_m_s, p.v1_m_s
            ));
        }
        let (name, gap) = worst_gap(&n, &p);
        let severity = graded(gap, 0.15, 0.30);
        if severity.is_some() {
            problems.push(format!("{name} {:+.1} %", 100.0 * gap));
        }
        if !problems.is_empty() {
            out.add(
                P_RESTRICTION.id,
                severity.unwrap_or(Severity::Medium),
                format!(
                    "{side} side, native {} against priced {}: {}",
                    n.line("leg"),
                    p.line("leg"),
                    problems.join("; ")
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leg(v1: f64, h1: f64, time: f64, distance: f64, fuel: f64) -> LegFacts {
        LegFacts {
            v0_m_s: 65.0,
            v1_m_s: v1,
            h0_m: 0.0,
            h1_m: h1,
            time_s: time,
            distance_m: distance,
            fuel_kg: fuel,
        }
    }

    #[test]
    fn legs_that_agree_record_nothing() {
        let mut out = Findings::new("case".to_owned());
        let a = leg(77.0, 450.0, 60.0, 3_000.0, 80.0);
        let b = leg(77.4, 455.0, 61.0, 3_050.0, 81.0);
        check_takeoff(&mut out, Some(a), Some(b));
        assert!(out.items().is_empty());
    }

    #[test]
    fn a_leg_that_differs_in_state_or_cost_is_recorded_with_its_grade() {
        let a = leg(77.0, 450.0, 60.0, 3_000.0, 80.0);
        let mut out = Findings::new("case".to_owned());
        check_takeoff(
            &mut out,
            Some(a),
            Some(leg(77.0, 450.0, 60.0, 3_000.0, 90.0)),
        );
        assert_eq!(out.items().len(), 1);
        assert_eq!(out.items()[0].severity, Severity::High);
        let mut out = Findings::new("case".to_owned());
        check_takeoff(
            &mut out,
            Some(a),
            Some(leg(80.0, 450.0, 60.0, 3_000.0, 80.0)),
        );
        assert_eq!(out.items().len(), 1);
        let mut out = Findings::new("case".to_owned());
        check_takeoff(&mut out, Some(a), None);
        assert_eq!(out.items()[0].severity, Severity::High);
    }

    #[test]
    fn a_restriction_side_flown_by_one_model_only_is_high() {
        let a = leg(179.0, 3_100.0, 40.0, 6_000.0, 50.0);
        let mut out = Findings::new("case".to_owned());
        check_restriction(&mut out, (Some(a), None), (None, None));
        assert_eq!(out.items().len(), 1);
        assert_eq!(out.items()[0].severity, Severity::High);
        let mut out = Findings::new("case".to_owned());
        check_restriction(&mut out, (Some(a), None), (Some(a), None));
        assert!(out.items().is_empty());
    }
}
