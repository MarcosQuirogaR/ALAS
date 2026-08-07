// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/geometry_config.py (`EmpennageConfig`)
// Reference: alas @ rust-port-baseline.

//! The tail surfaces, at unit scale.
//!
//! Every in-plane dimension here is multiplied by the design vector's tail
//! scale when the aircraft is built, so what this struct holds is the shape of
//! the empennage and not its size: the chords and tip positions describe a
//! reference tail that the optimizer then grows or shrinks as one piece. The
//! two offsets from the tail tip are the exception -- they place the surfaces
//! along the fuselage, which fixes the moment arm every static-stability
//! result depends on.
//!
//! Both surfaces share one airfoil, and it is symmetric: a stabilizer with
//! camber carries load at zero incidence, which a trimming surface must not.
//!
//! Several fields here declared a label and a unit upstream but no
//! explanation. This port supplies one for each, as CONTRIBUTING.md requires;
//! that adds prose and changes no value, and the parity test compares `help`
//! only where the dataclass declared one.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Horizontal and vertical stabilizer scaffold, at unit tail scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct EmpennageConfig {
    /// The section both surfaces are built from.
    #[config(
        options = Airfoil,
        label = "Tail airfoil section",
        help = "Reference airfoil shared by both the horizontal and vertical stabilisers (usually symmetric, e.g. NACA 00xx)."
    )]
    pub tail_airfoil: String,

    /// How finely each tail surface is panelled for the vortex lattice.
    #[config(
        label = "Tail VLM panel count",
        help = "Spanwise panel refinement per tail surface for the vortex-lattice solver."
    )]
    pub n_subdivisions: i64,

    /// Where the horizontal tail sits along the fuselage.
    #[config(
        label = "H-stab offset forward of tail tip",
        unit = "m",
        help = "How far forward of the fuselage tail tip the horizontal-stabiliser root leading edge sits."
    )]
    pub hstab_offset_from_tail_m: f64,

    /// Where the horizontal tail sits vertically.
    #[config(
        label = "H-stab vertical offset",
        unit = "m",
        help = "Vertical placement of the horizontal stabiliser relative to the fuselage centerline."
    )]
    pub hstab_z_m: f64,

    /// Chord where the horizontal tail meets the fuselage.
    #[config(
        label = "H-stab root chord",
        unit = "m",
        help = "Chord of the horizontal-stabiliser root section, before the design vector's tail scale is applied."
    )]
    pub hstab_root_chord_m: f64,

    /// Chord at the horizontal tail tip.
    #[config(
        label = "H-stab tip chord",
        unit = "m",
        help = "Chord of the horizontal-stabiliser tip section, before the design vector's tail scale is applied. Shorter than the root chord gives a tapered surface."
    )]
    pub hstab_tip_chord_m: f64,

    /// Incidence at the horizontal tail root.
    #[config(
        label = "H-stab root twist",
        unit = "deg",
        help = "Incidence of the horizontal stabiliser root -- usually slightly negative to trim the wing's nose-down pitching moment."
    )]
    pub hstab_root_twist_deg: f64,

    /// Incidence at the horizontal tail tip.
    #[config(
        label = "H-stab tip twist",
        unit = "deg",
        help = "Incidence of the horizontal stabiliser tip. Equal to the root incidence gives an untwisted surface, which is what a trimming tail normally has."
    )]
    pub hstab_tip_twist_deg: f64,

    /// Where the horizontal tip sits relative to its root.
    #[config(
        label = "H-stab tip leading edge (x, y, z)",
        unit = "m",
        help = "Position of the horizontal-stabiliser tip leading edge relative to its root, as (x, y, z)."
    )]
    pub hstab_tip_le_m: (f64, f64, f64),

    /// Where the fin sits along the fuselage.
    #[config(
        label = "V-stab offset forward of tail tip",
        unit = "m",
        help = "How far forward of the fuselage tail tip the vertical-stabiliser root leading edge sits."
    )]
    pub vstab_offset_from_tail_m: f64,

    /// Where the fin root sits vertically.
    #[config(
        label = "V-stab vertical offset",
        unit = "m",
        help = "Vertical placement of the vertical-stabiliser root relative to the fuselage centerline -- where the fin meets the top of the tailcone."
    )]
    pub vstab_z_m: f64,

    /// Chord where the fin meets the fuselage.
    #[config(
        label = "V-stab root chord",
        unit = "m",
        help = "Chord of the vertical-stabiliser root section, before the design vector's tail scale is applied."
    )]
    pub vstab_root_chord_m: f64,

    /// Chord at the fin tip.
    #[config(
        label = "V-stab tip chord",
        unit = "m",
        help = "Chord of the vertical-stabiliser tip section, before the design vector's tail scale is applied."
    )]
    pub vstab_tip_chord_m: f64,

    /// Where the fin tip sits relative to its root, which sets fin height.
    #[config(
        label = "V-stab tip leading edge (x, y, z)",
        unit = "m",
        help = "Position of the vertical-stabiliser tip leading edge relative to its root, as (x, y, z)."
    )]
    pub vstab_tip_le_m: (f64, f64, f64),
}

