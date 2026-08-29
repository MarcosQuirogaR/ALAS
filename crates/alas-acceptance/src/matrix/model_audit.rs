// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Detailed model observables retained for aircraft-correlation audits.
//!
//! The ordinary acceptance summary intentionally stays small. Correlation
//! work needs the intermediate geometry, polar, mass, payload, and structural
//! outputs as well, otherwise a later report cannot distinguish an input-data
//! discrepancy from a solver discrepancy.

use std::collections::BTreeMap;

use alas_config::AlasConfig;
use alas_pipeline::full_analysis::AnalysisReport;
use alas_pipeline::PipelineResult;
use serde::Serialize;

/// Machine-readable observables from one preset's model evaluation.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PresetModelAudit {
    /// Built exterior geometry and transport-planform stations.
    pub geometry: GeometryAudit,
    /// Cruise-point and fitted-polar quantities.
    pub aerodynamics: AerodynamicsAudit,
    /// Detailed component mass and balance output.
    pub mass_balance: MassBalanceAudit,
    /// Internal wingbox sizing and analytical response.
    pub structures: StructuresAudit,
    /// Payload and installed-cabin assumptions.
    pub payload: PayloadAudit,
    /// Numerical and operating-point settings that govern the results.
    pub settings: SettingsAudit,
}

/// Built geometry metrics.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GeometryAudit {
    /// Design variables used by the builder.
    pub design_vector: serde_json::Value,
    /// Scalar geometry values published by the full analysis.
    pub summary: BTreeMap<String, f64>,
    /// Resolved transport-planform stations and panels.
    pub transport_planform: serde_json::Value,
    /// Maximum fuselage width.
    pub fuselage_width_m: f64,
    /// Maximum fuselage height.
    pub fuselage_height_m: f64,
}

/// Aerodynamic observables.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AerodynamicsAudit {
    /// Geometric body angle at the trimmed cruise point.
    pub geometric_body_alpha_deg: Option<f64>,
    /// Compressibility-corrected display angle at the trimmed cruise point.
    pub reported_alpha_deg: Option<f64>,
    /// Trimmed horizontal-tail incidence.
    pub trim_incidence_deg: Option<f64>,
    /// Lift coefficient at the reported cruise point.
    pub cl: f64,
    /// Drag coefficient at the reported cruise point.
    pub cd: f64,
    /// Lift-to-drag ratio at the reported cruise point.
    pub l_over_d: f64,
    /// Fitted zero-lift drag coefficient.
    pub cd0: f64,
    /// Fitted induced-drag factor.
    pub induced_drag_k: f64,
    /// Fitted Oswald efficiency.
    pub oswald_e: f64,
    /// Static margin using the model mass state.
    pub static_margin: f64,
    /// Model neutral-point station.
    pub neutral_point_x_m: f64,
    /// Number of points in the retained polar.
    pub polar_points: usize,
}

/// Mass and center-of-gravity observables.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MassBalanceAudit {
    /// Component masses keyed by the model's stable labels.
    pub component_masses_kg: BTreeMap<String, f64>,
    /// Component centroids keyed by the model's stable labels.
    pub component_centroids_m: BTreeMap<String, [f64; 3]>,
    /// Overall physical center of gravity.
    pub physical_cg_m: [f64; 3],
}

/// Internal wingbox outputs.
#[derive(Debug, Clone, Default, Serialize)]
pub struct StructuresAudit {
    /// Structural stage state.
    pub status: String,
    /// Sized semi-wing primary-structure mass.
    pub wingbox_mass_kg: Option<f64>,
    /// Empirical whole-wing mass used for comparison.
    pub torenbeek_wing_mass_kg: Option<f64>,
    /// Number of modeled spars.
    pub spar_count: Option<usize>,
    /// Spar positions as fractions of local chord.
    pub spar_chord_fractions: Vec<f64>,
    /// Number of modeled ribs.
    pub rib_count: Option<i64>,
    /// Sized rib pitch.
    pub rib_spacing_m: Option<f64>,
    /// Skin thickness.
    pub skin_thickness_m: Option<f64>,
    /// Analytical tip deflection by load case.
    pub tip_deflection_m: BTreeMap<String, f64>,
    /// Largest absolute analytical spar stress.
    pub max_abs_spar_stress_pa: Option<f64>,
    /// Lowest finite analytical margin of safety.
    pub minimum_margin_of_safety: Option<f64>,
    /// First analytical bending-mode frequency.
    pub first_bending_frequency_hz: Option<f64>,
    /// Finite-element solver status, if requested.
    pub nastran_status: Option<String>,
}

/// Payload inputs and resolved layout status.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PayloadAudit {
    /// Requested planning passenger count.
    pub requested_passengers: i64,
    /// Named cabin layout.
    pub cabin_preset: String,
    /// Whether a detailed payload layout was built.
    pub detailed_layout_available: bool,
    /// Maximum structural payload input.
    pub maximum_structural_payload_kg: f64,
    /// Occupant-plus-baggage mass assumption.
    pub passenger_mass_kg: f64,
}

/// Settings needed to interpret and reproduce numerical results.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SettingsAudit {
    /// Cruise Mach number.
    pub cruise_mach: f64,
    /// Cruise altitude.
    pub cruise_altitude_m: f64,
    /// Final polar alpha lower bound.
    pub polar_alpha_min_deg: f64,
    /// Final polar alpha upper bound.
    pub polar_alpha_max_deg: f64,
    /// Requested final polar sample count.
    pub polar_sample_count: i64,
    /// Fine spanwise VLM resolution multiplier.
    pub fine_spanwise_resolution: i64,
    /// Fine chordwise VLM resolution multiplier.
    pub fine_chordwise_resolution: i64,
    /// Whether analytical/FE structural analysis was enabled.
    pub structures_enabled: bool,
    /// Search method saved in the effective configuration.
    pub optimization_method: String,
}

