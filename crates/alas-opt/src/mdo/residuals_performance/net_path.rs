// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The net take-off flight path (CS 25.115) and its obstacle clearance at the
//! departure aerodrome (CAT.POL.A.210).
//!
//! The net second-segment gradient is the gradient the installed thrust
//! delivers at `V2` with one engine out, minus the 0.8/0.9/1.0 percent
//! decrement; it is implied by the 25.121(b) second segment, so its row is a
//! guard at zero. The obstacle row is a hard check against a violation: a net
//! path of [`alas_perf::performance::obstacle`] that certainly fails to clear
//! the surveyed terrain and obstacles of the departure aerodrome rejects the
//! candidate. An undetermined verdict (the margin lies inside the survey
//! uncertainty of the dataset) is a reported warning, not a rejection. The curated surveys are public-data derivations, not an AIP
//! Type A chart; no regulation prints an allowance for incomplete obstacle data, so
//! the allowances the generator records are reported in the diagnostic
//! `takeoff_obstacle_survey_unverified` and are never added to the margin the
//! requirement needs. An aerodrome without
//! a survey (a hand-entered one) or a path that cannot be evaluated is a
//! failed check, not a diagnostic.

use alas_config::airport_obstacles::departure_surveys;
use alas_config::PerformanceConfig;
use alas_perf::performance::obstacle::{
    obstacle_clearance_margin, obstacle_clearance_margin_before_turn, NetPathInputs, ObstacleCheck,
    ObstacleMargin, ObstacleVerdict, GEAR_RETRACTION_TIME_S, OBSTACLE_VERTICAL_CLEARANCE_M,
};
use alas_perf::performance::{
    cs25_115_net_path_reduction, cs25_121a_gradient, ClimbSegment, VSpeeds,
    FINAL_TAKEOFF_MIN_OVER_VSR,
};

use super::certification::net_path_gradient_margin;
use crate::mdo::sizing::SizingOutcome;
use crate::mdo::types::ConstraintFamily::Performance;
use crate::mdo::types::{CandidateDrag, ConstraintResidual};
use crate::mdo::ResidualRole;

/// The terms of the engine-out balance at `V2`.
pub(super) struct NetPathTerms<'a> {
    /// Performance configuration (take-off lift and gear drag).
    pub perf: &'a PerformanceConfig,
    /// The candidate drag model (the low-speed field polar at the departure).
    pub drag: &'a CandidateDrag,
    /// CS 25.121(b) gradient the second segment requires.
    pub required_gradient: Option<f64>,
    /// In-flight engine-equivalent thrust-to-weight that holds it.
    pub required_inflight_tw: Option<f64>,
    /// `V2` thrust over static thrust.
    pub thrust_ratio: Option<f64>,
    /// Installed static thrust over weight.
    pub available_tw: f64,
    /// Departure aerodrome.
    pub departure: Option<&'a alas_config::airports::Airport>,
    /// Take-off speeds at the departure aerodrome.
    pub speeds: Option<VSpeeds>,
    /// Take-off distance to 35 ft at the design gross mass, m; the net path
    /// starts there. `None` starts it at the end of the TODA (the longest
    /// distance the runway offers, never an optimistic origin).
    pub takeoff_distance_m: Option<f64>,
    /// Time the continued take-off spends between lift-off and the 35 ft screen,
    /// s (the air leg of the CS 25.113 integration). The gear retraction is timed
    /// from lift-off, so this much of it is over when the net path begins.
    /// `None` (a field correlation with no air leg) charges the whole
    /// retraction time after the screen.
    pub airborne_to_screen_s: Option<f64>,
    /// Net gradient of the final segment the installed thrust delivers;
    /// `None` takes the CS 25.121(c) minimum less the decrement.
    pub final_segment_net_gradient: Option<f64>,
    /// Elevation the obstacle survey is referenced to when it differs from the
    /// performance departure (the sea-level certification basis keeps the real
    /// aerodrome's terrain).
    pub survey_elevation_m: Option<f64>,
    /// Role of the en-route obstacle rows (CAT.POL.A.215): hard in the design modes, a
    /// Diagnostic for the fixed baseline, whose operating limits are dispatch.
    pub en_route_role: ResidualRole,
    /// Declared departure track (`requirements.departure_track`): the en-route rule is a hard
    /// requirement only along a declared route.
    pub departure_track: Option<&'a [alas_config::DepartureTrackLeg]>,
}

