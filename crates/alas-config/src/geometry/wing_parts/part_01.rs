// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use serde::{Deserialize, Serialize};

use crate::{ConfigNode, DesignVector};

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

/// Main-wing scaffold not covered by the design vector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct WingConfig {
    /// How far aft of the nose the wing root sits.
    #[config(
        label = "Wing root X position",
        unit = "m",
        help = "Fuselage-station X of the wing-root leading-edge datum -- how far aft of the nose the wing sits."
    )]
    pub root_datum_x_m: f64,

    /// Where the root section sits vertically.
    #[config(
        label = "Wing root vertical offset",
        unit = "m",
        help = "Vertical (Z) placement of the wing-root leading edge relative to the fuselage centerline."
    )]
    pub root_z_m: f64,

    /// Where the crank section sits vertically.
    #[config(
        label = "Wing break vertical offset",
        unit = "m",
        help = "Vertical placement of the mid-span 'break' section leading edge, where the taper/dihedral rate changes."
    )]
    pub break_z_m: f64,

    /// Where the tip section sits vertically, which is what sets dihedral.
    #[config(
        label = "Wing tip vertical offset",
        unit = "m",
        help = "Vertical placement of the wing-tip leading edge. Tip above root gives positive dihedral."
    )]
    pub tip_z_m: f64,

    /// Incidence of the root section.
    #[config(
        label = "Wing root twist",
        unit = "deg",
        help = "Geometric twist (incidence) of the root section, positive = leading-edge-up (washin)."
    )]
    pub root_twist_deg: f64,

    /// Incidence of the crank section.
    #[config(
        label = "Wing break twist",
        unit = "deg",
        help = "Geometric twist of the mid-span break section."
    )]
    pub break_twist_deg: f64,

    /// Where along the semispan the planform cranks.
    #[config(
        label = "Wing break span location",
        unit = "0-1 of semispan",
        help = "Spanwise position of the trailing-edge break, as a fraction of the semispan (0 = root, 1 = tip)."
    )]
    pub break_span_fraction: f64,

    /// Optional side-of-body station for an extended inboard wing box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        label = "Wing side-of-body span location",
        unit = "0-1 of semispan",
        help = "Optional fuselage side-of-body station as a semispan fraction. Leave unset to retain the legacy centerline-root planform."
    )]
    pub side_of_body_span_fraction: Option<f64>,

    /// Chord retained at the optional side-of-body station.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        label = "Wing side-of-body chord ratio",
        unit = "root chord ratio",
        help = "Optional side-of-body chord divided by root chord. A value near one retains wing-box and high-lift depth inboard."
    )]
    pub side_of_body_chord_ratio: Option<f64>,

    /// Optional replacement for the historical fixed break location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        label = "Wing kink span location",
        unit = "0-1 of semispan",
        help = "Optional Yehudi-kink station as a semispan fraction. When unset, the legacy wing break span location remains active."
    )]
    pub kink_span_fraction: Option<f64>,

    /// Historical outboard sweep decrement retained for reference replay.
    #[config(
        label = "Outboard sweep reduction",
        unit = "deg",
        help = "How many degrees less swept the outboard panel is than the inboard panel (a common yehudi/crank shape)."
    )]
    pub outboard_sweep_decrement_deg: f64,

    /// Historical independent outboard leading-edge sweep retained for saved
    /// configuration compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        label = "Outboard leading-edge sweep",
        unit = "deg",
        help = "Legacy reference-replay override. Product transport wings use the design-vector sweep across both panels so this value cannot create an unintended leading-edge crank."
    )]
    pub outboard_le_sweep_deg: Option<f64>,

    /// The section the root is morphed from.
    #[config(
        options = Airfoil,
        label = "Root airfoil section",
        help = "Reference airfoil at the wing root, morphed by the design vector's thickness/camber scale factors."
    )]
    pub root_airfoil: String,

    /// The section the tip is morphed from.
    #[config(
        options = Airfoil,
        label = "Tip airfoil section",
        help = "Reference airfoil at the wing tip."
    )]
    pub tip_airfoil: String,

    /// How many spanwise panels the whole wing semispan is meshed into.
    #[config(
        label = "Wing VLM panel count",
        help = "Spanwise panels across the whole wing semispan for the vortex-lattice solver. This is an absolute count, not a count per section: a planform with a side-of-body station and a kink gets the same mesh density as one without, and adding a station no longer changes the panel count underneath a search. Every planform station -- root, side-of-body, kink, tip -- is always kept as a panel edge whatever the count, so refining the mesh never averages a kink away. The default of 24 is converged: a twelve-fold refinement moves the trimmed cruise attitude by 0.01 deg."
    )]
    pub n_subdivisions: i64,
}

impl Default for WingConfig {
    fn default() -> Self {
        // A supercritical inboard section washing out into a thinner
        // conventional tip, cranked at 35% semispan: the planform family of a
        // current twin-aisle transport.
        Self {
            root_datum_x_m: 26.24,
            root_z_m: -2.1,
            break_z_m: -0.3,
            tip_z_m: 2.5,
            root_twist_deg: 4.0,
            break_twist_deg: 2.0,
            break_span_fraction: 0.35,
            side_of_body_span_fraction: Some(0.10),
            // The side-of-body chord is derived from the root-to-kink panel.
            // Treating it as equal to the centreline chord created a false
            // extra crank and a forward-running exposed trailing edge.
            side_of_body_chord_ratio: None,
            kink_span_fraction: Some(0.37),
            outboard_sweep_decrement_deg: 2.0,
            outboard_le_sweep_deg: None,
            root_airfoil: "SC2-0714".to_owned(),
            tip_airfoil: "naca2410".to_owned(),
            n_subdivisions: 24,
        }
    }
}
