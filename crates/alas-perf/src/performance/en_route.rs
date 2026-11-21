// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! En-route one-engine-inoperative (OEI) obstacle clearance, CAT.POL.A.215
//! (Reg. (EU) 2023/217, Annex IV, text read in the EASA Easy Access Rules for Air
//! Operations, Mar. 2026, p. 1132; AMC1 CAT.POL.A.215, p. 1133).
//!
//! What the text prints:
//! * (b): "The gradient of the en-route net flight path shall be positive at least
//!   1 000 ft above all terrain and obstructions along the route within 9,3 km (5 NM)
//!   on either side of the intended track."
//! * (c): the net path "shall clear vertically, by at least 2 000 ft, all terrain and
//!   obstructions along the route within 9,3 km (5 NM) on either side of the intended
//!   track" (the drift-down case, flight continued to an aerodrome).
//! * (c)(5): "if the AFM does not contain en-route net flight path data, the gross OEI
//!   en-route flight path shall be reduced by a climb gradient of 1,1 % for two-engined
//!   aeroplanes, 1,4 % for three-engined aeroplanes, and 1,6 % for four-engined
//!   aeroplanes" (the same figures as CS 25.123(b)).
//! * (d): the width margins of (b) and (c) are increased to 18,5 km (10 NM) "if the
//!   navigational accuracy does not meet at least navigation specification RNAV 5".
//!
//! Model: the en-route net path begins where the net take-off path ends
//! ([`super::obstacle::take_off_path_end_m`], CS 25.111 (a), at the height the net take-off
//! path has there) and continues straight out along the survey track at the constant net
//! gradient `gamma`: with `gamma > 0` the 1 000 ft of (b) applies, with `gamma <= 0`
//! the path descends or levels (drift down) and the 2 000 ft of (c) applies. The gradient
//! is held constant (no thrust lapse with the altitude gained is modelled) and no wind,
//! fuel jettison or engine-failure point is modelled: the engine fails on the take-off
//! path, the most critical point for the terrain just beyond it. The survey is the
//! take-off one ([`DepartureSurvey`]); frame as in [`super::obstacle`]: distance `s`
//! along the runway heading from the start of the roll, heights above the aerodrome.
//! Not modelled: (a)'s positive gradient at 1 500 ft above the landing aerodrome, the
//! landing aerodrome criteria of (c)(4), and the navigation specification of (d).

use alas_config::airport_obstacles::DepartureSurvey;

use super::obstacle::{
    net_path_height_m, survey_reference_m, survey_symmetric_uncertainty_m, survey_uncertainty_m,
    take_off_path_end_m, NetPathInputs, MAX_ACCELERATION_HEIGHT_M, MIN_ACCELERATION_HEIGHT_M,
};

/// Vertical clearance over terrain while the net path has a positive gradient,
/// CAT.POL.A.215 (b): 1 000 ft.
pub const POSITIVE_GRADIENT_CLEARANCE_M: f64 = 1000.0 * 0.3048;
/// Vertical clearance over terrain for the drift-down path, CAT.POL.A.215 (c): 2 000 ft.
pub const DRIFT_DOWN_CLEARANCE_M: f64 = 2000.0 * 0.3048;
/// Half-width of the corridor, CAT.POL.A.215 (b), (c): 9,3 km (5 NM).
pub const CORRIDOR_HALF_WIDTH_M: f64 = 9_300.0;
/// Half-width of the corridor when the navigational accuracy does not meet RNAV 5,
/// CAT.POL.A.215 (d): 18,5 km (10 NM).
pub const CORRIDOR_HALF_WIDTH_WITHOUT_RNAV5_M: f64 = 18_500.0;
/// Step of the acceleration-height search, as in the take-off check: 25 ft.
const ACCELERATION_HEIGHT_STEP_M: f64 = 25.0 * 0.3048;

/// Gradient by which the gross OEI en-route flight path is reduced to the net path,
/// CAT.POL.A.215 (c)(5) (CS 25.123(b)): 1,1 %, 1,4 %, 1,6 % for two, three, four engines.
#[must_use]
pub fn en_route_net_path_reduction(n_engines: i64) -> Option<f64> {
    match n_engines {
        2 => Some(0.011),
        3 => Some(0.014),
        4 => Some(0.016),
        _ => None,
    }
}

