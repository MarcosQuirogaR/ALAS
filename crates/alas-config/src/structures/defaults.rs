// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Default structures configuration.

use super::*;

impl Default for StructuresConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            // Within the transport-wing ranges of Raymer, Aircraft Design: A
            // Conceptual Approach, 6th ed., ch. 8 (front spar 20-30 %, rear spar
            // 60-75 % of chord, p. 227) and Torenbeek, Advanced Aircraft Design
            // (2013), p. 179 (box 15-65 % of chord) / p. 322 (box 45-50 % of chord).
            spar_chord_fractions: vec![0.25, 0.70],
            te_rib_mode: "all".to_owned(),
            center_spar_enabled: false,
            center_spar_chord_fraction: 0.50,
            skin_material: "Al 7075-T6".to_owned(),
            spar_web_material: "Al 7075-T6".to_owned(),
            spar_cap_material: "CFRP UD".to_owned(),
            rib_material: "Al 7075-T6".to_owned(),
            additional_safety_factor: 1.0,
            max_linear_curvature_relative_error: default_linear_curvature_error(),
            lower_cover_fatigue_stress_1g_pa: 0.0,
            t_skin_min_m: 0.006,
            t_web_min_m: 0.002,
            t_rib_m: 0.004,
            t_te_strip_m: 0.002,
            cap_taper_eta_lock: 0.40,
            cap_taper_tip_fraction: 0.20,
            rib_buckling_coeff: 1.5,
            rib_radius_of_gyration_m: 0.030,
            num_ribs_override: None,
            spanwise_stations: 200,
            mesh_chordwise_points: 50,
            nastran_exe_path: String::new(),
            nastran_solver_path: String::new(),
            nastran95_dir_path: String::new(),
            nastran95_runtime_path: String::new(),
            nastran95_rf_stage_path: String::new(),
            nastran95_open_core_words: String::new(),
            run_nastran: true,
            run_sol_static: true,
            run_sol_modes: true,
            // SOL 111 is part of the default structural response set; the
            // NASTRAN adapter still honors explicit user/reference limits.
            run_sol_vibration_sine: true,
            // A unit force PSD makes SOL 111 immediately observable; a user
            // can replace it with the aircraft-specific excitation level.
            run_sol_vibration_random: true,
            timeout_s: 3600.0,
            nastran_memory_mb: default_nastran_memory_mb(),
            // Sixteen modes is the smallest full-mesh NASTRAN-95 request
            // validated to retain all four active Rayleigh target matches.
            n_modes: 16,
            freq_sweep_max_hz: 60.0,
            freq_step_hz: 1.0,
            modal_damping_ratio: 0.02,
            random_force_psd_n2_per_hz: 1.0,
            psd_base_g2_per_hz: 0.01,
            patran_exe_path: String::new(),
            run_patran_export: true,
        }
    }
}
