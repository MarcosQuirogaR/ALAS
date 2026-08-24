// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared hierarchy for schema-driven advanced-settings pages.
//!
//! The configuration schema owns values, bounds, labels, and help. This map
//! only groups related controls into scan-friendly cards so dense pages do not
//! present an undifferentiated wall of inputs.

use std::collections::HashSet;

use alas_config::Field;
use egui::{RichText, Ui};
use serde_json::Value;

use crate::views::form::{dynamic_form, FormEdit};
use crate::views::tr;

/// A visual subsection of a previously flat configuration page.
pub(super) struct PageSection {
    title: &'static str,
    names: &'static [&'static str],
    default_open: bool,
}

/// Return the hierarchy for pages that do not have a bespoke editor.
pub(super) fn page_sections(group: &str) -> Option<&'static [PageSection]> {
    const MASS_MODEL: &[PageSection] = &[
        PageSection {
            title: "Mass and propulsion",
            names: &[
                "suspended_mass_fraction",
                "max_airspeed_for_flaps_ms",
                "flap_deflection_angle_deg",
                "landing_gear_mass_fraction",
                "propulsion_twr_factor",
                "propulsion_installation_factor",
                "propulsion_mass_fallback_fraction",
            ],
            default_open: true,
        },
        PageSection {
            title: "Systems, payload and fuel",
            names: &[
                "systems_mass_method",
                "systems_mass_fraction",
                "furnishings_mass_fraction",
                "cabin_payload_density_kg_m",
                "fuel_density_kg_m3",
                "fuel_tank_usable_fraction",
            ],
            default_open: true,
        },
        PageSection {
            title: "FLOPS transport model",
            names: &["flops_transport"],
            default_open: true,
        },
        PageSection {
            title: "Gear load limits",
            names: &[
                "nlg_x_fraction",
                "mlg_x_fraction_mac",
                "pct_load_nlg_max",
                "pct_load_mlg_max",
                "pct_load_nlg_min",
                "mlw_fraction_mtow",
            ],
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
            default_open: false,
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
            default_open: false,
        },
    ];
    const STRUCTURES: &[PageSection] = &[
        PageSection {
            title: "Wingbox layout",
            names: &[
                "enabled",
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
            default_open: false,
        },
        PageSection {
            title: "Ribs and mesh",
            names: &[
                "rib_buckling_coeff",
                "rib_radius_of_gyration_m",
                "num_ribs_override",
                "spanwise_stations",
                "mesh_chordwise_points",
            ],
            default_open: false,
        },
        PageSection {
            title: "Solver cases",
            names: &[
                "nastran_exe_path",
                "nastran_solver_path",
                "run_nastran",
                "run_sol_static",
                "run_sol_modes",
                "run_sol_vibration_sine",
                "run_sol_vibration_random",
                "timeout_s",
                "n_modes",
                "freq_sweep_max_hz",
                "freq_step_hz",
                "modal_damping_ratio",
                "psd_base_g2_per_hz",
                "patran_exe_path",
                "run_patran_export",
            ],
            default_open: false,
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
            default_open: false,
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
            default_open: false,
        },
        PageSection {
            title: "Polar fit",
            names: &[
                "polar_fit_cl_min",
                "polar_fit_cl_max",
                "polar_fit_cl_min_fallback",
                "polar_fit_cl_max_fallback",
            ],
            default_open: false,
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
            default_open: false,
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
            default_open: false,
        },
        PageSection {
            title: "Approach speed schedule",
            names: &["vapp_vstall_land_factor", "vtd_vstall_land_factor"],
            default_open: false,
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
            default_open: false,
        },
    ];

    match group {
        "mass_model" => Some(MASS_MODEL),
        "landing_gear" => Some(LANDING_GEAR),
        "control_surfaces" => Some(CONTROL_SURFACES),
        "structures" => Some(STRUCTURES),
        "analysis" => Some(ANALYSIS),
        "performance" => Some(PERFORMANCE),
        "mses" => Some(MSES),
        _ => None,
    }
}

/// Render an explicitly grouped form while leaving the schema authoritative.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_sectioned_form(
    ui: &mut Ui,
    group: &str,
    fields: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    sections: &[PageSection],
) -> Vec<FormEdit> {
    let mut edits = Vec::new();
    for section in sections {
        let section_fields: Vec<Field> = fields
            .iter()
            .filter(|field| section.names.contains(&field.name))
            .cloned()
            .map(|mut field| {
                // The outer section is the page's explicit hierarchy. Do not
                // put an "Advanced" accordion immediately inside it, which
                // would split one concept across two unrelated menus.
                field.advanced = false;
                field
            })
            .collect();
        if section_fields.is_empty() {
            continue;
        }
        crate::theme::card_frame(ui).show(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new(tr(section.title)).strong())
                .id_salt(format!("{group}::{}", section.title))
                .default_open(section.default_open)
                .show(ui, |ui| {
                    edits.extend(dynamic_form(
                        ui,
                        &section_fields,
                        values,
                        error_fields,
                        lang,
                        show_help,
                    ));
                });
        });
        ui.add_space(6.0);
    }

    let remaining: Vec<Field> = fields
        .iter()
        .filter(|field| {
            !sections
                .iter()
                .any(|section| section.names.contains(&field.name))
        })
        .cloned()
        .collect();
    if !remaining.is_empty() {
        crate::theme::card_frame(ui).show(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new(tr("Additional settings")).strong())
                .id_salt(format!("{group}::additional"))
                .default_open(false)
                .show(ui, |ui| {
                    edits.extend(dynamic_form(
                        ui,
                        &remaining,
                        values,
                        error_fields,
                        lang,
                        show_help,
                    ));
                });
        });
    }
    edits
}
