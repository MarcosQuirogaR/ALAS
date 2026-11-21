// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Local offsets of the drawn planform edges at chosen span stations.
//!
//! The defining stations (side of body, kink, tip, and the optional
//! trailing-edge breaks and rake start) draw the leading and trailing edges as
//! straight lines. A manufacturer drawing also shows features those lines
//! cannot reach: an inboard trailing-edge break between the side of body and
//! the kink, a leading-edge fillet at the side of body, a curved raked tip. A
//! [`WingEdgeStation`] moves both edges by a declared amount at one station;
//! the edges run straight from there to the next station on either side.
//!
//! Frame and signs: `span_fraction` is the distance from the centreline over
//! the semispan; offsets are metres in the planform (XY) projection, positive
//! aft, measured from the straight lines through the defining stations at that
//! span. A leading-edge fillet is a negative `le_offset_m`; a trailing edge
//! pulled forward is a negative `te_offset_m`.
//!
//! A station on the side-of-body fraction moves the side-of-body station itself.
//! A station on a trailing-edge break or on the rake start replaces that station
//! in the loft.
//! A station on the tip of a raked wing moves the tip section itself, so the drawn
//! tip end need not stand on the trailing-edge line the rake holds. The tip of a
//! wing without a rake is a defining station of the reference trapezoid and takes
//! no offset.
//! Other stations must lie outboard of the side of body, inboard of the tip, and
//! off the kink and every other station, whose own values set the edges there.
//!
//! The reference trapezoid uses only the kink and the outboard straight lines,
//! so edge stations move neither the published reference mapping nor `S_ref` in
//! the `Published` and `Trapezoid` conventions. The drawn area sees them all. The
//! lattice and the spar-box integrals see only those on or outboard of where the
//! edge runs start: a fillet inboard of that is a blend body, data for drawing the
//! blend, and the wing stays on its edge lines there (the `inboard_lines` module).

use serde::{Deserialize, Serialize};

use super::config::WingConfig;
use super::planform::{
    MainWingStation, MainWingStationKind, TransportPlanform, TransportPlanformError,
};
use super::planform_inputs::validate_finite;
use super::profile::STATION_SPAN_TOLERANCE;

/// Smallest span-fraction gap from an edge station to any other station. Smaller
/// than `MIN_STATION_SPACING` because a raked tip is drawn over a few percent of
/// the semispan; the sections it makes are still several panels wide.
pub const MIN_EDGE_STATION_SPACING: f64 = 0.01;

/// Most edge stations a wing may declare: an inboard break, a fillet and a curved
/// tip are a handful of stations, and the screening sampler holds them by value.
pub const MAX_EDGE_STATIONS: usize = 8;

/// Offsets of the leading and trailing edges at one span station (see the
/// module documentation for frame and signs).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WingEdgeStation {
    /// Distance from the centreline over the semispan.
    pub span_fraction: f64,
    /// Leading-edge offset from the straight line, metres, positive aft.
    #[serde(default)]
    pub le_offset_m: f64,
    /// Trailing-edge offset from the straight line, metres, positive aft.
    #[serde(default)]
    pub te_offset_m: f64,
}

impl WingConfig {
    /// Resolve [`Self::edge_stations`] onto `planform`, whose defining stations
    /// are already validated. Offsets are measured from the lines before any
    /// edge station applies, so the result does not depend on their neighbours.
    ///
    /// # Errors
    ///
    /// [`TransportPlanformError::InvalidEdgeStation`] for a station out of order
    /// or too close to another one, and the non-finite and non-positive errors
    /// for bad offsets or a chord that closes.
    pub(super) fn apply_edge_stations(
        &self,
        planform: &mut TransportPlanform,
    ) -> Result<(), TransportPlanformError> {
        let lines = planform.stations_with_rake();
        let inboard_limit = planform.side_of_body.map_or(0.0, |sob| sob.span_fraction);
        let mut previous = inboard_limit;
        if let Some(extra) = self.edge_stations.get(MAX_EDGE_STATIONS) {
            return Err(TransportPlanformError::InvalidEdgeStation {
                fraction: extra.span_fraction,
            });
        }
        for station in &self.edge_stations {
            validate_finite("edge station span", station.span_fraction)?;
            validate_finite("edge station leading-edge offset", station.le_offset_m)?;
            validate_finite("edge station trailing-edge offset", station.te_offset_m)?;
            let fraction = station.span_fraction;
            let invalid = || TransportPlanformError::InvalidEdgeStation { fraction };
            if let Some(side_of_body) = planform
                .side_of_body
                .as_mut()
                .filter(|sob| (fraction - sob.span_fraction).abs() <= STATION_SPAN_TOLERANCE)
            {
                side_of_body.leading_edge_x_m += station.le_offset_m;
                side_of_body.chord_m += station.te_offset_m - station.le_offset_m;
                if side_of_body.chord_m <= 0.0 {
                    return Err(TransportPlanformError::NonPositive {
                        field: "side-of-body chord with edge offsets",
                        value: side_of_body.chord_m,
                    });
                }
                continue;
            }
            if planform.tip_rake.is_some()
                && (fraction - planform.tip.span_fraction).abs() <= STATION_SPAN_TOLERANCE
            {
                planform.tip.leading_edge_x_m += station.le_offset_m;
                planform.tip.chord_m += station.te_offset_m - station.le_offset_m;
                if planform.tip.chord_m <= 0.0 {
                    return Err(TransportPlanformError::NonPositive {
                        field: "tip chord with edge offsets",
                        value: planform.tip.chord_m,
                    });
                }
                continue;
            }
            let clear_of_lines = lines.iter().all(|line| {
                let gap = (fraction - line.span_fraction).abs();
                gap >= MIN_EDGE_STATION_SPACING
                    || (gap <= STATION_SPAN_TOLERANCE
                        && matches!(
                            line.kind,
                            MainWingStationKind::TeBreak | MainWingStationKind::TipRake
                        ))
            });
            if !clear_of_lines || fraction <= previous || fraction >= 1.0 {
                return Err(invalid());
            }
            if planform
                .edge_stations
                .last()
                .is_some_and(|last| fraction - last.span_fraction < MIN_EDGE_STATION_SPACING)
            {
                return Err(invalid());
            }
            previous = fraction;
            let y_m = fraction * planform.tip.y_m;
            let pair = lines
                .windows(2)
                .find(|pair| y_m <= pair[1].y_m)
                .ok_or_else(invalid)?;
            let t = (y_m - pair[0].y_m) / (pair[1].y_m - pair[0].y_m);
            let leading_edge_x_m = pair[0].leading_edge_x_m
                + t * (pair[1].leading_edge_x_m - pair[0].leading_edge_x_m);
            let chord_m = pair[0].chord_m + t * (pair[1].chord_m - pair[0].chord_m);
            let moved_chord_m = chord_m + station.te_offset_m - station.le_offset_m;
            if moved_chord_m <= 0.0 {
                return Err(TransportPlanformError::NonPositive {
                    field: "edge station chord",
                    value: moved_chord_m,
                });
            }
            planform.edge_stations.push(MainWingStation {
                kind: MainWingStationKind::EdgeStation,
                span_fraction: fraction,
                y_m,
                leading_edge_x_m: leading_edge_x_m + station.le_offset_m,
                chord_m: moved_chord_m,
            });
        }
        Ok(())
    }
}
