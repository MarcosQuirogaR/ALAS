// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/design_report.py
// Reference: alas @ rust-port-baseline.

//! Exporting design databases to JSON, Selig `.dat` files, and formatted summaries.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use alas_config::AlasConfig;
use alas_geom::airfoil_library::{build_section, AirfoilLibrary};
use alas_mass::breakdown::OEW_KEYS;
use serde::{Deserialize, Serialize};

use crate::feasibility::{
    CarriedFuelBasis, FeasibilityReport, FuelCapacityEvidence, MissionFuelStatus,
};
use crate::full_analysis::AnalysisReport;

/// Hierarchical export structure of the aircraft design database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignDatabase {
    /// Source and tool metadata.
    pub metadata: HashMap<String, String>,
    /// Optimized design variables.
    pub design_vector: serde_json::Value,
    /// Derived geometry metrics.
    pub geometry: HashMap<String, f64>,
    /// Aerodynamic operating point and polar characteristics.
    pub aerodynamics: serde_json::Value,
    /// Weights and mass properties.
    pub weights: serde_json::Value,
    /// Physical feasibility evidence from the evaluated load case.
    pub feasibility: serde_json::Value,
    /// Canonical CPACS aircraft reference for this compatibility report.
    #[serde(default)]
    pub cpacs: Option<CpacsReference>,
}

/// Link from the legacy JSON report to the authoritative CPACS aircraft file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpacsReference {
    /// CPACS file path, relative to the report output directory when possible.
    pub path: String,
    /// CPACS schema version used for the aircraft document.
    pub version: String,
    /// Stable aircraft model UID used by downstream tools.
    pub aircraft_model_uid: String,
    /// Stable engine UID used by downstream tools.
    pub engine_uid: String,
}

/// Convert an [`AnalysisReport`] and [`AlasConfig`] into the standard [`DesignDatabase`].
pub fn report_to_database(report: &AnalysisReport, config: &AlasConfig) -> DesignDatabase {
    let req = &config.requirements;
    let mut meta = HashMap::new();
    meta.insert("name".to_owned(), "ALAS optimized design".to_owned());
    meta.insert("source".to_owned(), "alas.pipeline".to_owned());
    meta.insert("tool".to_owned(), "ALAS".to_owned());

    let aero = serde_json::json!({
        "cruise_mach": req.cruise_mach,
        "cruise_altitude_m": req.cruise_altitude_m,
        "cd0_cruise": report.polar_fit.cd0,
        "k_factor": report.polar_fit.k,
        "oswald_efficiency": report.polar_fit.oswald_e,
        "aspect_ratio": report.polar_fit.aspect_ratio,
        "design_point": report.design_point,
        "static_margin": report.static_margin,
        "trimmed_design_point": report.trimmed_design_point,
    });

    let weights = serde_json::json!({
        "mtow_kg": req.mtow_kg,
        "systems_mass_method": config.mass_model.systems_mass_method,
        "systems_mass_status": match config.mass_model.systems_mass_method {
            alas_config::SystemsMassMethod::ReferenceCompatibleFractions => {
                "compatibility_only"
            }
            alas_config::SystemsMassMethod::FlopsTransportV1 => "verified_architecture",
        },
        "physical_cg_m": report.physical_cg,
        "component_masses_kg": report.component_masses,
        "mass_coordinates_m": report.mass_coordinates,
    });

    DesignDatabase {
        metadata: meta,
        design_vector: serde_json::to_value(report.design).unwrap_or_default(),
        geometry: report.geometry_summary.clone(),
        aerodynamics: aero,
        weights,
        feasibility: serde_json::Value::Null,
        cpacs: None,
    }
}

/// Add the load-case evidence that is unavailable until mission evaluation completes.
pub fn report_to_database_with_feasibility(
    report: &AnalysisReport,
    config: &AlasConfig,
    feasibility: &FeasibilityReport,
) -> DesignDatabase {
    let mut database = report_to_database(report, config);
    database.feasibility = feasibility_to_json(feasibility);
    database
}

/// Write the design database to `path` as formatted JSON.
pub fn export_json(
    report: &AnalysisReport,
    config: &AlasConfig,
    path: &Path,
) -> io::Result<DesignDatabase> {
    let data = report_to_database(report, config);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &data)?;
    Ok(data)
}

/// Write a design database with the evaluated fuel and cruise-force records.
pub fn export_json_with_feasibility(
    report: &AnalysisReport,
    config: &AlasConfig,
    feasibility: &FeasibilityReport,
    path: &Path,
) -> io::Result<DesignDatabase> {
    let data = report_to_database_with_feasibility(report, config, feasibility);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &data)?;
    Ok(data)
}

