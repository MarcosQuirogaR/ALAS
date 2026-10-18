// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Spanwise dihedral and thickness profiles of the main wing.
//!
//! The scaffold heights (`root_z_m`, `break_z_m`, `tip_z_m`) describe a wing
//! whose dihedral changes once, at the kink. Real transports bend more
//! gradually: a large inboard dihedral that relaxes outboard (Airbus A380,
//! Boeing 747), a cranked gull (Embraer E-Jets) or anhedral. A
//! [`WingDihedralStation`] adds a leading-edge height at an interior span
//! station; the height polyline then runs through root, stations, kink and tip.
//!
//! Likewise a real root section is much thicker than the sections outboard
//! (t/c about 15 to 16 % at the root against 10 to 12 % over the outer wing
//! of a jet), and the thickness falls quickly over the inboard third. A
//! [`WingThicknessStation`] multiplies the thickness of the section the
//! root-kink-tip blend would give at its station; the multiplier is linear
//! between stations and constant beyond the first and last.
//!
//! Both lists are empty by default, which leaves the wing exactly as the
//! root/break/tip scaffold describes it.
//!
//! Frame: `span_fraction` is the distance from the centreline over the
//! semispan; `z_m` is the ground-shape leading-edge height, metres up in the
//! geometry axes (see [`super::WingShape`]).

use serde::{Deserialize, Serialize};

use super::config::WingConfig;

/// Leading-edge height at an interior span station (ground shape).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WingDihedralStation {
    /// Distance from the centreline over the semispan, strictly in `(0, 1)`
    /// and distinct from the kink station.
    pub span_fraction: f64,
    /// Ground-shape leading-edge height in metres, up.
    pub z_m: f64,
    /// Rise of this station from the ground shape to the 1 g flight shape,
    /// metres. `None` takes the uniform dihedral increment
    /// `tip_rise x span_fraction` of [`super::WingShape::Flight`]; a value
    /// lets a wing whose flight shape bends non-uniformly (A380: most of the
    /// dihedral inboard) keep its static ground fit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flight_rise_m: Option<f64>,
}

/// Thickness multiplier at a span station.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WingThicknessStation {
    /// Distance from the centreline over the semispan, in `[0, 1]`.
    pub span_fraction: f64,
    /// Multiplier on the section thickness (t/c) of the root-kink-tip blend.
    pub thickness_scale: f64,
}

/// Why a spanwise profile cannot be lofted.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum WingProfileError {
    /// A value is not finite.
    #[error("wing {list} station {index} {field} must be finite, got {value}")]
    NonFinite {
        /// Which list.
        list: &'static str,
        /// Zero-based index.
        index: usize,
        /// Field name.
        field: &'static str,
        /// Offending value.
        value: f64,
    },
    /// A station lies outside its allowed span interval.
    #[error("wing {list} station {index} span fraction {value} is outside its allowed range")]
    SpanOutOfRange {
        /// Which list.
        list: &'static str,
        /// Zero-based index.
        index: usize,
        /// Offending value.
        value: f64,
    },
    /// Stations are not strictly increasing in span.
    #[error("wing {list} stations must be strictly increasing in span fraction (index {index})")]
    InvalidOrder {
        /// Which list.
        list: &'static str,
        /// Zero-based index of the second of the offending pair.
        index: usize,
    },
    /// A thickness multiplier is not in the physical range.
    #[error("wing thickness scale {value} at station {index} must lie in (0.3, 2.0)")]
    ThicknessScaleOutOfRange {
        /// Zero-based index.
        index: usize,
        /// Offending value.
        value: f64,
    },
    /// A station lies closer than the minimum spacing to a loft section the
    /// planform already has, which would mesh a sliver panel.
    #[error("wing {list} station {index} lies within {MIN_STATION_SPACING} of semispan of the {anchor} station without coinciding with it")]
    TooCloseToStation {
        /// Which list.
        list: &'static str,
        /// Zero-based index.
        index: usize,
        /// Planform station it is too close to.
        anchor: &'static str,
    },
    /// A dihedral station coincides with the kink.
    #[error("wing dihedral station {index} coincides with the kink; set the break height instead (a kink station may only carry a flight rise at the break height)")]
    CoincidesWithKink {
        /// Zero-based index.
        index: usize,
    },
}

/// Smallest span-fraction gap between two distinct loft sections: closer sections mesh a
/// sliver panel the vortex lattice cannot condition (pivot ratio above 1e5).
pub const MIN_STATION_SPACING: f64 = 0.03;

/// Tolerance for two span fractions to denote the same station.
pub const STATION_SPAN_TOLERANCE: f64 = 5.0e-4;