/// Illustrative fast gear retraction time, s, for the sensitivity of the obstacle verdict (not a
/// model value).
const FAST_GEAR_RETRACTION_TIME_S: f64 = 8.0;

/// The obstacle-clearance constraint row.
const ID: &str = "takeoff_obstacle_clearance";
/// The Diagnostic warning row of an undetermined obstacle verdict.
const UNDETERMINED_ID: &str = "takeoff_obstacle_clearance_undetermined";
/// The Diagnostic row that flags a verdict read on an unverified survey.
const UNVERIFIED_ID: &str = "takeoff_obstacle_survey_unverified";

fn failed_check(
    id: &'static str,
    why: impl Into<String>,
    role: ResidualRole,
) -> ConstraintResidual {
    ConstraintResidual::direct(id, Performance, 1.0, 0.0, "bool", 1.0, 1.0, role).with_detail(why)
}

/// The net-path gradient guard and the obstacle-clearance row.
pub(super) fn net_path_residuals(
    outcome: &SizingOutcome,
    terms: &NetPathTerms<'_>,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let n_engines = outcome.n_engines;
    let Some(reduction) = cs25_115_net_path_reduction(n_engines) else {
        return Vec::new();
    };
    let net_gradient = match (
        terms.required_inflight_tw,
        terms.thrust_ratio,
        terms.required_gradient,
    ) {
        (Some(inflight_tw), Some(ratio), Some(gradient)) => {
            net_path_gradient_margin(n_engines, gradient, inflight_tw, terms.available_tw, ratio)
        }
        _ => None,
    };
    let mut residuals = Vec::new();
    if let Some(margin) = net_gradient {
        residuals.push(ConstraintResidual::direct(
            "takeoff_net_path_gradient",
            Performance,
            margin,
            0.0,
            "gradient",
            -margin,
            (-margin / reduction).max(0.0),
            role,
        ));
    }
    // CS 25.121(a): the gear-down first-segment gradient is positive (0.3 % and
    // 0.5 % for three and four engines): the second segment's gross gradient
    // less the gear drag increment over the lift coefficient at `V2`.
    let second_gross = net_gradient.map(|net| net + reduction);
    let first_gross = first_segment_gross_gradient(terms, second_gross);
    if let (Some(gross), Some(required)) = (first_gross, cs25_121a_gradient(n_engines)) {
        let row = ConstraintResidual::direct(
            "oei_first_segment",
            Performance,
            gross,
            required,
            "gradient",
            required - gross,
            ((required - gross) / reduction).max(0.0),
            role,
        );
        residuals.push(row.with_detail(format!(
            "gear-down gross gradient at VLOF {gross:.4} from the lattice polar at the take-off lift coefficients, flap increment and gear increment as modelled (no fixed efficiency factor: the apparent Oswald factor rises with flap angle, Obert 2009, Fig. 27.39, printed p. 363)"
        )));
    }
    residuals.extend(obstacle_rows(outcome, terms, net_gradient, reduction, role));
    residuals
}

pub mod v2_polar;
pub use v2_polar::{climb_out_aspect_ratio, EngineFailureClass, V2ClimbOutPolar};