pub(super) fn extract(
    config: &AlasConfig,
    full: &AnalysisReport,
    result: &PipelineResult,
) -> PresetModelAudit {
    let transport_planform = config
        .geometry
        .wing
        .transport_planform(&full.design)
        .ok()
        .and_then(|planform| serde_json::to_value(planform).ok())
        .unwrap_or(serde_json::Value::Null);
    let geometry = GeometryAudit {
        design_vector: serde_json::to_value(full.design).unwrap_or(serde_json::Value::Null),
        summary: full
            .geometry_summary
            .iter()
            .map(|(name, value)| (name.clone(), *value))
            .collect(),
        transport_planform,
        fuselage_width_m: config.geometry.fuselage.diameter_m,
        fuselage_height_m: config.geometry.fuselage.effective_height_m(),
    };
    let trimmed = full.trimmed_design_point;
    let aerodynamics = AerodynamicsAudit {
        geometric_body_alpha_deg: trimmed.map(|point| point.geometric_body_alpha_deg),
        reported_alpha_deg: trimmed.map(|point| point.alpha_deg),
        trim_incidence_deg: trimmed.map(|point| point.trim_ih_deg),
        cl: trimmed.map_or(full.design_point.cl, |point| point.cl),
        cd: trimmed.map_or(full.design_point.cd, |point| point.cd),
        l_over_d: trimmed.map_or(full.design_point.l_over_d, |point| point.l_over_d),
        cd0: full.polar_fit.cd0,
        induced_drag_k: full.polar_fit.k,
        oswald_e: full.polar_fit.oswald_e,
        static_margin: full.static_margin,
        neutral_point_x_m: full.x_neutral_point,
        polar_points: full.polar.alpha_deg.len(),
    };
    let mass_balance = MassBalanceAudit {
        component_masses_kg: full
            .component_masses
            .iter()
            .map(|(name, value)| (name.clone(), *value))
            .collect(),
        component_centroids_m: full
            .mass_coordinates
            .iter()
            .map(|(name, value)| (name.clone(), *value))
            .collect(),
        physical_cg_m: full.physical_cg,
    };
    let structures = extract_structures(result);
    let payload = PayloadAudit {
        requested_passengers: config.requirements.num_passengers,
        cabin_preset: config.requirements.cabin_preset.clone(),
        detailed_layout_available: full.payload_layout.is_some(),
        maximum_structural_payload_kg: config.requirements.max_structural_payload_kg,
        passenger_mass_kg: config.requirements.passenger_mass_kg,
    };
    let settings = SettingsAudit {
        cruise_mach: config.requirements.cruise_mach,
        cruise_altitude_m: config.requirements.cruise_altitude_m,
        polar_alpha_min_deg: config.analysis.sweep_alpha_min_deg,
        polar_alpha_max_deg: config.analysis.sweep_alpha_max_deg,
        polar_sample_count: config.analysis.sweep_n_points,
        fine_spanwise_resolution: config.analysis.fine_spanwise_resolution,
        fine_chordwise_resolution: config.analysis.fine_chordwise_resolution,
        structures_enabled: config.structures.enabled,
        optimization_method: config.optimizer.solver.method.clone(),
    };
    PresetModelAudit {
        geometry,
        aerodynamics,
        mass_balance,
        structures,
        payload,
        settings,
    }
}

fn extract_structures(result: &PipelineResult) -> StructuresAudit {
    let Some(structural) = result.structural_result.as_ref() else {
        return StructuresAudit::default();
    };
    let mut output = StructuresAudit {
        status: structural.status.clone(),
        torenbeek_wing_mass_kg: structural
            .torenbeek_wing_mass_kg
            .is_finite()
            .then_some(structural.torenbeek_wing_mass_kg),
        nastran_status: structural
            .nastran
            .as_ref()
            .map(|nastran| format!("{:?}", nastran.static_solve.status)),
        ..StructuresAudit::default()
    };
    if let Some(sizing) = structural.sizing.as_ref() {
        output.wingbox_mass_kg = Some(sizing.total_mass_kg);
        output.spar_count = Some(sizing.spars.len());
        output.spar_chord_fractions = sizing.spar_fracs.clone();
        output.rib_count = Some(sizing.num_ribs);
        output.rib_spacing_m = Some(sizing.installed_rib_spacing_m());
        output.skin_thickness_m = Some(sizing.t_skin);
    }
    if let Some(analysis) = structural.analysis.as_ref() {
        let mut max_stress = 0.0_f64;
        let mut minimum_margin = f64::INFINITY;
        for load_case in &analysis.load_cases {
            output
                .tip_deflection_m
                .insert(load_case.name.to_owned(), load_case.tip_deflection_m);
            for spar in &load_case.spar_stress {
                for stress in &spar.stress_pa {
                    max_stress = max_stress.max(stress.abs());
                }
                for margin in &spar.margin_of_safety {
                    if margin.is_finite() {
                        minimum_margin = minimum_margin.min(*margin);
                    }
                }
            }
        }
        output.max_abs_spar_stress_pa = Some(max_stress);
        output.minimum_margin_of_safety = minimum_margin.is_finite().then_some(minimum_margin);
        output.first_bending_frequency_hz = analysis.modal.frequencies_hz.first().copied();
    }
    output
}
