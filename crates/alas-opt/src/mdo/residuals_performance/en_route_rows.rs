// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The en-route OEI obstacle rows, CAT.POL.A.215 (EASA Easy Access Rules for Air
//! Operations, Mar. 2026, p. 1132): the check [`alas_perf::performance::en_route`] makes
//! on the departure survey beyond the end of the take-off path, as constraint rows.
//!
//! * `enroute_obstacle_clearance`: margin against the dataset uncertainty (clears or
//!   violated), or, when the verdict is undetermined or the survey does not cover the
//!   corridor, the violated bound (most favourable margin plus its symmetric
//!   uncertainty, at or above zero, so met).
//! * `enroute_obstacle_clearance_undetermined` and
//!   `enroute_obstacle_survey_does_not_cover_corridor`: Diagnostic rows that carry why
//!   the row is not a pass. The survey is a strip of +-615 m about the straight-out
//!   track and the corridor is 9,3 km either side, so a clearance over the strip is
//!   never a pass; terrain inside the strip that violates still is a certain violation.

use alas_config::airport_obstacles::DepartureSurvey;
use alas_config::airports::Airport;
use alas_perf::performance::en_route::{
    en_route_clearance_margin, en_route_net_path_reduction, EnRouteCheck, EnRouteInputs,
    EnRouteMargin, EnRouteVerdict, CORRIDOR_HALF_WIDTH_M, DRIFT_DOWN_CLEARANCE_M,
    POSITIVE_GRADIENT_CLEARANCE_M,
};
use alas_perf::performance::obstacle::NetPathInputs;

use crate::mdo::types::ConstraintFamily::Performance;
use crate::mdo::types::ConstraintResidual;
use crate::mdo::ResidualRole;

const ID: &str = "enroute_obstacle_clearance";
const UNDETERMINED_ID: &str = "enroute_obstacle_clearance_undetermined";
const NOT_COVERED_ID: &str = "enroute_obstacle_survey_does_not_cover_corridor";
const GAP_ID: &str = "enroute_obstacle_clearance_evidence_gap";
/// Diagnostic that carries the straight-out margin when no departure track is declared.
const ASSUMED_TRACK_ID: &str = "enroute_track_assumed_straight_out";

/// The en-route rows for the departure survey. `final_gross_gradient` is the gross OEI
/// gradient in the en-route configuration (clean, maximum continuous, 1.18 VSR: the
/// CS 25.121(c) final-segment state) the net take-off path was built with.
pub(super) fn en_route_rows(
    departure: &Airport,
    surveys: &[DepartureSurvey],
    take_off: NetPathInputs,
    n_engines: i64,
    final_gross_gradient: f64,
    role: ResidualRole,
    track: Option<&[alas_config::DepartureTrackLeg]>,
) -> Vec<ConstraintResidual> {
    let first_leg = track
        .and_then(|legs| legs.first())
        .map(|leg| (leg.heading_deg, leg.distance_m));
    let Some(reduction) = en_route_net_path_reduction(n_engines) else {
        return Vec::new();
    };
    let inputs = EnRouteInputs {
        take_off,
        net_gradient: final_gross_gradient - reduction,
        // RNAV 5 navigation accuracy is assumed (CAT.POL.A.215 (d) widens to 18,5 km
        // otherwise); the aeroplane's navigation specification is not modelled.
        corridor_half_width_m: CORRIDOR_HALF_WIDTH_M,
        first_leg,
    };
    let icao = &departure.icao;
    match en_route_clearance_margin(surveys, &inputs) {
        EnRouteCheck::Invalid => vec![ConstraintResidual::direct(
            GAP_ID,
            Performance,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            role,
        )
        .with_detail(format!("{icao}: en-route net path inputs are not physical"))],
        // The take-off path never ends inside the survey: the 35 ft take-off rule
        // governs all of it.
        EnRouteCheck::NotReached => Vec::new(),
        EnRouteCheck::NoSurveyBeyondPath {
            path_end_s_m,
            survey_edge_s_m,
        } => vec![not_covered_row(
            format!(
                "{icao}: survey does not cover corridor: the take-off path ends {path_end_s_m:.0} m from the start of the roll and the survey ends at {survey_edge_s_m:.0} m, so no terrain beyond the take-off path is surveyed (CAT.POL.A.215)"
            ),
        )],
        EnRouteCheck::Margin(margin) => margin_rows(icao, &inputs, &margin, role, first_leg.is_some()),
    }
}