impl WingConfig {
    /// Validate both spanwise profiles against the kink station.
    ///
    /// # Errors
    ///
    /// [`WingProfileError`] naming the first offending station.
    pub fn validate_profiles(&self, kink_span_fraction: f64) -> Result<(), WingProfileError> {
        let sob = self
            .side_of_body_chord_ratio
            .and(self.side_of_body_span_fraction);
        let too_close = |fraction: f64| -> Option<&'static str> {
            let near = |anchor: f64| {
                let gap = (fraction - anchor).abs();
                gap > STATION_SPAN_TOLERANCE && gap < MIN_STATION_SPACING
            };
            if near(kink_span_fraction) {
                Some("kink")
            } else if sob.is_some_and(near) {
                Some("side-of-body")
            } else {
                None
            }
        };
        let mut previous: Option<f64> = None;
        for (index, station) in self.dihedral_stations.iter().enumerate() {
            if let Some(anchor) = too_close(station.span_fraction) {
                return Err(WingProfileError::TooCloseToStation {
                    list: "dihedral",
                    index,
                    anchor,
                });
            }
            for (field, value) in [
                ("span_fraction", station.span_fraction),
                ("z_m", station.z_m),
                ("flight_rise_m", station.flight_rise_m.unwrap_or(0.0)),
            ] {
                if !value.is_finite() {
                    return Err(WingProfileError::NonFinite {
                        list: "dihedral",
                        index,
                        field,
                        value,
                    });
                }
            }
            if !(station.span_fraction > 0.0 && station.span_fraction < 1.0) {
                return Err(WingProfileError::SpanOutOfRange {
                    list: "dihedral",
                    index,
                    value: station.span_fraction,
                });
            }
            // A station on the kink only carries the flight rise of the kink
            // (its ground height is the break height).
            if (station.span_fraction - kink_span_fraction).abs() <= STATION_SPAN_TOLERANCE
                && ((station.z_m - self.break_z_m).abs() > 1.0e-9
                    || station.flight_rise_m.is_none())
            {
                return Err(WingProfileError::CoincidesWithKink { index });
            }
            if previous.is_some_and(|p| station.span_fraction <= p) {
                return Err(WingProfileError::InvalidOrder {
                    list: "dihedral",
                    index,
                });
            }
            previous = Some(station.span_fraction);
        }
        previous = None;
        for (index, station) in self.thickness_stations.iter().enumerate() {
            if let Some(anchor) = too_close(station.span_fraction) {
                return Err(WingProfileError::TooCloseToStation {
                    list: "thickness",
                    index,
                    anchor,
                });
            }
            for (field, value) in [
                ("span_fraction", station.span_fraction),
                ("thickness_scale", station.thickness_scale),
            ] {
                if !value.is_finite() {
                    return Err(WingProfileError::NonFinite {
                        list: "thickness",
                        index,
                        field,
                        value,
                    });
                }
            }
            if !(0.0..=1.0).contains(&station.span_fraction) {
                return Err(WingProfileError::SpanOutOfRange {
                    list: "thickness",
                    index,
                    value: station.span_fraction,
                });
            }
            if !(station.thickness_scale > 0.3 && station.thickness_scale < 2.0) {
                return Err(WingProfileError::ThicknessScaleOutOfRange {
                    index,
                    value: station.thickness_scale,
                });
            }
            if previous.is_some_and(|p| station.span_fraction <= p) {
                return Err(WingProfileError::InvalidOrder {
                    list: "thickness",
                    index,
                });
            }
            previous = Some(station.span_fraction);
        }
        Ok(())
    }

    /// Thickness multiplier at span fraction `eta`: 1 with no stations,
    /// piecewise linear between stations and held beyond the first and last.
    pub fn thickness_scale_at(&self, eta: f64) -> f64 {
        let stations = &self.thickness_stations;
        let (Some(first), Some(last)) = (stations.first(), stations.last()) else {
            return 1.0;
        };
        if eta <= first.span_fraction {
            return first.thickness_scale;
        }
        if eta >= last.span_fraction {
            return last.thickness_scale;
        }
        for pair in stations.windows(2) {
            if eta <= pair[1].span_fraction {
                let t =
                    (eta - pair[0].span_fraction) / (pair[1].span_fraction - pair[0].span_fraction);
                return pair[0].thickness_scale
                    + t * (pair[1].thickness_scale - pair[0].thickness_scale);
            }
        }
        last.thickness_scale
    }

    /// Span fractions, ascending, at which the profiles need a loft station
    /// of their own: every dihedral station and every thickness station
    /// strictly inside `(0, 1)`.
    pub fn profile_span_fractions(&self) -> Vec<f64> {
        let mut fractions: Vec<f64> = self
            .dihedral_stations
            .iter()
            .map(|s| s.span_fraction)
            .chain(
                self.thickness_stations
                    .iter()
                    .map(|s| s.span_fraction)
                    .filter(|f| *f > 0.0 && *f < 1.0),
            )
            .collect();
        fractions.sort_by(f64::total_cmp);
        fractions.dedup_by(|a, b| (*a - *b).abs() <= STATION_SPAN_TOLERANCE);
        fractions
    }
}