/// Gross gradient of the gear-down part of the take-off path beyond the 35 ft
/// screen (one engine out), `None` when the second segment or the speeds are
/// unavailable.
///
/// CS 25.111(c)(2): "the aeroplane must reach V2 before it is 35 ft above the
/// take-off surface and must continue at a speed as close as practical to, but
/// not less than V2, until it is 400 ft above the take-off surface". From the
/// screen, where the net take-off flight path begins (CS 25.115(a)), the speed
/// is therefore `V2`, and the path differs from the second segment only by the
/// extended gear: the second-segment gross gradient less the gear drag
/// increment over the lift coefficient at `V2`. This is the state the
/// CS 25.113 integration reaches the screen in (`V2`, gear down, the climb-out
/// locus plus the gear increment). The CS 25.121(a) requirement is a different
/// quantity, the gradient at `VLOF` ([`first_segment_gross_gradient`], its own
/// row): the lift-off point lies before the screen, inside the take-off
/// distance, and is not on the net path.
fn screen_to_gear_up_gross_gradient(
    terms: &NetPathTerms<'_>,
    second_gross: Option<f64>,
) -> Option<f64> {
    let speeds = terms.speeds?;
    let perf = terms.perf;
    let cl_v2 = perf.cl_max_to * (speeds.v_stall_to_ms / speeds.v2_ms).powi(2);
    let gradient = second_gross? - perf.takeoff_gear_delta_cd / cl_v2;
    gradient.is_finite().then_some(gradient)
}

/// Gross first-segment gradient at `VLOF` (gear down, one engine out), the
/// CS 25.121(a) quantity, `None` when the second segment or the speeds are
/// unavailable.
fn first_segment_gross_gradient(
    terms: &NetPathTerms<'_>,
    second_gross: Option<f64>,
) -> Option<f64> {
    let speeds = terms.speeds?;
    let perf = terms.perf;
    // CS 25.121(a): at VLOF, gear extended. VLOF is not below VR, so the lift
    // coefficient at VR is the larger (conservative) one; VR is the delayed one
    // when the improved climb raises V2 ([`lift_off_speed_m_s`]). The thrust is the
    // second segment's, which is the smaller (thrust lapses with speed). Thrust
    // and weight are the same in both states, so the gradient moves from the
    // second segment's by the change of CD / CL read from the candidate's own
    // drag table at the two lift coefficients, plus the flap increment at both and
    // the gear increment at VLOF only. The table's induced drag is the trimmed
    // lattice polar the rest of the take-off path reads (its span efficiency is
    // 0.78 to 0.86 at these lift coefficients); no fixed airplane efficiency factor is
    // imposed, because the apparent Oswald factor of wing-engined transports rises
    // with flap angle (Obert, Aerodynamic Design of Transport Aircraft, 2009,
    // Fig. 27.39, printed p. 363: 0.0026 per degree) and the failed-engine drag is the
    // separate `oei_climb_delta_cd` and failed-engine terms.
    let departure = terms.departure?;
    let cl_v2 = perf.cl_max_to * (speeds.v_stall_to_ms / speeds.v2_ms).powi(2);
    let cl_lof =
        perf.cl_max_to * (speeds.v_stall_to_ms / lift_off_speed_m_s(perf, &speeds)).powi(2);
    let atmosphere =
        alas_atmo::us1976_compute_values(departure.elevation_m, departure.isa_deviation_c);
    let mach = speeds.v2_ms / atmosphere.speed_of_sound_m_s;
    let cd = |cl: f64| {
        terms.drag.cd_airport_base_at_atmosphere(
            cl,
            mach,
            departure.elevation_m,
            departure.isa_deviation_c,
            crate::mdo::drag_table::high_lift_lift_shift(perf.cl_max_to, perf.cl_max_clean),
        ) + perf.oei_climb_delta_cd
    };
    let gradient =
        second_gross? + cd(cl_v2) / cl_v2 - (cd(cl_lof) + perf.takeoff_gear_delta_cd) / cl_lof;
    // An unavailable drag is no gradient: the caller reports the evidence gap.
    gradient.is_finite().then_some(gradient)
}