/// What the en-route check depends on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnRouteInputs {
    /// The net take-off path the en-route path continues from (its gear time is
    /// taken as given: [`super::obstacle::GEAR_RETRACTION_TIME_S`] for the registered types).
    pub take_off: NetPathInputs,
    /// OEI en-route net gradient, m/m: the gross gradient in the en-route
    /// configuration less [`en_route_net_path_reduction`].
    pub net_gradient: f64,
    /// Half-width of the corridor, m.
    pub corridor_half_width_m: f64,
    /// First leg of the declared departure track, `(true heading in degrees, length in m)`,
    /// flown from the end of the take-off path: the straight-out survey describes the track
    /// for that leg only, and only if its heading stays within the surveyed width. `None`
    /// takes the straight-out track as an assumption for the whole survey.
    pub first_leg: Option<(f64, f64)>,
}

/// The outcomes of a margin that carries the dataset uncertainty and the survey
/// coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnRouteVerdict {
    /// Cleared by at least the symmetric uncertainty, over a surveyed corridor.
    Clears,
    /// Even the most favourable margin lies below minus the symmetric uncertainty:
    /// the surveyed part of the corridor alone already violates it.
    Violated,
    /// The margin lies inside the dataset uncertainty.
    Undetermined,
    /// The margin clears over what is surveyed, but the survey does not cover the
    /// corridor: terrain outside it is unknown. A diagnostic, never a pass.
    SurveyDoesNotCoverCorridor,
}

/// Least en-route margin over the surveyed corridor.
#[derive(Debug, Clone, PartialEq)]
pub struct EnRouteMargin {
    /// Margin the clearance needs, m (the symmetric dataset uncertainty).
    pub uncertainty_m: f64,
    /// Least vertical margin of the net path over the required clearance, m, at the
    /// gear retraction time that hurts most.
    pub margin_m: f64,
    /// The same at the gear retraction time that helps most, m.
    pub margin_best_m: f64,
    /// Most favourable margin plus its symmetric uncertainty, m; negative is a certain
    /// violation.
    pub violation_slack_m: f64,
    /// Clearance required, m (1 000 ft or 2 000 ft).
    pub required_clearance_m: f64,
    /// True when the net path descends or is level (the 2 000 ft case).
    pub drift_down: bool,
    /// Runway direction that gives the best margin.
    pub runway: String,
    /// Along-track position of the limiting cell or obstacle from the start of the roll, m.
    pub limiting_s_m: f64,
    /// Its height above the aerodrome, m.
    pub limiting_height_m: f64,
    /// Lateral offset of the limiting cell or obstacle, m.
    pub limiting_y_m: f64,
    /// Along-track end of the take-off path from the start of the roll, m.
    pub path_end_s_m: f64,
    /// Along-track edge of the surveyed terrain, m.
    pub survey_edge_s_m: f64,
    /// Lateral half-width the survey covers, m.
    pub covered_half_width_m: f64,
    /// Lateral half-width the corridor needs, m.
    pub corridor_half_width_m: f64,
}

impl EnRouteMargin {
    /// True when the survey covers the corridor laterally.
    #[must_use]
    pub fn corridor_covered(&self) -> bool {
        self.covered_half_width_m >= self.corridor_half_width_m
    }

    /// The verdict: violated is certain whatever lies outside the survey; a pass needs
    /// the corridor covered.
    #[must_use]
    pub fn verdict(&self) -> EnRouteVerdict {
        if self.violation_slack_m < 0.0 {
            EnRouteVerdict::Violated
        } else if !self.corridor_covered() {
            EnRouteVerdict::SurveyDoesNotCoverCorridor
        } else if self.margin_m >= self.uncertainty_m {
            EnRouteVerdict::Clears
        } else {
            EnRouteVerdict::Undetermined
        }
    }
}

/// What the en-route check found.
#[derive(Debug, Clone, PartialEq)]
pub enum EnRouteCheck {
    /// The inputs are not finite or physical.
    Invalid,
    /// The net take-off path never climbs to the end of the take-off path
    /// (non-positive gradient): the en-route phase is not reached and the take-off
    /// check governs the whole survey.
    NotReached,
    /// No surveyed terrain or obstacle lies beyond the end of the take-off path.
    NoSurveyBeyondPath {
        /// Along-track end of the take-off path, m.
        path_end_s_m: f64,
        /// Along-track edge of the surveyed terrain, m.
        survey_edge_s_m: f64,
    },
    /// The least margin over the surveyed corridor.
    Margin(EnRouteMargin),
}