fn not_covered_row(detail: String) -> ConstraintResidual {
    ConstraintResidual::direct(
        NOT_COVERED_ID,
        Performance,
        0.0,
        1.0,
        "bool",
        1.0,
        0.0,
        ResidualRole::Diagnostic,
    )
    .with_detail(detail)
}

fn margin_rows(
    icao: &str,
    inputs: &EnRouteInputs,
    margin: &EnRouteMargin,
    role: ResidualRole,
    declared: bool,
) -> Vec<ConstraintResidual> {
    let verdict = margin.verdict();
    let kind = if margin.drift_down {
        format!(
            "net path descends or is level ({:+.4}), drift-down {:.0} ft",
            inputs.net_gradient,
            DRIFT_DOWN_CLEARANCE_M / 0.3048
        )
    } else {
        format!(
            "net path climbs ({:+.4}), {:.0} ft",
            inputs.net_gradient,
            POSITIVE_GRADIENT_CLEARANCE_M / 0.3048
        )
    };
    let detail = format!(
        "{icao} {} CAT.POL.A.215 {verdict:?}: {kind}; margin {:.2} m at the generic gear retraction time, required {:.2} m = symmetric survey uncertainty; limiting ground or obstacle {:.1} m above the aerodrome at {:.0} m from the start of the roll, {:+.0} m off the track (take-off path ends at {:.0} m, survey ends at {:.0} m); survey covers +-{:.0} m of the +-{:.0} m corridor",
        margin.runway,
        margin.margin_m,
        margin.uncertainty_m,
        margin.limiting_height_m,
        margin.limiting_s_m,
        margin.limiting_y_m,
        margin.path_end_s_m,
        margin.survey_edge_s_m,
        margin.covered_half_width_m,
        margin.corridor_half_width_m,
    );
    let shortfall = margin.uncertainty_m - margin.margin_m;
    if !declared {
        // No route is declared: CAT.POL.A.215 applies along the planned route, so the
        // straight-out track is an assumption and the verdict is a Diagnostic.
        let mut rows = vec![ConstraintResidual::direct(
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
            "{icao}: no departure track is declared (requirements.departure_track), so the straight-out track is assumed, not the route; the verdict is not a requirement. {detail}"
        ))];
        if !margin.corridor_covered() {
            rows.push(not_covered_row(format!(
                "{icao}: survey does not cover corridor: the surveyed strip is +-{:.0} m of the +-{:.0} m corridor of CAT.POL.A.215 (b), (c); a clearance over the strip is not a pass",
                margin.covered_half_width_m, margin.corridor_half_width_m
            )));
        }
        return rows;
    }
    match verdict {
        EnRouteVerdict::Clears | EnRouteVerdict::Violated => vec![ConstraintResidual::direct(
            ID,
            Performance,
            margin.margin_m,
            margin.uncertainty_m,
            "m",
            shortfall,
            (shortfall / POSITIVE_GRADIENT_CLEARANCE_M).max(0.0),
            role,
        )
        .with_detail(detail)],
        EnRouteVerdict::Undetermined | EnRouteVerdict::SurveyDoesNotCoverCorridor => {
            let slack = margin.violation_slack_m;
            let bound = ConstraintResidual::direct(
                ID,
                Performance,
                slack,
                0.0,
                "m",
                -slack,
                (-slack / POSITIVE_GRADIENT_CLEARANCE_M).max(0.0),
                role,
            )
            .with_detail(format!(
                "{detail}; not a rejection: the row holds the violated bound (most favourable margin plus its symmetric uncertainty, {slack:.2} m, at or above zero)"
            ));
            let why = if verdict == EnRouteVerdict::Undetermined {
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
                    "warning, the terrain dataset accuracy does not decide the en-route clearance: {detail}"
                ))
            } else {
                not_covered_row(
                    format!(
                        "{icao}: survey does not cover corridor: the surveyed strip (+-{:.0} m, to {:.0} m from the start of the roll) clears the en-route net path by {:.2} m, but CAT.POL.A.215 (b), (c) count terrain within +-{:.0} m of the track; the clearance is not a pass. {detail}",
                        margin.covered_half_width_m,
                        margin.survey_edge_s_m,
                        margin.margin_m,
                        margin.corridor_half_width_m,
                    ),
                )
            };
            vec![bound, why]
        }
    }
}