/// Write a compatibility report linked to its canonical CPACS aircraft.
pub fn export_json_with_feasibility_and_cpacs(
    report: &AnalysisReport,
    config: &AlasConfig,
    feasibility: &FeasibilityReport,
    cpacs: CpacsReference,
    path: &Path,
) -> io::Result<DesignDatabase> {
    let mut data = report_to_database_with_feasibility(report, config, feasibility);
    data.cpacs = Some(cpacs);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &data)?;
    Ok(data)
}

fn feasibility_to_json(report: &FeasibilityReport) -> serde_json::Value {
    let fuel = report.fuel_loading;
    let cruise_equilibrium = report.cruise_equilibrium.as_ref().map_or_else(
        || serde_json::json!({ "status": "not_evaluated" }),
        |assessment| {
            serde_json::json!({
                "status": if assessment.is_finite() { "finite" } else { "invalid" },
                "all_segments_converged": assessment.all_segments_converged,
                "control_points": assessment.control_points,
                "max_abs_thrust_minus_drag_n": assessment.max_abs_thrust_minus_drag_n,
                "max_abs_lift_minus_weight_n": assessment.max_abs_lift_minus_weight_n,
                "max_abs_longitudinal_residual_n": assessment.max_abs_longitudinal_residual_n,
                "max_abs_vertical_residual_n": assessment.max_abs_vertical_residual_n,
                "max_abs_residual_acceleration_m_s2": assessment
                    .max_abs_residual_acceleration_m_s2,
                "minimum_altitude_m": assessment.minimum_altitude_m,
                "maximum_altitude_m": assessment.maximum_altitude_m,
                "minimum_true_airspeed_m_s": assessment.minimum_true_airspeed_m_s,
                "maximum_true_airspeed_m_s": assessment.maximum_true_airspeed_m_s,
            })
        },
    );
    serde_json::json!({
        "fuel_loading": {
            "mtow_closure_fuel_kg": fuel.mtow_closure_fuel_kg,
            "usable_capacity_kg": fuel.usable_capacity.capacity_kg,
            "usable_capacity_evidence": fuel_capacity_evidence_name(
                fuel.usable_capacity.evidence,
            ),
            "analyzed_carried_fuel_kg": fuel.analyzed_carried_fuel_kg,
            "carried_fuel_basis": carried_fuel_basis_name(fuel.carried_fuel_basis),
            "zero_fuel_mass_kg": fuel.zero_fuel_mass_kg,
            "analyzed_takeoff_mass_kg": fuel.analyzed_takeoff_mass_kg,
            "mtow_shortfall_kg": fuel.mtow_shortfall_kg,
            "mission": {
                "status": mission_fuel_status_name(fuel.mission.status),
                "burned_fuel_kg": fuel.mission.burned_fuel_kg,
                "required_trip_fuel_kg": fuel.mission.required_trip_fuel_kg,
            },
        },
        "cruise_equilibrium": cruise_equilibrium,
    })
}

fn fuel_capacity_evidence_name(evidence: FuelCapacityEvidence) -> &'static str {
    match evidence {
        FuelCapacityEvidence::PublishedPreset => "published_preset",
        FuelCapacityEvidence::GeometryEstimate => "geometry_estimate",
        FuelCapacityEvidence::Unavailable => "unavailable",
    }
}

fn carried_fuel_basis_name(basis: CarriedFuelBasis) -> &'static str {
    match basis {
        CarriedFuelBasis::MtowMassClosure => "mtow_mass_closure",
        CarriedFuelBasis::UsableFuelCapacity => "usable_fuel_capacity",
        CarriedFuelBasis::CapacityUnverified => "capacity_unverified",
    }
}

fn mission_fuel_status_name(status: MissionFuelStatus) -> &'static str {
    match status {
        MissionFuelStatus::NotRequested => "not_requested",
        MissionFuelStatus::Unavailable => "unavailable",
        MissionFuelStatus::NotConverged => "not_converged",
        MissionFuelStatus::Completed => "completed",
        MissionFuelStatus::Exhausted => "exhausted",
    }
}

/// Reconstruct and save the optimized root section coordinates to a Selig `.dat` file.
pub fn export_airfoil_dat(
    report: &AnalysisReport,
    config: &AlasConfig,
    path: &Path,
    name: &str,
) -> io::Result<()> {
    let base_name = &config.geometry.wing.root_airfoil;
    let base_airfoil = AirfoilLibrary::get(base_name).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("airfoil {base_name} not found"),
        )
    })?;

    let section = build_section(&report.design, &base_airfoil.coordinates)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{e:?}")))?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = File::create(path)?;
    writeln!(file, "{name}")?;
    for pt in &section.coordinates {
        writeln!(file, "{:10.6} {:10.6}", pt.0, pt.1)?;
    }

    Ok(())
}

