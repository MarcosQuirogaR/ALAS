// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Take-off obstacle clearance, CAT.POL.A.210 (Reg. (EU) 965/2012, Annex IV,
//! text read in the EASA Easy Access Rules for Air Operations, Mar. 2026) with
//! the CS 25.115 net flight path.
//!
//! Frame: distances along the take-off direction from the start of the roll
//! (the net path from the end of the take-off distance, the horizontal
//! accountability width from the end of the take-off distance available,
//! TODA), heights above the elevation of the
//! departure aerodrome, lateral offsets `y` to the right of the extended
//! centreline; SI units.
//!
//! Net take-off flight path (AMC1 CAT.POL.A.210 (a)(1): it begins at 35 ft
//! above the runway or clearway at the end of the take-off distance, the
//! longer of 115 % of the all-engine distance and the continued OEI
//! distance to 35 ft, AMC1 (a)(2)):
//!
//! 1. first segment, from 35 ft to the gear retracted, flown at `V2` (CS
//!    25.111(c)(2): `V2` is reached before 35 ft and held to 400 ft): the
//!    second-segment gradient less the gear drag increment over the lift
//!    coefficient at `V2` (the CS 25.121(a) gradient at `VLOF` is a separate
//!    requirement with its own row), reduced by the CS 25.115(b) decrement
//!    (0.8 / 0.9 / 1.0 %), over `V2 * t`, with `t` the part of the retraction
//!    time that is left at the screen: the retraction is timed from lift-off
//!    ([`GEAR_RETRACTION_TIME_S`], an uncertain input flagged as such) and the
//!    continued take-off has been airborne for its air leg when it passes
//!    35 ft;
//! 2. second segment at the net gradient `gamma_net` at `V2` up to the
//!    acceleration height `h_acc`;
//! 3. level acceleration from `V2` to the final take-off speed `1.18 VSR`
//!    (CS 25.107(g)) at `a = g * gamma_net` (the net level-flight
//!    acceleration, CS 25.115(b)(2));
//! 4. final segment at its net gradient.
//!
//! The acceleration height is chosen between 400 ft (CS 25.111(c)(3)
//! minimum) and 1500 ft (the end of the final segment, CS 25.111(c)(4)) as
//! the one that clears the obstacles best; the operator may select it per
//! aerodrome. A straight departure along the runway heading is assumed (no
//! track change, so the (b)(6) lateral cut-off applies); the better of the
//! two directions of the surveyed runway is taken, as the operator selects
//! the take-off direction.
//!
//! The net take-off flight path ends where the take-off path does: CS 25.111 (a)
//! (CS-25 Amdt 27, p. 138) extends the take-off path "to a point in the take-off at
//! which the aeroplane is 457 m (1500 ft) above the take-off surface, or at which the
//! transition from the take-off to the en-route configuration is completed and VFTO is
//! reached, whichever point is higher"; CS 25.115 (b) reduces that path. Beyond it the
//! obstacle clearance is the en-route one (CAT.POL.A.210 (c) names CAT.POL.A.215, 1000 ft
//! over terrain), not the 35 ft of (a), so a cell or obstacle past
//! [`take_off_path_end_m`] is not a take-off obstacle.
//!
//! Which obstacles count (CAT.POL.A.210 (a), (b)(6)): an obstacle at `D`
//! is cleared by 35 ft vertically or by `90 m + 0.125 D` horizontally, or
//! `60 m + b/2 + 0.125 D` when the wingspan `b` is under 60 m; obstacles
//! beyond 600 m laterally (the "all other conditions" cut-off of
//! (b)(6)(ii)) are not considered, 300 m needing a navigation accuracy the
//! model does not demonstrate.

use alas_config::airport_obstacles::DepartureSurvey;

/// Required vertical clearance over an obstacle: 35 ft.
pub const OBSTACLE_VERTICAL_CLEARANCE_M: f64 = 35.0 * 0.3048;
/// Height of the start of the net path at the end of the take-off distance: 35 ft.
pub const NET_PATH_START_HEIGHT_M: f64 = 35.0 * 0.3048;
/// Lowest acceleration height: 400 ft (CS 25.111(c)(3)).
pub const MIN_ACCELERATION_HEIGHT_M: f64 = 400.0 * 0.3048;
/// Height above the take-off surface at which the take-off path ends once the
/// transition to the en-route configuration is complete: 457 m (1500 ft),
/// CS 25.111 (a).
pub const TAKE_OFF_PATH_END_HEIGHT_M: f64 = 1500.0 * 0.3048;
/// Lowest height of the net path from which a track change is allowed: 50 ft
/// (CAT.POL.A.210 (b)(2), or half the wingspan when that is more).
pub const MIN_TURN_HEIGHT_M: f64 = 50.0 * 0.3048;
/// Highest acceleration height: 1500 ft, the end of the final segment
/// (CS 25.111(c)(4)).
pub const MAX_ACCELERATION_HEIGHT_M: f64 = 1500.0 * 0.3048;
/// Lateral cut-off of obstacle accountability for a straight departure,
/// CAT.POL.A.210 (b)(6)(ii).
pub const LATERAL_CUTOFF_M: f64 = 600.0;
/// Vertical uncertainty taken for a survey that states none: the SRTM class
/// (USGS "closer to +-10 m", VE90) plus the 0.1 m quantisation, m.
pub const DEFAULT_VERTICAL_UNCERTAINTY_M: f64 = 10.1;
/// A survey whose terrain reference disagrees with the published aerodrome
/// elevation by more than this, m, carries the disagreement as extra
/// uncertainty (RJTT: the tile reads the reclaimed runway as sea).
pub const DATUM_OFFSET_THRESHOLD_M: f64 = 2.0;
/// Time from lift-off to the gear retracted, s: the delay before retraction starts plus the
/// retraction sequence. One value, each part sourced; the sequence is a GENERIC figure, flagged:
/// no per-type time is public (no Airbus or Boeing airport-planning document, TCDS or FCOM
/// excerpt for the registered types states one; searched in Torenbeek 2013,
/// Raymer, Roskam, Jenkinson, Obert and Sadraey, none gives a duration).
/// - Delay 3 s: Torenbeek, Advanced Aircraft Design (2013), Sec. 9.3.2, p. 266: "Undercarriage
///   retraction begins at least three seconds after lift-off, the condition of full retraction
///   denotes the end of the first segment"; CS-25 AMC 25.111(b), footnote 2 (average
///   demonstrated delay, not less than 3 s). The floor is taken.
/// - Sequence 12 s: BEA report on F-BTSC (Concorde, 25 July 2000), section 1.6, note under
///   Figure 2: "a complete gear retraction sequence lasts, with only one pump, about twelve
///   seconds" (doors included). Concorde with one pump is a slow case for a modern twin, so
///   the value is conservative for the registered types; it is the only complete-sequence
///   figure read on a page. The type's own AMM (ATA 32) time replaces it when sourced.
pub const GEAR_RETRACTION_TIME_S: f64 = 3.0 + 12.0;
/// Step of the acceleration-height search, 25 ft.
const ACCELERATION_HEIGHT_STEP_M: f64 = 25.0 * 0.3048;
const G: f64 = super::G;

