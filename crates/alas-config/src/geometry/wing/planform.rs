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
    /// Outboard trailing-edge break between the kink and the tip: the leading
    /// edge stays on the straight outboard line and the chord is the straight
    /// taper chord plus an offset. Not part of [`TransportPlanform::stations`];
    /// see [`TransportPlanform::te_breaks`].
    TeBreak,
    /// Start of a swept-back tip rake, between the kink and the tip. It is
    /// not part of [`TransportPlanform::stations`]; see
    /// [`TransportPlanform::tip_rake`].
    TipRake,
    /// Station whose edges are offset from the straight lines (see
    /// [`super::WingEdgeStation`]). Not part of [`TransportPlanform::stations`];
    /// see [`TransportPlanform::edge_stations`].
    EdgeStation,
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
    /// Centerline root station: the design datum the inboard panel extends to.
    /// A product wing does not loft this chord: its edges stay on their own inboard
    /// lines to `y = 0` (see [`Self::inboard_edge_lines`] and the builder's
    /// `carry_through` module).
    pub root: MainWingStation,
    /// Fuselage side-of-body station when requested.
    pub side_of_body: Option<MainWingStation>,
    /// Yehudi crank station.
    pub kink: MainWingStation,
    /// Wingtip station.
    pub tip: MainWingStation,
    /// Optional start of a swept-back tip rake: the leading edge follows the
    /// straight design sweep to here and sweeps further aft outboard of it,
    /// while the trailing edge stays one straight line from the kink to the
    /// tip. Present only when the wing declares a rake; kept out of
    /// [`Self::stations`], which lists the defining stations, and included by
    /// [`Self::reference_area_m2`], [`Self::panels`] and
    /// [`Self::leading_edge_x_at`].
    pub tip_rake: Option<MainWingStation>,
    /// Optional outboard trailing-edge break stations (kind
    /// [`MainWingStationKind::TeBreak`], zero to two, in span order): on the straight
    /// outboard leading-edge line, with the straight kink-to-tip chord plus an offset.
    /// Kept out of [`Self::stations`]; included by [`Self::stations_with_rake`], the
    /// reference area, the panels and [`Self::leading_edge_x_at`], but not by
    /// the reference trapezoid (which stays on the straight outboard lines).
    pub te_breaks: Vec<MainWingStation>,
    /// Optional stations with offset edges (kind [`MainWingStationKind::EdgeStation`], in
    /// span order, outboard of the side of body), inboard or outboard of the kink. Treated
    /// as the trailing-edge breaks are: out of [`Self::stations`] and the reference
    /// trapezoid, in [`Self::stations_with_rake`] and everything built on it. Those
    /// that make a fillet are blend bodies, not wing: see [`Self::without_blends`].
    pub edge_stations: Vec<MainWingStation>,
    /// The user-defined sections (kind [`MainWingStationKind::EdgeStation`]), each a
    /// section of the built wing as drawn. Those between the side of body and the kink
    /// are vertices of the edge polylines ([`Self::inboard_edge_lines`]) and stations of
    /// [`Self::without_blends`]; nothing moves them.
    pub custom_stations: Vec<MainWingStation>,
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
    /// The tip-rake start station does not lie strictly outboard of the kink and inboard of the tip.
    #[error(
        "tip-rake start must satisfy kink < rake start < tip; got kink {kink}, rake start {rake_start} (semispan fractions)"
    )]
    TipRakeStartOutOfOrder {
        /// Kink semispan fraction.
        kink: f64,
        /// Tip-rake start semispan fraction.
        rake_start: f64,
    },
    /// The tip-rake chord is not below the design tip chord: the rake would not sweep the
    /// leading edge aft of its straight line, and the chord would not fall outboard.
    #[error(
        "tip-rake tip chord ({rake_tip_chord_m} m) must be below the design tip chord the straight edges reach ({tip_chord_m} m)"
    )]
    TipRakeChordNotBelowTip {
        /// Declared chord at the extreme tip, in metres.
        rake_tip_chord_m: f64,
        /// Design-vector tip chord, in metres.
        tip_chord_m: f64,
    },
    /// A trailing-edge break lies off its allowed span range.
    #[error(
        "trailing-edge breaks must lie outboard of the kink ({kink}), in increasing order and inboard of the tip rake; got {fraction}"
    )]
    InvalidTrailingEdgeBreak {
        /// Kink semispan fraction.
        kink: f64,
        /// Offending break semispan fraction.
        fraction: f64,
    },
    /// An edge station lies off its allowed span range.
    #[error(
        "wing edge stations (at most {}) must lie outboard of the side of body and inboard of the tip, in increasing order and off every other station by at least {} of the semispan; got {fraction}",
        super::MAX_EDGE_STATIONS,
        super::MIN_EDGE_STATION_SPACING
    )]
    InvalidEdgeStation {
        /// Offending semispan fraction.
        fraction: f64,
    },
    /// A query falls outside the modeled semispan.
    #[error("spanwise position {y_m} m lies outside the modeled semispan of {tip_y_m} m")]
    SpanwisePositionOutsidePlanform {
        /// Requested absolute spanwise position, in metres.
        y_m: f64,
        /// Modeled semispan, in metres.
        tip_y_m: f64,
    },
    /// Chord increases moving outboard, which is outside this transport family. A root
    /// chord that is only a datum ([`super::WingConfig::root_chord_is_only_a_datum`]) is
    /// not compared.
    #[error(
        "main-wing chord must be non-increasing from root to tip; {inboard} is followed by {outboard}"
    )]
    NonMonotoneChord {
        /// Inboard chord, in metres.
        inboard: f64,
        /// Outboard chord, in metres.
        outboard: f64,
    },
    /// The static-to-1 g tip rise lies outside the small-deflection range.
    #[error(
        "flight tip rise must lie between 0 and {} of the semispan, got {value}",
        super::MAX_FLIGHT_TIP_RISE_SEMISPAN_FRACTION
    )]
    FlightTipRiseOutOfRange {
        /// Invalid semispan fraction.
        value: f64,
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

    /// The optional trailing-edge-break, edge and tip-rake stations between the
    /// kink and the tip, in span order.
    pub fn outboard_stations(&self) -> Vec<MainWingStation> {
        // An edge station on a break or on the rake start stands in for it: the
        // straight-line station stays on its field for the reference trapezoid.
        let lofted = |station: &MainWingStation| {
            !self.edge_stations.iter().any(|edge| {
                (edge.span_fraction - station.span_fraction).abs() <= super::STATION_SPAN_TOLERANCE
            })
        };
        let mut stations: Vec<_> = self
            .te_breaks
            .iter()
            .chain(&self.tip_rake)
            .filter(|station| lofted(station))
            .chain(
                self.edge_stations
                    .iter()
                    .filter(|station| station.y_m > self.kink.y_m),
            )
            .copied()
            .collect();
        stations.sort_by(|a, b| a.y_m.total_cmp(&b.y_m));
        stations
    }

    /// Defining stations plus the optional outboard stations, centreline to tip.
    pub fn stations_with_rake(&self) -> Vec<MainWingStation> {
        let mut stations = self.stations();
        stations.extend(self.outboard_stations());
        stations.extend(
            self.edge_stations
                .iter()
                .filter(|station| station.y_m < self.kink.y_m),
        );
        stations.sort_by(|a, b| a.y_m.total_cmp(&b.y_m));
        stations
    }

    /// Projected (XY) reference area of both semispans, m^2: the straight
    /// trapezoidal panels between the stations, `2 sum (c_i + c_o)/2 (y_o -
    /// y_i)`. The geometry builder lofts these panels with chord linear in
    /// span, so this is the main wing's `s_ref` before any non-planar tip
    /// device.
    pub fn reference_area_m2(&self) -> f64 {
        self.stations_with_rake()
            .windows(2)
            .map(|pair| (pair[0].chord_m + pair[1].chord_m) * (pair[1].y_m - pair[0].y_m))
            .sum()
    }

    /// Return every straight panel, including the optional side-of-body panel.
    pub fn panels(&self) -> Vec<MainWingPanel> {
        let stations = self.stations_with_rake();
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
        let stations = self.stations_with_rake();
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
    /// The station lies at or inboard of the side of body, where the wing is set by
    /// the side-of-body section and the two edge lines.
    #[error(
        "custom wing section {index} at span fraction {span_fraction} lies at or inboard of the \
         side of body at {side_of_body}: the wing there is set by the side-of-body section and \
         the edge lines, so the section could not be honoured; move it outboard of the side of \
         body or remove it"
    )]
    AtOrInboardOfSideOfBody {
        /// Zero-based section index.
        index: usize,
        /// Semispan fraction of the section.
        span_fraction: f64,
        /// Semispan fraction of the side of body.
        side_of_body: f64,
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
