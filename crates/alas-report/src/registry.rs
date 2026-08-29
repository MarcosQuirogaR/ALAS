// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/sidecar/figures.py
// Reference: alas @ rust-port-baseline.

//! Figure registry and metadata catalogue for interactive UI and batch export.
#![allow(missing_docs)] // The registry table is self-describing data; field docs add noise to every entry.

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
    } else if any_id(id, &["mses_pressure", "mses_mach_contours"]) {
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

pub static RESULT_FIGURES: &[FigureDescriptor] = &[
    // Optimization
    descriptor(
        "optimization_history",
        "Optimization History",
        "Optimization",
        "Convergence trace of objective function versus evaluations.",
    ),
    descriptor(
        "design_evolution",
        "Design Evolution",
        "Optimization",
        "Planform evolution across valid optimization evaluations.",
    ),
    descriptor(
        "airfoil_comparison",
        "Airfoil Comparison",
        "Optimization",
        "Initial and optimized root airfoil section comparison.",
    ),
    descriptor(
        "airfoil_evolution",
        "Airfoil Spanwise Evolution",
        "Optimization",
        "Airfoil sections along the optimized wing span.",
    ),
    descriptor(
        "polar_comparison",
        "Drag Polar (Baseline vs Optimized)",
        "Optimization",
        "Baseline and optimized lift-drag polar comparison.",
    ),
    descriptor(
        "planform_comparison",
        "Planform Comparison",
        "Optimization",
        "Baseline and optimized aircraft planform overlay.",
    ),
    descriptor(
        "wireframe_wing",
        "Wing Wireframe",
        "Optimization",
        "Isolated optimized main-wing wireframe.",
    ),
    descriptor(
        "wireframe_fuselage",
        "Fuselage Wireframe",
        "Optimization",
        "Isolated optimized fuselage wireframe.",
    ),
    descriptor(
        "wireframe_empennage",
        "Empennage Wireframe",
        "Optimization",
        "Isolated optimized tail-surface wireframe.",
    ),
    descriptor(
        "threeview",
        "Three-View",
        "Optimization",
        "Top, front, side, and isometric aircraft views.",
    ),
    // Aerodynamics and stability
    descriptor(
        "aero_panel",
        "Lift / Drag / Moment",
        "Aerodynamics",
        "Lift curve, drag polar, efficiency, and pitching-moment panels.",
    ),
    descriptor(
        "drag_breakdown",
        "Drag Component Breakdown",
        "Aerodynamics",
        "Parasite, induced, wave, and total drag at the design point.",
    ),
    descriptor(
        "span_loading",
        "Spanwise Lift Distribution",
        "Aerodynamics",
        "Spanwise loading compared with the elliptic reference.",
    ),
    descriptor(
        "vn_diagram",
        "V-n Flight Envelope",
        "Aerodynamics",
        "Maneuver and gust flight envelope diagram.",
    ),
    descriptor(
        "vlm_flow",
        "VLM Flow Streamlines",
        "Aerodynamics",
        "Vortex-lattice flow visualization around the aircraft.",
    ),
    descriptor(
        "dynamic_modes",
        "Dynamic Stability Modes",
        "Aerodynamics",
        "Eigenvalue poles of longitudinal and lateral dynamic modes.",
    ),
    descriptor(
        "control_surfaces",
        "Control Surfaces and Tail Sizing",
        "Aerodynamics",
        "Configured control-surface layout and tail-volume checks.",
    ),
    descriptor(
        "airfoil_reynolds",
        "Airfoil vs Reynolds",
        "Aerodynamics",
        "Airfoil aerodynamic response across Reynolds number and angle.",
    ),
    descriptor(
        "mses_pressure",
        "MSES Pressure Distribution",
        "Aerodynamics",
        "MSES surface pressure distribution for the root section.",
    ),
    descriptor(
        "mses_mach_contours",
        "MSES Mach Contours",
        "Aerodynamics",
        "MSES surface and flowfield Mach contours.",
    ),
    // Weight and balance
    descriptor(
        "mass_breakdown",
        "Mass Breakdown",
        "Weight & Balance",
        "Aircraft component mass breakdown.",
    ),
    descriptor(
        "fuel_volume_check",
        "Fuel-Volume Check",
        "Weight & Balance",
        "Required fuel volume compared with wing-tank capacity.",
    ),
    descriptor(
        "mass_distribution",
        "Plan-View Mass Distribution",
        "Weight & Balance",
        "Component masses and centroids over the aircraft planform.",
    ),
    descriptor(
        "cg_envelope",
        "Model CG Loading-State Check",
        "Weight & Balance",
        "Model-derived aggregate CG loading-state check; not an AFM/WBM operational envelope.",
    ),
    descriptor(
        "landing_gear_planform",
        "Landing-Gear Planform",
        "Weight & Balance",
        "Landing-gear positions and planform stability geometry.",
    ),
    descriptor(
        "stability_side_view",
        "Stability - Side View",
        "Weight & Balance",
        "Longitudinal stability markers over the aircraft side view.",
    ),
    descriptor(
        "cabin_payload",
        "Cabin / Payload Layout",
        "Weight & Balance",
        "Passenger, cargo, cabin, and payload layout.",
    ),
    // Propulsion
    descriptor(
        "propulsion_cycle_summary",
        "Propulsion Cycle Summary",
        "Propulsion",
        "On-design turbofan stations and performance summary.",
    ),
    descriptor(
        "propulsion_carpet_plot",
        "Propulsion Carpet Plot",
        "Propulsion",
        "Specific-thrust and TSFC trade space.",
    ),
    descriptor(
        "propulsion_efficiency_decomposition",
        "Efficiency Decomposition",
        "Propulsion",
        "Thermal, propulsive, and overall efficiency versus pressure ratio.",
    ),
    descriptor(
        "propulsion_bpr_sensitivity",
        "BPR Sensitivity",
        "Propulsion",
        "Specific thrust and TSFC sensitivity to bypass ratio.",
    ),
    descriptor(
        "propulsion_altitude_sweep",
        "Altitude and Mach Sweep",
        "Propulsion",
        "Installed thrust and TSFC across altitude and Mach.",
    ),
    // Structures
    descriptor(
        "structures_sizing",
        "Wingbox Sizing",
        "Structures",
        "Wingbox sizing distribution and structural mass summary.",
    ),
    descriptor(
        "structures_loads",
        "Static Loads",
        "Structures",
        "Spanwise stiffness, moment, and deflection load results.",
    ),
    descriptor(
        "structures_stress",
        "Stress Margins",
        "Structures",
        "Spar-cap margin-of-safety distributions.",
    ),
    descriptor(
        "structures_modes",
        "Normal Modes",
        "Structures",
        "Structural normal-mode frequency and shape comparison.",
    ),
    descriptor(
        "structures_vibration",
        "Vibration",
        "Structures",
        "Sine-sweep and random-vibration response results.",
    ),
    descriptor(
        "structures_patran",
        "Patran Renders",
        "Structures",
        "External Patran deformation render outputs.",
    ),
    // Mission and route
    descriptor(
        "mission_route_2d",
        "Route Map (2D)",
        "Mission",
        "Route waypoints and flown profile over geographic coordinates.",
    ),
    descriptor(
        "mission_route_3d",
        "Route Globe (3D)",
        "Mission",
        "Orbitable Earth-centered route with great-circle legs, altitude, and flown mass.",
    ),
    descriptor(
        "payload_range",
        "Conceptual Payload-Range",
        "Mission",
        "Idealized Breguet payload-range curve using typed capacity evidence; not an AFM/WBM operational envelope.",
    ),
    descriptor(
        "mission_profile",
        "Mission Profile",
        "Mission",
        "Altitude, mass, speed, and fuel-consumption mission timelines.",
    ),
    descriptor(
        "mission_velocities",
        "Airspeeds",
        "Mission",
        "True/equivalent airspeed and Mach throughout the mission.",
    ),
    descriptor(
        "mission_flight_path",
        "Flight Path",
        "Mission",
        "Cumulative range and flight-path angle throughout the mission.",
    ),
    descriptor(
        "mission_aero_coefficients",
        "Aero Coefficients",
        "Mission",
        "Angle of attack, lift, drag, and lift-to-drag mission histories.",
    ),
    descriptor(
        "mission_aero_forces",
        "Aero Forces",
        "Mission",
        "Throttle, lift, thrust, and drag mission histories.",
    ),
    descriptor(
        "mission_drag_components",
        "Drag Components",
        "Mission",
        "Mission drag-component histories.",
    ),
    // Field performance and model comparison
    descriptor(
        "matching_chart",
        "Matching Chart",
        "Field Performance",
        "Thrust-to-weight and wing-loading design constraints.",
    ),
    descriptor(
        "lto_departure",
        "Landing & Take-Off - Departure",
        "Field Performance",
        "Departure runway schematic and field-performance distances.",
    ),
    descriptor(
        "lto_arrival",
        "Landing & Take-Off - Arrival",
        "Field Performance",
        "Arrival runway schematic and landing distances.",
    ),
    descriptor(
        "model_comparison",
        "Whole-Aircraft Model Comparison",
        "Model Comparison",
        "Whole-aircraft VLM and mission comparison; 2-D MSES section data are reported separately.",
    ),
];

pub static SCREENING_FIGURES: &[FigureDescriptor] = &[
    screening_descriptor(
        "trade_map",
        "Trade map - L/D vs fuel capacity",
        "Screening",
        "Cruise lift-to-drag ratio versus resulting wing fuel capacity.",
    ),
    screening_descriptor(
        "rerank_2d_3d",
        "2-D shortlist reshuffled in 3-D",
        "Screening",
        "Comparison of isolated-section screening with the refined wing result.",
    ),
    screening_descriptor(
        "ranking_bars",
        "Top airfoils for this design",
        "Screening",
        "Top candidates ranked by final cruise lift-to-drag ratio.",
    ),
    screening_descriptor(
        "section_shapes",
        "Section shapes - top picks vs current",
        "Screening",
        "Overlaid geometries of the leading candidates and current section.",
    ),
    screening_descriptor(
        "mses_verification",
        "MSES verification - real shock/viscous effects",
        "Screening",
        "Stage-2 wing predictions compared with verified MSES results.",
    ),
];

/// Look up a figure descriptor by its unique identifier in any registry.
pub fn find_figure(id: &str) -> Option<&'static FigureDescriptor> {
    PREVIEW_FIGURES
        .iter()
        .chain(RESULT_FIGURES.iter())
        .chain(SCREENING_FIGURES.iter())
        .find(|f| f.id == id)
}