/// Speed of the gear-down first segment, true, m/s: the rotation speed, the
/// lower bound of the lift-off speed (the larger, conservative lift
/// coefficient), never above `V2`.
///
/// An improved-climb `V2` is flown by delaying the rotation (Torenbeek,
/// *Advanced Aircraft Design*, 2013, Sec. 9.3.2: "this overspeed situation is
/// achieved by delaying the rotation"), and the take-off field length is
/// charged for it by the square of the ratio of that `V2` to the schedule
/// `V2` (`PerformanceConfig::improved_climb_field_factor`). The speed schedule
/// itself leaves `VR` at its minimum, so the rotation speed read here is moved
/// by the same ratio: the lift-off the field length was charged for. Without
/// an improved climb the ratio is one.
pub(super) fn lift_off_speed_m_s(perf: &PerformanceConfig, speeds: &VSpeeds) -> f64 {
    (speeds.v_r_ms * perf.improved_climb_field_factor().sqrt()).min(speeds.v2_ms)
}

/// Stall speed in the clean configuration at the take-off mass, true, m/s:
/// `VFTO = 1.18 VSR` is defined in the en-route configuration (CS 25.107(g)).
fn clean_stall_speed_m_s(outcome: &SizingOutcome, perf: &PerformanceConfig) -> Option<f64> {
    let departure = outcome.departure?;
    let rho = alas_atmo::us1976_compute_values(departure.elevation_m, departure.isa_deviation_c)
        .density_kg_m3;
    let speed = (2.0 * outcome.sized.takeoff_mass_kg * alas_atmo::STANDARD_GRAVITY
        / (rho * outcome.plane.s_ref * perf.cl_max_clean))
        .sqrt();
    speed.is_finite().then_some(speed)
}

