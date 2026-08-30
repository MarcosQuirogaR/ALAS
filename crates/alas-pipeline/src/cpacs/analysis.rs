// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// CPACS analysis nodes backed by completed ALAS reports.
//
// CPACS 3.5 has standard containers for aerodynamic maps, weight and balance,
// and mission trajectories. It does not define a generic feasibility node or a
// scalar reference-span field, so this module emits only the standard values
// that have a direct source in the report and leaves the rest absent.

use std::fmt::Write as FmtWrite;

use alas_config::AlasConfig;
use alas_mass::breakdown::{
    FUEL, FURNISHINGS, FUSELAGE, GEAR, H_STAB, PAYLOAD, PROPULSION, SYSTEMS, V_STAB, WING,
};
use alas_mission::MissionResult;

use crate::feasibility::FeasibilityReport;
use crate::full_analysis::AnalysisReport;

use super::{number, write_text, write_vector};

const MASS_COMPONENTS: [&str; 10] = [
    WING,
    H_STAB,
    V_STAB,
    FUSELAGE,
    GEAR,
    PROPULSION,
    SYSTEMS,
    FURNISHINGS,
    PAYLOAD,
    FUEL,
];

/// Write the standard aircraft-analysis branches for a completed report.
pub(super) fn write_aircraft_analyses(
    xml: &mut String,
    report: &AnalysisReport,
    config: &AlasConfig,
    feasibility: Option<&FeasibilityReport>,
    mission: Option<&MissionResult>,
) {
    let has_aerodynamics = has_finite_polar(report, config);
    let analysis_mass_case = analysis_mass_case(report);
    let feasibility_mass_case = feasibility.and_then(|value| analyzed_mass_case(report, value));
    let has_weight_balance = analysis_mass_case.is_some() || feasibility_mass_case.is_some();
    let mission_summary = mission.and_then(complete_mission_summary);

    if !has_aerodynamics && !has_weight_balance && mission_summary.is_none() {
        return;
    }

    let _ = writeln!(xml, "        <analyses>");
    if has_aerodynamics {
        write_aero_performance(xml, report, config);
    }
    if has_weight_balance {
        write_weight_and_balance(xml, analysis_mass_case, feasibility_mass_case);
    }
    if let Some((fuel_mass_kg, block_time_s, distance_m)) = mission_summary {
        write_trajectory(xml, fuel_mass_kg, block_time_s, distance_m);
    }
    let _ = writeln!(xml, "        </analyses>");
}

fn has_finite_polar(report: &AnalysisReport, config: &AlasConfig) -> bool {
    let alpha = &report.polar.alpha_deg;
    !alpha.is_empty()
        && config.requirements.cruise_mach.is_finite()
        && config.requirements.cruise_altitude_m.is_finite()
        && alpha.iter().all(|value| value.is_finite())
}

fn write_aero_performance(xml: &mut String, report: &AnalysisReport, config: &AlasConfig) {
    let polar = &report.polar;
    let alpha = &polar.alpha_deg;
    let sample_count = alpha.len();
    let cl = finite_vector(&polar.cl, sample_count);
    let cd = finite_vector(&polar.cd, sample_count);
    let cm = finite_vector(&polar.cm, sample_count);

    let _ = writeln!(xml, "          <aeroPerformance>");
    write_polar_map(
        xml,
        "alas-aero-polar",
        "ALAS aerodynamic polar",
        alpha,
        cl,
        cd,
        cm,
        config,
    );

    if let Some(design_point) = finite_design_point(report) {
        let design_cm = design_point_cm(report, design_point.alpha_deg);
        write_aero_map(
            xml,
            "alas-aero-design-point",
            "ALAS aerodynamic design point",
            "Single-point aerodynamic result selected from the analyzed polar.",
            config,
            &[design_point.alpha_deg],
            Some(&[design_point.cl]),
            Some(&[design_point.cd]),
            design_cm.as_deref(),
        );
    }
    let _ = writeln!(xml, "          </aeroPerformance>");
}

// CPACS keeps map identity, flight condition, and each coefficient channel as
// separate schema fields; grouping them would obscure the serialization map.
#[allow(clippy::too_many_arguments)]
fn write_polar_map(
    xml: &mut String,
    uid: &str,
    name: &str,
    alpha_deg: &[f64],
    cl: Option<&[f64]>,
    cd: Option<&[f64]>,
    cm: Option<&[f64]>,
    config: &AlasConfig,
) {
    write_aero_map(
        xml,
        uid,
        name,
        "Fine-mesh ALAS aerodynamic polar; cms is the available pitching-moment stability record.",
        config,
        alpha_deg,
        cl,
        cd,
        cm,
    );
}

