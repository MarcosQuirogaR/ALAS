// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Emitting fuselage, wing, airfoil and landing-gear components into the script.

use super::*;

/// Size the gear this export draws, or report that this aircraft has no
/// main-gear station to draw it at.
///
/// The stations come from [`crate::gear_stations::resolved_gear_stations`]
/// rather than from a fallback rebuilt here, so an aircraft whose layout is
/// outside the wing-mounted rule's domain cannot be exported with legs at a
/// station the mass model refuses to supply. Every aircraft that has a
/// station keeps exactly the one it had.
///
/// # Errors
///
/// [`MainGearFallbackRefusal`] when no main-gear station is available.
pub(super) fn landing_gear_for_report(
    report: &AnalysisReport,
    config: &AlasConfig,
) -> Result<LandingGearLayout, MainGearFallbackRefusal> {
    let mac = report.airplane.c_ref.max(0.001);
    // Canonical MAC frame: the main wing's own `mac_station()`
    // leading edge, never reconstructed from
    // `aerodynamic_center(0.25) - 0.25 * c_ref`.
    let x_mac_le = report
        .airplane
        .mac_frame()
        .map_or(0.0, |frame| frame.x_lemac_m);
    let aero_aft_x = report.x_neutral_point - config.requirements.target_static_margin * mac;
    let aero_fwd_x = aero_aft_x - config.requirements.cg_range_pct_mac / 100.0 * mac;

    let main_fuselage = &report.airplane.fuselages[0];
    let fus_start = main_fuselage
        .xsecs
        .first()
        .map_or(0.0, |section| section.xyz_c[0]);
    let fus_end = main_fuselage
        .xsecs
        .last()
        .map_or(fus_start, |section| section.xyz_c[0]);
    let fallback_x_nlg = fus_start + (fus_end - fus_start) * config.mass_model.nlg_x_fraction;
    let fallback_x_mlg = x_mac_le + config.mass_model.mlg_x_fraction_mac * mac;
    let stations = resolved_gear_stations(
        config,
        &report.airplane,
        fallback_x_nlg,
        fallback_x_mlg,
        fus_start,
        fus_end - fus_start,
    )?;
    let mass_kg = report.component_masses.values().copied().sum();
    let diameter_m = config.geometry.fuselage.diameter_m;

    Ok(size_landing_gear_with_group_stations(
        mass_kg,
        stations.x_nlg_m,
        stations.x_mlg_m,
        aero_fwd_x,
        aero_aft_x,
        diameter_m,
        diameter_m * 1.1,
        &stations.main_gear_x_m,
        &config.landing_gear,
    ))
}

