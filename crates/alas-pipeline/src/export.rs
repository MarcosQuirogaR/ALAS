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
        "polar_fit_status": report.polar_fit.status.as_str(),
        "design_point": report.design_point,
        "static_margin": report.static_margin,
        "trimmed_design_point": report.trimmed_design_point,
    });

    let weights = serde_json::json!({
        "mtow_kg": req.mtow_kg,
        "mtow_limit_kg": report
            .geometry_summary
            .get("analysis_mtow_limit_kg")
            .copied()
            .unwrap_or(req.mtow_kg),
        "analysis_mass_basis_kg": report
            .geometry_summary
            .get("analysis_mass_basis_kg")
            .copied()
            .unwrap_or(req.mtow_kg),
        "analysis_mass_basis": if report
            .geometry_summary
            .get("analysis_mass_basis_is_sized")
            .is_some_and(|value| *value > 0.5)
        {
            "mission_sized_finalist"
        } else {
            "configured_mtow"
        },
        "mass_architecture": config.mass_model.mass_architecture.as_str(),
        "systems_mass_method": config.mass_model.systems_mass_method,
        "systems_mass_status": if config.mass_model.mass_architecture.is_pure_flops() {
            "verified_architecture"
        } else {
            "compatibility_only"
        },
        "flops_mass_buildup": report
            .flops_mass_buildup
            .as_deref()
            .map(flops_mass_buildup_to_json),
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

/// Export the grouped FLOPS result that produced the report's lumped slots.
///
/// The equation inputs and airframe breakdown are intentionally kept as
/// separate objects: the former records the resolved SI quantities and the
/// latter records the ownership boundary used by the ledger.  Provenance is
/// copied verbatim from the completed evaluation so a user-declared scenario
/// cannot be mistaken for source-backed aircraft data downstream.
fn flops_mass_buildup_to_json(
    buildup: &alas_mass::breakdown::FlopsMassBuildup,
) -> serde_json::Value {
    let systems = &buildup.systems_and_operating_items.systems;
    let operating = &buildup.systems_and_operating_items.operating_items;
    let airframe = &buildup.airframe;
    serde_json::json!({
        "provenance": &*buildup.provenance,
        "inputs": {
            "maximum_mach": buildup.inputs.maximum_mach,
            "design_range_nmi": buildup.inputs.design_range_nmi,
            "design_gross_mass_kg": buildup.inputs.design_gross_mass_kg,
            "wing_area_m2": buildup.inputs.wing_area_m2,
            "movable_surface_area_m2": buildup.inputs.movable_surface_area_m2,
            "wing_span_m": buildup.inputs.wing_span_m,
            "quarter_chord_sweep_deg": buildup.inputs.quarter_chord_sweep_deg,
            "fuselage_length_m": buildup.inputs.fuselage_length_m,
            "fuselage_width_m": buildup.inputs.fuselage_width_m,
            "fuselage_depth_m": buildup.inputs.fuselage_depth_m,
            "fuselage_count": buildup.inputs.fuselage_count,
            "passenger_compartment_length_m": buildup.inputs.passenger_compartment_length_m,
            "first_class_passenger_count": buildup.inputs.first_class_passenger_count,
            "business_class_passenger_count": buildup.inputs.business_class_passenger_count,
            "tourist_class_passenger_count": buildup.inputs.tourist_class_passenger_count,
            "flight_crew_count": buildup.inputs.flight_crew_count,
            "flight_attendant_count": buildup.inputs.flight_attendant_count,
            "galley_crew_count": buildup.inputs.galley_crew_count,
            "wing_mounted_engine_count": buildup.inputs.wing_mounted_engine_count,
            "fuselage_mounted_engine_count": buildup.inputs.fuselage_mounted_engine_count,
            "engine_count": buildup.inputs.engine_count,
            "rated_thrust_per_engine_n": buildup.inputs.rated_thrust_per_engine_n,
            "nacelle_diameter_m": buildup.inputs.nacelle_diameter_m,
            "hydraulic_pressure_pa": buildup.inputs.hydraulic_pressure_pa,
            "variable_sweep_penalty": buildup.inputs.variable_sweep_penalty,
            "maximum_fuel_capacity_kg": buildup.inputs.maximum_fuel_capacity_kg,
            "fuel_tank_count": buildup.inputs.fuel_tank_count,
            "containerized_cargo_kg": buildup.inputs.containerized_cargo_kg,
        },
        "systems_kg": {
            "surface_controls": systems.surface_controls_kg,
            "apu": systems.apu_kg,
            "instruments": systems.instruments_kg,
            "hydraulics": systems.hydraulics_kg,
            "electrical": systems.electrical_kg,
            "avionics": systems.avionics_kg,
            "furnishings": systems.furnishings_kg,
            "air_conditioning": systems.air_conditioning_kg,
            "anti_ice": systems.anti_ice_kg,
            "total": systems.total_kg,
        },
        "operating_items_kg": {
            "cabin_crew_and_baggage": operating.cabin_crew_and_baggage_kg,
            "flight_crew_and_baggage": operating.flight_crew_and_baggage_kg,
            "unusable_fuel": operating.unusable_fuel_kg,
            "engine_oil": operating.engine_oil_kg,
            "passenger_service": operating.passenger_service_kg,
            "cargo_containers": operating.cargo_containers_kg,
            "total": operating.total_kg,
        },
        "airframe_ownership": {
            "nacelle_kg": airframe.structure.as_ref().map(|group| group.nacelle_kg),
            // A turboprop carries its propulsion in `turboprop_propulsion`
            // and leaves the thrust-based `propulsion` group empty, so
            // reading only the latter exported `null` for the whole ATR
            // propulsion group while the same mass was inside the exported
            // `Propulsion` component slot. The accessor covers both groups
            // and is the same one the ledger closure test uses.
            "propulsion_without_nacelles_kg": buildup.propulsion_without_nacelles_kg(),
            "nacelles_owned_by": "propulsion",
            "nacelles_counted_once": true,
        },
        // Present only for a shaft-power aircraft. The item ledger is what
        // lets a reader see that the ATR's propulsion mass is engines,
        // propellers and installation rather than a thrust-scaled group, and
        // that its gearboxes are charged inside the certificated engine mass
        // rather than a second time.
        "turboprop_propulsion_kg": airframe.turboprop_propulsion.as_ref().map(|group| {
            serde_json::json!({
                "engine_mass_source": group.engine_mass_source,
                "engine_each": group.engine_each_kg,
                "engines": group.engines_kg,
                "gearboxes": group.gearboxes_kg,
                "propeller_each": group.propeller_each_kg,
                "propellers": group.propellers_kg,
                "nacelles": group.nacelles_kg,
                "pylons": group.pylons_kg,
                "engine_installation": group.engine_installation_kg,
                "fuel_system": group.fuel_system_kg,
                "unusable_fuel": group.unusable_fuel_kg,
                "total_without_nacelles": group.total_without_nacelles_kg,
            })
        }),
        "component_masses_kg": {
            "Wing": buildup.masses.wing,
            "H-Stab": buildup.masses.h_stab,
            "V-Stab": buildup.masses.v_stab,
            "Fuselage": buildup.masses.fuselage,
            "Gear": buildup.masses.gear,
            "Propulsion": buildup.masses.propulsion,
            "Systems": buildup.masses.systems,
            "Furnishings": buildup.masses.furnishings,
            "Payload": buildup.masses.payload,
            "Fuel": buildup.masses.fuel,
        },
    })
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
            "analyzed_landing_mass_kg": fuel.analyzed_landing_mass_kg,
            "mtow_shortfall_kg": fuel.mtow_shortfall_kg,
            "mission": {
                "status": mission_fuel_status_name(fuel.mission.status),
                "burned_fuel_kg": fuel.mission.burned_fuel_kg,
                "required_trip_fuel_kg": fuel.mission.required_trip_fuel_kg,
            },
            "dispatch": fuel.dispatch.map(dispatch_to_json),
        },
        "cruise_equilibrium": cruise_equilibrium,
        "mass_balance": report.mass_balance.as_ref().map(mass_balance_to_json),
    })
}