/// What the net path depends on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetPathInputs {
    /// Take-off distance available of the surveyed runway, m: the reference
    /// of the horizontal clearance.
    pub toda_m: f64,
    /// Take-off distance to 35 ft from the start of the roll, m: where the
    /// net path begins (AMC1 CAT.POL.A.210 (a)(1)).
    pub takeoff_distance_m: f64,
    /// Elevation of the departure aerodrome, m above mean sea level: the
    /// reference of a survey that carries none of its own.
    pub reference_elevation_m: f64,
    /// Wingspan, m.
    pub wingspan_m: f64,
    /// Take-off safety speed `V2` (true), m/s.
    pub v2_m_s: f64,
    /// Final take-off speed (true), m/s.
    pub v_final_takeoff_m_s: f64,
    /// Time from the 35 ft screen (the start of the net path) to the gear
    /// retracted, s: the retraction time from lift-off
    /// ([`GEAR_RETRACTION_TIME_S`]) less the time the continued take-off is
    /// airborne before the screen.
    pub gear_retraction_time_s: f64,
    /// Net gradient of the first segment (gear down), m/m.
    pub first_segment_net_gradient: f64,
    /// Net second-segment gradient the installed thrust delivers, m/m.
    pub second_segment_net_gradient: f64,
    /// Net final-segment gradient, m/m.
    pub final_segment_net_gradient: f64,
}

/// Elevation, m in the survey's terrain datum, of the surface the net path is measured above:
/// the aerodrome. The terrain tiles are orthometric (MSL) heights; where the tile reads the
/// aerodrome off the published elevation by more than [`DATUM_OFFSET_THRESHOLD_M`]
/// (`reference_minus_aerodrome_elevation_m`, RJTT: the tile reads the reclaimed runway as sea,
/// -6.1 m) the offset is a known correction, applied to the reference here, not an
/// uncertainty added to the margin. The survey's metadata names no other vertical datum
/// (SRTM and EU-DEM are EGM96/EGM2008 orthometric, the aerodrome elevation MSL), so there is
/// no datum shift to convert beyond this.
#[must_use]
pub fn survey_reference_m(survey: &DepartureSurvey, aerodrome_elevation_m: f64) -> f64 {
    let reference = survey
        .reference_elevation_m
        .unwrap_or(aerodrome_elevation_m);
    survey
        .reference_minus_aerodrome_elevation_m
        .filter(|d| d.is_finite() && d.abs() > DATUM_OFFSET_THRESHOLD_M)
        .map_or(reference, |offset| reference - offset)
}

/// SYMMETRIC vertical uncertainty of a survey's margin, m: the dataset's published accuracy
/// (LE90) of a terrain-minus-reference difference, and nothing else; the datum offset of
/// [`survey_reference_m`] is corrected, not added. The true height may lie on either side, so
/// it widens both the "clears" and the "violated" test.
#[must_use]
pub fn survey_symmetric_uncertainty_m(survey: &DepartureSurvey) -> f64 {
    survey
        .vertical_uncertainty_m
        .filter(|u| u.is_finite() && *u > 0.0)
        .unwrap_or(DEFAULT_VERTICAL_UNCERTAINTY_M)
}

/// Allowance an UNVERIFIED survey would add to a clearance, m: mobile objects the
/// terrain data omit, the sampling of a footprint maximum, and the height of
/// trees and buildings the data may omit. Each can only make the true height
/// larger. This is a diagnostic, not part of the margin the requirement
/// needs: CAT.POL.A.210 (a) asks 35 ft and no regulation prints an allowance
/// for incomplete obstacle data. Of the generator's figures, 14 CFR 77.9(c)
/// (23 ft) is the traverse-way height of a railroad and applies to the cells
/// that hold a road or railway, which the survey does not identify; the
/// sampling figure is a measurement at one aerodrome (EGLL) and the completeness
/// figure bounds no printed accuracy statement. They are reported, never imposed.
/// A non-finite or negative entry counts as none.
#[must_use]
pub fn survey_unverified_allowance_m(survey: &DepartureSurvey) -> f64 {
    [survey.one_sided_bias_m, survey.completeness_allowance_m]
        .into_iter()
        .flatten()
        .filter(|value| value.is_finite() && *value > 0.0)
        .sum()
}