pub(super) fn emit_fuselage(script: &mut String, index: usize, fuselage: &Fuselage) {
    if fuselage.xsecs.len() < 2 {
        return;
    }
    let id = format!("body_{index}");
    let surf = format!("body_surf_{index}");
    let first = fuselage.xsecs[0];
    let last = fuselage.xsecs[fuselage.xsecs.len() - 1];
    let length = (last.xyz_c[0] - first.xyz_c[0]).abs().max(1.0e-6);
    let _ = writeln!(script, "    string {id} = AddGeom( \"FUSELAGE\", \"\" );");
    let _ = writeln!(script, "    SetSetFlag( {id}, 4, true );");
    let _ = writeln!(
        script,
        "    SetGeomName( {id}, \"{}\" );",
        script_string(&fuselage.name)
    );
    set_parm(script, &id, "Length", "Design", length);
    set_parm(script, &id, "X_Rel_Location", "XForm", first.xyz_c[0]);
    set_parm(script, &id, "Y_Rel_Location", "XForm", first.xyz_c[1]);
    set_parm(script, &id, "Z_Rel_Location", "XForm", first.xyz_c[2]);
    let _ = writeln!(script, "    string {surf} = GetXSecSurf( {id}, 0 );");
    let _ = writeln!(
        script,
        "    while ( GetNumXSec( {surf} ) > 2 ) {{ CutXSec( {id}, 1 ); }}"
    );
    for insert_index in 0..fuselage.xsecs.len().saturating_sub(2) {
        let _ = writeln!(
            script,
            "    InsertXSec( {id}, {insert_index}, XS_ELLIPSE );"
        );
    }
    for (section_index, section) in fuselage.xsecs.iter().enumerate() {
        let x_fraction = (section.xyz_c[0] - first.xyz_c[0]) / length;
        let y_fraction = (section.xyz_c[1] - first.xyz_c[1]) / length;
        let z_fraction = (section.xyz_c[2] - first.xyz_c[2]) / length;
        let shape = if section.width <= 1.0e-9 || section.height <= 1.0e-9 {
            "XS_POINT"
        } else {
            "XS_ELLIPSE"
        };
        let xsec_id = format!("body_xsec_{index}_{section_index}");
        let _ = writeln!(
            script,
            "    ChangeXSecShape( {surf}, {section_index}, {shape} );"
        );
        let _ = writeln!(
            script,
            "    string {xsec_id} = GetXSec( {surf}, {section_index} );"
        );
        let _ = writeln!(
            script,
            "    SetXSecWidthHeight( {xsec_id}, {:.12}, {:.12} );",
            section.width, section.height
        );
        set_xsec_parm(script, &xsec_id, "XLocPercent", x_fraction);
        set_xsec_parm(script, &xsec_id, "YLocPercent", y_fraction);
        set_xsec_parm(script, &xsec_id, "ZLocPercent", z_fraction);

        // OpenVSP's default FUSELAGE skin leaves tangent strengths at their
        // nonzero spline defaults. That skin overshoots the ALAS station
        // envelope between sections, producing visible nose/tail ripples.
        // Zero tangent strength selects OpenVSP's piecewise-linear skin
        // interpolation, matching Fuselage's documented linear loft exactly.
        // Explicit C0 continuity and zero tangents make that choice stable
        // across OpenVSP defaults. Every ALAS station coordinate and envelope
        // remains the requested value.
        let _ = writeln!(script, "    SetXSecContinuity( {xsec_id}, 0 );");
        let _ = writeln!(
            script,
            "    SetXSecTanAngles( {xsec_id}, XSEC_BOTH_SIDES, 0 );"
        );
        let _ = writeln!(
            script,
            "    SetXSecTanStrengths( {xsec_id}, XSEC_BOTH_SIDES, 0 );"
        );
    }

    // Retain OpenVSP's endpoint angle convention only for point caps. Do not
    // apply these angles to ordinary nonzero end sections (for example,
    // nacelle inlet/exit stations), where they would change the intended
    // end-section shape. Zero tangent strength keeps the linear envelope.
    if first.width <= 1.0e-9 || first.height <= 1.0e-9 {
        let first_xsec_id = format!("body_xsec_{index}_0");
        let _ = writeln!(
            script,
            "    SetXSecTanAngles( {first_xsec_id}, XSEC_BOTH_SIDES, 90 );"
        );
    }
    if last.width <= 1.0e-9 || last.height <= 1.0e-9 {
        let last_xsec_id = format!("body_xsec_{index}_{}", fuselage.xsecs.len() - 1);
        let _ = writeln!(
            script,
            "    SetXSecTanAngles( {last_xsec_id}, XSEC_BOTH_SIDES, -90 );"
        );
    }
    script.push('\n');
}