fn survey_edge_s_m(survey: &DepartureSurvey) -> f64 {
    survey.terrain_s0_m + survey.terrain_max_elevation_m.len() as f64 * survey.cell_along_m
}

fn covered_half_width_m(survey: &DepartureSurvey) -> f64 {
    survey
        .lateral_centres_m
        .iter()
        .fold(0.0_f64, |widest, centre| widest.max(centre.abs()))
        + survey.cell_lateral_m / 2.0
}

/// Least margin of one direction for one acceleration height: `(margin, s, height, y)`.
fn direction_margin(
    survey: &DepartureSurvey,
    inputs: &EnRouteInputs,
    h_acc_m: f64,
) -> Option<(f64, f64, f64, f64)> {
    let take_off = &inputs.take_off;
    let end_d = take_off_path_end_m(take_off, h_acc_m);
    let s_end = take_off.takeoff_distance_m + end_d;
    let h_end = net_path_height_m(take_off, h_acc_m, end_d);
    let gamma = inputs.net_gradient;
    let clearance = if gamma > 0.0 {
        POSITIVE_GRADIENT_CLEARANCE_M
    } else {
        DRIFT_DOWN_CLEARANCE_M
    };
    let reference = survey_reference_m(survey, take_off.reference_elevation_m);
    let covered = covered_half_width_m(survey);
    let s_limit = inputs.first_leg.map_or(f64::INFINITY, |(heading, length)| {
        let delta = (heading - survey.heading_deg + 540.0).rem_euclid(360.0) - 180.0;
        if (delta.to_radians().sin() * length).abs() > covered {
            s_end
        } else {
            s_end + length
        }
    });
    let mut worst: Option<(f64, f64, f64, f64)> = None;
    // The highest ground lies somewhere in `s_lo ..= s_hi`: the path (linear) is read at
    // its least there, an end of the range.
    let mut consider = |s_lo: f64, s_hi: f64, top_msl: f64, y: f64| {
        if s_hi <= s_end || s_lo >= s_limit {
            return;
        }
        let s_lo = s_lo.max(s_end);
        let s_hi = s_hi.min(s_limit);
        if s_hi <= s_lo {
            return;
        }
        let (path, s) = [s_lo, s_hi]
            .into_iter()
            .map(|s| (h_end + gamma * (s - s_end), s))
            .fold((f64::INFINITY, s_lo), |a, b| if b.0 < a.0 { b } else { a });
        let top = top_msl - reference;
        let margin = path - clearance - top;
        if worst.is_none_or(|(best, ..)| margin < best) {
            worst = Some((margin, s, top, y));
        }
    };
    for (index, row) in survey.terrain_max_elevation_m.iter().enumerate() {
        let near = survey.terrain_s0_m + index as f64 * survey.cell_along_m;
        for (&centre, &top) in survey.lateral_centres_m.iter().zip(row) {
            if centre.abs() - survey.cell_lateral_m / 2.0 < inputs.corridor_half_width_m {
                consider(near, near + survey.cell_along_m, top, centre);
            }
        }
    }
    for obstacle in &survey.obstacles {
        let s_far = obstacle.s_max_m.unwrap_or(obstacle.s_m).max(obstacle.s_m);
        if obstacle.y_m.abs() < inputs.corridor_half_width_m {
            consider(obstacle.s_m, s_far, obstacle.top_m, obstacle.y_m);
        }
    }
    worst
}

/// Margin of the 35 ft take-off rule at one acceleration height; infinite when no
/// accountable obstacle lies on the take-off path.
fn direction_margin_take_off(
    survey: &DepartureSurvey,
    take_off: &NetPathInputs,
    h_acc_m: f64,
) -> f64 {
    super::obstacle::direction_margin(survey, take_off, h_acc_m, false)
        .map_or(f64::INFINITY, |m| m.0)
}

