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
    let script = render_script(
        &airplane,
        None,
        "aircraft.vsp3",
        "aircraft.cad_preview.vsp3",
        "aircraft.preview.png",
    );
    assert!(validate_script(&script).is_ok());
    assert_eq!(script.matches("AddGeom( \"WING\"").count(), 3);
    assert_eq!(
        script.matches("AddGeom( \"FUSELAGE\"").count(),
        airplane.fuselages.len()
    );
    assert!(script.contains("SetAirfoilPnts"));
    assert!(script.contains("WriteVSPFile(\"aircraft.vsp3\", SET_ALL)"));
    assert!(script.contains("WriteVSPFile(\"aircraft.cad_preview.vsp3\", SET_ALL)"));
    assert!(script.contains("ScreenGrab(\"aircraft.preview.png\", 1600, 900, true, true)"));
    assert!(script.contains("if ( IsGUIBuild() )"));
    assert!(script.contains("ALAS_OPENVSP_PREVIEW_UNAVAILABLE"));
    assert_eq!(script.matches("SetSetFlag( wing_").count(), 3);
    assert_eq!(
        script.matches("SetSetFlag( body_").count(),
        airplane.fuselages.len()
    );
    assert!(script.contains(
        "SetIntAnalysisInput( alas_vspaero_geometry_analysis, \"GeomSet\", { SET_NONE }, 0 )"
    ));
    assert!(script.contains(
        "SetIntAnalysisInput( alas_vspaero_geometry_analysis, \"ThinGeomSet\", { 3 }, 0 )"
    ));
    assert!(script.contains(
        "SetIntAnalysisInput( alas_cad_preview_geometry_analysis, \"GeomSet\", { 4 }, 0 )"
    ));
    assert!(script.contains(
        "SetIntAnalysisInput( alas_cad_preview_geometry_analysis, \"ThinGeomSet\", { 3 }, 0 )"
    ));
    Ok(())
}

/// Regression test for the reported bug: the OpenVSP geometry preview showed
/// no fuselage. Root cause: the solver's `VSPAEROComputeGeometry` call only
/// ever meshed the wings (`ThinGeomSet=3`) with `GeomSet=SET_NONE`, and the
/// report/GUI preview drew exclusively from that thin-surface mesh. This
/// locks in the fix's two load-bearing invariants: (1) the fuselage is
/// flagged into a distinct Set (4) and a *second*, independent
/// `VSPAEROComputeGeometry` call meshes it as a thick body for CAD-preview
/// evidence only, and (2) the solver-facing call is textually untouched, so
/// an installed VSPAERO run still solves exactly the wing-only geometry it
/// solved before this patch.
#[test]
fn the_cad_preview_export_adds_the_fuselage_without_touching_the_solver_geometry(
) -> Result<(), alas_geom::builder::BuildError> {
    let config = AlasConfig::default();
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&DesignVector::default()), true)?;
    assert!(
        !airplane.fuselages.is_empty(),
        "fixture must exercise the fuselage-omission bug"
    );
    let script = render_script(
        &airplane,
        None,
        "aircraft.vsp3",
        "aircraft.cad_preview.vsp3",
        "aircraft.preview.png",
    );

    // The fuselage is a thick preview body (Set 4), never a thin lifting
    // surface (Set 3): a fuselage is not a valid zero-thickness VLM panel.
    assert!(script.contains("SetSetFlag( body_0, 4, true );"));
    assert!(!script.contains("SetSetFlag( body_0, 3, true );"));

    // The two VSPAEROComputeGeometry calls are independent named analyses
    // writing to independent WriteVSPFile targets.
    let solver_geom_set = "SetIntAnalysisInput( alas_vspaero_geometry_analysis, \"GeomSet\", { SET_NONE }, 0 );";
    let preview_geom_set =
        "SetIntAnalysisInput( alas_cad_preview_geometry_analysis, \"GeomSet\", { 4 }, 0 );";
    assert!(script.contains(solver_geom_set));
    assert!(script.contains(preview_geom_set));
    let solver_write = script.find("WriteVSPFile(\"aircraft.vsp3\", SET_ALL);").unwrap();
    let solver_analysis = script.find(solver_geom_set).unwrap();
    let preview_write = script
        .find("WriteVSPFile(\"aircraft.cad_preview.vsp3\", SET_ALL);")
        .unwrap();
    let preview_analysis = script.find(preview_geom_set).unwrap();
    // The solver's WriteVSPFile/ExecAnalysis pair happens strictly before the
    // preview-only pair, so the solver's on-disk .vspgeom is fully written
    // (and thus immune to the preview export's later, independent Set-4
    // panelization) before the CAD-preview mesh is ever produced.
    assert!(solver_write < preview_write);
    assert!(solver_analysis < preview_analysis);
    assert!(preview_write < preview_analysis);

    Ok(())
}

#[test]
fn the_script_preserves_each_section_twist_about_the_leading_edge(
) -> Result<(), alas_geom::builder::BuildError> {
    let config = AlasConfig::default();
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&DesignVector::default()), true)?;
    let script = render_script(
        &airplane,
        None,
        "aircraft.vsp3",
        "aircraft.cad_preview.vsp3",
        "aircraft.preview.png",
    );

    for (wing_index, wing) in airplane.wings.iter().enumerate() {
        for (section_index, section) in wing.xsecs.iter().enumerate() {
            let group = format!("XSec_{section_index}");
            assert!(script.contains(&format!(
                "SetParmVal( wing_{wing_index}, \"Twist_Location\", \"{group}\", 0.000000000000 );"
            )));
            assert!(script.contains(&format!(
                "SetParmVal( wing_{wing_index}, \"Twist\", \"{group}\", {:.12} );",
                section.twist
            )));
        }
    }
    Ok(())
}

#[test]
fn non_ascii_names_cannot_break_the_generated_script() {
    assert_eq!(
        script_string("Ala \"derecha\" - n\u{00fa}mero 1"),
        "Ala _derecha_ - n_mero 1"
    );
}