/// Margin a survey needs to certify a clearance, m: the symmetric uncertainty
/// of the dataset (its published accuracy and the datum disagreement). The
/// unverified-survey allowance ([`survey_unverified_allowance_m`]) is reported
/// beside it and not added.
#[must_use]
pub fn survey_uncertainty_m(survey: &DepartureSurvey) -> f64 {
    survey_symmetric_uncertainty_m(survey)
}

/// The three outcomes of a margin that carries an uncertainty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObstacleVerdict {
    /// The margin exceeds the symmetric uncertainty: the net path clears.
    Clears,
    /// The margin is below minus the symmetric uncertainty: the net path does not
    /// clear.
    Violated,
    /// The margin lies between: the data cannot decide; this is an evidence gap,
    /// never a pass.
    Undetermined,
}

/// The outcome of the obstacle check.
#[derive(Debug, Clone, PartialEq)]
pub struct ObstacleMargin {
    /// Margin a clearance needs, m: the symmetric uncertainty of the dataset
    /// ([`survey_uncertainty_m`]).
    pub uncertainty_m: f64,
    /// The symmetric uncertainty, m (equal to `uncertainty_m`).
    pub symmetric_uncertainty_m: f64,
    /// Allowance the survey would add if it were verified complete, m
    /// ([`survey_unverified_allowance_m`]): reported, never part of the verdict.
    pub unverified_allowance_m: f64,
    /// Margin at the gear retraction time that helps most, m; `margin_m` is the
    /// one at the time that hurts most.
    pub margin_best_m: f64,
    /// Margin `margin_best_m` needs to certify a clearance, m: its own direction,
    /// datum and bias.
    pub uncertainty_best_m: f64,
    /// Most favourable value over directions, acceleration heights and gear times
    /// of the margin plus its symmetric uncertainty, m: the net path is certainly
    /// violated when this is negative.
    pub violation_slack_m: f64,
    /// Least vertical margin of the net path over any accountable obstacle
    /// beyond the 35 ft requirement, m; negative is a violation.
    pub margin_m: f64,
    /// Take-off direction that gives the best margin.
    pub runway: String,
    /// Acceleration height above the aerodrome that gives it, m.
    pub acceleration_height_m: f64,
    /// Distance of the limiting obstacle beyond the end of the take-off
    /// distance (where the net path starts), m.
    pub limiting_distance_m: f64,
    /// Height of the limiting obstacle above the aerodrome, m.
    pub limiting_height_m: f64,
}

impl ObstacleMargin {
    /// Clears when the margin is at least the symmetric uncertainty, violated
    /// when even the most favourable margin lies below minus the symmetric
    /// uncertainty, undetermined in between.
    #[must_use]
    pub fn verdict(&self) -> ObstacleVerdict {
        if self.margin_m >= self.uncertainty_m {
            ObstacleVerdict::Clears
        } else if self.violation_slack_m < 0.0 {
            ObstacleVerdict::Violated
        } else {
            ObstacleVerdict::Undetermined
        }
    }
}

/// Obstacle semi-width for the horizontal clearance of CAT.POL.A.210 (a)
/// at distance `d_m` beyond the end of the TODA, bounded by the lateral
/// cut-off of (b)(6)(ii).
pub fn accountability_semi_width_m(wingspan_m: f64, d_m: f64) -> f64 {
    let clearance = if wingspan_m < 60.0 {
        60.0 + wingspan_m / 2.0 + 0.125 * d_m
    } else {
        90.0 + 0.125 * d_m
    };
    clearance.min(LATERAL_CUTOFF_M)
}

/// Distance from the start of the roll at which the accountable strip first reaches the
/// lateral offset `edge_m` (the inner edge of a cell, or an obstacle's offset): the strip
/// widens with the distance beyond the TODA ([`accountability_semi_width_m`]), so ground
/// at the edge of it counts only from there on, not from the near edge of its cell.
/// negative infinity when the edge is inside the strip at the TODA already (nothing is
/// trimmed); infinite beyond the 600 m cut-off.
fn strip_entry_s_m(wingspan_m: f64, toda_m: f64, edge_m: f64) -> f64 {
    if edge_m >= LATERAL_CUTOFF_M {
        return f64::INFINITY;
    }
    let at_toda = accountability_semi_width_m(wingspan_m, 0.0);
    if edge_m < at_toda {
        return f64::NEG_INFINITY;
    }
    // Inverse of `at_toda + 0.125 d` (CAT.POL.A.210 (a)).
    toda_m + (edge_m - at_toda) / 0.125
}

/// Height of the net path above the aerodrome at distance `d_m` beyond the end
/// of the take-off distance for the acceleration height `h_acc_m`.
pub fn net_path_height_m(inputs: &NetPathInputs, h_acc_m: f64, d_m: f64) -> f64 {
    let d1 = (inputs.v2_m_s * inputs.gear_retraction_time_s).max(0.0);
    let g1 = inputs.first_segment_net_gradient;
    let g2 = inputs.second_segment_net_gradient;
    let gf = inputs.final_segment_net_gradient;
    if d_m <= d1 {
        return NET_PATH_START_HEIGHT_M + g1 * d_m;
    }
    let h1 = NET_PATH_START_HEIGHT_M + g1 * d1;
    if g2 <= 0.0 {
        return h1 + g2 * (d_m - d1);
    }
    let d_a = d1 + ((h_acc_m - h1) / g2).max(0.0);
    if d_m <= d_a {
        return h1 + g2 * (d_m - d1);
    }
    let h_a = h1 + g2 * (d_a - d1);
    let level =
        ((inputs.v_final_takeoff_m_s.powi(2) - inputs.v2_m_s.powi(2)).max(0.0)) / (2.0 * G * g2);
    if d_m <= d_a + level {
        return h_a;
    }
    h_a + gf * (d_m - d_a - level)
}