/// The best margin over directions and acceleration heights at one gear time.
fn best_margin(surveys: &[DepartureSurvey], inputs: &EnRouteInputs) -> Option<EnRouteMargin> {
    let mut best: Option<EnRouteMargin> = None;
    let mut best_key = f64::NEG_INFINITY;
    for survey in surveys {
        let mut h_acc = MIN_ACCELERATION_HEIGHT_M;
        while h_acc <= MAX_ACCELERATION_HEIGHT_M + 1e-9 {
            let step_h = h_acc;
            h_acc += ACCELERATION_HEIGHT_STEP_M;
            let Some((margin, s, top, y)) = direction_margin(survey, inputs, step_h) else {
                continue;
            };
            let end_d = take_off_path_end_m(&inputs.take_off, step_h);
            let symmetric = survey_symmetric_uncertainty_m(survey);
            let slack = margin + symmetric;
            // The operator flies one acceleration height: it must serve the 35 ft take-off
            // rule up to the end of the take-off path and this rule beyond it, so the
            // height is the one with the best of the two margins taken together.
            let take_off_margin = direction_margin_take_off(survey, &inputs.take_off, step_h);
            let key = margin.min(take_off_margin);
            if best.is_none() || key > best_key {
                best_key = key;
                best = Some(EnRouteMargin {
                    uncertainty_m: survey_uncertainty_m(survey),
                    margin_m: margin,
                    margin_best_m: margin,
                    violation_slack_m: slack,
                    required_clearance_m: if inputs.net_gradient > 0.0 {
                        POSITIVE_GRADIENT_CLEARANCE_M
                    } else {
                        DRIFT_DOWN_CLEARANCE_M
                    },
                    drift_down: inputs.net_gradient <= 0.0,
                    runway: survey.runway.clone(),
                    limiting_s_m: s,
                    limiting_height_m: top,
                    limiting_y_m: y,
                    path_end_s_m: inputs.take_off.takeoff_distance_m + end_d,
                    survey_edge_s_m: survey_edge_s_m(survey),
                    covered_half_width_m: covered_half_width_m(survey),
                    corridor_half_width_m: inputs.corridor_half_width_m,
                });
            }
        }
    }
    best
}

/// The en-route margin: the best over the surveyed take-off directions and acceleration
/// heights (the operator selects them), the worst over the gear retraction time range.
#[must_use]
pub fn en_route_clearance_margin(
    surveys: &[DepartureSurvey],
    inputs: &EnRouteInputs,
) -> EnRouteCheck {
    let t = &inputs.take_off;
    let finite = [
        t.toda_m,
        t.takeoff_distance_m,
        t.reference_elevation_m,
        t.wingspan_m,
        t.v2_m_s,
        t.v_final_takeoff_m_s,
        t.first_segment_net_gradient,
        t.second_segment_net_gradient,
        t.final_segment_net_gradient,
        inputs.net_gradient,
        inputs.corridor_half_width_m,
    ]
    .iter()
    .all(|value| value.is_finite());
    if !finite || t.v2_m_s <= 0.0 || inputs.corridor_half_width_m <= 0.0 {
        return EnRouteCheck::Invalid;
    }
    if take_off_path_end_m(t, MAX_ACCELERATION_HEIGHT_M).is_infinite() {
        return EnRouteCheck::NotReached;
    }
    let best = best_margin(surveys, inputs);
    match best {
        Some(margin) => EnRouteCheck::Margin(margin),
        None => {
            let edge = surveys.iter().map(survey_edge_s_m).fold(0.0_f64, f64::max);
            EnRouteCheck::NoSurveyBeyondPath {
                path_end_s_m: t.takeoff_distance_m
                    + take_off_path_end_m(t, MAX_ACCELERATION_HEIGHT_M),
                survey_edge_s_m: edge,
            }
        }
    }
}

