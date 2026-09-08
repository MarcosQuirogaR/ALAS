// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


fn emit_wing(script: &mut String, index: usize, wing: &Wing) {
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

fn emit_landing_gear(script: &mut String, airplane: &Airplane, gear: &LandingGearLayout) {
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

fn set_parm(script: &mut String, id: &str, name: &str, group: &str, value: f64) {
    let _ = writeln!(
        script,
        "    SetParmVal( {id}, \"{name}\", \"{group}\", {value:.12} );"
    );
}

fn set_xsec_parm(script: &mut String, xsec_id: &str, name: &str, value: f64) {
    let _ = writeln!(
        script,
        "    SetParmVal( GetXSecParm( {xsec_id}, \"{name}\" ), {value:.12} );"
    );
}

fn script_string(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\\' => '/',
            '"' | '\n' | '\r' => '_',
            other if other.is_ascii() => other,
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
#[path = "../openvsp_tests.rs"]
mod tests;