// This mirrors the CPACS aeroPerformanceMap fields one-for-one so omitted
// coefficient channels remain explicit rather than hidden in a loose bundle.
#[allow(clippy::too_many_arguments)]
fn write_aero_map(
    xml: &mut String,
    uid: &str,
    name: &str,
    description: &str,
    config: &AlasConfig,
    alpha_deg: &[f64],
    cl: Option<&[f64]>,
    cd: Option<&[f64]>,
    cm: Option<&[f64]>,
) {
    let _ = writeln!(xml, "            <aeroMap uID=\"{uid}\">");
    write_text(xml, 14, "name", name);
    write_text(xml, 14, "description", description);
    let _ = writeln!(xml, "              <boundaryConditions>");
    write_text(xml, 16, "atmosphericModel", "ISA");
    let _ = writeln!(xml, "              </boundaryConditions>");
    let _ = writeln!(xml, "              <aeroPerformanceMap>");
    write_double_vector(
        xml,
        16,
        "altitude",
        std::iter::repeat_n(config.requirements.cruise_altitude_m, alpha_deg.len()),
    );
    write_double_vector(
        xml,
        16,
        "machNumber",
        std::iter::repeat_n(config.requirements.cruise_mach, alpha_deg.len()),
    );
    write_double_vector(
        xml,
        16,
        "angleOfSideslip",
        std::iter::repeat_n(0.0, alpha_deg.len()),
    );
    write_double_vector(xml, 16, "angleOfAttack", alpha_deg.iter().copied());
    if let Some(cl) = cl {
        write_double_vector(xml, 16, "cl", cl.iter().copied());
    }
    if let Some(cd) = cd {
        write_double_vector(xml, 16, "cd", cd.iter().copied());
    }
    if let Some(cm) = cm {
        write_double_vector(xml, 16, "cms", cm.iter().copied());
    }
    let _ = writeln!(xml, "              </aeroPerformanceMap>");
    let _ = writeln!(xml, "            </aeroMap>");
}

fn write_double_vector<I>(xml: &mut String, indent: usize, element: &str, values: I)
where
    I: IntoIterator<Item = f64>,
{
    let serialized = values.into_iter().map(number).collect::<Vec<_>>().join(";");
    write_vector(xml, indent, element, &serialized);
}

fn finite_vector(values: &[f64], expected_len: usize) -> Option<&[f64]> {
    (values.len() == expected_len && values.iter().all(|value| value.is_finite())).then_some(values)
}

fn finite_design_point(report: &AnalysisReport) -> Option<DesignPointValues> {
    let point = report.design_point;
    (point.alpha_deg.is_finite() && point.cl.is_finite() && point.cd.is_finite()).then_some(
        DesignPointValues {
            alpha_deg: point.alpha_deg,
            cl: point.cl,
            cd: point.cd,
        },
    )
}

fn design_point_cm(report: &AnalysisReport, alpha_deg: f64) -> Option<Vec<f64>> {
    let index = report
        .polar
        .alpha_deg
        .iter()
        .enumerate()
        .filter(|(_, value)| value.is_finite())
        .min_by(|(_, left), (_, right)| {
            (alpha_deg - **left)
                .abs()
                .total_cmp(&(alpha_deg - **right).abs())
        })
        .map(|(index, _)| index)?;
    let cm = *report.polar.cm.get(index)?;
    cm.is_finite().then_some(vec![cm])
}

#[derive(Debug, Clone, Copy)]
struct DesignPointValues {
    alpha_deg: f64,
    cl: f64,
    cd: f64,
}

fn write_weight_and_balance(
    xml: &mut String,
    analysis_case: Option<MassCase>,
    feasibility_case: Option<MassCase>,
) {
    let _ = writeln!(xml, "          <weightAndBalance>");
    let _ = writeln!(xml, "            <operationalCases>");
    if let Some(case) = analysis_case {
        write_operational_case(
            xml,
            "alas-wb-analysis",
            "ALAS analysis mass properties",
            case,
        );
    }
    if let Some(case) = feasibility_case {
        write_operational_case(
            xml,
            "alas-wb-analyzed-takeoff",
            "ALAS analyzed takeoff load case",
            case,
        );
    }
    let _ = writeln!(xml, "            </operationalCases>");
    let _ = writeln!(xml, "          </weightAndBalance>");
}