fn mass_balance_to_json(
    assessment: &crate::feasibility::MassBalanceAssessment,
) -> serde_json::Value {
    let states: Vec<serde_json::Value> = assessment
        .states
        .iter()
        .map(|state| {
            serde_json::json!({
                "state": state.label,
                "mass_kg": state.mass_kg,
                "cg_m": state.cg_m,
                "cg_pct_mac": state.cg_pct_mac,
                "inertia_cg_kg_m2": {
                    "ixx": state.inertia_cg.ixx,
                    "iyy": state.inertia_cg.iyy,
                    "izz": state.inertia_cg.izz,
                    "pxy": state.inertia_cg.pxy,
                    "pxz": state.inertia_cg.pxz,
                    "pyz": state.inertia_cg.pyz,
                },
            })
        })
        .collect();
    let tanks: Vec<serde_json::Value> = assessment
        .tanks
        .iter()
        .map(|tank| {
            serde_json::json!({
                "id": tank.id,
                "kind": tank.kind,
                "usable_capacity_kg": tank.usable_capacity_kg,
                "unusable_kg": tank.unusable_kg,
                "centroid_m": tank.centroid_m,
                "capacity_source": tank.capacity_source,
                "burn_priority": tank.burn_priority,
            })
        })
        .collect();
    let items: Vec<serde_json::Value> = assessment
        .ledger_items
        .iter()
        .map(|item| {
            serde_json::json!({
                "id": item.id,
                "group": item.group,
                "mass_kg": item.mass_kg,
                "position_m": item.position_m,
            })
        })
        .collect();
    serde_json::json!({
        "states": states,
        "tanks": tanks,
        "usable_capacity_kg": assessment.usable_capacity_kg,
        "unusable_fuel_kg": assessment.unusable_fuel_kg,
        "geometric_calibration_factor": assessment.geometric_calibration_factor,
        "fuel_cg_curve": assessment
            .fuel_cg_curve
            .iter()
            .map(|point| serde_json::json!({ "fuel_kg": point.fuel_kg, "cg_m": point.cg_m }))
            .collect::<Vec<_>>(),
        "radii_of_gyration_m": {
            "ledger": assessment.radii_check.ledger_radii_m,
            "raymer_jet_transport": assessment.radii_check.reference_radii_m,
            "ratio": assessment.radii_check.ratio,
        },
        "ledger_items": items,
        "lumped_takeoff_cg_pct_mac": assessment.lumped_takeoff_cg_pct_mac,
    })
}