pub(super) fn emit_wing(script: &mut String, index: usize, wing: &Wing) {
    if wing.xsecs.len() < 2 {
        return;
    }
    let id = format!("wing_{index}");
    let surf = format!("wing_surf_{index}");
    let root = &wing.xsecs[0];
    let _ = writeln!(script, "    string {id} = AddGeom( \"WING\", \"\" );");
    let _ = writeln!(script, "    SetSetFlag( {id}, 3, true );");
    let _ = writeln!(
        script,
        "    SetGeomName( {id}, \"{}\" );",
        script_string(&wing.name)
    );
    set_parm(script, &id, "X_Rel_Location", "XForm", root.xyz_le[0]);
    set_parm(script, &id, "Y_Rel_Location", "XForm", root.xyz_le[1]);
    set_parm(script, &id, "Z_Rel_Location", "XForm", root.xyz_le[2]);
    set_parm(
        script,
        &id,
        "Sym_Planar_Flag",
        "Sym",
        if wing.symmetric { 2.0 } else { 0.0 },
    );
    for section_index in 1..wing.xsecs.len().saturating_sub(1) {
        let _ = writeln!(
            script,
            "    InsertXSec( {id}, {section_index}, XS_FILE_AIRFOIL );"
        );
    }
    let _ = writeln!(script, "    string {surf} = GetXSecSurf( {id}, 0 );");

    // Mirrored halves must meet in their symmetry plane. Rotating the root
    // with dihedral sends a cambered/twisted root across that plane, producing
    // overlapping surfaces and a split trailing edge in VSPAERO's thin mesh.
    // An unmirrored fin still needs the rotated frame: otherwise OpenVSP's
    // root-thickness sec(dihedral) scaling is singular at 90 degrees.
    set_parm(
        script,
        &id,
        "RotateMatchDideralFlag",
        "XSec_0",
        if wing.symmetric { 0.0 } else { 1.0 },
    );

    for section_index in 1..wing.xsecs.len() {
        let inside = &wing.xsecs[section_index - 1];
        let outside = &wing.xsecs[section_index];
        let dx = outside.xyz_le[0] - inside.xyz_le[0];
        let dy = outside.xyz_le[1] - inside.xyz_le[1];
        let dz = outside.xyz_le[2] - inside.xyz_le[2];
        let span = dy.hypot(dz).max(1.0e-6);
        let sweep = dx.atan2(span).to_degrees();
        let dihedral = dz.atan2(dy.abs().max(1.0e-12)).to_degrees();
        let _ = writeln!(
            script,
            "    SetDriverGroup( {id}, {section_index}, SPAN_WSECT_DRIVER, ROOTC_WSECT_DRIVER, TIPC_WSECT_DRIVER );"
        );
        let group = format!("XSec_{section_index}");
        set_parm(script, &id, "Span", &group, span);
        set_parm(script, &id, "Root_Chord", &group, inside.chord);
        set_parm(script, &id, "Tip_Chord", &group, outside.chord);
        set_parm(script, &id, "Sweep", &group, sweep);
        set_parm(script, &id, "Sweep_Location", &group, 0.0);
        set_parm(script, &id, "Dihedral", &group, dihedral);
        // OpenVSP solves the active section's driver group (and adjacent
        // chords) on Update, not every section changed since the last call.
        // Flush each segment before moving on so derived Area/AvgChord and
        // the saved wing totals agree with the requested span/chords. Stale
        // totals otherwise rescale the whole wing when ReadVSPFile loads it.
        let _ = writeln!(script, "    Update();");
    }
    // ALAS defines each section's twist about its leading edge.  OpenVSP
    // stores both the root section (XSec_0) and every outboard section's
    // twist in the section group, with a quarter-chord pivot by default.
    // Write the complete section field set explicitly so the root is not
    // silently left at zero and no section changes pivot on import.
    for (section_index, section) in wing.xsecs.iter().enumerate() {
        let group = format!("XSec_{section_index}");
        set_parm(script, &id, "Twist_Location", &group, 0.0);
        set_parm(script, &id, "Twist", &group, section.twist);
    }
    for (section_index, section) in wing.xsecs.iter().enumerate() {
        emit_airfoil(
            script,
            &surf,
            index,
            section_index,
            &section.airfoil.coordinates,
        );
    }
    script.push('\n');
}

