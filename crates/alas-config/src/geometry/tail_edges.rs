// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Local offsets of the built tail edges from their straight lines: rounded
//! corners, curved leading edges and sloped caps.
//!
//! The root and tip chords and the tip offset draw a tail surface as a
//! trapezoid. A manufacturer drawing also shows a rounded leading-edge corner at
//! the tip, or a fin cap that slopes down aft, neither of which a section at
//! constant height can reach. A [`TailEdgeStation`] moves both edges by a
//! declared amount at one station of that trapezoid, and the drawn edges run
//! straight from there to the next station on either side. A cap that slopes
//! down aft is the trailing edge pulled forward along it, ending on the tip
//! station.
//!
//! Frame and signs: `span_fraction` is the distance from the configured root
//! line (the tailplane centreline, the fin root line) over the distance to the
//! configured tip, in `(0, 1]`, where 1 is the tip itself. Offsets are metres at
//! unit tail scale in the plane of the surface, positive aft, measured from the
//! straight lines at that fraction, so a rounded leading-edge corner is a
//! positive `le_offset_m` and a trailing edge pulled forward a negative
//! `te_offset_m`.
//!
//! # What reads them
//!
//! The stations are the surface: the built sections carry them, and the vortex
//! lattice, the exports, the renders and the wetted area all read those. The
//! straight trapezoid stays as the surface's reference planform, as the main
//! wing's reference trapezoid does: tail area, mean aerodynamic chord, arm, taper
//! and sweep in the tail volumes and the handbook correlations are read there,
//! because those methods are stated on that planform (Raymer, Aircraft Design,
//! ch. 7; NASA TM-2017-219627, the FLOPS theoretical tail areas).

use serde::{Deserialize, Serialize};

/// Most edge stations one tail surface may declare: a corner and a cap are a
/// handful of stations.
pub const MAX_TAIL_EDGE_STATIONS: usize = 8;

/// Smallest span-fraction gap between two edge stations. Every station is a lattice
/// section, so a gap below this would be a repeated station or a sliver strip. The
/// builder treats a station this close to the built root as reached by it.
pub const MIN_TAIL_EDGE_STATION_SPACING: f64 = 0.002;

/// Offsets of the leading and trailing edges at one tail station (see the
/// module documentation for frame and signs).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TailEdgeStation {
    /// Distance from the configured root line over the distance to the tip.
    pub span_fraction: f64,
    /// Leading-edge offset from the straight line, metres, positive aft.
    #[serde(default)]
    pub le_offset_m: f64,
    /// Trailing-edge offset from the straight line, metres, positive aft.
    #[serde(default)]
    pub te_offset_m: f64,
}

/// One drawn station of the ventral fin pair under the tail, starboard side, in the
/// fin's frame so that it moves with the fin. The section joins the two points, so a
/// fin standing canted under a rising keel needs no twist of its own. Metres, x aft of
/// the fin root leading edge, y from the centreline, z above the fin root line.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VentralFinStation {
    /// Distance from the centreline.
    pub y_m: f64,
    /// Leading edge x.
    pub le_x_m: f64,
    /// Leading edge z.
    pub le_z_m: f64,
    /// Trailing edge x.
    pub te_x_m: f64,
    /// Trailing edge z.
    pub te_z_m: f64,
}

impl VentralFinStation {
    /// A station read off a drawing in aircraft axes, for a fin root leading edge at
    /// `fin_x_m` on the fin root line `fin_z_m`.
    #[must_use]
    pub const fn drawn(
        (fin_x_m, fin_z_m): (f64, f64),
        y_m: f64,
        (le_x_m, le_z_m): (f64, f64),
        (te_x_m, te_z_m): (f64, f64),
    ) -> Self {
        Self {
            y_m,
            le_x_m: le_x_m - fin_x_m,
            le_z_m: le_z_m - fin_z_m,
            te_x_m: te_x_m - fin_x_m,
            te_z_m: te_z_m - fin_z_m,
        }
    }
}

/// Why a list of tail edge stations cannot be drawn.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TailEdgeStationError {
    /// More stations than [`MAX_TAIL_EDGE_STATIONS`].
    #[error("a tail surface takes at most {MAX_TAIL_EDGE_STATIONS} edge stations, got {count}")]
    TooMany {
        /// Stations declared.
        count: usize,
    },
    /// A value is not finite.
    #[error("tail edge station {index} {field} must be finite, got {value}")]
    NonFinite {
        /// Zero-based index.
        index: usize,
        /// Field name.
        field: &'static str,
        /// Offending value.
        value: f64,
    },
    /// A station lies off `(0, 1]`, or out of order or too close to the previous one.
    #[error(
        "tail edge station {index} span fraction {value} must lie in (0, 1], ascending and at least {MIN_TAIL_EDGE_STATION_SPACING} after the previous station"
    )]
    InvalidSpan {
        /// Zero-based index.
        index: usize,
        /// Offending value.
        value: f64,
    },
}

/// Validate the order, range and finiteness of `stations`.
///
/// # Errors
///
/// [`TailEdgeStationError`] naming the first offending station.
pub fn validate_tail_edge_stations(
    stations: &[TailEdgeStation],
) -> Result<(), TailEdgeStationError> {
    if stations.len() > MAX_TAIL_EDGE_STATIONS {
        return Err(TailEdgeStationError::TooMany {
            count: stations.len(),
        });
    }
    let mut previous = 0.0;
    for (index, station) in stations.iter().enumerate() {
        for (field, value) in [
            ("span_fraction", station.span_fraction),
            ("le_offset_m", station.le_offset_m),
            ("te_offset_m", station.te_offset_m),
        ] {
            if !value.is_finite() {
                return Err(TailEdgeStationError::NonFinite {
                    index,
                    field,
                    value,
                });
            }
        }
        let value = station.span_fraction;
        let clear = if index == 0 {
            value > 0.0
        } else {
            value >= previous + MIN_TAIL_EDGE_STATION_SPACING
        };
        if !clear || value > 1.0 {
            return Err(TailEdgeStationError::InvalidSpan { index, value });
        }
        previous = value;
    }
    Ok(())
}

// A test asserts on values it constructed here directly, so a failed unwrap
// is the assertion failing.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn at(span_fraction: f64) -> TailEdgeStation {
        TailEdgeStation {
            span_fraction,
            le_offset_m: 0.1,
            te_offset_m: 0.0,
        }
    }

    #[test]
    fn stations_up_to_the_tip_in_order_are_valid() {
        validate_tail_edge_stations(&[]).unwrap();
        validate_tail_edge_stations(&[at(0.9), at(0.95), at(1.0)]).unwrap();
    }

    #[test]
    fn a_station_out_of_range_or_order_is_rejected() {
        for stations in [
            vec![at(0.0)],
            vec![at(1.01)],
            vec![at(0.9), at(0.9)],
            vec![at(0.95), at(0.9)],
            vec![at(f64::NAN)],
        ] {
            assert!(validate_tail_edge_stations(&stations).is_err());
        }
        assert!(validate_tail_edge_stations(&[at(0.5); 9]).is_err());
    }
}
