// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/geometry_config.py (`WingConfig`)
// Reference: alas @ rust-port-baseline.

//! The parts of the main wing the optimizer is not allowed to move.
//!
//! The design vector owns span, area, sweep, the chords and the section
//! morphing factors. What is left here is everything that decides which
//! *family* of wing those numbers describe: where along the fuselage the root
//! sits, how the three defining sections are stacked vertically, how they are
//! twisted, and where the planform cranks. Two runs with different values here
//! are not searching the same design space, so these are fixed for the length
//! of a run and configurable between runs -- which is the whole reason they
//! are named fields rather than the constants the original scripts buried
//! inside their geometry builders.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

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

    /// How much less swept the outboard panel is than the inboard one.
    #[config(
        label = "Outboard sweep reduction",
        unit = "deg",
        help = "How many degrees less swept the outboard panel is than the inboard panel (a common yehudi/crank shape)."
    )]
    pub outboard_sweep_decrement_deg: f64,

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
            outboard_sweep_decrement_deg: 2.0,
            root_airfoil: "SC2-0714".to_owned(),
            tip_airfoil: "naca2410".to_owned(),
            n_subdivisions: 8,
        }
    }
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
