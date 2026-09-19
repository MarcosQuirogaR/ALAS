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

/// A real analysis report for the clean-sheet default, reused by the gear
/// tests below.
///
/// The report is the only way to obtain the neutral point and component
/// masses `landing_gear_for_report` sizes against; nothing cheaper carries
/// them without inventing them.
fn default_report() -> (AlasConfig, AnalysisReport) {
    let mut config = AlasConfig::default();
    config.mission.enabled = false;
    config.structures.enabled = false;
    config.mses.enabled = false;
    let report = crate::full_analysis::FullAnalysis::new(config.clone())
        .run(&DesignVector::default(), false)
        .unwrap_or_else(|error| panic!("clean-sheet analysis: {error}"));
    (config, report)
}

#[test]
fn an_aircraft_with_no_measured_main_gear_station_is_not_exported_with_gear() {
    // The ATR geometry and its configuration, carried on an otherwise valid
    // report: the export must refuse rather than draw legs under a wing root
    // that has no gear bay. The report's own masses and neutral point are the
    // clean-sheet ones and are never reached, because the station is refused
    // before any sizing runs.
    let (_, mut report) = default_report();
    let preset = alas_config::presets::get("ATR72-600").expect("registered ATR preset");
    let mut atr = AlasConfig::from_value(&serde_json::json!({ "preset": "ATR72-600" }))
        .expect("ATR configuration");
    // This export refusal test uses an intentionally unmeasured fixture. The
    // production ATR preset now carries its published gear anchors.
    atr.landing_gear.reference_station_fuselage_length_m = None;
    atr.landing_gear.reference_nlg_x_fraction = None;
    atr.landing_gear.reference_mlg_x_fractions = None;
    report.airplane = AircraftBuilder::new(Some(atr.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("ATR geometry");

    let refusal = landing_gear_for_report(&report, &atr)
        .expect_err("an ATR-like layout has no main-gear station to export");
    assert!(
        refusal.wing_root_z_m > refusal.fuselage_crown_z_m,
        "the refusal must carry the two heights that decided it: {refusal}"
    );

    let script_path = std::env::temp_dir().join(format!(
        "alas-openvsp-refused-{}-{:?}.vspscript",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_file(&script_path);
    let error = export_openvsp_script(&report, &atr, &script_path)
        .expect_err("the export must fail rather than publish an aeroplane without gear");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(
        error.to_string().contains("landing-gear export refused"),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("no main-gear longitudinal station is available"),
        "{error}"
    );
    assert!(
        !script_path.exists(),
        "no partial artifact may be left behind"
    );
}

#[test]
fn an_aircraft_with_a_main_gear_station_exports_exactly_the_station_it_had() {
    // The gate must be inert wherever a station exists: the sized layout is
    // the one the unchecked fallback resolution produces, to the last bit.
    let (config, report) = default_report();
    let gear = landing_gear_for_report(&report, &config)
        .expect("the clean-sheet default has a main-gear station");

    let main_wing = &report.airplane.wings[0];
    let mac = report.airplane.c_ref.max(0.001);
    let x_mac_le = main_wing.aerodynamic_center(0.25)[0] - 0.25 * mac;
    let fuselage = &report.airplane.fuselages[0];
    let fus_start = fuselage.xsecs.first().map_or(0.0, |s| s.xyz_c[0]);
    let fus_end = fuselage.xsecs.last().map_or(fus_start, |s| s.xyz_c[0]);
    let unchecked = config.landing_gear.resolved_station_positions(
        fus_start + (fus_end - fus_start) * config.mass_model.nlg_x_fraction,
        x_mac_le + config.mass_model.mlg_x_fraction_mac * mac,
        fus_start,
        fus_end - fus_start,
    );
    assert_eq!(gear.x_nlg, unchecked.x_nlg_m);
    assert_eq!(gear.x_mlg, unchecked.x_mlg_m);
    assert!(!gear.wheels.is_empty());
}
