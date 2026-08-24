// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/geometry_config.py (`WingConfig`)
// Reference: alas @ rust-port-baseline.

//! The parts of the main wing the optimizer is not allowed to move.
//!
//! The design vector owns span, area, sweep, the chords and the section
//! morphing factors. What is left here is everything that decides which
//! *family* of wing those numbers describe: where along the fuselage the root
//! sits, how the defining sections are stacked vertically, how they are
//! twisted, and where the planform cranks. Two runs with different values here
//! are not searching the same design space, so these are fixed for the length
//! of a run and configurable between runs -- which is the whole reason they
//! are named fields rather than the constants the original scripts buried
//! inside their geometry builders.

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
    /// The exposed trailing edge runs forward from the body to the kink, or
    /// from the kink to the tip. That makes the included angle with the
    /// fuselage exceed 90 degrees and produces a reflex planform corner.
    #[error(
        "main-wing trailing edge must not run forward outboard; {inboard:?} trailing edge is at {inboard_x_m} m and {outboard:?} is at {outboard_x_m} m"
    )]
    ForwardSweptTrailingEdge {
        /// Inboard station at the invalid panel.
        inboard: MainWingStationKind,
        /// Outboard station at the invalid panel.
        outboard: MainWingStationKind,
        /// Inboard trailing-edge X coordinate, in metres.
        inboard_x_m: f64,
        /// Outboard trailing-edge X coordinate, in metres.
        outboard_x_m: f64,
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

    /// How finely each wing section is panelled for the vortex lattice.
    #[config(
        label = "Wing VLM panel count",
        help = "Spanwise panel refinement per wing section for the vortex-lattice solver. Higher = more accurate, slower."
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
            n_subdivisions: 8,
        }
    }
}

