// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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
    // Native external geometry evidence
    descriptor(
        "openvsp_cad_preview",
        "OpenVSP CAD Preview",
        "Geometry",
        "Native transparent CAD preview materialized by OpenVSP from the retained VSP3 project.",
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
    descriptor(
        "mses_convergence",
        "MSES Sweep Convergence",
        "Aerodynamics",
        "Converged and non-converged MSES operating points with retained solver evidence.",
    ),
    descriptor(
        "vspaero_polar",
        "VSPAERO Native Polar",
        "Aerodynamics",
        "Native VSPAERO lift, drag, moment, and efficiency data, including rejected comparisons.",
    ),
    descriptor(
        "vspaero_wake_convergence",
        "VSPAERO Wake Convergence",
        "Aerodynamics",
        "Final native wake residual and iteration count for every VSPAERO angle case.",
    ),
    descriptor(
        "vspaero_load_distribution",
        "VSPAERO Native Load Distribution",
        "Aerodynamics",
        "Native VSPAERO sectional lift and induced-drag loading from the retained LOD export.",
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
    descriptor(
        "cabin_section",
        "Cabin Cross-Section",
        "Weight & Balance",
        "Representative transverse section showing decks, seats, aisles, overhead bins, and lower-hold loading.",
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