/// Least height of the net path over `d_lo_m ..= d_hi_m` beyond the end of the
/// take-off distance, m, and the distance where it occurs. The path is piecewise
/// linear, so the least height is at an end of the range or at a break of the
/// path; a descending first segment (gradient reduced below zero by
/// CS 25.115(b)) puts it at the far end, not the near one.
fn net_path_min_height_m(
    inputs: &NetPathInputs,
    h_acc_m: f64,
    d_lo_m: f64,
    d_hi_m: f64,
) -> (f64, f64) {
    let d1 = (inputs.v2_m_s * inputs.gear_retraction_time_s).max(0.0);
    let g2 = inputs.second_segment_net_gradient;
    let h1 = NET_PATH_START_HEIGHT_M + inputs.first_segment_net_gradient * d1;
    let mut breaks = [d1, d1, d1];
    if g2 > 0.0 {
        let d_a = d1 + ((h_acc_m - h1) / g2).max(0.0);
        let level = ((inputs.v_final_takeoff_m_s.powi(2) - inputs.v2_m_s.powi(2)).max(0.0))
            / (2.0 * G * g2);
        breaks = [d1, d_a, d_a + level];
    }
    let mut least = (net_path_height_m(inputs, h_acc_m, d_lo_m), d_lo_m);
    for d in std::iter::once(d_hi_m).chain(breaks) {
        if d > d_lo_m && d <= d_hi_m {
            let height = net_path_height_m(inputs, h_acc_m, d);
            if height < least.0 {
                least = (height, d);
            }
        }
    }
    least
}

/// Distance beyond the end of the take-off distance, m, at which the net take-off
/// path ends for the acceleration height `h_acc_m`: the higher of the point where
/// the path is 1500 ft above the take-off surface and the point where the transition
/// to the en-route configuration is completed and the final take-off speed reached
/// (the end of the level acceleration), CS 25.111 (a). Infinite when the path never
/// climbs to 1500 ft (non-positive gradient on the segment that must climb), so
/// nothing is cut off.
pub fn take_off_path_end_m(inputs: &NetPathInputs, h_acc_m: f64) -> f64 {
    let d1 = (inputs.v2_m_s * inputs.gear_retraction_time_s).max(0.0);
    let h1 = NET_PATH_START_HEIGHT_M + inputs.first_segment_net_gradient * d1;
    let g2 = inputs.second_segment_net_gradient;
    let gf = inputs.final_segment_net_gradient;
    if g2 <= 0.0 {
        return f64::INFINITY;
    }
    let d_a = d1 + ((h_acc_m - h1) / g2).max(0.0);
    let level =
        ((inputs.v_final_takeoff_m_s.powi(2) - inputs.v2_m_s.powi(2)).max(0.0)) / (2.0 * G * g2);
    let d_transition = d_a + level;
    let h_a = h1 + g2 * (d_a - d1);
    if h_a >= TAKE_OFF_PATH_END_HEIGHT_M {
        return d_transition;
    }
    if gf <= 0.0 {
        return f64::INFINITY;
    }
    d_transition.max(d_transition + (TAKE_OFF_PATH_END_HEIGHT_M - h_a) / gf)
}

/// Distance beyond the end of the take-off distance, m, at which the net take-off path first
/// reaches the height from which a track change is allowed: half the wingspan, not less than
/// 50 ft, above the elevation of the end of the TORA (CAT.POL.A.210 (b)(2)). Infinite when it
/// never does.
pub fn turn_start_m(inputs: &NetPathInputs, h_acc_m: f64) -> f64 {
    let h_turn = (inputs.wingspan_m / 2.0).max(MIN_TURN_HEIGHT_M);
    let d1 = (inputs.v2_m_s * inputs.gear_retraction_time_s).max(0.0);
    let g2 = inputs.second_segment_net_gradient;
    let mut points = vec![d1];
    if g2 > 0.0 {
        let h1 = NET_PATH_START_HEIGHT_M + inputs.first_segment_net_gradient * d1;
        let d_a = d1 + ((h_acc_m - h1) / g2).max(0.0);
        let level = ((inputs.v_final_takeoff_m_s.powi(2) - inputs.v2_m_s.powi(2)).max(0.0))
            / (2.0 * G * g2);
        points.extend([d_a, d_a + level]);
    }
    let mut prev = (0.0, net_path_height_m(inputs, h_acc_m, 0.0));
    if prev.1 >= h_turn {
        return 0.0;
    }
    for d in points {
        if d <= prev.0 {
            continue;
        }
        let h = net_path_height_m(inputs, h_acc_m, d);
        if h >= h_turn {
            return prev.0 + (h_turn - prev.1) / (h - prev.1) * (d - prev.0);
        }
        prev = (d, h);
    }
    let slope = if g2 > 0.0 {
        inputs.final_segment_net_gradient
    } else {
        g2
    };
    if slope > 0.0 {
        prev.0 + (h_turn - prev.1) / slope
    } else {
        f64::INFINITY
    }
}