impl WingConfig {
    /// Resolve the side-of-body/root/kink/tip planform for `design`.
    ///
    /// Existing configurations leave all optional transport fields unset and
    /// therefore produce the historical root/kink/tip geometry exactly. New
    /// configurations can make the side-of-body and outboard sweep explicit
    /// without consumers duplicating sweep or trailing-edge calculations.
    ///
    /// # Errors
    ///
    /// Returns [`TransportPlanformError`] when station order, dimensions,
    /// chords, or sweep angles cannot describe a finite physical wing.
    pub fn transport_planform(
        &self,
        design: &DesignVector,
    ) -> Result<TransportPlanform, TransportPlanformError> {
        validate_finite("span", design.span_m)?;
        validate_positive("span", design.span_m)?;
        validate_finite("root chord", design.root_chord_m)?;
        validate_positive("root chord", design.root_chord_m)?;
        validate_finite("kink chord", design.break_chord_m)?;
        validate_positive("kink chord", design.break_chord_m)?;
        validate_finite("tip chord", design.tip_chord_m)?;
        validate_positive("tip chord", design.tip_chord_m)?;
        validate_finite("inboard leading-edge sweep", design.sweep_deg)?;

        let semi_span_m = design.span_m / 2.0;
        let side_of_body_span_fraction = self.side_of_body_span_fraction.unwrap_or(0.0);
        let kink_span_fraction = self.kink_span_fraction.unwrap_or(self.break_span_fraction);
        let transport_extension_active =
            self.side_of_body_span_fraction.is_some() || self.kink_span_fraction.is_some();
        let outboard_le_sweep_deg = if transport_extension_active {
            design.sweep_deg
        } else {
            self.outboard_le_sweep_deg
                .unwrap_or(design.sweep_deg - self.outboard_sweep_decrement_deg)
        };

        validate_finite("side-of-body span fraction", side_of_body_span_fraction)?;
        validate_finite("kink span fraction", kink_span_fraction)?;
        if let Some(side_of_body_chord_ratio) = self.side_of_body_chord_ratio {
            validate_finite("side-of-body chord ratio", side_of_body_chord_ratio)?;
            validate_positive("side-of-body chord ratio", side_of_body_chord_ratio)?;
        }
        validate_finite("outboard leading-edge sweep", outboard_le_sweep_deg)?;
        validate_sweep("inboard leading-edge sweep", design.sweep_deg)?;
        validate_sweep("outboard leading-edge sweep", outboard_le_sweep_deg)?;
        if side_of_body_span_fraction < 0.0
            || side_of_body_span_fraction >= kink_span_fraction
            || kink_span_fraction >= 1.0
        {
            return Err(TransportPlanformError::InvalidStationOrder {
                side_of_body: side_of_body_span_fraction,
                kink: kink_span_fraction,
            });
        }
        let root = MainWingStation {
            kind: MainWingStationKind::Root,
            span_fraction: 0.0,
            y_m: 0.0,
            leading_edge_x_m: 0.0,
            chord_m: design.root_chord_m,
        };
        let kink_y_m = kink_span_fraction * semi_span_m;
        let kink = MainWingStation {
            kind: MainWingStationKind::Kink,
            span_fraction: kink_span_fraction,
            y_m: kink_y_m,
            leading_edge_x_m: kink_y_m * design.sweep_deg.to_radians().tan(),
            chord_m: design.break_chord_m,
        };
        let side_of_body_y_m = side_of_body_span_fraction * semi_span_m;
        let side_of_body = (side_of_body_span_fraction > 0.0).then(|| {
            let root_to_kink_fraction = side_of_body_span_fraction / kink_span_fraction;
            let interpolated_chord_m = design.root_chord_m
                + root_to_kink_fraction * (design.break_chord_m - design.root_chord_m);
            let leading_edge_x_m = side_of_body_y_m * design.sweep_deg.to_radians().tan();
            let kink_trailing_edge_x_m = kink.leading_edge_x_m + kink.chord_m;
            // The centreline root is a carry-through datum inside the body,
            // not an exposed aerodynamic chord. When no explicit body chord
            // is supplied, retain the linear panel unless that would make the
            // exposed body-to-kink trailing edge run forward; in that case
            // the side-of-body chord ends at the kink trailing-edge station.
            let derived_chord_m =
                interpolated_chord_m.min(kink_trailing_edge_x_m - leading_edge_x_m);
            MainWingStation {
                kind: MainWingStationKind::SideOfBody,
                span_fraction: side_of_body_span_fraction,
                y_m: side_of_body_y_m,
                leading_edge_x_m,
                chord_m: self
                    .side_of_body_chord_ratio
                    .map_or(derived_chord_m, |ratio| design.root_chord_m * ratio),
            }
        });
        let tip_y_m = semi_span_m;
        let tip = MainWingStation {
            kind: MainWingStationKind::Tip,
            span_fraction: 1.0,
            y_m: tip_y_m,
            leading_edge_x_m: kink.leading_edge_x_m
                + (tip_y_m - kink_y_m) * outboard_le_sweep_deg.to_radians().tan(),
            chord_m: design.tip_chord_m,
        };

        let planform = TransportPlanform {
            root,
            side_of_body,
            kink,
            tip,
            inboard_le_sweep_deg: design.sweep_deg,
            outboard_le_sweep_deg,
        };
        if let Some(side_of_body) = planform.side_of_body {
            validate_positive("side-of-body chord", side_of_body.chord_m)?;
        }
        validate_exposed_trailing_edge(&planform)?;
        Ok(planform)
    }

    /// Return the exposed inboard section for a 2-D aerodynamic calculation.
    ///
    /// # Errors
    ///
    /// Returns [`TransportPlanformError`] when the parent planform is not
    /// physically constructible.
    pub fn inboard_aerodynamic_station(
        &self,
        design: &DesignVector,
    ) -> Result<InboardAerodynamicStation, TransportPlanformError> {
        let planform = self.transport_planform(design)?;
        if let Some(side_of_body) = planform.side_of_body {
            let root_to_kink_fraction = side_of_body.y_m / planform.kink.y_m;
            return Ok(InboardAerodynamicStation {
                y_m: side_of_body.y_m,
                leading_edge_x_m: side_of_body.leading_edge_x_m,
                chord_m: side_of_body.chord_m,
                twist_deg: self.root_twist_deg
                    + root_to_kink_fraction * (self.break_twist_deg - self.root_twist_deg),
            });
        }
        Ok(InboardAerodynamicStation {
            y_m: planform.root.y_m,
            leading_edge_x_m: planform.root.leading_edge_x_m,
            chord_m: planform.root.chord_m,
            twist_deg: self.root_twist_deg,
        })
    }
}

fn validate_finite(field: &'static str, value: f64) -> Result<(), TransportPlanformError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(TransportPlanformError::NonFinite { field, value })
    }
}

fn validate_positive(field: &'static str, value: f64) -> Result<(), TransportPlanformError> {
    if value > 0.0 {
        Ok(())
    } else {
        Err(TransportPlanformError::NonPositive { field, value })
    }
}

