// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Transport planform description: stations, panels and the errors a planform can raise.

use serde::{Deserialize, Serialize};

/// A named station on one half of the main-wing planform.
///
/// The leading-edge coordinates are relative to the centerline root leading
/// edge. The geometry builder applies the fuselage-station datum afterwards,
/// so this description remains useful to aerodynamic, structural, and
/// reporting code without making those callers reproduce planform algebra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MainWingStationKind {
    /// Centerline continuation of the wing box.
    Root,
    /// Fuselage side-of-body station, when the transport extension is active.
    SideOfBody,
    /// Yehudi crank between the inboard and outboard panels.
    Kink,
    /// Wingtip.
    Tip,
}

/// One planform station, measured on the right semispan.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MainWingStation {
    /// Identity of this station in the transport planform.
    pub kind: MainWingStationKind,
    /// Spanwise position as a fraction of semispan.
    pub span_fraction: f64,
    /// Spanwise coordinate from the centerline in metres.
    pub y_m: f64,
    /// Leading-edge X coordinate relative to the centerline root in metres.
    pub leading_edge_x_m: f64,
    /// Local aerodynamic chord in metres.
    pub chord_m: f64,
}

/// Geometric data for the exposed inboard section used by a 2-D section solver.
///
/// A side-of-body station is the physical inboard aerodynamic section when
/// the transport extension is active. Legacy root/kink/tip planforms have no
/// such station, so they retain the centerline root section and incidence.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct InboardAerodynamicStation {
    /// Spanwise position from the centerline, in metres.
    pub y_m: f64,
    /// Leading-edge offset relative to the centerline root, in metres.
    pub leading_edge_x_m: f64,
    /// Local chord, in metres.
    pub chord_m: f64,
    /// Geometric incidence after root-to-kink twist interpolation, in degrees.
    pub twist_deg: f64,
}

/// A straight planform panel between two named stations.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MainWingPanel {
    /// Inboard panel station.
    pub inboard: MainWingStation,
    /// Outboard panel station.
    pub outboard: MainWingStation,
    /// Leading-edge sweep, positive aft, in degrees.
    pub leading_edge_sweep_deg: f64,
    /// Trailing-edge sweep, positive aft, in degrees.
    pub trailing_edge_sweep_deg: f64,
}

/// Validated transport planform resolved from a wing scaffold and design vector.
///
/// The optional side-of-body station leaves existing root/kink/tip designs
/// unchanged when disabled. When enabled, it resolves the additional inboard
/// chord and exposes the leading- and trailing-edge geometry from one source
/// of truth.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransportPlanform {
    /// Centerline root station.
    pub root: MainWingStation,
    /// Fuselage side-of-body station when requested.
    pub side_of_body: Option<MainWingStation>,
    /// Yehudi crank station.
    pub kink: MainWingStation,
    /// Wingtip station.
    pub tip: MainWingStation,
    /// Inboard leading-edge sweep set by the design vector.
    pub inboard_le_sweep_deg: f64,
    /// Outboard leading-edge sweep. Product transport planforms keep this
    /// equal to the design-vector sweep; legacy replay retains its decrement.
    pub outboard_le_sweep_deg: f64,
}

/// Why a proposed main-wing planform is not physically constructible.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TransportPlanformError {
    /// A required geometric quantity is not a finite real number.
    #[error("{field} must be finite, got {value}")]
    NonFinite {
        /// Name of the invalid quantity.
        field: &'static str,
        /// Invalid value.
        value: f64,
    },
    /// Span or chord is zero or negative.
    #[error("{field} must be positive, got {value}")]
    NonPositive {
        /// Name of the invalid quantity.
        field: &'static str,
        /// Invalid value.
        value: f64,
    },
    /// The side-of-body or kink location does not define ordered stations.
    #[error(
        "transport stations must satisfy 0 <= side-of-body < kink < tip; got side-of-body {side_of_body}, kink {kink}"
    )]
    InvalidStationOrder {
        /// Side-of-body semispan fraction.
        side_of_body: f64,
        /// Kink semispan fraction.
        kink: f64,
    },
    /// A query falls outside the modeled semispan.
    #[error("spanwise position {y_m} m lies outside the modeled semispan of {tip_y_m} m")]
    SpanwisePositionOutsidePlanform {
        /// Requested absolute spanwise position, in metres.
        y_m: f64,
        /// Modeled semispan, in metres.
        tip_y_m: f64,
    },
    /// Chord increases moving outboard, which is outside this transport family.
    #[error(
        "main-wing chord must be non-increasing from root to tip; {inboard} is followed by {outboard}"
    )]
    NonMonotoneChord {
        /// Inboard chord, in metres.
        inboard: f64,
        /// Outboard chord, in metres.
        outboard: f64,
    },
    /// A sweep approaches a spanwise leading or trailing edge.
    #[error("{field} must lie strictly between -89 and 89 deg, got {value} deg")]
    InvalidSweep {
        /// Name of the invalid sweep.
        field: &'static str,
        /// Invalid value.
        value: f64,
    },
}

impl TransportPlanform {
    /// Return stations in centerline-to-tip order without a duplicate root.
    pub fn stations(&self) -> Vec<MainWingStation> {
        let mut stations = vec![self.root];
        if let Some(side_of_body) = self.side_of_body {
            stations.push(side_of_body);
        }
        stations.push(self.kink);
        stations.push(self.tip);
        stations
    }