/// Minimum margin of one direction for one acceleration height, with the
/// limiting obstacle (`d`, height above the aerodrome).
pub(crate) fn direction_margin(
    survey: &DepartureSurvey,
    inputs: &NetPathInputs,
    h_acc_m: f64,
    until_turn: bool,
) -> Option<(f64, f64, f64)> {
    let mut path_end_m = take_off_path_end_m(inputs, h_acc_m);
    if until_turn {
        path_end_m = path_end_m.min(turn_start_m(inputs, h_acc_m));
    }
    let reference = survey_reference_m(survey, inputs.reference_elevation_m);
    let mut worst: Option<(f64, f64, f64)> = None;
    let mut consider = |d_lo_m: f64, d_hi_m: f64, top_msl: f64| {
        // Past the end of the take-off path the 35 ft rule no longer applies
        // (CS 25.111 (a), CAT.POL.A.210 (c)): an obstacle wholly beyond it is not
        // taken, one that straddles it is read up to it.
        if d_lo_m >= path_end_m {
            return;
        }
        let d_hi_m = d_hi_m.min(path_end_m);
        // Before the end of the take-off distance the aeroplane is below 35 ft:
        // the path height there is not extrapolated backwards. The obstacle (or
        // the highest ground of a cell) lies somewhere in `d_lo ..= d_hi`; its
        // margin is read at the least path height over that range.
        let d_lo_m = d_lo_m.max(0.0);
        let d_hi_m = d_hi_m.max(d_lo_m);
        let top = top_msl - reference;
        let (path, d_m) = net_path_min_height_m(inputs, h_acc_m, d_lo_m, d_hi_m);
        let margin = path - OBSTACLE_VERTICAL_CLEARANCE_M - top;
        if worst.is_none_or(|(best, _, _)| margin < best) {
            worst = Some((margin, d_m, top));
        }
    };
    for (index, row) in survey.terrain_max_elevation_m.iter().enumerate() {
        let near = survey.terrain_s0_m + index as f64 * survey.cell_along_m;
        let far = near + survey.cell_along_m;
        if far <= inputs.toda_m {
            continue;
        }
        // The highest ground of the cell lies somewhere in it: the path is read at
        // its least over the cell and the accountable strip at the far edge (widest).
        let d_hi = far - inputs.takeoff_distance_m;
        let semi_width = accountability_semi_width_m(inputs.wingspan_m, far - inputs.toda_m);
        for (&centre, &top) in survey.lateral_centres_m.iter().zip(row) {
            let edge = centre.abs() - survey.cell_lateral_m / 2.0;
            if edge < semi_width {
                // Only the part of the cell inside the strip counts: it begins where the
                // strip reaches the cell's inner lateral edge.
                let entry = strip_entry_s_m(inputs.wingspan_m, inputs.toda_m, edge);
                consider(near.max(entry) - inputs.takeoff_distance_m, d_hi, top);
            }
        }
    }
    for obstacle in &survey.obstacles {
        if obstacle.s_m < inputs.toda_m {
            continue;
        }
        // A merged obstacle spans `s_m ..= s_max_m`: the strip is read at its far
        // end (widest) and the path at its least over the span.
        let s_far = obstacle.s_max_m.unwrap_or(obstacle.s_m).max(obstacle.s_m);
        let semi_width = accountability_semi_width_m(inputs.wingspan_m, s_far - inputs.toda_m);
        if obstacle.y_m.abs() < semi_width {
            let entry = strip_entry_s_m(inputs.wingspan_m, inputs.toda_m, obstacle.y_m.abs());
            consider(
                obstacle.s_m.max(entry) - inputs.takeoff_distance_m,
                s_far - inputs.takeoff_distance_m,
                obstacle.top_m,
            );
        }
    }
    worst
}

/// What the obstacle check found.
#[derive(Debug, Clone, PartialEq)]
pub enum ObstacleCheck {
    /// The inputs are not finite or physical.
    Invalid,
    /// No terrain cell or obstacle lies in the accountability area.
    Clear,
    /// The best margin over the surveyed directions and acceleration heights.
    Margin(ObstacleMargin),
}

/// The best margin over the surveyed take-off directions and the
/// acceleration heights.
pub fn obstacle_clearance_margin(
    surveys: &[DepartureSurvey],
    inputs: &NetPathInputs,
) -> ObstacleCheck {
    obstacle_check(surveys, inputs, false)
}

/// The obstacle check up to the point where the first track change is allowed, hard on the
/// straight-out track whatever the route: CAT.POL.A.210 (b)(2) allows no track change until the
/// net path is half a wingspan (not less than 50 ft) high, so every obstacle before it is
/// on the straight track. Beyond it the route decides ([`obstacle_clearance_margin`] reads the
/// straight-out track as an assumption).
pub fn obstacle_clearance_margin_before_turn(
    surveys: &[DepartureSurvey],
    inputs: &NetPathInputs,
) -> ObstacleCheck {
    obstacle_check(surveys, inputs, true)
}

fn obstacle_check(
    surveys: &[DepartureSurvey],
    inputs: &NetPathInputs,
    until_turn: bool,
) -> ObstacleCheck {
    let finite = [
        inputs.toda_m,
        inputs.takeoff_distance_m,
        inputs.reference_elevation_m,
        inputs.wingspan_m,
        inputs.v2_m_s,
        inputs.v_final_takeoff_m_s,
        inputs.first_segment_net_gradient,
        inputs.second_segment_net_gradient,
        inputs.final_segment_net_gradient,
    ]
    .iter()
    .all(|value| value.is_finite());
    if !finite || inputs.v2_m_s <= 0.0 || inputs.wingspan_m <= 0.0 {
        return ObstacleCheck::Invalid;
    }
    // The gear retraction time is the caller's input ([`GEAR_RETRACTION_TIME_S`] for the
    // registered types; a sensitivity study varies it).
    match best_margin(surveys, inputs, until_turn) {
        Some(margin) => ObstacleCheck::Margin(margin),
        // No accountable obstacle: nothing to clear.
        None => ObstacleCheck::Clear,
    }
}