fn validate_sweep(field: &'static str, value: f64) -> Result<(), TransportPlanformError> {
    if value.abs() < 89.0 {
        Ok(())
    } else {
        Err(TransportPlanformError::InvalidSweep { field, value })
    }
}

fn validate_exposed_trailing_edge(
    planform: &TransportPlanform,
) -> Result<(), TransportPlanformError> {
    // The centreline-to-side-of-body carry-through lies inside the fuselage.
    // The exposed wing begins at side-of-body when that station is present.
    // A missing side-of-body identifies the frozen legacy geometry contract,
    // whose exact root/kink/tip algebra must remain reproducible.
    let Some(exposed_root) = planform.side_of_body else {
        return Ok(());
    };
    for (inboard, outboard) in [(exposed_root, planform.kink), (planform.kink, planform.tip)] {
        let inboard_x_m = inboard.leading_edge_x_m + inboard.chord_m;
        let outboard_x_m = outboard.leading_edge_x_m + outboard.chord_m;
        if outboard_x_m + 1e-10 < inboard_x_m {
            return Err(TransportPlanformError::ForwardSweptTrailingEdge {
                inboard: inboard.kind,
                outboard: outboard.kind,
                inboard_x_m,
                outboard_x_m,
            });
        }
    }
    Ok(())
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entry, OptionSource};

    #[test]
    fn the_default_wing_washes_out_from_root_to_tip() {
        // Washout is what keeps the tip from stalling before the root, which
        // is what keeps the ailerons working through the stall.
        let wing = WingConfig::default();
        assert!(wing.root_twist_deg > wing.break_twist_deg);
    }

    #[test]
    fn the_default_wing_has_positive_dihedral() {
        let wing = WingConfig::default();
        assert!(wing.tip_z_m > wing.root_z_m);
    }

    #[test]
    fn the_break_sits_strictly_between_the_root_and_the_tip() {
        // At 0 or 1 the crank collapses onto a defining section and the
        // outboard sweep decrement has nothing to apply to.
        let wing = WingConfig::default();
        assert!(wing.break_span_fraction > 0.0);
        assert!(wing.break_span_fraction < 1.0);
    }

    #[test]
    fn the_product_defaults_derive_a_collinear_side_of_body_station() {
        let wing = WingConfig::default();
        let design = DesignVector::default();
        let planform = wing
            .transport_planform(&design)
            .expect("the default planform is physically valid");

        assert!(planform.side_of_body.is_some());
        assert_eq!(planform.kink.span_fraction, 0.37);
        assert_eq!(planform.stations().len(), 4);
        assert_eq!(planform.panels().len(), 3);
        assert_eq!(planform.inboard_le_sweep_deg, design.sweep_deg);
        assert_eq!(planform.outboard_le_sweep_deg, design.sweep_deg);
        let side_of_body = planform.side_of_body.expect("the station is active");
        let fraction = side_of_body.y_m / planform.kink.y_m;
        let expected_chord =
            planform.root.chord_m + fraction * (planform.kink.chord_m - planform.root.chord_m);
        assert!((side_of_body.chord_m - expected_chord).abs() < 1e-12);
    }

    #[test]
    fn legacy_saved_wing_configuration_uses_the_pre_transport_planform() {
        let mut value = serde_json::to_value(WingConfig::default()).unwrap();
        let object = value
            .as_object_mut()
            .expect("a wing configuration serializes as an object");
        for field in [
            "side_of_body_span_fraction",
            "side_of_body_chord_ratio",
            "kink_span_fraction",
            "outboard_le_sweep_deg",
        ] {
            object.remove(field);
        }
        let legacy: WingConfig = serde_json::from_value(value)
            .expect("new optional transport fields do not invalidate saved configurations");
        let design = DesignVector::default();
        let planform = legacy
            .transport_planform(&design)
            .expect("the legacy planform remains valid");

        assert!(planform.side_of_body.is_none());
        assert_eq!(planform.kink.span_fraction, legacy.break_span_fraction);
        assert_eq!(
            planform.outboard_le_sweep_deg,
            design.sweep_deg - legacy.outboard_sweep_decrement_deg
        );
    }

    #[test]
    fn explicit_transport_stations_cannot_create_a_second_leading_edge_sweep() {
        let wing = WingConfig {
            side_of_body_span_fraction: Some(0.10),
            side_of_body_chord_ratio: Some(0.90),
            kink_span_fraction: Some(0.40),
            outboard_le_sweep_deg: Some(28.0),
            ..WingConfig::default()
        };
        let design = DesignVector::default();

        let planform = wing
            .transport_planform(&design)
            .expect("the configured transport planform is valid");
        let panels = planform.panels();

        assert_eq!(planform.stations().len(), 4);
        assert_eq!(panels.len(), 3);
        assert_eq!(planform.kink.span_fraction, 0.40);
        assert_eq!(planform.kink.y_m, 0.40 * design.span_m / 2.0);
        assert!((panels[0].leading_edge_sweep_deg - design.sweep_deg).abs() < 1e-12);
        assert!((panels[2].leading_edge_sweep_deg - design.sweep_deg).abs() < 1e-12);
        assert!(panels[1].trailing_edge_sweep_deg < panels[1].leading_edge_sweep_deg);
        assert!(panels[2].trailing_edge_sweep_deg < panels[2].leading_edge_sweep_deg);
    }

    #[test]
    fn inboard_aerodynamic_station_uses_side_of_body_chord_and_interpolated_twist() {
        let wing = WingConfig::default();
        let design = DesignVector::default();
        let planform = wing
            .transport_planform(&design)
            .expect("the default transport planform is valid");
        let side_of_body = planform
            .side_of_body
            .expect("product default has a side-of-body station");
        let station = wing
            .inboard_aerodynamic_station(&design)
            .expect("the default inboard station is valid");
        let expected_twist_deg = wing.root_twist_deg
            + (side_of_body.y_m / planform.kink.y_m) * (wing.break_twist_deg - wing.root_twist_deg);

        assert_eq!(station.y_m, side_of_body.y_m);
        assert_eq!(station.chord_m, side_of_body.chord_m);
        assert!((station.twist_deg - expected_twist_deg).abs() < 1e-12);
    }

    #[test]
    fn legacy_inboard_aerodynamic_station_remains_at_the_centerline_root() {
        let wing = WingConfig {
            side_of_body_span_fraction: None,
            side_of_body_chord_ratio: None,
            ..WingConfig::default()
        };
        let design = DesignVector::default();
        let station = wing
            .inboard_aerodynamic_station(&design)
            .expect("the legacy inboard station is valid");

        assert_eq!(station.y_m, 0.0);
        assert_eq!(station.chord_m, design.root_chord_m);
        assert_eq!(station.twist_deg, wing.root_twist_deg);
    }

    #[test]
    fn unconventional_reverse_taper_is_not_mislabeled_as_nonphysical() {
        let wing = WingConfig::default();
        let design = DesignVector {
            break_chord_m: 17.0,
            sweep_deg: 40.0,
            ..DesignVector::default()
        };

        assert!(wing.transport_planform(&design).is_ok());
    }

    #[test]
    fn an_exposed_trailing_edge_over_ninety_degrees_to_the_fuselage_is_rejected() {
        let wing = WingConfig {
            side_of_body_chord_ratio: Some(1.0),
            ..WingConfig::default()
        };

        assert!(matches!(
            wing.transport_planform(&DesignVector::default()),
            Err(TransportPlanformError::ForwardSweptTrailingEdge {
                inboard: MainWingStationKind::SideOfBody,
                outboard: MainWingStationKind::Kink,
                ..
            })
        ));
    }

    #[test]
    fn a_planform_query_outboard_of_the_tip_is_a_typed_error() {
        let planform = WingConfig::default()
            .transport_planform(&DesignVector::default())
            .expect("the default planform is valid");

        assert!(matches!(
            planform.leading_edge_x_at(planform.tip.y_m + 0.01),
            Err(TransportPlanformError::SpanwisePositionOutsidePlanform { .. })
        ));
    }

    #[test]
    fn both_section_fields_offer_the_airfoil_library_and_still_accept_a_naca_code() {
        // The geometry layer resolves any NACA 4-digit code without the
        // library carrying it, so a strict list would reject valid input.
        for name in ["root_airfoil", "tip_airfoil"] {
            let schema = WingConfig::default().schema();
            let Entry::Leaf(leaf) = &schema.field(name).unwrap().entry else {
                panic!("{name} is not a group");
            };
            assert_eq!(leaf.options, Some(OptionSource::Airfoil));
        }
        assert!(OptionSource::Airfoil.editable());
    }
}