    /// Return every straight panel, including the optional side-of-body panel.
    pub fn panels(&self) -> Vec<MainWingPanel> {
        let stations = self.stations();
        stations
            .windows(2)
            .map(|pair| MainWingPanel::from_stations(pair[0], pair[1]))
            .collect()
    }

    /// Leading-edge X offset at `spanwise_position_m` on either semispan.
    ///
    /// # Errors
    ///
    /// Returns [`TransportPlanformError::SpanwisePositionOutsidePlanform`]
    /// when a caller asks outside the finite root-to-tip station interval. The
    /// value is otherwise obtained from the panel that contains the requested
    /// station.
    pub fn leading_edge_x_at(
        &self,
        spanwise_position_m: f64,
    ) -> Result<f64, TransportPlanformError> {
        if !spanwise_position_m.is_finite() {
            return Err(TransportPlanformError::NonFinite {
                field: "spanwise position",
                value: spanwise_position_m,
            });
        }
        let y = spanwise_position_m.abs();
        let stations = self.stations();
        let tip_y_m = self.tip.y_m;
        if y > tip_y_m {
            return Err(TransportPlanformError::SpanwisePositionOutsidePlanform {
                y_m: y,
                tip_y_m,
            });
        }
        for panel in stations.windows(2) {
            let inboard = panel[0];
            let outboard = panel[1];
            if y <= outboard.y_m {
                let fraction = (y - inboard.y_m) / (outboard.y_m - inboard.y_m);
                return Ok(inboard.leading_edge_x_m
                    + fraction * (outboard.leading_edge_x_m - inboard.leading_edge_x_m));
            }
        }
        Ok(self.tip.leading_edge_x_m)
    }
}

impl MainWingPanel {
    fn from_stations(inboard: MainWingStation, outboard: MainWingStation) -> Self {
        let span_m = outboard.y_m - inboard.y_m;
        let leading_edge_sweep_deg = (outboard.leading_edge_x_m - inboard.leading_edge_x_m)
            .atan2(span_m)
            .to_degrees();
        let inboard_trailing_edge_x_m = inboard.leading_edge_x_m + inboard.chord_m;
        let outboard_trailing_edge_x_m = outboard.leading_edge_x_m + outboard.chord_m;
        let trailing_edge_sweep_deg = (outboard_trailing_edge_x_m - inboard_trailing_edge_x_m)
            .atan2(span_m)
            .to_degrees();
        Self {
            inboard,
            outboard,
            leading_edge_sweep_deg,
            trailing_edge_sweep_deg,
        }
    }
}

/// An additional user-controlled station on the right main-wing semispan.
///
/// The span fraction is measured from the centerline to the tip. The section
/// is lofted with the stations supplied here; its airfoil name is resolved by
/// `alas-geom` without changing a preset's locked geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WingSection {
    /// Position from the centerline as a fraction of semispan, in `(0, 1)`.
    pub span_fraction: f64,
    /// Leading-edge X offset from the root leading-edge datum, in metres.
    pub leading_edge_x_m: f64,
    /// Local aerodynamic chord, in metres.
    pub chord_m: f64,
    /// Leading-edge vertical position, in metres.
    pub z_m: f64,
    /// Geometric section twist, in degrees.
    pub twist_deg: f64,
    /// Airfoil name resolved through the shared airfoil library.
    pub airfoil: String,
}

/// Why a user-defined wing station cannot be lofted safely.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum WingSectionError {
    /// A geometric field is not finite.
    #[error("custom wing section {index} field {field} must be finite, got {value}")]
    NonFinite {
        /// Zero-based section index.
        index: usize,
        /// Field name.
        field: &'static str,
        /// Invalid value.
        value: f64,
    },
    /// A chord is zero or negative.
    #[error("custom wing section {index} chord must be positive, got {value}")]
    NonPositiveChord {
        /// Zero-based section index.
        index: usize,
        /// Invalid chord, in metres.
        value: f64,
    },
    /// The station lies at or beyond a defining root/tip section.
    #[error("custom wing section {index} span fraction must lie strictly in (0, 1), got {value}")]
    SpanOutOfRange {
        /// Zero-based section index.
        index: usize,
        /// Invalid semispan fraction.
        value: f64,
    },
    /// Stations must be supplied in centerline-to-tip order.
    #[error("custom wing sections must have strictly increasing span fractions; section {index} has {current} after {previous}")]
    InvalidOrder {
        /// Zero-based section index of the later station.
        index: usize,
        /// Previous span fraction.
        previous: f64,
        /// Current span fraction.
        current: f64,
    },
    /// A custom station would duplicate an existing planform edge.
    #[error("custom wing section at span fraction {span_fraction} duplicates the {kind:?} planform station")]
    DuplicatePlanformStation {
        /// Duplicate span fraction.
        span_fraction: f64,
        /// Existing station identity.
        kind: MainWingStationKind,
    },
    /// A custom chord would reverse the transport taper.
    #[error("custom wing chord increases outboard from {inboard} m to {outboard} m")]
    NonMonotoneChord {
        /// Inboard chord, in metres.
        inboard: f64,
        /// Outboard chord, in metres.
        outboard: f64,
    },
    /// The airfoil lookup key is empty.
    #[error("custom wing section {0} needs a non-empty airfoil name")]
    EmptyAirfoil(usize),
}