impl Default for EmpennageConfig {
    fn default() -> Self {
        // A conventional low-set tailplane and a single swept fin, sized and
        // placed as on a current twin-aisle transport.
        Self {
            tail_airfoil: "naca0012".to_owned(),
            n_subdivisions: 6,
            hstab_offset_from_tail_m: 10.7,
            hstab_z_m: 1.2,
            hstab_root_chord_m: 8.0,
            hstab_tip_chord_m: 2.2,
            hstab_root_twist_deg: -2.0,
            hstab_tip_twist_deg: -2.0,
            hstab_tip_le_m: (7.5, 11.0, 1.0),
            vstab_offset_from_tail_m: 12.2,
            vstab_z_m: 2.0,
            vstab_root_chord_m: 9.5,
            vstab_tip_chord_m: 3.2,
            vstab_tip_le_m: (9.0, 0.0, 9.8),
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entry, Kind};

    #[test]
    fn the_tailplane_is_set_to_push_down_rather_than_lift() {
        // A conventional tail carries download in cruise to balance the
        // wing's nose-down moment, which is what negative incidence produces.
        let tail = EmpennageConfig::default();
        assert!(tail.hstab_root_twist_deg < 0.0);
        assert!(tail.hstab_tip_twist_deg < 0.0);
    }

    #[test]
    fn both_surfaces_taper_toward_their_tips() {
        let tail = EmpennageConfig::default();
        assert!(tail.hstab_tip_chord_m < tail.hstab_root_chord_m);
        assert!(tail.vstab_tip_chord_m < tail.vstab_root_chord_m);
    }

    #[test]
    fn the_fin_grows_in_z_and_the_tailplane_in_y() {
        // The two surfaces are the same construction rotated a quarter turn,
        // and swapping the axes is the mistake that construction invites.
        let tail = EmpennageConfig::default();
        let (_, hstab_y, _) = tail.hstab_tip_le_m;
        let (_, vstab_y, vstab_z) = tail.vstab_tip_le_m;
        assert!(hstab_y > 0.0);
        assert_eq!(vstab_y, 0.0);
        assert!(vstab_z > 0.0);
    }

    #[test]
    fn the_fin_is_mounted_ahead_of_the_tailplane() {
        // Both offsets are measured forward from the tail tip, so the larger
        // one is the surface further forward.
        let tail = EmpennageConfig::default();
        assert!(tail.vstab_offset_from_tail_m > tail.hstab_offset_from_tail_m);
    }

    #[test]
    fn a_tip_position_reaches_the_form_as_one_row_of_three_numbers() {
        let schema = EmpennageConfig::default().schema();
        let Entry::Leaf(leaf) = &schema.field("hstab_tip_le_m").unwrap().entry else {
            panic!("the tip position is not a group");
        };
        assert_eq!(leaf.kind, Kind::NumberList);
        assert_eq!(leaf.value, serde_json::json!([7.5, 11.0, 1.0]));
    }
}
