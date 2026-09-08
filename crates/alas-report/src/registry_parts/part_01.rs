// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use serde::{Deserialize, Serialize};

/// Figure metadata descriptor for UI menus and report documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FigureDescriptor {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub required_stage: RequiredStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequiredStage {
    Preview,
    Baseline,
    Optimization,
    FullAnalysis,
    Mission,
    Mses,
    Structures,
    Screening,
}

const fn descriptor(
    id: &'static str,
    title: &'static str,
    category: &'static str,
    description: &'static str,
) -> FigureDescriptor {
    descriptor_with_stage(id, title, category, description, required_stage_for_id(id))
}

const fn descriptor_with_stage(
    id: &'static str,
    title: &'static str,
    category: &'static str,
    description: &'static str,
    required_stage: RequiredStage,
) -> FigureDescriptor {
    FigureDescriptor {
        id,
        title,
        category,
        description,
        required_stage,
    }
}

const fn preview_descriptor(
    id: &'static str,
    title: &'static str,
    category: &'static str,
    description: &'static str,
) -> FigureDescriptor {
    descriptor_with_stage(id, title, category, description, RequiredStage::Preview)
}

const fn screening_descriptor(
    id: &'static str,
    title: &'static str,
    category: &'static str,
    description: &'static str,
) -> FigureDescriptor {
    descriptor_with_stage(id, title, category, description, RequiredStage::Screening)
}

const fn required_stage_for_id(id: &str) -> RequiredStage {
    // Stage ownership is a data contract, not a naming convention. Several
    // aerodynamic and stability IDs begin with `d` or `s`, and their data are
    // produced by the full analysis rather than optimization or structures.
    // Keep the complete mapping here so a new descriptor cannot inherit an
    // accidental stage merely because its title starts with a particular
    // letter.
    if any_id(
        id,
        &[
            "optimization_history",
            "design_evolution",
            "airfoil_comparison",
            "airfoil_evolution",
            "polar_comparison",
            "planform_comparison",
            "wireframe_wing",
            "wireframe_fuselage",
            "wireframe_empennage",
            "threeview",
        ],
    ) {
        RequiredStage::Optimization
    } else if any_id(
        id,
        &["mses_pressure", "mses_mach_contours", "mses_convergence"],
    ) {
        RequiredStage::Mses
    } else if any_id(
        id,
        &[
            "structures_sizing",
            "structures_loads",
            "structures_stress",
            "structures_modes",
            "structures_vibration",
            "structures_patran",
        ],
    ) {
        RequiredStage::Structures
    } else if any_id(
        id,
        &[
            "mission_route_2d",
            "mission_route_3d",
            "payload_range",
            "mission_profile",
            "mission_velocities",
            "mission_flight_path",
            "mission_aero_coefficients",
            "mission_aero_forces",
            "mission_drag_components",
        ],
    ) {
        RequiredStage::Mission
    } else {
        RequiredStage::FullAnalysis
    }
}

const fn any_id(id: &str, candidates: &[&str]) -> bool {
    let mut index = 0;
    while index < candidates.len() {
        if same_id(id, candidates[index]) {
            return true;
        }
        index += 1;
    }
    false
}

const fn same_id(left: &str, right: &str) -> bool {
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    if left_bytes.len() != right_bytes.len() {
        return false;
    }
    let mut index = 0;
    while index < left_bytes.len() {
        if left_bytes[index] != right_bytes[index] {
            return false;
        }
        index += 1;
    }
    true
}

pub static PREVIEW_FIGURES: &[FigureDescriptor] = &[
    preview_descriptor(
        "exterior_3d",
        "3D Exterior Wireframe",
        "Geometry",
        "3D wireframe preview of the aircraft fuselage, wings, and nacelles.",
    ),
    preview_descriptor(
        "cabin_3d",
        "Cabin / Payload Layout",
        "Geometry",
        "Live cabin and payload layout preview; the current SVG renderer is 2D.",
    ),
    preview_descriptor(
        "geometry",
        "Planform Geometry",
        "Geometry",
        "2D top-down planform drawing with aerodynamic surfaces.",
    ),
    preview_descriptor(
        "drag",
        "Drag vs Mach (illustrative)",
        "Aerodynamics",
        "Live parasite and wave-drag trends across the cruise Mach range.",
    ),
    preview_descriptor(
        "mass_cg",
        "Mass and CG Envelope",
        "Weight & Balance",
        "Live center-of-gravity envelope from the current geometry and mass model.",
    ),
    preview_descriptor(
        "landing_gear",
        "Landing-Gear Planform",
        "Weight & Balance",
        "Live landing-gear positions and planform stability geometry.",
    ),
    preview_descriptor(
        "control_surfaces",
        "Control Surfaces and Tail Sizing",
        "Aerodynamics",
        "Live configured control-surface layout and tail-volume checks.",
    ),
    preview_descriptor(
        "structures",
        "Wingbox Planform",
        "Structures",
        "Live wingbox spar and sizing preview from the current design.",
    ),
    preview_descriptor(
        "engine",
        "Engine Designer Preview",
        "Propulsion",
        "Editable nacelle profile preview.",
    ),
];
