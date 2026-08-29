// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Render a standalone visual audit of UAV production-core integration.

use std::fmt::Write as _;
use std::path::PathBuf;

use alas_uav::optimizer::{
    EmpennageGeometry, FuselageGeometry, GeneratedGeometry, LandingGearGeometry, WingGeometry,
};
use alas_uav::{assess_generated_geometry_with_shared_core, SharedCoreInputs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os().nth(1).map_or_else(
        || PathBuf::from("outputs/uav_shared_core_audit.svg"),
        PathBuf::from,
    );
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let geometry = geometry();
    let assessment = assess_generated_geometry_with_shared_core(
        geometry,
        5.4,
        0.72,
        0.045,
        0.035,
        &SharedCoreInputs {
            main_airfoil_name: "naca2412".to_owned(),
            tail_airfoil_name: "naca0012".to_owned(),
            altitude_m: 0.0,
            speed_m_s: 20.0,
            angle_of_attack_deg: 4.0,
            spanwise_resolution: 8,
            chordwise_resolution: 4,
        },
    )?;

    let mut svg = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1440" height="820" viewBox="0 0 1440 820">
<rect width="1440" height="820" fill="#15191f"/>
<text x="64" y="62" fill="#f4f7fb" font-family="Segoe UI, sans-serif" font-size="30" font-weight="600">Fixed-Wing UAV - Shared Production-Core Audit</text>
<text x="64" y="94" fill="#aeb8c5" font-family="Segoe UI, sans-serif" font-size="16">Generated geometry converted to Airplane primitives; native VLM evaluated independently</text>
<rect x="48" y="126" width="740" height="612" rx="8" fill="#20262e" stroke="#536171"/>
<text x="76" y="166" fill="#69b7ff" font-family="Segoe UI, sans-serif" font-size="20" font-weight="600">Generated planform and installed layout</text>
<rect x="820" y="126" width="572" height="612" rx="8" fill="#20262e" stroke="#536171"/>
<text x="850" y="166" fill="#69b7ff" font-family="Segoe UI, sans-serif" font-size="20" font-weight="600">Independent aerodynamic comparison</text>
"##,
    );
    planform(&mut svg, geometry)?;
    let rows = [
        ("Airplane primitives", "3 wings + 1 fuselage".to_owned()),
        ("Main / tail airfoils", "NACA 2412 / NACA 0012".to_owned()),
        (
            "Operating point",
            "20.0 m/s, alpha 4.0 deg, sea level".to_owned(),
        ),
        ("Takeoff mass", "5.400 kg".to_owned()),
        (
            "Required one-g CL",
            format!("{:.6}", assessment.required_lift_coefficient),
        ),
        ("Native VLM CL", format!("{:.6}", assessment.vlm.cl_lift)),
        (
            "Native VLM induced CD",
            format!("{:.6}", assessment.vlm.cd_drag),
        ),
        (
            "Preliminary k CL^2",
            format!("{:.6}", assessment.preliminary_induced_drag_coefficient),
        ),
        (
            "Induced-CD discrepancy",
            format!("{:+.6}", assessment.induced_drag_coefficient_delta),
        ),
        ("Lift margin", format!("{:+.3} N", assessment.lift_margin_n)),
        (
            "Preliminary parasite CD0",
            format!(
                "{:.6} (not in VLM)",
                assessment.preliminary_zero_lift_drag_coefficient
            ),
        ),
    ];
    for (index, (label, value)) in rows.into_iter().enumerate() {
        let y = 212 + index * 43;
        writeln!(
            svg,
            r##"<text x="850" y="{y}" fill="#aeb8c5" font-family="Segoe UI, sans-serif" font-size="14">{label}</text>
<text x="1160" y="{y}" fill="#f4f7fb" font-family="Consolas, monospace" font-size="14">{value}</text>"##
        )?;
    }
    svg.push_str(
        r##"<line x1="850" y1="680" x2="1360" y2="680" stroke="#536171"/>
<text x="850" y="704" fill="#f0b35b" font-family="Segoe UI, sans-serif" font-size="13">VLM supplies inviscid lift and induced drag only.</text>
<text x="850" y="724" fill="#f0b35b" font-family="Segoe UI, sans-serif" font-size="13">CD0, CLmax, propulsion, and strength remain separate explicit checks.</text>
</svg>
"##,
    );
    std::fs::write(output, svg)?;
    Ok(())
}

