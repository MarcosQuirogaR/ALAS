// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/geometry_config.py (`FuselageConfig`)
// Reference: alas @ rust-port-baseline.

//! The fuselage, as the stations its surface is lofted through.
//!
//! Total length is a design variable; what is here is how that length is
//! divided and how the body is shaped along it, where the nose taper ends,
//! how long the tailcone is, and how far the nose and tail tips sit off the
//! centerline. Those three vertical offsets are what give the body its droop
//! and its tail upsweep, which is ground clearance at rotation rather than
//! decoration.
//!
//! Diameter is the one field here that a second discipline reads: the cabin
//! layout takes its usable width from it, so a change made for drag reasons
//! shows up as a seat abreast gained or lost.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Fuselage body-of-revolution or ovoid stations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct FuselageConfig {
    /// The widest cross-section, which sets cabin width and wetted area.
    #[config(
        label = "Fuselage diameter (width)",
        unit = "m",
        help = "Maximum fuselage cross-sectional diameter: the main driver of cabin width and wetted area. Best judged on the 3-view preview's front/isometric panels, not the top view."
    )]
    pub diameter_m: f64,

    /// The vertical dimension, or unset for a circular section.
    #[config(
        label = "Fuselage height (if non-circular)",
        unit = "m",
        help = "Vertical cross-section dimension. Leave blank/none for a circular fuselage where height equals diameter."
    )]
    pub height_m: Option<f64>,

    /// How far the nose tip droops below the centerline.
    #[config(
        label = "Nose vertical offset",
        unit = "m",
        help = "Vertical offset of the nose tip relative to the fuselage centerline."
    )]
    pub nose_z_m: f64,

    /// Where the nose taper ends and the constant section begins.
    #[config(
        label = "Cabin start X position",
        unit = "m",
        help = "X-station where the fuselage first reaches full diameter, i.e. the end of the nose taper."
    )]
    pub cabin_start_x_m: f64,

    /// Where the constant section sits vertically.
    #[config(
        label = "Cabin vertical offset",
        unit = "m",
        help = "Vertical offset of the cylindrical cabin section relative to the fuselage centerline."
    )]
    pub cabin_z_m: f64,

    /// How much of the length is aft taper.
    #[config(
        label = "Tailcone length",
        unit = "m",
        help = "Length of the aft taper, from the end of the cylindrical cabin section to the tail tip."
    )]
    pub tailcone_length_m: f64,

    /// How far the tail tip rises above the centerline.
    #[config(
        label = "Tail vertical offset",
        unit = "m",
        help = "Vertical offset of the upswept tail tip (positive = tail rises above centerline, typical for ground clearance/rotation)."
    )]
    pub tail_z_m: f64,

    /// How many stations the body is lofted through.
    #[config(
        label = "Fuselage cross-section count",
        help = "Number of longitudinal stations used to loft the fuselage body. Higher = smoother surface, slower to draw."
    )]
    pub n_subdivisions: i64,
}

impl Default for FuselageConfig {
    fn default() -> Self {
        // A circular twin-aisle body: a six-metre nose taper, a long
        // constant-section cabin, and a fourteen-metre upswept tailcone.
        Self {
            diameter_m: 6.2,
            height_m: None,
            nose_z_m: -0.5,
            cabin_start_x_m: 6.0,
            cabin_z_m: 0.2,
            tailcone_length_m: 14.0,
            tail_z_m: 1.8,
            n_subdivisions: 12,
        }
    }
}

impl FuselageConfig {
    /// The vertical cross-section dimension, falling back to the diameter.
    ///
    /// An unset height means a circular section, which is the shape the
    /// default describes and the one every consumer has to be able to assume.
    pub fn effective_height_m(&self) -> f64 {
        self.height_m.unwrap_or(self.diameter_m)
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
    fn an_unset_height_means_a_circular_section_rather_than_a_flat_one() {
        let fuselage = FuselageConfig::default();
        assert_eq!(fuselage.effective_height_m(), fuselage.diameter_m);
    }

    #[test]
    fn a_declared_height_is_what_an_ovoid_section_is_measured_by() {
        let fuselage = FuselageConfig {
            height_m: Some(7.1),
            ..Default::default()
        };
        assert_eq!(fuselage.effective_height_m(), 7.1);
    }

    #[test]
    fn an_unset_height_reaches_the_form_blank_rather_than_as_zero() {
        // A height box showing 0 would describe a fuselage of no depth, which
        // is a different aircraft from the circular one meant.
        let schema = FuselageConfig::default().schema();
        let Entry::Leaf(leaf) = &schema.field("height_m").unwrap().entry else {
            panic!("the height is not a group");
        };
        assert_eq!(leaf.kind, Kind::Optional);
        assert_eq!(leaf.value, serde_json::Value::Null);
    }

    #[test]
    fn the_nose_droops_and_the_tail_rises() {
        // Tail upsweep is the rotation clearance; a tail below the centerline
        // would strike the runway.
        let fuselage = FuselageConfig::default();
        assert!(fuselage.nose_z_m < 0.0);
        assert!(fuselage.tail_z_m > 0.0);
    }

    #[test]
    fn the_nose_and_the_tailcone_leave_a_cabin_between_them() {
        // The two tapers are measured from opposite ends of a length the
        // design vector owns, so together they have to be shorter than the
        // shortest fuselage the search will accept without penalizing it.
        let fuselage = FuselageConfig::default();
        let tapers = fuselage.cabin_start_x_m + fuselage.tailcone_length_m;
        let floor = crate::ObjectiveWeights::default().fuselage_floor_m;
        assert!(
            tapers < floor,
            "{tapers} m of taper in a {floor} m fuselage"
        );
    }
}