fn dispatch_to_json(dispatch: crate::feasibility::DispatchAssessment) -> serde_json::Value {
    let plan = dispatch.plan.map(|plan| {
        let quantity = |quantity: alas_mass::fuel_plan::FuelQuantity| {
            serde_json::json!({ "kg": quantity.kg, "rule": format!("{:?}", quantity.rule) })
        };
        serde_json::json!({
            "scheme": plan.scheme.as_str(),
            "taxi": quantity(plan.taxi),
            "trip": quantity(plan.trip),
            "contingency": quantity(plan.contingency),
            "alternate": quantity(plan.alternate),
            "final_reserve": quantity(plan.final_reserve),
            "additional": quantity(plan.additional),
            "extra": quantity(plan.extra),
            "takeoff_fuel_kg": plan.takeoff_fuel_kg(),
            "ramp_fuel_kg": plan.ramp_fuel_kg(),
            "block_fuel_kg": plan.block_fuel_kg(),
            "trip_time_s": plan.trip_time_s,
            "destination_landing_mass_kg": plan.destination_landing_mass_kg,
            "reserve_landing_mass_kg": plan.reserve_landing_mass_kg,
        })
    });
    serde_json::json!({
        "outcome": dispatch.outcome.as_str(),
        "takeoff_mass_kg": dispatch.takeoff_mass_kg,
        "shortfall_kg": dispatch.shortfall_kg,
        "native_flights": dispatch.native_flights,
        "plan": plan,
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
        CarriedFuelBasis::ReservePolicyClosure => "reserve_policy_closure",
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
    if let Some(mass_basis) = g.get("analysis_mass_basis_kg") {
        lines.push(format!(
            "  Analysis mass    : {:>10.0} kg (mission-sized finalist)",
            mass_basis
        ));
    }
    let system_method = config.map(|cfg| cfg.mass_model.systems_mass_method);
    let system_status = config
        .map(|cfg| {
            if cfg.mass_model.mass_architecture.is_pure_flops() {
                "FLOPS architecture"
            } else {
                "compatibility-only fractions"
            }
        })
        .unwrap_or("mass architecture unavailable");
    lines.extend([
        format!("  MTOW (computed)  : {:>10.0} kg", m_total),
        format!("  OEW              : {:>10.0} kg", m_oew),
        format!(
            "  Systems mass     : {:?} ({})",
            system_method, system_status
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
        format!("  Polar fit status  : {}", pf.status.as_str()),
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

#[cfg(test)]
mod tests {
    use super::report_to_database;
    use crate::full_analysis::{AnalysisReport, DesignPoint, PolarFit, PolarFitStatus};
    use alas_aero::analysis::PolarSweep;
    use alas_config::design_variables::DesignVector;
    use alas_config::AlasConfig;
    use alas_geom::aircraft::airplane::Airplane;
    use std::collections::HashMap;

    #[test]
    fn polar_fit_fallback_status_survives_json_export() {
        let report = AnalysisReport {
            design: DesignVector::default(),
            airplane: Airplane {
                name: "export fixture".to_owned(),
                xyz_ref: [0.0, 0.0, 0.0],
                wings: Vec::new(),
                fuselages: Vec::new(),
                s_ref: 1.0,
                c_ref: 1.0,
                b_ref: 1.0,
            },
            polar: PolarSweep {
                alpha_deg: Vec::new(),
                geometric_alpha_deg: Vec::new(),
                cl: Vec::new(),
                cd: Vec::new(),
                cd_induced: Vec::new(),
                cd_wave: Vec::new(),
                cd_parasite: Vec::new(),
                cm: Vec::new(),
                l_over_d: Vec::new(),
            },
            design_point: DesignPoint {
                alpha_deg: 0.0,
                cl: 0.0,
                cd: 0.0,
                l_over_d: 0.0,
            },
            polar_fit: PolarFit {
                cd0: 0.02,
                k: 0.04,
                oswald_e: 1.0 / (std::f64::consts::PI * 10.0 * 0.04),
                aspect_ratio: 10.0,
                status: PolarFitStatus::FallbackInsufficientPoints,
            },
            static_margin: 0.0,
            x_neutral_point: 0.0,
            geometry_summary: HashMap::new(),
            component_masses: HashMap::new(),
            flops_mass_buildup: None,
            mass_coordinates: HashMap::new(),
            physical_cg: [0.0, 0.0, 0.0],
            payload_layout: None,
            trimmed_design_point: None,
            cg_envelope_ok: None,
        };

        let database = report_to_database(&report, &AlasConfig::default());
        assert_eq!(
            database.aerodynamics["polar_fit_status"],
            "fallback_insufficient_points"
        );
    }
}
