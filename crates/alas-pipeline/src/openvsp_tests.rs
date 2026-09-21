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
    let solver_geom_set =
        "SetIntAnalysisInput( alas_vspaero_geometry_analysis, \"GeomSet\", { SET_NONE }, 0 );";
    let preview_geom_set =
        "SetIntAnalysisInput( alas_cad_preview_geometry_analysis, \"GeomSet\", { 4 }, 0 );";
    assert!(script.contains(solver_geom_set));
    assert!(script.contains(preview_geom_set));
    let solver_write = script
        .find("WriteVSPFile(\"aircraft.vsp3\", SET_ALL);")
        .unwrap();
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
fn the_fuselage_export_pins_the_documented_linear_loft_and_point_caps(
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

    for (fuselage_index, fuselage) in airplane.fuselages.iter().enumerate() {
        for section_index in 0..fuselage.xsecs.len() {
            let xsec_id = format!("body_xsec_{fuselage_index}_{section_index}");
            assert!(script.contains(&format!(
                "SetXSecContinuity( {xsec_id}, 0 );"
            )));
            assert!(script.contains(&format!(
                "SetXSecTanAngles( {xsec_id}, XSEC_BOTH_SIDES, 0 );"
            )));
            assert!(script.contains(&format!(
                "SetXSecTanStrengths( {xsec_id}, XSEC_BOTH_SIDES, 0 );"
            )));
        }

        let first = &fuselage.xsecs[0];
        let last_index = fuselage.xsecs.len() - 1;
        let last = &fuselage.xsecs[last_index];
        let first_cap = format!("body_xsec_{fuselage_index}_0");
        let last_cap = format!("body_xsec_{fuselage_index}_{last_index}");
        let first_angle = format!(
            "SetXSecTanAngles( {first_cap}, XSEC_BOTH_SIDES, 90 );"
        );
        let last_angle = format!(
            "SetXSecTanAngles( {last_cap}, XSEC_BOTH_SIDES, -90 );"
        );
        assert_eq!(
            script.contains(&first_angle),
            first.width <= 1.0e-9 || first.height <= 1.0e-9
        );
        assert_eq!(
            script.contains(&last_angle),
            last.width <= 1.0e-9 || last.height <= 1.0e-9
        );
    }
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

/// Numerical round-trip against OpenVSP, rather than a text-only check of
/// requested driver values: stale derived totals used to shrink the root
/// chord and increase span when the native project was reopened.
#[test]
#[ignore = "requires ALAS_OPENVSP_EXE pointing to installed vspscript"]
fn native_wing_roundtrip_preserves_section_chords_and_tip_position() {
    let executable = std::env::var_os("ALAS_OPENVSP_EXE")
        .expect("set ALAS_OPENVSP_EXE to the installed script runner");
    let airplane = AircraftBuilder::new(Some(AlasConfig::default().geometry))
        .build(Some(&DesignVector::default()), true)
        .unwrap();
    let wing = &airplane.wings[0];
    let tip = wing.xsecs.last().unwrap();
    let directory =
        std::env::temp_dir().join(format!("alas-wing-roundtrip-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let mut script = String::from("void main() {\n    ClearVSPModel();\n");
    emit_wing(&mut script, 0, wing);
    script.push_str("    Update();\n    WriteVSPFile(\"wing.vsp3\", SET_ALL);\n    bool valid = true;\n    for ( int phase = 0; phase < 2; phase++ ) {\n        if ( phase == 1 ) { ClearVSPModel(); ReadVSPFile(\"wing.vsp3\"); }\n");
    let _ = writeln!(
        script,
        "        string geom = FindGeom(\"{}\", 0);",
        script_string(&wing.name)
    );
    script.push_str("        string surf = GetXSecSurf(geom, 0);\n");
    for (index, section) in wing.xsecs.iter().enumerate() {
        let _ = writeln!(script, "        double chord_{index} = GetParmVal(GetXSecParm(GetXSec(surf, {index}), \"Chord\"));");
        let _ = writeln!(script, "        if (chord_{index} < {:.12} || chord_{index} > {:.12}) {{ valid = false; Print(\"CHORD_MISMATCH_{index}\"); }}", section.chord - 1.0e-7, section.chord + 1.0e-7);
    }
    script.push_str("        vec3d tip = CompPnt01(geom, 0, 1.0, 0.5);\n");
    for (axis, expected) in ["x", "y", "z"].into_iter().zip(tip.xyz_le) {
        let _ = writeln!(script, "        if (tip.{axis}() < {:.12} || tip.{axis}() > {:.12}) {{ valid = false; Print(\"TIP_MISMATCH_{axis}\"); }}", expected - 1.0e-7, expected + 1.0e-7);
    }
    script.push_str("    }\n    if (valid) { Print(\"ALAS_WING_ROUNDTRIP_OK\"); }\n}\n");
    fs::write(directory.join("roundtrip.vspscript"), script).unwrap();
    let output = Command::new(executable)
        .arg("-script")
        .arg("roundtrip.vspscript")
        .current_dir(&directory)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("ALAS_WING_ROUNDTRIP_OK"),
        "OpenVSP round-trip failed; retained evidence at {}\n{}\n{}",
        directory.display(),
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Native surface samples verify the exporter preserves ALAS's documented
/// piecewise-linear loft, rather than merely hiding the spline ripple through
/// display tessellation. The sampled side radius is compared with the linear
/// interpolation of adjacent station envelopes in metres, and nose/tail
/// samples are required to remain monotone.
#[test]
#[ignore = "requires ALAS_OPENVSP_EXE pointing to installed vspscript"]
fn native_fuselage_surface_samples_match_the_linear_station_loft() {
    let executable = std::env::var_os("ALAS_OPENVSP_EXE")
        .expect("set ALAS_OPENVSP_EXE to the installed script runner");
    let airplane = AircraftBuilder::new(Some(AlasConfig::default().geometry))
        .build(Some(&DesignVector::default()), true)
        .unwrap();
    let fuselage = &airplane.fuselages[0];
    let section_count = fuselage.xsecs.len();
    assert!(section_count >= 3, "fixture needs nose, cabin and tail stations");
    let max_width = fuselage
        .xsecs
        .iter()
        .map(|section| section.width)
        .fold(0.0, f64::max);
    let nose_end = fuselage
        .xsecs
        .iter()
        .position(|section| (section.width - max_width).abs() <= 1.0e-12)
        .unwrap();
    let tail_start = fuselage
        .xsecs
        .iter()
        .rposition(|section| (section.width - max_width).abs() <= 1.0e-12)
        .unwrap();
    let denominator = (section_count - 1) as f64;
    let nose_break = nose_end as f64 / denominator;
    let tail_break = tail_start as f64 / denominator;
    let directory = std::env::var_os("ALAS_OPENVSP_FUSELAGE_TEST_OUTPUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!(
                "alas-fuselage-loft-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ))
        });
    fs::create_dir_all(&directory).unwrap();

    let mut script = String::from("void main() {\n    ClearVSPModel();\n");
    emit_fuselage(&mut script, 0, fuselage);
    script.push_str(
        r#"    Update();
    WriteVSPFile( "fuselage.vsp3", SET_ALL );
    ClearVSPModel();
    ReadVSPFile( "fuselage.vsp3" );
    string body = FindGeom( "Fuselage", 0 );
    bool valid = true;
    double previous_x = -1.0e30;
    double previous_nose_y = -1.0e30;
    double previous_tail_y = 1.0e30;
    bool have_nose = false;
    bool have_tail = false;
    for ( int sample = 0; sample <= 400; sample++ ) {
        double u = sample / 400.0;
        vec3d p = CompPnt01( body, 0, u, 0.0 );
        if ( sample > 0 && p.x() + 1.0e-8 < previous_x ) {
            valid = false;
            Print( string( "ALAS_FUSELAGE_X_NONMONOTONE\n" ) );
        }
        previous_x = p.x();
        double expected_radius = 0.0;
"#,
    );
    for (index, pair) in fuselage.xsecs.windows(2).enumerate() {
        let left = pair[0];
        let right = pair[1];
        let slope = (right.width - left.width) / (right.xyz_c[0] - left.xyz_c[0]) / 2.0;
        let condition = if index == 0 { "if" } else { "else if" };
        let _ = writeln!(
            script,
            "        {condition} ( p.x() <= {:.12} ) {{ expected_radius = {:.12} + ( p.x() - {:.12} ) * {:.12}; }}",
            right.xyz_c[0],
            left.width / 2.0,
            left.xyz_c[0],
            slope
        );
    }
    script.push_str(
        r#"        if ( abs( p.y() - expected_radius ) > 1.0e-6 ) {
            valid = false;
            Print( string( "ALAS_FUSELAGE_LINEAR_MISMATCH\n" ) );
        }
"#,
    );
    let _ = writeln!(
        script,
        r#"        if ( u <= {:.12} ) {{
            if ( have_nose && p.y() + 1.0e-8 < previous_nose_y ) {{
                valid = false;
                Print( string( "ALAS_FUSELAGE_NOSE_RIPPLE\n" ) );
            }}
            previous_nose_y = p.y();
            have_nose = true;
        }}
        if ( u >= {:.12} ) {{
            if ( have_tail && p.y() > previous_tail_y + 1.0e-8 ) {{
                valid = false;
                Print( string( "ALAS_FUSELAGE_TAIL_RIPPLE\n" ) );
            }}
            previous_tail_y = p.y();
            have_tail = true;
        }}
    }}
    if ( valid ) {{ Print( string( "ALAS_FUSELAGE_LINEAR_OK\n" ) ); }}
}}
"#,
        nose_break,
        tail_break
    );
    fs::write(directory.join("fuselage.vspscript"), script).unwrap();
    let output = Command::new(executable)
        .arg("-script")
        .arg("fuselage.vspscript")
        .current_dir(&directory)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("ALAS_FUSELAGE_LINEAR_OK"),
        "OpenVSP fuselage loft failed; retained evidence at {}\n{}\n{}",
        directory.display(),
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
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