/// The best margin over directions and acceleration heights at one gear time.
fn best_margin(
    surveys: &[DepartureSurvey],
    inputs: &NetPathInputs,
    until_turn: bool,
) -> Option<ObstacleMargin> {
    let mut best: Option<ObstacleMargin> = None;
    for survey in surveys {
        let mut h_acc = MIN_ACCELERATION_HEIGHT_M;
        while h_acc <= MAX_ACCELERATION_HEIGHT_M + 1e-9 {
            let Some((margin, d, top)) = direction_margin(survey, inputs, h_acc, until_turn) else {
                // A direction and acceleration height whose take-off path meets no
                // accountable obstacle is free of them: the operator selects it.
                return None;
            };
            {
                let symmetric = survey_symmetric_uncertainty_m(survey);
                let uncertainty_m = survey_uncertainty_m(survey);
                let slack = margin + symmetric;
                // The best direction is the one that certifies the most: margin less
                // uncertainty. The violation slack is the best over all of them.
                let slack = best
                    .as_ref()
                    .map_or(slack, |b| b.violation_slack_m.max(slack));
                if best
                    .as_ref()
                    .is_none_or(|b| margin - uncertainty_m > b.margin_m - b.uncertainty_m)
                {
                    best = Some(ObstacleMargin {
                        uncertainty_m,
                        symmetric_uncertainty_m: symmetric,
                        unverified_allowance_m: survey_unverified_allowance_m(survey),
                        violation_slack_m: slack,
                        margin_best_m: margin,
                        uncertainty_best_m: uncertainty_m,
                        margin_m: margin,
                        runway: survey.runway.clone(),
                        acceleration_height_m: h_acc,
                        limiting_distance_m: d,
                        limiting_height_m: top,
                    });
                } else if let Some(b) = best.as_mut() {
                    b.violation_slack_m = slack;
                }
            }
            h_acc += ACCELERATION_HEIGHT_STEP_M;
        }
    }
    best
}

