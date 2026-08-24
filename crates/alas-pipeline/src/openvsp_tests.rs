// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use alas_config::design_variables::DesignVector;
use alas_geom::builder::AircraftBuilder;

#[test]
fn the_script_contains_every_computed_outer_geometry_component(
) -> Result<(), alas_geom::builder::BuildError> {
    let config = AlasConfig::default();
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&DesignVector::default()), true)?;
    let script = render_script(&airplane, None, "aircraft.vsp3");
    assert!(validate_script(&script).is_ok());
    assert_eq!(script.matches("AddGeom( \"WING\"").count(), 3);
    assert_eq!(
        script.matches("AddGeom( \"FUSELAGE\"").count(),
        airplane.fuselages.len()
    );
    assert!(script.contains("SetAirfoilPnts"));
    assert!(script.contains("WriteVSPFile(\"aircraft.vsp3\", SET_ALL)"));
    assert_eq!(script.matches("SetSetFlag( wing_").count(), 3);
    assert!(script.contains(
        "SetIntAnalysisInput( alas_vspaero_geometry_analysis, \"GeomSet\", { SET_NONE }, 0 )"
    ));
    assert!(script.contains(
        "SetIntAnalysisInput( alas_vspaero_geometry_analysis, \"ThinGeomSet\", { 3 }, 0 )"
    ));
    Ok(())
}

#[test]
fn non_ascii_names_cannot_break_the_generated_script() {
    assert_eq!(
        script_string("Ala \"derecha\" - n\u{00fa}mero 1"),
        "Ala _derecha_ - n_mero 1"
    );
}
