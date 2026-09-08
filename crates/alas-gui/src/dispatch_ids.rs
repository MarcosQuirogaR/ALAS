// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Static dispatch inventories used to audit GUI/report registry coverage.
//!
//! Keeping these lists beside the dispatch module makes adding a descriptor a
//! compile-visible review step while avoiding a second dynamic registry.

/// IDs handled by the live preview match.
pub const PREVIEW_DISPATCH_IDS: &[&str] = &[
    "cabin_3d",
    "geometry",
    "exterior_3d",
    "engine",
    "structures",
    "drag",
    "mass_cg",
    "landing_gear",
    "control_surfaces",
];

/// IDs handled by the result-figure match.
pub const RESULT_DISPATCH_IDS: &[&str] = &[
    "propulsion_cycle_summary",
    "propulsion_carpet_plot",
    "propulsion_efficiency_decomposition",
    "propulsion_bpr_sensitivity",
    "propulsion_altitude_sweep",
    "matching_chart",
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
    "openvsp_cad_preview",
    "aero_panel",
    "span_loading",
    "vlm_flow",
    "drag_breakdown",
    "airfoil_reynolds",
    "mses_pressure",
    "mses_mach_contours",
    "mses_convergence",
    "vspaero_polar",
    "vspaero_wake_convergence",
    "vspaero_load_distribution",
    "mass_breakdown",
    "fuel_volume_check",
    "mass_distribution",
    "dynamic_modes",
    "cg_envelope",
    "landing_gear_planform",
    "control_surfaces",
    "stability_side_view",
    "cabin_payload",
    "cabin_section",
    "payload_range",
    "lto_departure",
    "lto_arrival",
    "mission_profile",
    "mission_velocities",
    "mission_flight_path",
    "mission_aero_coefficients",
    "mission_aero_forces",
    "mission_drag_components",
    "vn_diagram",
    "structures_sizing",
    "structures_loads",
    "structures_stress",
    "structures_modes",
    "structures_vibration",
    "structures_patran",
    "mission_route_2d",
    "mission_route_3d",
    "model_comparison",
];

/// IDs handled by the standalone screening match.
pub const SCREENING_DISPATCH_IDS: &[&str] = &[
    "trade_map",
    "rerank_2d_3d",
    "ranking_bars",
    "mses_verification",
    "section_shapes",
];