// A test asserts on values it built here, so a failed unwrap is the assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn take_off() -> NetPathInputs {
        NetPathInputs {
            toda_m: 3000.0,
            takeoff_distance_m: 2600.0,
            gear_retraction_time_s: 10.0,
            reference_elevation_m: 100.0,
            wingspan_m: 36.0,
            v2_m_s: 75.0,
            v_final_takeoff_m_s: 82.0,
            first_segment_net_gradient: -0.008,
            second_segment_net_gradient: 0.024,
            final_segment_net_gradient: 0.004,
        }
    }

    /// One terrain row of a flat strip at `top_msl` over `rows` cells of 250 m from
    /// `s0`, lateral centres `+-half`.
    fn survey(top_msl: f64, rows: usize, half: f64) -> DepartureSurvey {
        let centres: Vec<f64> = (0..)
            .map(|k| -half + 30.0 * f64::from(k))
            .take_while(|c| *c <= half)
            .collect();
        DepartureSurvey {
            runway: "09".to_owned(),
            heading_deg: 90.0,
            reference_elevation_m: None,
            terrain_source: String::new(),
            vertical_uncertainty_m: Some(10.0),
            one_sided_bias_m: None,
            completeness_allowance_m: None,
            reference_minus_aerodrome_elevation_m: None,
            terrain_s0_m: 3000.0,
            terrain_max_elevation_m: vec![vec![top_msl; centres.len()]; rows],
            cell_along_m: 250.0,
            lateral_centres_m: centres,
            cell_lateral_m: 30.0,
            obstacles: Vec::new(),
            provenance: String::new(),
        }
    }

    fn inputs(net: f64, half_width: f64) -> EnRouteInputs {
        EnRouteInputs {
            take_off: take_off(),
            net_gradient: net,
            corridor_half_width_m: half_width,
            first_leg: None,
        }
    }

    fn margin(surveys: &[DepartureSurvey], i: &EnRouteInputs) -> EnRouteMargin {
        match en_route_clearance_margin(surveys, i) {
            EnRouteCheck::Margin(m) => m,
            other => panic!("expected a margin, got {other:?}"),
        }
    }

    #[test]
    fn the_net_reduction_is_the_printed_one() {
        assert_eq!(en_route_net_path_reduction(2), Some(0.011));
        assert_eq!(en_route_net_path_reduction(3), Some(0.014));
        assert_eq!(en_route_net_path_reduction(4), Some(0.016));
        assert_eq!(en_route_net_path_reduction(1), None);
    }

    /// Positive gradient: 1 000 ft; zero or negative: 2 000 ft.
    #[test]
    fn the_clearance_is_1000_ft_climbing_and_2000_ft_descending() {
        let s = survey(100.0 + 300.0, 200, 600.0);
        let up = margin(std::slice::from_ref(&s), &inputs(0.004, 600.0));
        assert!(!up.drift_down);
        assert!((up.required_clearance_m - 304.8).abs() < 1e-9);
        let down = margin(std::slice::from_ref(&s), &inputs(-0.004, 600.0));
        assert!(down.drift_down);
        assert!((down.required_clearance_m - 609.6).abs() < 1e-9);
        assert!(down.margin_m < up.margin_m);
    }

    /// The path starts at the end of the take-off path and the margin follows it.
    #[test]
    fn the_margin_is_the_path_over_the_clearance_over_the_ground() {
        let s = survey(100.0 + 300.0, 200, 600.0);
        let i = inputs(0.004, 600.0);
        let m = margin(std::slice::from_ref(&s), &i);
        // Level ground 300 m up: the limiting cell is the nearest (the path climbs).
        let h_acc = MAX_ACCELERATION_HEIGHT_M;
        let end_d = take_off_path_end_m(&i.take_off, h_acc);
        assert!(m.path_end_s_m > i.take_off.takeoff_distance_m);
        assert!(m.limiting_s_m >= m.path_end_s_m - 1e-6);
        assert!(m.margin_m.is_finite());
        assert!(end_d.is_finite());
    }

    /// Ground that violates in the strip is a certain violation even though the strip
    /// does not cover the corridor; ground that clears is not a pass.
    #[test]
    fn an_uncovered_corridor_is_a_diagnostic_never_a_pass() {
        let high = survey(100.0 + 900.0, 200, 600.0);
        let m = margin(&[high], &inputs(0.004, CORRIDOR_HALF_WIDTH_M));
        assert!(!m.corridor_covered());
        assert_eq!(m.verdict(), EnRouteVerdict::Violated);
        let low = survey(100.0 + 50.0, 200, 600.0);
        let m = margin(&[low], &inputs(0.004, CORRIDOR_HALF_WIDTH_M));
        assert!(m.margin_m > m.uncertainty_m);
        assert_eq!(m.verdict(), EnRouteVerdict::SurveyDoesNotCoverCorridor);
        let covered = survey(100.0 + 50.0, 200, 9_330.0);
        let m = margin(&[covered], &inputs(0.004, CORRIDOR_HALF_WIDTH_M));
        assert_eq!(m.verdict(), EnRouteVerdict::Clears);
    }

    #[test]
    fn a_survey_that_ends_inside_the_take_off_path_has_nothing_beyond_it() {
        let short = survey(100.0 + 900.0, 2, 600.0);
        assert!(matches!(
            en_route_clearance_margin(&[short], &inputs(0.004, 600.0)),
            EnRouteCheck::NoSurveyBeyondPath { .. }
        ));
    }

    #[test]
    fn a_non_finite_gradient_is_invalid() {
        let s = survey(100.0, 200, 600.0);
        assert_eq!(
            en_route_clearance_margin(&[s], &inputs(f64::NAN, 600.0)),
            EnRouteCheck::Invalid
        );
    }
}
