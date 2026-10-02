// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The per-page section table: which fields of each configuration group share a card.

use super::PageSection;
use crate::nav::Surface;

/// Return the hierarchy for pages that do not have a bespoke editor.
pub(in crate::views::form_page) fn page_sections(
    group: &str,
    surface: Surface,
) -> Option<&'static [PageSection]> {
    const MASS_MODEL: &[PageSection] = &[
        PageSection {
            title: "Mass architecture",
            names: &["mass_architecture", "geometric_component_stations"],
            default_open: true,
        },
        PageSection {
            title: "Compatibility mass fractions",
            names: &[
                "landing_gear_mass_fraction",
                "propulsion_mass_fallback_fraction",
                "systems_mass_fraction",
                "furnishings_mass_fraction",
            ],
            default_open: true,
        },
        PageSection {
            title: "Payload, fuel and stations",
            names: &[
                "cabin_payload_density_kg_m",
                "fuel_density_kg_m3",
                "fuel_tank_usable_fraction",
            ],
            default_open: true,
        },
    ];
    const MASS_MODEL_ADVANCED: &[PageSection] = &[
        PageSection {
            title: "FLOPS transport inputs and technology",
            names: &["flops_transport", "flops_structure"],
            default_open: true,
        },
        PageSection {
            title: "Flops turboprop",
            names: &["flops_turboprop"],
            default_open: true,
        },
        PageSection {
            title: "High-lift mass loads",
            names: &[
                "suspended_mass_fraction",
                "max_airspeed_for_flaps_ms",
                "flap_deflection_angle_deg",
            ],
            default_open: true,
        },
    ];
    const DRAG_MODEL: &[PageSection] = &[
        PageSection {
            title: "Parasite drag build-up",
            names: &[
                "exclude_buried_main_wing_area",
                "max_thickness_chordwise_loc",
                "interference_factor_wing",
                "interference_factor_fuselage",
                "interference_factor_nacelle",
                "viscous_margin",
            ],
            default_open: true,
        },
        PageSection {
            title: "Wave drag",
            names: &["wave_drag_onset_mach", "wave_drag_coefficient"],
            default_open: true,
        },
    ];
    const OPTIMIZER: &[PageSection] = &[
        PageSection {
            title: "Objective and requirements",
            names: &["objective"],
            default_open: true,
        },
        PageSection {
            title: "Differential evolution settings",
            names: &["solver"],
            default_open: true,
        },
        // Two questions the objective does not answer: where this program's
        // own correlations stop being trustworthy, and whether an
        // overconstrained problem may miss a limit at all. Both are closed
        // by default because neither belongs in a routine run.
        PageSection {
            title: "Model validity domain",
            names: &["plausibility"],
            default_open: false,
        },
        PageSection {
            title: "Controlled constraint relaxation",
            names: &["relaxation"],
            default_open: false,
        },
    ];
    const LANDING_GEAR: &[PageSection] = &[
        PageSection {
            title: "Automatic sizing",
            names: &[
                "tire_safety_factor",
                "n_nlg_wheels",
                "nlg_dual_wheel_mtow_kg",
                "n_mlg_struts",
                "mlg_body_gear_mtow_kg",
                "wheels_per_mlg_strut",
                "tire_class",
                "strut_material",
            ],
            default_open: true,
        },
        PageSection {
            title: "Geometry and stability",
            names: &["track_diameter_factor", "turnover_angle_limit_deg"],
            default_open: true,
        },
    ];
    const CONTROL_SURFACES: &[PageSection] = &[
        PageSection {
            title: "Leading edge slats",
            names: &[
                "slat_chord_fraction",
                "slat_span_start_frac",
                "slat_span_end_frac",
            ],
            default_open: true,
        },
        PageSection {
            title: "Trailing edge flaps",
            names: &[
                "flap_chord_fraction",
                "flap_span_start_frac",
                "flap_span_end_frac",
            ],
            default_open: true,
        },
        PageSection {
            title: "Roll and lift dump",
            names: &[
                "aileron_chord_fraction",
                "aileron_span_start_frac",
                "aileron_span_end_frac",
                "spoiler_chord_fraction",
                "spoiler_span_start_frac",
                "spoiler_span_end_frac",
            ],
            default_open: true,
        },
        PageSection {
            title: "Tail surfaces",
            names: &[
                "elevator_chord_fraction",
                "elevator_span_start_frac",
                "elevator_span_end_frac",
                "rudder_chord_fraction",
                "rudder_span_start_frac",
                "rudder_span_end_frac",
            ],
            default_open: true,
        },
    ];
    const STRUCTURES: &[PageSection] = &[
        PageSection {
            title: "Wingbox layout",
            names: &[
                "spar_chord_fractions",
                "te_rib_mode",
                "center_spar_enabled",
                "center_spar_chord_fraction",
            ],
            default_open: true,
        },
        PageSection {
            title: "Materials",
            names: &[
                "skin_material",
                "spar_web_material",
                "spar_cap_material",
                "rib_material",
            ],
            default_open: true,
        },
        PageSection {
            title: "Gauges and taper",
            names: &[
                "additional_safety_factor",
                "t_skin_min_m",
                "t_web_min_m",
                "t_rib_m",
                "t_te_strip_m",
                "cap_taper_eta_lock",
                "cap_taper_tip_fraction",
            ],
            default_open: true,
        },
    ];
    const STRUCTURES_ADVANCED: &[PageSection] = &[
        PageSection {
            title: "Ribs and mesh",
            names: &[
                "rib_buckling_coeff",
                "rib_radius_of_gyration_m",
                "num_ribs_override",
                "spanwise_stations",
                "mesh_chordwise_points",
            ],
            default_open: true,
        },
        PageSection {
            title: "Solver limits",
            names: &[
                "timeout_s",
                "n_modes",
                "freq_sweep_max_hz",
                "freq_step_hz",
                "modal_damping_ratio",
            ],
            default_open: true,
        },
        PageSection {
            title: "Random excitation",
            names: &["random_force_psd_n2_per_hz"],
            default_open: true,
        },
    ];
    const ANALYSIS: &[PageSection] = &[
        PageSection {
            title: "Cruise sweep",
            names: &[
                "sweep_alpha_min_deg",
                "sweep_alpha_max_deg",
                "sweep_n_points",
            ],
            default_open: true,
        },
        PageSection {
            title: "Fast VLM mesh",
            names: &["spanwise_resolution", "chordwise_resolution"],
            default_open: true,
        },
        PageSection {
            title: "Fine VLM mesh",
            names: &["fine_spanwise_resolution", "fine_chordwise_resolution"],
            default_open: true,
        },
        PageSection {
            title: "Trim and stability",
            names: &[
                "probe_alpha_low_deg",
                "probe_alpha_high_deg",
                "trim_incidence_probe_delta_deg",
                "autobalance_velocity_m_s",
                "autobalance_alpha_low_deg",
                "autobalance_alpha_high_deg",
                "tail_efficiency",
                "include_fuselage_stability",
            ],
            default_open: true,
        },
        PageSection {
            title: "Polar fit",
            names: &[
                "polar_fit_cl_min",
                "polar_fit_cl_max",
                "polar_fit_cl_min_fallback",
                "polar_fit_cl_max_fallback",
            ],
            default_open: true,
        },
    ];
    const PERFORMANCE: &[PageSection] = &[
        PageSection {
            title: "High lift",
            names: &["cl_max_to", "cl_max_land", "cl_max_clean", "cl_min_clean"],
            default_open: true,
        },
        PageSection {
            title: "Engine-out and landing",
            names: &[
                "thrust_lapse",
                "oei_gradient",
                "k_land",
                "oei_climb_cl",
                "oei_climb_delta_cd",
                "oei_condition_to_sls_thrust_ratio",
                "oei_asymmetric_trim_cd",
                "oei_windmilling_cd",
            ],
            default_open: true,
        },
        PageSection {
            title: "Matching chart",
            names: &[
                "ws_min_pa",
                "ws_max_pa",
                "bfl_factor",
                "matching_chart_resolution",
            ],
            default_open: true,
        },
        PageSection {
            title: "Take-off speed schedule",
            names: &[
                "vmc_vstall_factor",
                "vr_vmc_factor",
                "vr_vstall_factor",
                "v2_vstall_factor",
                "v1_vr_factor",
            ],
            default_open: true,
        },
        PageSection {
            title: "Approach speed schedule",
            names: &["vapp_vstall_land_factor", "vtd_vstall_land_factor"],
            default_open: true,
        },
    ];
    const MSES: &[PageSection] = &[
        PageSection {
            title: "Operating point",
            names: &[
                "n_crit",
                "xtr_upper",
                "xtr_lower",
                "alpha_sweep_halfwidth_deg",
                "alpha_sweep_n_points",
            ],
            default_open: true,
        },
        PageSection {
            title: "Solver safeguards",
            names: &["timeout_mset_s", "timeout_mses_s", "max_iterations"],
            default_open: true,
        },
        PageSection {
            title: "Mesh resolution",
            names: &["mset_n", "mset_e"],
            default_open: true,
        },
    ];

    // Mission Analysis: whether and how long the stage runs, then how the
    // route is built. The phase launcher card follows these sections.
    const MISSION_ADVANCED: &[PageSection] = &[
        PageSection {
            title: "Mission run",
            names: &["enabled", "timeout_s"],
            default_open: true,
        },
        PageSection {
            title: "Route construction",
            names: &[
                "great_circle_points",
                "max_airway_stretch",
                "use_airway_endpoint_coordinates",
                "simbrief_username",
                "simbrief_timeout_s",
                "simbrief_overrides_airports",
            ],
            default_open: true,
        },
    ];

    match (group, surface) {
        ("mission", Surface::Advanced) => Some(MISSION_ADVANCED),
        ("mass_model", Surface::Advanced) => Some(MASS_MODEL_ADVANCED),
        ("mass_model", _) => Some(MASS_MODEL),
        ("drag_model", _) => Some(DRAG_MODEL),
        ("optimizer", _) => Some(OPTIMIZER),
        ("landing_gear", _) => Some(LANDING_GEAR),
        ("control_surfaces", _) => Some(CONTROL_SURFACES),
        ("structures", Surface::Advanced) => Some(STRUCTURES_ADVANCED),
        ("structures", _) => Some(STRUCTURES),
        ("analysis", _) => Some(ANALYSIS),
        ("performance", _) => Some(PERFORMANCE),
        ("mses", _) => Some(MSES),
        _ => None,
    }
}