fn planform(svg: &mut String, geometry: GeneratedGeometry) -> Result<(), std::fmt::Error> {
    let x0 = 110.0;
    let center_y = 440.0;
    let scale = 180.0;
    let px = |x_m: f64| x0 + scale * x_m;
    let py = |y_m: f64| center_y - scale * y_m;
    let wing = geometry.wing;
    let wing_x = px(wing.leading_edge_x_m);
    let wing_y = py(0.5 * wing.span_m);
    let wing_width = scale * wing.mean_chord_m;
    let wing_height = scale * wing.span_m;
    writeln!(
        svg,
        r##"<rect x="{wing_x:.2}" y="{wing_y:.2}" width="{wing_width:.2}" height="{wing_height:.2}" fill="#2f87d8" fill-opacity="0.45" stroke="#69b7ff" stroke-width="2"/>"##
    )?;
    let fuselage_x = px(0.0);
    let fuselage_y = py(0.5 * geometry.fuselage.diameter_m);
    let fuselage_width = scale * geometry.fuselage.length_m;
    let fuselage_height = scale * geometry.fuselage.diameter_m;
    writeln!(
        svg,
        r##"<rect x="{fuselage_x:.2}" y="{fuselage_y:.2}" width="{fuselage_width:.2}" height="{fuselage_height:.2}" rx="12" fill="#aeb8c5" fill-opacity="0.38" stroke="#e1e7ef"/>"##
    )?;
    let tail = geometry.empennage;
    let tail_chord = tail.horizontal_area_m2 / tail.horizontal_span_m;
    let tail_x = px(0.9 * geometry.fuselage.length_m - 0.25 * tail_chord);
    let tail_y = py(0.5 * tail.horizontal_span_m);
    let tail_width = scale * tail_chord;
    let tail_height = scale * tail.horizontal_span_m;
    writeln!(
        svg,
        r##"<rect x="{tail_x:.2}" y="{tail_y:.2}" width="{tail_width:.2}" height="{tail_height:.2}" fill="#42b883" fill-opacity="0.45" stroke="#74d6a8" stroke-width="2"/>"##
    )?;
    let main_x_m = wing.leading_edge_x_m + 0.25 * wing.mean_chord_m;
    let nose_x_m = main_x_m - geometry.landing_gear.wheelbase_m;
    for (x_m, y_m) in [
        (main_x_m, -0.5 * geometry.landing_gear.track_m),
        (main_x_m, 0.5 * geometry.landing_gear.track_m),
        (nose_x_m, 0.0),
    ] {
        writeln!(
            svg,
            r##"<circle cx="{:.2}" cy="{:.2}" r="6" fill="#f4f7fb"/>"##,
            px(x_m),
            py(y_m)
        )?;
    }
    writeln!(
        svg,
        r##"<text x="76" y="708" fill="#aeb8c5" font-family="Segoe UI, sans-serif" font-size="14">S = {:.3} m2   b = {:.3} m   AR = {:.3}   fuselage = {:.3} m   gear track = {:.3} m</text>"##,
        wing.area_m2,
        wing.span_m,
        wing.aspect_ratio,
        geometry.fuselage.length_m,
        geometry.landing_gear.track_m
    )?;
    Ok(())
}

fn geometry() -> GeneratedGeometry {
    GeneratedGeometry {
        wing: WingGeometry {
            area_m2: 0.98,
            aspect_ratio: 8.0,
            span_m: 2.8,
            mean_chord_m: 0.35,
            leading_edge_x_m: 0.54,
        },
        fuselage: FuselageGeometry {
            length_m: 1.9,
            diameter_m: 0.18,
            equipment_bay: alas_uav::feasibility::EquipmentBay {
                min_x_m: 0.2,
                max_x_m: 1.3,
                min_y_m: -0.06,
                max_y_m: 0.06,
                min_z_m: -0.05,
                max_z_m: 0.05,
            },
        },
        empennage: EmpennageGeometry {
            horizontal_area_m2: 0.22,
            horizontal_span_m: 1.1,
            vertical_area_m2: 0.105,
            vertical_span_m: 0.42,
            tail_arm_m: 1.08,
        },
        landing_gear: LandingGearGeometry {
            track_m: 0.5,
            wheelbase_m: 0.65,
            minimum_leg_length_m: 0.24,
            design_load_factor: 3.0,
        },
    }
}