/// Format a concise multi-line summary of the design report for display.
pub fn format_summary(report: &AnalysisReport, config: Option<&AlasConfig>) -> String {
    let dp = &report.design_point;
    let pf = &report.polar_fit;
    let g = &report.geometry_summary;
    let m = &report.component_masses;

    let m_oew: f64 = OEW_KEYS
        .iter()
        .map(|&k| m.get(k).copied().unwrap_or(0.0))
        .sum();
    let m_total =
        m_oew + m.get("Payload").copied().unwrap_or(0.0) + m.get("Fuel").copied().unwrap_or(0.0);

    let mut lines = vec![
        "=========== ALAS Design Summary ===========".to_owned(),
        format!("  Cruise alpha (reported): {:6.2} deg", dp.alpha_deg),
        format!("  Cruise CL        : {:6.3}", dp.cl),
        format!("  Cruise CD        : {:7.5}", dp.cd),
        format!("  L/D (cruise)     : {:6.2}", dp.l_over_d),
        format!(
            "  Aero static marg : {:5.1} %",
            report.static_margin * 100.0
        ),
    ];

    if let Some(tdp) = &report.trimmed_design_point {
        lines.push(format!(
            "  Trimmed body alpha: {:6.2} deg  (reported = {:5.2} deg, h-stab i_h = {:5.2} deg, L/D = {:5.2})",
            tdp.geometric_body_alpha_deg, tdp.alpha_deg, tdp.trim_ih_deg, tdp.l_over_d
        ));
    }

    lines.push("  ------------------ Weights -------------------".to_owned());
    if let Some(cfg) = config {
        lines.push(format!(
            "  MTOW (specified) : {:>10.0} kg",
            cfg.requirements.mtow_kg
        ));
    }
    lines.extend([
        format!("  MTOW (computed)  : {:>10.0} kg", m_total),
        format!("  OEW              : {:>10.0} kg", m_oew),
        format!(
            "  Systems mass     : {:?} ({})",
            config
                .map(|cfg| cfg.mass_model.systems_mass_method)
                .unwrap_or(alas_config::SystemsMassMethod::ReferenceCompatibleFractions),
            config
                .map(|cfg| match cfg.mass_model.systems_mass_method {
                    alas_config::SystemsMassMethod::ReferenceCompatibleFractions => {
                        "compatibility-only fractions"
                    }
                    alas_config::SystemsMassMethod::FlopsTransportV1 => "FLOPS architecture",
                })
                .unwrap_or("compatibility-only fractions")
        ),
        format!(
            "  Payload          : {:>10.0} kg",
            m.get("Payload").copied().unwrap_or(0.0)
        ),
        format!(
            "  Fuel             : {:>10.0} kg",
            m.get("Fuel").copied().unwrap_or(0.0)
        ),
        format!(
            "  Wing structure   : {:>10.0} kg",
            m.get("Wing").copied().unwrap_or(0.0)
        ),
        format!(
            "  Fuselage struct  : {:>10.0} kg",
            m.get("Fuselage").copied().unwrap_or(0.0)
        ),
        format!("  Physical CG X    : {:6.2} m", report.physical_cg[0]),
        "  ----------------------------------------------".to_owned(),
        format!("  CD0 (clean fit)  : {:7.5}", pf.cd0),
        format!("  k factor         : {:6.4}", pf.k),
        format!("  Oswald e         : {:6.3}", pf.oswald_e),
        format!("  Aspect ratio     : {:6.2}", pf.aspect_ratio),
        "  ----------------------------------------------".to_owned(),
        format!(
            "  Span             : {:6.2} m",
            g.get("span_m").copied().unwrap_or(0.0)
        ),
        format!(
            "  Wing area        : {:6.1} m^2",
            g.get("wing_area_m2").copied().unwrap_or(0.0)
        ),
        format!(
            "  Taper ratio      : {:6.3}",
            g.get("taper_ratio").copied().unwrap_or(0.0)
        ),
        format!(
            "  Root / break / tip chord : {:5.2} / {:5.2} / {:5.2} m",
            g.get("root_chord_m").copied().unwrap_or(0.0),
            g.get("break_chord_m").copied().unwrap_or(0.0),
            g.get("tip_chord_m").copied().unwrap_or(0.0),
        ),
        format!(
            "  Wing break       : {:5.2} m semispan; sweep {:4.1} -> {:4.1} deg",
            g.get("break_span_m").copied().unwrap_or(0.0),
            g.get("inboard_sweep_deg").copied().unwrap_or(0.0),
            g.get("outboard_sweep_deg").copied().unwrap_or(0.0),
        ),
        format!(
            "  Sweep            : {:6.1} deg",
            g.get("sweep_deg").copied().unwrap_or(0.0)
        ),
        format!(
            "  Fuselage length  : {:6.2} m",
            g.get("fuselage_length_m").copied().unwrap_or(0.0)
        ),
        "================================================".to_owned(),
    ]);

    lines.join("\n")
}
