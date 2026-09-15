// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/geometry_config.py
// Reference: alas @ rust-port-baseline.

//! The scaffold the design vector hangs on.
//!
//! [`crate::design_variables`] holds the degrees of freedom the optimizer may
//! vary. This module holds everything else the aircraft is built from:
//! vertical placements, section twists, the empennage layout, the fuselage
//! stations, the nacelle silhouette, which is fixed for the length of a run
//! and configurable between runs. The split is what makes a run reproducible:
//! the search moves inside a family of aircraft, and this is the definition of
//! the family.
//!
//! Every value here was a literal buried inside the original scripts'
//! geometry builders. Naming them is most of what this module is for.
//!
//! Lengths are metres and angles are degrees unless a field says otherwise.

mod empennage;
mod engine;
mod fuselage;
mod wing;

pub use empennage::EmpennageConfig;
pub use engine::{ActiveEngineModel, EngineBindingError, EngineConfig};
pub use fuselage::FuselageConfig;
pub use wing::{
    InboardAerodynamicStation, MainWingPanel, MainWingStation, MainWingStationKind,
    TransportPlanform, TransportPlanformError, WingConfig,
};

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// The composed geometry scaffold for the whole aircraft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct GeometryConfig {
    /// The main wing.
    #[config(
        nested,
        help = "Main-wing scaffold parameters not already covered by the optimizer's design vector."
    )]
    pub wing: WingConfig,

    /// The tail surfaces.
    #[config(nested, help = "Horizontal and vertical stabiliser scaffold.")]
    pub empennage: EmpennageConfig,

    /// The body.
    #[config(
        nested,
        help = "Fuselage body-of-revolution stations, incl. diameter (width) and length breakdown."
    )]
    pub fuselage: FuselageConfig,

    /// The engine, which this form does not show.
    ///
    /// Hidden because it has a dedicated editor. Two forms writing the same
    /// fields is how the two of them come to disagree; see
    /// [`EngineConfig`] for why the cycle data lives in the design at all
    /// rather than being looked up.
    #[config(
        hidden,
        nested,
        help = "Podded engine / nacelle placement, shape, and design parameters: edited on the dedicated 'Engine Designer' Advanced Settings tab, not here."
    )]
    pub engine: EngineConfig,

    /// Wetted area of a wing, as a multiple of its planform area.
    #[config(
        label = "Wing wetted-area factor",
        help = "Ratio of wetted (exposed, both sides) surface area to planform area for a thin wing. Used by the drag buildup."
    )]
    pub wing_wetted_area_factor: f64,

    /// How much less wetted area a tapered body has than a cylinder.
    #[config(
        label = "Fuselage wetted-area factor",
        help = "Correction factor on pi*diameter*length for a non-cylindrical (tapered nose/tail) fuselage body."
    )]
    pub fuselage_wetted_factor: f64,
}

impl Default for GeometryConfig {
    fn default() -> Self {
        Self {
            wing: WingConfig::default(),
            empennage: EmpennageConfig::default(),
            fuselage: FuselageConfig::default(),
            engine: EngineConfig::default(),
            // Both are geometric reference data the parasite-drag buildup
            // reads; they live here rather than with the drag model because
            // they describe shapes, not a flow model.
            wing_wetted_area_factor: 2.05,
            fuselage_wetted_factor: 0.9,
        }
    }
}

impl GeometryConfig {
    /// Restore the spanwise subdivision the frozen Python builder used.
    ///
    /// `n_subdivisions` changed meaning, not just value: the reference reads
    /// it as a per-section multiplier, the product as an absolute panel count
    /// across the whole surface (`alas_geom::aircraft::spanwise`). Eight
    /// sections-worth and twenty-four panels are the same mesh on a
    /// three-section planform and a different one everywhere else, so a
    /// reference replay has to restore the ratio as well as the rule.
    ///
    /// Literals rather than a second `Default`, so that moving the product
    /// count cannot drag the frozen fixtures along with it.
    pub fn restore_reference_spanwise_mesh(&mut self) {
        self.wing.n_subdivisions = 8;
        self.empennage.n_subdivisions = 6;
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_engine_is_configuration_but_this_form_does_not_show_it() {
        // It has its own editor, and a second form writing the same fields is
        // how the two come to disagree. Hidden from the schema, and still
        // saved and loaded with everything else.
        let schema = GeometryConfig::default().schema();
        assert!(schema.field("engine").is_none());

        let saved = serde_json::to_value(GeometryConfig::default()).unwrap();
        assert!(saved.get("engine").is_some());
    }

    #[test]
    fn the_form_shows_the_three_airframe_groups_then_the_drag_reference_data() {
        let schema = GeometryConfig::default().schema();
        let names: Vec<&str> = schema.fields.iter().map(|field| field.name).collect();
        assert_eq!(
            names,
            vec![
                "wing",
                "empennage",
                "fuselage",
                "wing_wetted_area_factor",
                "fuselage_wetted_factor",
            ]
        );
    }

    #[test]
    fn a_wing_is_wetted_on_both_sides_and_a_tapered_body_less_than_a_cylinder() {
        let geometry = GeometryConfig::default();
        assert!(geometry.wing_wetted_area_factor > 2.0);
        assert!(geometry.fuselage_wetted_factor < 1.0);
    }

    #[test]
    fn a_geometry_round_trips_through_serialization() {
        let geometry = GeometryConfig::default();
        let text = serde_json::to_string(&geometry).unwrap();
        assert_eq!(
            serde_json::from_str::<GeometryConfig>(&text).unwrap(),
            geometry
        );
    }
}