fn write_operational_case(xml: &mut String, uid: &str, name: &str, case: MassCase) {
    let _ = writeln!(xml, "              <operationalCase uID=\"{uid}\">");
    write_text(xml, 16, "name", name);
    write_vector(xml, 16, "mass", &number(case.mass_kg));
    let _ = writeln!(xml, "                <coG>");
    write_vector(xml, 18, "x", &number(case.cg_m[0]));
    write_vector(xml, 18, "y", &number(case.cg_m[1]));
    write_vector(xml, 18, "z", &number(case.cg_m[2]));
    let _ = writeln!(xml, "                </coG>");
    let _ = writeln!(xml, "              </operationalCase>");
}

fn analysis_mass_case(report: &AnalysisReport) -> Option<MassCase> {
    let mass_kg = component_mass_total(report, None)?;
    let cg_m = report.physical_cg;
    finite_mass_case(mass_kg, cg_m)
}

fn analyzed_mass_case(
    report: &AnalysisReport,
    feasibility: &FeasibilityReport,
) -> Option<MassCase> {
    let fuel_mass_kg = feasibility.fuel_loading.analyzed_carried_fuel_kg;
    let (mass_kg, cg_m) = component_mass_and_cg(report, Some(fuel_mass_kg))?;
    finite_mass_case(mass_kg, cg_m)
}

fn component_mass_total(report: &AnalysisReport, fuel_override_kg: Option<f64>) -> Option<f64> {
    MASS_COMPONENTS
        .into_iter()
        .map(|name| {
            if name == FUEL {
                fuel_override_kg.or_else(|| report.component_masses.get(name).copied())
            } else {
                report.component_masses.get(name).copied()
            }
        })
        .collect::<Option<Vec<_>>>()
        .and_then(|values| {
            values
                .iter()
                .all(|value| value.is_finite())
                .then(|| values.into_iter().sum())
        })
}

fn component_mass_and_cg(
    report: &AnalysisReport,
    fuel_override_kg: Option<f64>,
) -> Option<(f64, [f64; 3])> {
    let mut total_mass_kg = 0.0;
    let mut moment_kg_m = [0.0; 3];
    for name in MASS_COMPONENTS {
        let mass_kg = if name == FUEL {
            fuel_override_kg.or_else(|| report.component_masses.get(name).copied())?
        } else {
            report.component_masses.get(name).copied()?
        };
        let cg_m = report.mass_coordinates.get(name).copied()?;
        if !mass_kg.is_finite() || !cg_m.iter().all(|value| value.is_finite()) {
            return None;
        }
        total_mass_kg += mass_kg;
        for axis in 0..3 {
            moment_kg_m[axis] += mass_kg * cg_m[axis];
        }
    }
    if !total_mass_kg.is_finite() || total_mass_kg <= 0.0 {
        return None;
    }
    let cg_m = moment_kg_m.map(|moment| moment / total_mass_kg);
    Some((total_mass_kg, cg_m))
}

#[derive(Debug, Clone, Copy)]
struct MassCase {
    mass_kg: f64,
    cg_m: [f64; 3],
}

fn finite_mass_case(mass_kg: f64, cg_m: [f64; 3]) -> Option<MassCase> {
    (mass_kg.is_finite() && mass_kg > 0.0 && cg_m.iter().all(|value| value.is_finite()))
        .then_some(MassCase { mass_kg, cg_m })
}

fn complete_mission_summary(mission: &MissionResult) -> Option<(f64, f64, f64)> {
    mission.completed_summary().map(|summary| {
        (
            summary.trip_fuel_kg,
            summary.block_time_s,
            summary.distance_flown_m,
        )
    })
}

fn write_trajectory(xml: &mut String, fuel_mass_kg: f64, block_time_s: f64, distance_m: f64) {
    let _ = writeln!(xml, "          <trajectories>");
    let _ = writeln!(
        xml,
        "            <trajectory uID=\"alas-mission-trajectory\">"
    );
    write_text(xml, 16, "name", "ALAS analyzed mission");
    write_text(
        xml,
        16,
        "description",
        "Complete converged mission summary from the native ALAS mission result.",
    );
    let _ = writeln!(xml, "              <global>");
    super::write_number_element(xml, 16, "fuelMass", fuel_mass_kg);
    super::write_number_element(xml, 16, "time", block_time_s);
    super::write_number_element(xml, 16, "distance", distance_m);
    let _ = writeln!(xml, "              </global>");
    let _ = writeln!(xml, "            </trajectory>");
    let _ = writeln!(xml, "          </trajectories>");
}