fn obstacle_rows(
    outcome: &SizingOutcome,
    terms: &NetPathTerms<'_>,
    net_gradient: Option<f64>,
    reduction: f64,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    const GAP: &str = "takeoff_obstacle_clearance_evidence_gap";
    let Some(departure) = outcome.departure else {
        return vec![failed_check(GAP, "departure aerodrome unresolved", role)];
    };
    let Some(surveys) = departure_surveys(&departure.icao) else {
        return vec![failed_check(
            GAP,
            format!(
                "{}: no departure obstacle survey; CAT.POL.A.210 cannot be shown",
                departure.icao
            ),
            role,
        )];
    };
    let (Some(speeds), Some(second)) = (terms.speeds, net_gradient) else {
        return vec![failed_check(
            GAP,
            "net flight path could not be evaluated",
            role,
        )];
    };
    let Some(final_gross) = ClimbSegment::FinalSegment.gradient(outcome.n_engines) else {
        return vec![failed_check(GAP, "final-segment gradient undefined", role)];
    };
    // Gear-down part of the path beyond the screen: flown at V2 (CS 25.111(c)(2)),
    // the second segment less the gear increment, not lifted to any minimum.
    let perf = terms.perf;
    let Some(first_gross) = screen_to_gear_up_gross_gradient(terms, Some(second + reduction))
    else {
        return vec![failed_check(
            GAP,
            "first-segment gradient could not be evaluated",
            role,
        )];
    };
    let Some(v_stall_clean) = clean_stall_speed_m_s(outcome, perf) else {
        return vec![failed_check(
            GAP,
            "clean stall speed could not be evaluated",
            role,
        )];
    };
    // The retraction time counts from lift-off (3 s delay plus the sequence); the
    // continued take-off is airborne for `airborne_to_screen_s` of it before the
    // net path begins at the screen, gear still down (the CS 25.113 integration
    // keeps it down to 35 ft). What is left is flown gear-down beyond the screen.
    let airborne_s = terms.airborne_to_screen_s.unwrap_or(0.0).max(0.0);
    let after_screen = |from_lift_off_s: f64| (from_lift_off_s - airborne_s).max(0.0);
    let inputs = NetPathInputs {
        toda_m: departure.toda_m,
        takeoff_distance_m: terms.takeoff_distance_m.unwrap_or(departure.toda_m),
        reference_elevation_m: terms.survey_elevation_m.unwrap_or(departure.elevation_m),
        wingspan_m: outcome.history.span_m,
        v2_m_s: speeds.v2_ms,
        v_final_takeoff_m_s: (FINAL_TAKEOFF_MIN_OVER_VSR * v_stall_clean).max(speeds.v2_ms),
        gear_retraction_time_s: after_screen(GEAR_RETRACTION_TIME_S),
        first_segment_net_gradient: first_gross - reduction,
        second_segment_net_gradient: second,
        final_segment_net_gradient: terms
            .final_segment_net_gradient
            .unwrap_or(final_gross - reduction),
    };
    let final_gross_actual = terms
        .final_segment_net_gradient
        .map_or(final_gross, |net| net + reduction);
    // CAT.POL.A.210 (b)(2): no track change until the net path is half a wingspan (not
    // under 50 ft) high, so obstacles up to there are on the straight-out track whatever
    // the route and stay hard. Beyond it the route decides: hard along a declared
    // departure track, a Diagnostic where the straight-out track is only assumed.
    let declared = terms.departure_track.is_some_and(|legs| !legs.is_empty());
    let full = obstacle_clearance_margin(&surveys, &inputs);
    let hard = if declared {
        full.clone()
    } else {
        obstacle_clearance_margin_before_turn(&surveys, &inputs)
    };
    let beyond_turn_matters = full != hard;
    // The gear retraction time is a generic flagged value (no per-type time is public): the
    // check is also made at 8 s (an illustrative fast retraction, not a model value). A verdict
    // that differs between the two is undetermined, never violated or cleared.
    let check_at = |gear_time_s: f64| {
        let varied = NetPathInputs {
            gear_retraction_time_s: after_screen(gear_time_s),
            ..inputs
        };
        if declared {
            obstacle_clearance_margin(&surveys, &varied)
        } else {
            obstacle_clearance_margin_before_turn(&surveys, &varied)
        }
    };
    let fast = check_at(FAST_GEAR_RETRACTION_TIME_S);
    let fast_text = match &fast {
        ObstacleCheck::Margin(m) => format!("{:.2} m", m.margin_m),
        _ => "no accountable obstacle".to_owned(),
    };
    let gear_sensitivity = format!(
        "gear retraction sensitivity: margin {fast_text} at {FAST_GEAR_RETRACTION_TIME_S:.0} s and {} at {GEAR_RETRACTION_TIME_S:.0} s",
        match &hard {
            ObstacleCheck::Margin(m) => format!("{:.2} m", m.margin_m),
            _ => "no accountable obstacle".to_owned(),
        }
    );
    let mut rows = match hard {
        ObstacleCheck::Invalid => vec![failed_check(GAP, "net path inputs are not physical", role)],
        // Every cell and obstacle lies outside the accountability area.
        ObstacleCheck::Clear => {
            vec![
                ConstraintResidual::direct(ID, Performance, 0.0, 0.0, "m", 0.0, 0.0, role)
                    .with_detail("no obstacle in the accountability area"),
            ]
        }
        ObstacleCheck::Margin(mut margin) => {
            // Clears when the margin reaches the data uncertainty; violated when even
            // the most favourable margin lies below minus the symmetric uncertainty.
            // In between the verdict is undetermined: an evidence gap, never a pass,
            // reported as a warning and not a rejection (the
            // obstacle survey completeness is not established).
            let own_verdict = margin.verdict();
            // The verdict at the fast end: a missing obstacle there is a clearance.
            let (fast_verdict, fast_margin, fast_slack) = match &fast {
                ObstacleCheck::Margin(m) => (m.verdict(), m.margin_m, m.violation_slack_m),
                _ => (
                    ObstacleVerdict::Clears,
                    margin.uncertainty_m,
                    margin.uncertainty_m,
                ),
            };
            let gear_split =
                fast_verdict != own_verdict && own_verdict != ObstacleVerdict::Undetermined;
            let verdict = if gear_split {
                // The row holds the most favourable bound of the two ends.
                margin.violation_slack_m = margin.violation_slack_m.max(fast_slack);
                margin.margin_best_m = fast_margin;
                ObstacleVerdict::Undetermined
            } else {
                own_verdict
            };
            let detail = format!(
                "{} {}: {:?} (margin {:.2} m at the generic gear retraction time {:.0} s from lift-off (3 s delay + 12 s sequence, flagged: no per-type time is public; {:.1} s of it airborne before the 35 ft screen, the rest gear-down at V2 beyond it), required margin {:.2} m = symmetric survey uncertainty of the dataset; unverified-survey allowances {:.2} m reported, not imposed); ground or obstacle {:.1} m above the aerodrome, {:.0} m beyond the end of the take-off distance ({:.0} m; TODA {:.0} m); acceleration height {:.0} m; {}",
                departure.icao,
                margin.runway,
                verdict,
                margin.margin_m,
                GEAR_RETRACTION_TIME_S,
                airborne_s,
                margin.uncertainty_m,
                margin.unverified_allowance_m,
                margin.limiting_height_m,
                margin.limiting_distance_m,
                inputs.takeoff_distance_m,
                inputs.toda_m,
                margin.acceleration_height_m,
                gear_sensitivity,
            );
            let mut rows = margin_rows(&departure.icao, &margin, verdict, detail, role);
            if gear_split {
                for row in rows.iter_mut().filter(|row| row.id == UNDETERMINED_ID) {
                    row.detail = Some(format!(
                        "warning, {GEAR_UNSOURCED_REASON}: {}: the verdict is {fast_verdict:?} at {FAST_GEAR_RETRACTION_TIME_S:.0} s (margin {fast_margin:.2} m) and {own_verdict:?} at the generic {GEAR_RETRACTION_TIME_S:.0} s (margin {:.2} m) against {:.2} m required; the type's retraction time must be sourced",
                        departure.icao, margin.margin_m, margin.uncertainty_m
                    ));
                }
            }
            rows
        }
    };
    if let (true, ObstacleCheck::Margin(margin)) = (beyond_turn_matters && !declared, &full) {
        rows.push(assumed_track_row(&departure.icao, margin));
    }
    rows.extend(super::en_route_rows::en_route_rows(
        departure,
        &surveys,
        inputs,
        outcome.n_engines,
        final_gross_actual,
        terms.en_route_role,
        terms.departure_track,
    ));
    rows
}