// A test asserts on values it built here, so a failed unwrap is the assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::airport_obstacles::DepartureObstacle;

    fn inputs(g2: f64) -> NetPathInputs {
        NetPathInputs {
            toda_m: 3000.0,
            takeoff_distance_m: 2600.0,
            gear_retraction_time_s: 10.0,
            reference_elevation_m: 100.0,
            wingspan_m: 36.0,
            v2_m_s: 75.0,
            v_final_takeoff_m_s: 82.0,
            first_segment_net_gradient: -0.008,
            second_segment_net_gradient: g2,
            final_segment_net_gradient: 0.004,
        }
    }

    fn survey(top_msl: f64, s_m: f64, y_m: f64) -> DepartureSurvey {
        DepartureSurvey {
            runway: "09".to_owned(),
            heading_deg: 90.0,
            reference_elevation_m: None,
            terrain_source: String::new(),
            vertical_uncertainty_m: Some(0.3),
            one_sided_bias_m: None,
            completeness_allowance_m: None,
            reference_minus_aerodrome_elevation_m: None,
            terrain_s0_m: 3000.0,
            terrain_max_elevation_m: Vec::new(),
            cell_along_m: 250.0,
            lateral_centres_m: Vec::new(),
            cell_lateral_m: 150.0,
            obstacles: vec![DepartureObstacle {
                s_m,
                s_max_m: None,
                y_m,
                top_m: top_msl,
                kind: "mast".to_owned(),
                source: "test".to_owned(),
            }],
            provenance: String::new(),
        }
    }

    fn margin_of(surveys: &[DepartureSurvey], g2: f64) -> ObstacleMargin {
        let mut i = inputs(g2);
        i.gear_retraction_time_s = GEAR_RETRACTION_TIME_S;
        match obstacle_clearance_margin(surveys, &i) {
            ObstacleCheck::Margin(margin) => margin,
            other => panic!("expected a margin, got {other:?}"),
        }
    }

    /// A descending first segment (CS 25.115(b) decrement above the gross
    /// gradient) puts the least path height at the far end of a merged obstacle
    /// or terrain cell, not the near one.
    #[test]
    fn a_descending_first_segment_is_read_at_the_far_end_of_a_merged_obstacle() {
        let mut i = inputs(0.03);
        i.first_segment_net_gradient = -0.003;
        i.gear_retraction_time_s = 14.0;
        let mut s = survey(100.0 + 20.0, 3100.0, 0.0);
        s.obstacles[0].s_max_m = Some(3300.0);
        let ObstacleCheck::Margin(m) = obstacle_clearance_margin(&[s], &i) else {
            panic!("expected a margin");
        };
        let far = 3300.0 - i.takeoff_distance_m;
        let expected = net_path_height_m(&i, m.acceleration_height_m, far)
            - OBSTACLE_VERTICAL_CLEARANCE_M
            - 20.0;
        assert!(
            (m.margin_m - expected).abs() < 1e-9,
            "{} vs {expected}",
            m.margin_m
        );
        assert!((m.limiting_distance_m - far).abs() < 1e-9);
    }

    #[test]
    fn the_net_path_starts_at_35_ft_and_climbs_at_the_net_gradient() {
        let i = inputs(0.03);
        assert!((net_path_height_m(&i, 150.0, 0.0) - NET_PATH_START_HEIGHT_M).abs() < 1e-12);
        let d1 = 75.0 * 10.0;
        let h1 = NET_PATH_START_HEIGHT_M - 0.008 * d1;
        let h = net_path_height_m(&i, 150.0, d1 + 1000.0);
        assert!((h - (h1 + 30.0)).abs() < 1e-9);
    }

    #[test]
    fn the_acceleration_segment_is_level_then_the_final_gradient_applies() {
        let i = inputs(0.02);
        let level = (82.0_f64.powi(2) - 75.0_f64.powi(2)) / (2.0 * G * 0.02);
        let d1 = 75.0 * 10.0;
        let h1 = NET_PATH_START_HEIGHT_M - 0.008 * d1;
        let d_a = d1 + (150.0 - h1) / 0.02;
        assert!((net_path_height_m(&i, 150.0, d_a + level / 2.0) - 150.0).abs() < 1e-9);
        let beyond = net_path_height_m(&i, 150.0, d_a + level + 1000.0);
        assert!((beyond - (150.0 + 4.0)).abs() < 1e-9);
    }

    #[test]
    fn an_obstacle_inside_the_corridor_limits_and_one_outside_does_not() {
        // 2000 m beyond the TODA, 120 m above the aerodrome, on the track.
        let on_track = survey(220.0, 5000.0, 0.0);
        let margin = margin_of(&[on_track], 0.024);
        assert!(margin.margin_m < 0.0, "{margin:?}");
        // The same obstacle 400 m to the side: the horizontal clearance there
        // is 60 + 18 + 0.125 * 2000 = 328 m, so it is cleared horizontally.
        let aside = survey(220.0, 5000.0, 400.0);
        assert_eq!(
            obstacle_clearance_margin(&[aside], &inputs(0.024)),
            ObstacleCheck::Clear
        );
    }

    #[test]
    fn a_steeper_net_path_clears_the_same_obstacle() {
        // 5400 m beyond the take-off distance, so the gear (15 s) has long been up.
        let s = survey(140.0, 8000.0, 0.0);
        let weak = margin_of(std::slice::from_ref(&s), 0.008);
        let strong = margin_of(&[s], 0.04);
        assert!(strong.margin_m > weak.margin_m);
        assert!(strong.margin_m > 0.0 && weak.margin_m < 0.0);
    }

    #[test]
    fn the_verdict_carries_the_uncertainty_in_three_states() {
        let make = |margin_m: f64| ObstacleMargin {
            uncertainty_m: 2.0,
            symmetric_uncertainty_m: 2.0,
            unverified_allowance_m: 0.0,
            violation_slack_m: margin_m + 2.0,
            margin_best_m: margin_m,
            uncertainty_best_m: 2.0,
            margin_m,
            runway: "09".to_owned(),
            acceleration_height_m: 122.0,
            limiting_distance_m: 0.0,
            limiting_height_m: 0.0,
        };
        assert_eq!(make(2.5).verdict(), ObstacleVerdict::Clears);
        assert_eq!(make(0.5).verdict(), ObstacleVerdict::Undetermined);
        assert_eq!(make(-1.9).verdict(), ObstacleVerdict::Undetermined);
        assert_eq!(make(-2.5).verdict(), ObstacleVerdict::Violated);
    }

    /// The unverified-survey allowances (mobile objects, sampling, untagged trees and
    /// buildings) are reported and never part of the margin the requirement needs:
    /// no regulation prints them (CAT.POL.A.210 asks 35 ft).
    #[test]
    fn the_unverified_survey_allowances_are_reported_and_not_imposed() {
        let mut s = survey(100.0 + 160.0, 8000.0, 0.0);
        s.one_sided_bias_m = Some(5.4);
        s.completeness_allowance_m = Some(20.0);
        let m = margin_of(std::slice::from_ref(&s), 0.04);
        assert!((m.symmetric_uncertainty_m - 0.3).abs() < 1e-12);
        assert!((m.uncertainty_m - 0.3).abs() < 1e-12);
        assert!((m.unverified_allowance_m - 25.4).abs() < 1e-12);
        // A margin of a few metres clears: 25.4 m of allowance would have left it
        // undetermined.
        assert!(m.margin_m > 0.3 && m.margin_m < 25.7, "{m:?}");
        assert_eq!(m.verdict(), ObstacleVerdict::Clears);
        // A certain violation stays one.
        let hard = survey(100.0 + 150.0, 5000.0, 0.0);
        let m = margin_of(&[hard], 0.024);
        assert!(m.margin_m < -0.3 && m.violation_slack_m < 0.0, "{m:?}");
        assert_eq!(m.verdict(), ObstacleVerdict::Violated);
    }

    /// The 35 ft rule stops where the take-off path does (CS 25.111 (a)): ground that
    /// rises above the net path beyond that point is an en-route matter.
    #[test]
    fn nothing_beyond_the_end_of_the_take_off_path_is_a_take_off_obstacle() {
        let i = inputs(0.024);
        // Acceleration at 1500 ft: the path ends at the end of the level acceleration.
        let level = (82.0_f64.powi(2) - 75.0_f64.powi(2)) / (2.0 * G * 0.024);
        let d1 = 75.0 * 10.0;
        let h1 = NET_PATH_START_HEIGHT_M - 0.008 * d1;
        let d_a = d1 + (MAX_ACCELERATION_HEIGHT_M - h1) / 0.024;
        let end = take_off_path_end_m(&i, MAX_ACCELERATION_HEIGHT_M);
        assert!((end - (d_a + level)).abs() < 1e-6, "{end}");
        // Acceleration at 400 ft: the final segment climbs on to 1500 ft.
        let low = take_off_path_end_m(&i, MIN_ACCELERATION_HEIGHT_M);
        let d_a_low = d1 + (MIN_ACCELERATION_HEIGHT_M - h1) / 0.024;
        let expected =
            d_a_low + level + (TAKE_OFF_PATH_END_HEIGHT_M - MIN_ACCELERATION_HEIGHT_M) / 0.004;
        assert!((low - expected).abs() < 1e-6, "{low} vs {expected}");
        // A mast on the track 60 km out, far above any net path, is not accountable.
        let far = survey(100.0 + 2000.0, 2600.0 + 60_000.0, 0.0);
        assert_eq!(obstacle_clearance_margin(&[far], &i), ObstacleCheck::Clear);
        // The same mast inside the take-off path is.
        let near = survey(100.0 + 2000.0, 2600.0 + 3_000.0, 0.0);
        assert!(margin_of(&[near], 0.024).margin_m < 0.0);
        // A path that never climbs is not cut off.
        assert!(take_off_path_end_m(&inputs(0.0), 150.0).is_infinite());
    }

    #[test]
    fn a_datum_offset_corrects_the_reference_and_is_not_uncertainty() {
        let mut s = survey(120.0, 5000.0, 0.0);
        s.vertical_uncertainty_m = Some(0.3);
        s.reference_elevation_m = Some(0.0);
        // Within the threshold nothing changes.
        s.reference_minus_aerodrome_elevation_m = Some(-1.5);
        assert!((survey_uncertainty_m(&s) - 0.3).abs() < 1e-12);
        assert!((survey_reference_m(&s, 6.0)).abs() < 1e-12);
        // A tile that reads the aerodrome 6.1 m low: the aerodrome is 6.1 m in the tile datum, and
        // the uncertainty stays the dataset's own.
        s.reference_minus_aerodrome_elevation_m = Some(-6.1);
        assert!((survey_uncertainty_m(&s) - 0.3).abs() < 1e-12);
        assert!((survey_reference_m(&s, 6.0) - 6.1).abs() < 1e-12);
        s.vertical_uncertainty_m = None;
        assert!((survey_uncertainty_m(&s) - DEFAULT_VERTICAL_UNCERTAINTY_M).abs() < 1e-12);
        // The margin follows the corrected reference: ground the tile reads 5 m up is 1.1 m below
        // the aerodrome.
        let mut raw_survey = survey(5.0, 5000.0, 0.0);
        raw_survey.reference_elevation_m = Some(0.0);
        let mut ground = raw_survey.clone();
        ground.reference_minus_aerodrome_elevation_m = Some(-6.1);
        let raw = margin_of(&[raw_survey], 0.04).margin_m;
        let corrected = margin_of(&[ground], 0.04).margin_m;
        assert!((corrected - raw - 6.1).abs() < 1e-9, "{raw} {corrected}");
    }

    /// Ground at the inner edge of the strip counts from where the strip reaches it, not from
    /// the near edge of its cell: a cell 255 to 285 m off the track is inside the strip (78 m + 0.125 d
    /// for a 36 m span) only from d = 1416 m beyond the TODA.
    #[test]
    fn a_cell_at_the_strip_edge_counts_from_where_the_strip_reaches_it() {
        let i = inputs(0.03);
        // Wingspan 36 m: the strip is 60 + 18 + 0.125 d = 78 m + 0.125 d wide.
        let entry = strip_entry_s_m(i.wingspan_m, i.toda_m, 255.0);
        assert!((entry - (i.toda_m + (255.0 - 78.0) / 0.125)).abs() < 1e-9);
        assert_eq!(
            strip_entry_s_m(i.wingspan_m, i.toda_m, 50.0),
            f64::NEG_INFINITY
        );
        assert!(strip_entry_s_m(i.wingspan_m, i.toda_m, 600.0).is_infinite());
        let mut s = survey(0.0, 0.0, 0.0);
        s.obstacles.clear();
        s.cell_along_m = 250.0;
        s.cell_lateral_m = 30.0;
        s.lateral_centres_m = vec![270.0];
        s.terrain_s0_m = i.toda_m + 1200.0; // 4200 .. 4450 m; entry at 4416 m
        s.terrain_max_elevation_m = vec![vec![100.0 + 20.0]];
        let m = margin_of(&[s], 0.03);
        // Read at the entry (120 m into the cell), not at its near edge.
        assert!(
            (m.limiting_distance_m - (entry - i.takeoff_distance_m)).abs() < 1.0,
            "{m:?}"
        );
    }

    /// CAT.POL.A.210 (b)(2): no track change until the net path is half a wingspan (not under
    /// 50 ft) high; an obstacle beyond that point is not on a track the rule fixes.
    #[test]
    fn the_first_track_change_is_allowed_at_half_a_wingspan() {
        let i = inputs(0.03);
        // Span 36 m: 18 m. The first segment sinks to 35 ft - 0.008 * 750 m, then 3 %.
        let h1 = NET_PATH_START_HEIGHT_M - 0.008 * 750.0;
        let expected = 750.0 + (18.0 - h1) / 0.03;
        assert!((turn_start_m(&i, 150.0) - expected).abs() < 1e-9);
        // A span under 100 ft: the 50 ft floor governs.
        let mut small = inputs(0.03);
        small.wingspan_m = 20.0;
        assert!(
            ((turn_start_m(&small, 150.0) - 750.0) * 0.03 + h1 - MIN_TURN_HEIGHT_M).abs() < 1e-9
        );
        // A path that never climbs never turns.
        assert!(turn_start_m(&inputs(-0.01), 150.0).is_infinite());
        // A mast 3 km beyond the turn point on the track is hard in the full check and
        // outside the before-turn one.
        let mast = survey(100.0 + 2000.0, 2600.0 + 1194.0 + 3000.0, 0.0);
        assert!(matches!(
            obstacle_clearance_margin(std::slice::from_ref(&mast), &i),
            ObstacleCheck::Margin(_)
        ));
        assert_eq!(
            obstacle_clearance_margin_before_turn(&[mast], &i),
            ObstacleCheck::Clear
        );
    }

    #[test]
    fn the_semi_width_follows_the_regulation() {
        assert!((accountability_semi_width_m(36.0, 1000.0) - (60.0 + 18.0 + 125.0)).abs() < 1e-12);
        assert!((accountability_semi_width_m(65.0, 1000.0) - 215.0).abs() < 1e-12);
        assert_eq!(
            accountability_semi_width_m(65.0, 10_000.0),
            LATERAL_CUTOFF_M
        );
    }
}
