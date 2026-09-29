// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! [`WingConfig`]: the main-wing scaffold the optimizer does not move, and its validation.

use serde::{Deserialize, Serialize};

use super::planform::{
    InboardAerodynamicStation, MainWingStation, MainWingStationKind, TransportPlanform,
    TransportPlanformError, WingSection, WingSectionError,
};
use crate::{ConfigNode, DesignVector};

/// Main-wing scaffold not covered by the design vector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct WingConfig {
    /// How far aft of the nose the wing root sits.
    #[config(
        label = "Wing root X position",
        unit = "m",
        help = "Fuselage-station X of the wing-root leading-edge datum: how far aft of the nose the wing sits."
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

    /// Optional user-defined stations lofted between the defining planform
    /// edges. Preset geometry remains locked by the application while this
    /// list is available for a clean-sheet/custom geometry configuration.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[config(
        hidden,
        help = "Additional validated wing sections supplied by the user; edited by the custom-geometry editor rather than the generated scalar form."
    )]
    pub custom_sections: Vec<WingSection>,

    /// How many spanwise panels the whole wing semispan is meshed into.
    #[config(
        label = "Wing VLM panel count",
        help = "Spanwise panels across the whole wing semispan for the vortex-lattice solver. This is an absolute count, not a count per section: a planform with a side-of-body station and a kink gets the same mesh density as one without, and adding a station no longer changes the panel count underneath a search. Every planform station (root, side-of-body, kink, tip) is always kept as a panel edge whatever the count, so refining the mesh never averages a kink away. The default of 24 is converged: a twelve-fold refinement moves the trimmed cruise attitude by 0.01 deg."
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
            custom_sections: Vec::new(),
            n_subdivisions: 24,
        }
    }
}

impl WingConfig {
    /// Validate user-defined stations without resolving their airfoils.
    ///
    /// The builder performs the planform-dependent taper check after it has a
    /// design vector. Keeping the structural checks here also lets the GUI
    /// reject malformed saved values before a build is attempted.
    pub fn validate_custom_sections(&self) -> Result<(), WingSectionError> {
        let mut previous = None;
        for (index, section) in self.custom_sections.iter().enumerate() {
            for (field, value) in [
                ("span_fraction", section.span_fraction),
                ("leading_edge_x_m", section.leading_edge_x_m),
                ("chord_m", section.chord_m),
                ("z_m", section.z_m),
                ("twist_deg", section.twist_deg),
            ] {
                if !value.is_finite() {
                    return Err(WingSectionError::NonFinite {
                        index,
                        field,
                        value,
                    });
                }
            }
            if !(0.0..1.0).contains(&section.span_fraction) {
                return Err(WingSectionError::SpanOutOfRange {
                    index,
                    value: section.span_fraction,
                });
            }
            if section.chord_m <= 0.0 {
                return Err(WingSectionError::NonPositiveChord {
                    index,
                    value: section.chord_m,
                });
            }
            if section.airfoil.trim().is_empty() {
                return Err(WingSectionError::EmptyAirfoil(index));
            }
            if let Some(previous) = previous {
                if section.span_fraction <= previous {
                    return Err(WingSectionError::InvalidOrder {
                        index,
                        previous,
                        current: section.span_fraction,
                    });
                }
            }
            previous = Some(section.span_fraction);
        }
        Ok(())
    }

    /// Validate custom chords against the defining planform stations.
    pub fn validate_custom_sections_against_planform(
        &self,
        planform: &TransportPlanform,
    ) -> Result<(), WingSectionError> {
        self.validate_custom_sections()?;
        let planform_stations = planform.stations();
        let mut stations: Vec<(f64, f64)> = planform_stations
            .iter()
            .filter(|station| {
                station.kind != MainWingStationKind::SideOfBody
                    || self.side_of_body_chord_ratio.is_some()
            })
            .map(|station| (station.span_fraction, station.chord_m))
            .collect();
        stations.extend(
            self.custom_sections
                .iter()
                .map(|section| (section.span_fraction, section.chord_m)),
        );
        stations.sort_by(|left, right| left.0.total_cmp(&right.0));
        for pair in stations.windows(2) {
            if (pair[1].0 - pair[0].0).abs() <= 1.0e-9 {
                if let Some(station) = planform_stations.iter().find(|station| {
                    (station.span_fraction - pair[1].0).abs() <= 1.0e-9
                        && station.kind != MainWingStationKind::Root
                        && station.kind != MainWingStationKind::Tip
                        && (station.kind != MainWingStationKind::SideOfBody
                            || self.side_of_body_chord_ratio.is_some())
                }) {
                    return Err(WingSectionError::DuplicatePlanformStation {
                        span_fraction: pair[1].0,
                        kind: station.kind,
                    });
                }
            }
            if pair[1].1 > pair[0].1 + 1.0e-9 {
                return Err(WingSectionError::NonMonotoneChord {
                    inboard: pair[0].1,
                    outboard: pair[1].1,
                });
            }
        }
        Ok(())
    }

    /// Return custom stations in validated centerline-to-tip order.
    pub fn custom_sections_sorted(&self) -> Result<Vec<WingSection>, WingSectionError> {
        self.validate_custom_sections()?;
        let mut sections = self.custom_sections.clone();
        sections.sort_by(|left, right| left.span_fraction.total_cmp(&right.span_fraction));
        Ok(sections)
    }

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
            // not an exposed aerodynamic chord. Preserve the trailing-edge
            // clip when it stays inside the root-to-kink chord envelope. If
            // that clip would make the exposed side-of-body section narrower
            // than the kink, keep the kink chord instead: an inboard section
            // must not grow outboard into the kink.
            let derived_chord_m = interpolated_chord_m
                .min(kink_trailing_edge_x_m - leading_edge_x_m)
                .max(kink.chord_m);
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
        let stations = planform.stations();
        for pair in stations.windows(2) {
            if pair[1].chord_m > pair[0].chord_m + 1.0e-9 {
                return Err(TransportPlanformError::NonMonotoneChord {
                    inboard: pair[0].chord_m,
                    outboard: pair[1].chord_m,
                });
            }
        }
        if let Some(side_of_body) = planform.side_of_body {
            validate_positive("side-of-body chord", side_of_body.chord_m)?;
        }
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
