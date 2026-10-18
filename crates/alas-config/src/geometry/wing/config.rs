// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! [`WingConfig`]: the main-wing scaffold the optimizer does not move, and its validation.

use serde::{Deserialize, Serialize};

use super::airfoil_class::AirfoilClass;
use super::planform::WingSection;
use super::profile::{WingDihedralStation, WingThicknessStation};
use super::reference::ReferenceWingConfig;
use super::smooth::WingProfileInterpolation;
use crate::ConfigNode;

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

    /// Static-to-1 g rise of the wing tip over the semispan; the heights
    /// above are the static ground shape (see [`super::WingShape`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        label = "Wing tip rise in flight",
        unit = "0-1 of semispan",
        help = "How far the wing tip rises from the static ground shape (the heights above, as the ground-clearance tables measure them at maximum ramp weight) to the 1 g flight shape, as a fraction of the semispan. The aerodynamics and the dihedral checks use the flight shape; ground clearance uses the heights above. Leave unset when one shape serves both."
    )]
    pub flight_tip_rise_semispan_fraction: Option<f64>,

    /// Static-to-1 g rise of the centreline root height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        hidden,
        unit = "m",
        help = "Optional rise of the wing-root height from the static ground shape to the 1 g flight shape, metres; the flight height polyline then starts at the raised root. Unset keeps the root where the ground shape has it."
    )]
    pub flight_root_rise_m: Option<f64>,

    /// Nacelles keep their static height in the flight shape.
    #[serde(default, skip_serializing_if = "is_false")]
    #[config(
        hidden,
        help = "When set, the nacelles hang from the static ground-shape wing in the flight shape too, as the manufacturer front views and clearance tables draw them, instead of rising with the flight rise of the wing."
    )]
    pub nacelles_hold_ground_height: bool,

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
    ///
    /// Read only when neither a side-of-body nor a kink station is declared;
    /// a transport planform keeps one leading-edge sweep root to tip (see
    /// [`WingConfig::transport_planform`]).
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
    /// Leading-edge sweep of the inboard panel (centreline to kink) when it differs from the outboard one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        hidden,
        unit = "deg",
        help = "Optional leading-edge sweep of the inboard panel (centreline to kink), degrees. When unset the design-vector sweep runs straight from the centreline to the tip. Read only with a side-of-body or kink station."
    )]
    pub inboard_le_sweep_deg: Option<f64>,

    /// Chord at the extreme tip of a swept-back tip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        hidden,
        unit = "m",
        help = "Optional chord at the extreme wing tip of a swept-back tip, metres. The design-vector tip chord is then the chord the straight leading and trailing edges would reach at the tip; from the rake start the leading edge sweeps further aft while the trailing edge stays one straight line, so the tip chord shrinks to this value."
    )]
    pub tip_rake_tip_chord_m: Option<f64>,

    /// Where the swept-back tip rake starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        hidden,
        help = "Optional span fraction where a swept-back tip begins (used with the rake tip chord)."
    )]
    pub tip_rake_start_span_fraction: Option<f64>,

    /// Which wing the reference area, span, MAC and MAC leading edge describe.
    #[serde(default, skip_serializing_if = "ReferenceWingConfig::is_default")]
    #[config(
        hidden,
        help = "Reference-wing convention: Drawn (default) quotes S_ref, b_ref, MAC and the MAC leading edge on the drawn planform; Trapezoid quotes them on the manufacturer reference trapezoid made by extending the outboard panel leading and trailing edges to the centreline. Wetted area and the lattice panels always use the drawn planform. An optional published reference area is checked against the trapezoid (warning above 1 %)."
    )]
    pub reference_wing: ReferenceWingConfig,

    /// Optional wingtip device (winglet, sharklet or fence); see [`super::WingtipDevice`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        hidden,
        help = "Optional wingtip device (winglet, sharklet or tip fence) added as an extra lifting surface at the wing tip."
    )]
    pub wingtip_device: Option<super::wingtip::WingtipDevice>,

    /// Interior spanwise dihedral stations (ground-shape leading-edge heights).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[config(
        hidden,
        help = "Optional interior stations that bend the root-break-tip height polyline: dihedral that varies along the span (see WingDihedralStation)."
    )]
    pub dihedral_stations: Vec<WingDihedralStation>,

    /// Spanwise thickness multipliers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[config(
        hidden,
        help = "Optional stations scaling the section thickness along the span so a thick root falls to a thin outer wing (see WingThicknessStation)."
    )]
    pub thickness_stations: Vec<WingThicknessStation>,

    /// How the dihedral, twist and thickness stations are interpolated.
    #[serde(default, skip_serializing_if = "WingProfileInterpolation::is_linear")]
    #[config(
        hidden,
        help = "Interpolation of the spanwise dihedral, twist and thickness profiles: piecewise linear, or a C2 cubic spline through every station."
    )]
    pub wing_profile_interpolation: WingProfileInterpolation,

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

    /// Declared section technology of the main wing, which fixes the Korn
    /// technology factor (see [`AirfoilClass`]). Declared with the aircraft
    /// (each preset states it with its source); a clean-sheet wing is
    /// supercritical. Not a form field: it is a property of the aircraft
    /// being described, not a coefficient to adjust. Always serialized: a
    /// preset reapplies its own class on load, so an omitted value would
    /// silently become the preset's. A file saved before the class existed
    /// loads the preset's (or the clean-sheet supercritical) class.
    #[serde(default)]
    #[config(
        hidden,
        help = "Declared section technology of the main wing (conventional or supercritical), which fixes the Korn technology factor: 0.87 conventional, 0.95 supercritical (Mason, Configuration Aerodynamics, ch. 7; Malone and Mason 1995)."
    )]
    pub airfoil_class: AirfoilClass,

    /// Optional continuous Korn technology factor that replaces the two-valued
    /// [`AirfoilClass`] mapping (DRG-04). `None` keeps the class value (0.87
    /// conventional, 0.95 supercritical). Legal range 0.80-1.00: below the
    /// NACA 6-series value no transport section exists, and 1.00 is above
    /// every published supercritical section (NASA SC(2): 0.95). A value
    /// needs a documented source (section data, Mason ch. 7 chart of kappa
    /// against section family); it is not a free tuning knob. Not a form
    /// field and not serialized while unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        hidden,
        help = "Optional Korn technology factor overriding the section class value (0.87 conventional, 0.95 supercritical); valid 0.80-1.00. Needs a documented section-technology source."
    )]
    pub korn_technology_factor_override: Option<f64>,

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

fn is_false(value: &bool) -> bool {
    !*value
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
            flight_tip_rise_semispan_fraction: None,
            flight_root_rise_m: None,
            nacelles_hold_ground_height: false,
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
            inboard_le_sweep_deg: None,
            tip_rake_tip_chord_m: None,
            tip_rake_start_span_fraction: None,
            reference_wing: ReferenceWingConfig::default(),
            wingtip_device: None,
            dihedral_stations: Vec::new(),
            thickness_stations: Vec::new(),
            wing_profile_interpolation: WingProfileInterpolation::Linear,
            root_airfoil: "SC2-0714".to_owned(),
            tip_airfoil: "naca2410".to_owned(),
            airfoil_class: AirfoilClass::Supercritical,
            korn_technology_factor_override: None,
            custom_sections: Vec::new(),
            n_subdivisions: 24,
        }
    }
}