/// The Diagnostic row that carries the straight-out margin when no departure track is declared.
const ASSUMED_TRACK_ID: &str = "takeoff_track_assumed_straight_out";

/// The margin along the assumed straight-out track, as a Diagnostic: CAT.POL.A.210 (b)(2) fixes
/// the track only up to the first allowed turn; (b)(6) (track changes up to 15 degrees) and
/// (b)(7) (more) set the lateral cut-off once a route is declared.
fn assumed_track_row(icao: &str, margin: &ObstacleMargin) -> ConstraintResidual {
    let verdict = margin.verdict();
    let shortfall = margin.uncertainty_m - margin.margin_m;
    ConstraintResidual::direct(
        ASSUMED_TRACK_ID,
        Performance,
        margin.margin_m,
        margin.uncertainty_m,
        "m",
        shortfall,
        0.0,
        ResidualRole::Diagnostic,
    )
    .with_detail(format!(
        "{icao} {}: no departure track is declared (requirements.departure_track), so the straight-out track is assumed past the first allowed turn (net path half a wingspan, not under 50 ft high, CAT.POL.A.210 (b)(2)); the verdict {verdict:?} there is not a requirement: margin {:.2} m worst case, {:.2} m best case, required {:.2} m; ground or obstacle {:.1} m above the aerodrome, {:.0} m beyond the end of the take-off distance",
        margin.runway,
        margin.margin_m,
        margin.margin_best_m,
        margin.uncertainty_m,
        margin.limiting_height_m,
        margin.limiting_distance_m,
    ))
}

/// Reason of an undetermined obstacle warning whose verdict changes with the gear retraction time.
pub(super) const GEAR_UNSOURCED_REASON: &str = "gear retraction time unsourced";

