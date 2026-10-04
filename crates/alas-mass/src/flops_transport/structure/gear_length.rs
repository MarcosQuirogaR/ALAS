// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The extended landing-gear leg lengths `XMLG` and `XNLG` the FLOPS gear
//! equations 63-64 read.
//!
//! FLOPS equation 66 estimates the main leg of a gear carried under the wing:
//! from the nacelle diameter, the dihedral and the outboard engine station
//! when engines hang from the wing, else from the fuselage length
//! (NASA/TM-2017-219627 Vol. I, eq. 66). Neither form describes a main gear
//! carried by the fuselage. On a high-wing aircraft with sponson gear the
//! nacelle form prices a leg long enough to keep a wing nacelle off the
//! ground, which the sponson leg never has to do. A fuselage-mounted main
//! gear therefore takes its leg from the fuselage ground datum instead
//! ([`crate::stations::fuselage_mounted_main_gear_leg_m`]); equation 67 (the
//! nose leg at 0.7 of the main) applies to either.

use alas_config::{FlopsStructureConfig, GeometryConfig, LandingGearConfig};

use super::super::airframe_geometry::{ground_dihedral_deg, BuiltFuselage};
use super::{main_gear_oleo_length_m, nose_gear_oleo_length_m};

/// The two leg lengths, m, and where each came from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::flops_transport) struct GearLegLengths {
    /// Extended main-gear leg `XMLG`, m.
    pub main_m: f64,
    /// `declared`, `fuselage_mounted_ground_datum` or `flops_equation_66`.
    pub main_source: &'static str,
    /// Extended nose-gear leg `XNLG`, m.
    pub nose_m: f64,
    /// `declared` or `flops_equation_67`.
    pub nose_source: &'static str,
}

/// Resolve `XMLG` and `XNLG`: a declared length first, then the fuselage
/// ground datum for a fuselage-mounted main gear, then equation 66 from the
/// scaled nacelle diameter `FNAC` and the outboard wing-engine station; the
/// nose leg is declared or equation 67 of whichever main leg was resolved.
pub(in crate::flops_transport) fn gear_leg_lengths(
    technology: &FlopsStructureConfig,
    built: &BuiltFuselage<'_>,
    geometry: &GeometryConfig,
    landing_gear: &LandingGearConfig,
    scaled_nacelle_diameter_m: f64,
    outboard_wing_engine_y_m: Option<f64>,
) -> GearLegLengths {
    let fuselage_leg_m = crate::stations::fuselage_mounted_main_gear_leg_m(
        built.wing,
        built.fuselage,
        geometry,
        landing_gear,
        outboard_wing_engine_y_m,
    );
    let (main_m, main_source) = match (technology.main_gear_oleo_length_m, fuselage_leg_m) {
        (Some(value), _) => (value, "declared"),
        (None, Some(leg_m)) => (leg_m, "fuselage_mounted_ground_datum"),
        (None, None) => (
            main_gear_oleo_length_m(
                scaled_nacelle_diameter_m,
                ground_dihedral_deg(built.wing, &geometry.wing),
                outboard_wing_engine_y_m,
                built.width_m,
                built.length_m,
            ),
            "flops_equation_66",
        ),
    };
    let (nose_m, nose_source) = match technology.nose_gear_oleo_length_m {
        Some(value) => (value, "declared"),
        None => (nose_gear_oleo_length_m(main_m), "flops_equation_67"),
    };
    GearLegLengths {
        main_m,
        main_source,
        nose_m,
        nose_source,
    }
}