fn emit_airfoil(
    script: &mut String,
    wing_surf: &str,
    wing_index: usize,
    section_index: usize,
    coordinates: &[(f64, f64)],
) {
    if coordinates.len() < 3 {
        return;
    }
    let leading_edge = coordinates
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.0.total_cmp(&b.0))
        .map_or(0, |(index, _)| index);
    let xsec = format!("wing_xsec_{wing_index}_{section_index}");
    let upper = format!("upper_{wing_index}_{section_index}");
    let lower = format!("lower_{wing_index}_{section_index}");
    let _ = writeln!(
        script,
        "    ChangeXSecShape( {wing_surf}, {section_index}, XS_FILE_AIRFOIL );"
    );
    let _ = writeln!(
        script,
        "    string {xsec} = GetXSec( {wing_surf}, {section_index} );"
    );
    let _ = writeln!(script, "    array< vec3d > {upper};");
    for &(x, y) in coordinates[..=leading_edge].iter().rev() {
        let _ = writeln!(
            script,
            "    {upper}.insertLast( vec3d( {x:.12}, {y:.12}, 0.0 ) );"
        );
    }
    let _ = writeln!(script, "    array< vec3d > {lower};");
    for &(x, y) in &coordinates[leading_edge..] {
        let _ = writeln!(
            script,
            "    {lower}.insertLast( vec3d( {x:.12}, {y:.12}, 0.0 ) );"
        );
    }
    let _ = writeln!(script, "    SetAirfoilPnts( {xsec}, {upper}, {lower} );");
}

pub(super) fn emit_landing_gear(
    script: &mut String,
    airplane: &Airplane,
    gear: &LandingGearLayout,
) {
    let belly_z = airplane
        .fuselages
        .first()
        .and_then(|fuselage| {
            fuselage
                .xsecs
                .iter()
                .map(|section| section.xyz_c[2] - section.height / 2.0)
                .reduce(f64::min)
        })
        .unwrap_or(0.0);
    for (index, wheel) in gear.wheels.iter().enumerate() {
        let id = format!("wheel_{index}");
        let surf = format!("wheel_surf_{index}");
        let width = wheel.width_m.max(0.05);
        let diameter = wheel.diameter_m.max(0.05);
        let _ = writeln!(script, "    string {id} = AddGeom( \"FUSELAGE\", \"\" );");
        let _ = writeln!(script, "    SetSetFlag( {id}, 4, true );");
        let _ = writeln!(
            script,
            "    SetGeomName( {id}, \"{} wheel {}\" );",
            wheel.group,
            index + 1
        );
        set_parm(script, &id, "Length", "Design", width);
        set_parm(script, &id, "X_Rel_Location", "XForm", wheel.x);
        set_parm(
            script,
            &id,
            "Y_Rel_Location",
            "XForm",
            wheel.y - width / 2.0,
        );
        set_parm(
            script,
            &id,
            "Z_Rel_Location",
            "XForm",
            belly_z - wheel.diameter_m / 2.0,
        );
        set_parm(script, &id, "Z_Rel_Rotation", "XForm", 90.0);
        let _ = writeln!(script, "    string {surf} = GetXSecSurf( {id}, 0 );");
        let _ = writeln!(
            script,
            "    while ( GetNumXSec( {surf} ) > 3 ) {{ CutXSec( {id}, 1 ); }}"
        );
        for section_index in 0..3 {
            let shape = if section_index == 1 {
                "XS_ELLIPSE"
            } else {
                "XS_POINT"
            };
            let xsec = format!("wheel_xsec_{index}_{section_index}");
            let section_diameter = if section_index == 1 { diameter } else { 0.0 };
            let _ = writeln!(
                script,
                "    ChangeXSecShape( {surf}, {section_index}, {shape} );"
            );
            let _ = writeln!(
                script,
                "    string {xsec} = GetXSec( {surf}, {section_index} );"
            );
            let _ = writeln!(
                script,
                "    SetXSecWidthHeight( {xsec}, {section_diameter:.12}, {section_diameter:.12} );"
            );
            set_xsec_parm(script, &xsec, "XLocPercent", section_index as f64 / 2.0);
        }
    }
    script.push('\n');
}