/// Reason every other undetermined obstacle warning carries.
pub(super) const OBSTACLE_UNDETERMINED_REASON: &str =
    "the terrain dataset accuracy does not decide the clearance";

/// The obstacle rows of a margin. Clears or violated: one constraint row, the
/// margin against the margin the data certify. Undetermined: the constraint row
/// holds the violated bound (the most favourable margin plus its symmetric
/// uncertainty, at or above zero, so it is met) and a Diagnostic
/// `takeoff_obstacle_clearance_undetermined` warning carries the margin, the
/// margin the data certify and the cause; it never ranks or rejects.
pub(super) fn margin_rows(
    icao: &str,
    margin: &ObstacleMargin,
    verdict: ObstacleVerdict,
    detail: String,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let mut rows = margin_rows_of_verdict(icao, margin, verdict, detail, role);
    // The survey is a derivation from public data: the allowances for what it may
    // omit are shown beside the verdict, which they do not change.
    let unverified_need = margin.uncertainty_m + margin.unverified_allowance_m;
    if margin.unverified_allowance_m > 0.0 && margin.margin_m < unverified_need {
        rows.push(
            ConstraintResidual::direct(
                UNVERIFIED_ID,
                Performance,
                margin.margin_m,
                unverified_need,
                "m",
                unverified_need - margin.margin_m,
                0.0,
                ResidualRole::Diagnostic,
            )
            .with_detail(format!(
                "{icao}: unverified survey (public terrain and OSM/DOF data, not an AIP Type A chart): the margin {:.2} m would not clear with the {:.2} m the generator records for mobile objects, footprint sampling and untagged trees and buildings; no regulation prints that allowance, so it is a diagnostic and not part of the requirement",
                margin.margin_m, margin.unverified_allowance_m
            )),
        );
    }
    rows
}

fn margin_rows_of_verdict(
    icao: &str,
    margin: &ObstacleMargin,
    verdict: ObstacleVerdict,
    detail: String,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let shortfall = margin.uncertainty_m - margin.margin_m;
    if verdict != ObstacleVerdict::Undetermined {
        return vec![ConstraintResidual::direct(
            ID,
            Performance,
            margin.margin_m,
            margin.uncertainty_m,
            "m",
            shortfall,
            (shortfall / OBSTACLE_VERTICAL_CLEARANCE_M).max(0.0),
            role,
        )
        .with_detail(detail)];
    }
    let slack = margin.violation_slack_m;
    vec![
        ConstraintResidual::direct(
            ID,
            Performance,
            slack,
            0.0,
            "m",
            -slack,
            (-slack / OBSTACLE_VERTICAL_CLEARANCE_M).max(0.0),
            role,
        )
        .with_detail(format!(
            "{detail}; not a rejection: the row holds the violated bound (most favourable margin plus its symmetric uncertainty, {slack:.2} m, at or above zero)"
        )),
        ConstraintResidual::direct(
            UNDETERMINED_ID,
            Performance,
            margin.margin_m,
            margin.uncertainty_m,
            "m",
            shortfall,
            0.0,
            ResidualRole::Diagnostic,
        )
        .with_detail(format!(
            "warning, {OBSTACLE_UNDETERMINED_REASON}: {}",
            undetermined_cause(icao, margin)
        )),
    ]
}

/// Why a margin is undetermined: the margin lies inside the survey uncertainty.
fn undetermined_cause(icao: &str, margin: &ObstacleMargin) -> String {
    format!(
        "{icao}: the margin {:.2} m lies between minus the symmetric survey uncertainty ({:.2} m) and the margin the data certify ({:.2} m, the symmetric accuracy of the terrain dataset); better obstacle data (an AIP Type A chart) is needed. The gear retraction time is the generic {GEAR_RETRACTION_TIME_S:.0} s (flagged, not per type)",
        margin.margin_m, margin.symmetric_uncertainty_m, margin.uncertainty_m
    )
}
